# devloop-log.jq — turn a headless devloop's stream-json log into one record per event.
# Driven by devloop-log.sh (which owns the views and the follow loop); run with -nR so a
# torn or non-JSON line is skipped instead of killing a live follow.
#
# Args: $rules  [{who: null|name, kinds: [kind...]}]  an event prints if any rule matches
#       $width  0 = no truncation, else the line budget in characters
#       $full   true = keep newlines (continuation lines indented) instead of folding
#       $color  true = ANSI colour
#
# Kinds: start end phase spawn gate commit hook limit escalation   (the --phases view)
#        text msg status tool
#
# Every "phase" signal is read from what the session itself did, never inferred from prose:
#   phase       the Lead rewriting the `| Phase | `x` |` row of the devloop's main.md
#   gate        a layer-all.sh / layer-fast.sh invocation, and the pipeline's own
#               LAYER=/TOTAL_RESULT= summary lines in any tool result
#   commit      the result of a `git commit` command
#   hook        Stop-hook feedback injected into the Lead (devloop-stop-hook.sh)

def who_of($s; $e):
  if $e.parent_tool_use_id == null then "lead"
  else ($s.names[$e.parent_tool_use_id] // $e.subagent_type // "agent") end;

def blocks($e):
  ($e.message.content // []) | if type == "array" then .[] elif type == "string" then {type: "text", text: .} else empty end;

def result_text:
  .content | if type == "array" then map(.text? // "") | join("") elif type == "string" then . else "" end;

def dur:
  floor | if . >= 3600 then "\(. / 3600 | floor)h\(. % 3600 / 60 | floor)m"
          elif . >= 60 then "\(. / 60 | floor)m\(. % 60)s"
          else "\(.)s" end;

# The phase a tool call sets, or null. Edit/Write carry the new row directly; a Bash
# rewrite (sed s/old/new/, python .replace(old, new)) names old then new, so it needs
# two matches and the last one wins — a lone match is a grep, not a transition.
def phase_set($tool; $in):
  (if $tool == "Edit" then ($in.new_string // "")
   elif $tool == "Write" then ($in.content // "")
   elif $tool == "Bash" then ($in.command // "")
   else "" end)
  | [match("\\| *Phase *\\| *\\\\?`([a-z_-]+)\\\\?` *\\|"; "g") | .captures[0].string]
  | if ($tool == "Bash" and length < 2) then null else last end;

# Only an invocation in command position counts (after env assignments and `timeout N`,
# `env`, `time`, `nice` wrappers); `grep layer-all.sh` is not a gate run.
def gate_cmd:
  (capture("(^|[;&|(]|\\n)\\s*((\\w+=\\S*|timeout(\\s+-\\S+)*\\s+\\S+|env|time|nice)\\s+)*(\\./|/work/)?scripts/(?<s>layer-(all|fast)\\.sh)") | .s) // null;

def commit_cmd: test("(^|[;&|(]|\\n)\\s*git\\s+(-C\\s+\\S+\\s+)?commit\\b");

def gate_summary:
  [match("(?m)^LAYER=(\\d+) RESULT=(\\S+)"; "g") | {n: (.captures[0].string | tonumber), r: .captures[1].string}] as $ls
  | ((capture("TOTAL_DURATION=(?<d>\\d+)") | .d | tonumber) // null) as $d
  | ((capture("TOTAL_RESULT=(?<r>\\S+)") | .r) // "?") as $r
  | ($ls | map(select(.r != "OK")) | map("L\(.n) \(.r)") | join(", ")) as $bad
  | {sev: (if $r == "OK" then "ok" else "fail" end),
     text: ("gate " + (if ($ls | length) > 0 then "layers \($ls | map(.n) | min)-\($ls | map(.n) | max) " else "" end)
            + $r + (if $d then " (\($d | dur))" else "" end) + (if $bad != "" then ": " + $bad else "" end))};

def commit_summary($b):
  ($b | result_text) as $t
  | if ($b.is_error == true) or ($t | test("(?m)^EXIT=[1-9]")) then
      {sev: "fail", text: ("commit FAILED: " + ($t | split("\n") | map(select(test("\\S"))) | map(select(test("^EXIT=") | not)) | last // ""))}
    else
      (($t | capture("\\[\\S+ (?<h>[0-9a-f]{7,})\\] (?<s>[^\\n]*)")) // ($t | capture("(?m)^(?<h>[0-9a-f]{7,40}) (?<s>.*)$")) // null) as $c
      | {sev: "ok", text: (if $c then "commit \($c.h) \($c.s)" else "commit done" end)}
    end;

def tool_arg($in):
  ($in.command // $in.file_path // $in.pattern // $in.query // $in.skill // $in.description // ($in | tojson)) | tostring;

def escalation($tool; $in):
  ($tool == "Write" and (($in.file_path // "") | endswith(".devloop-escalation.json")))
  or ($tool == "Bash" and (($in.command // "") | test(">\\s*\\S*\\.devloop-escalation\\.json")));

def rec($kind; $who; $text): {kind: $kind, who: $who, text: $text};

def on_assistant($e):
  who_of(.; $e) as $who
  | reduce blocks($e) as $b (.;
      if $b.type == "text" then
        if (($b.text // "") | test("\\S")) then .out += [rec("text"; $who; $b.text)] else . end
      elif $b.type == "tool_use" then
        ($b.input // {}) as $in
        | if $b.name == "Agent" then
            ($in.name // $in.description // $in.subagent_type // "agent") as $n
            | .names[$b.id // ""] = $n
            | .out += [rec("spawn"; $who; "spawn \($n) (\($in.subagent_type // "?")): \($in.description // "")")]
          elif $b.name == "SendMessage" then
            .out += [rec("msg"; $who; "→ \($in.to // "?"): \($in.message // "" | if type == "string" then . else tojson end)")]
          else
            (if $who == "lead" then phase_set($b.name; $in) else null end) as $ph
            | (if $ph != null and $ph != .phase then
                 .out += [rec("phase"; $who; (if .phase then "phase \(.phase) → \($ph)" else "phase → \($ph)" end))] | .phase = $ph
               else . end)
            | (($in.command // "") | if $b.name == "Bash" then gate_cmd else null end) as $g
            | (if $g then .out += [rec("gate"; $who; "gate \($g) started") + {sev: "run"}] else . end)
            | (if $b.name == "Bash" and (($in.command // "") | commit_cmd) then .commits[$b.id // ""] = true else . end)
            | (if escalation($b.name; $in) then .out += [rec("escalation"; $who; "escalation file written")] else . end)
            | .out += [rec("tool"; $who; "\($b.name): \(tool_arg($in))")]
          end
      else . end);

def on_user($e):
  who_of(.; $e) as $who
  | reduce blocks($e) as $b (.;
      if $b.type == "tool_result" then
        ($b | result_text) as $t
        | (if ($t | test("TOTAL_RESULT=")) then
             ($t | gate_summary) as $g
             | if .lastgate == [$who, $g.text] then . else .out += [rec("gate"; $who; $g.text) + {sev: $g.sev}] | .lastgate = [$who, $g.text] end
           else . end)
        | (if .commits[$b.tool_use_id // ""] then
             commit_summary($b) as $c
             | .out += [rec("commit"; $who; $c.text) + {sev: $c.sev}] | .commits |= del(.[$b.tool_use_id])
           else . end)
      elif $b.type == "text" and (($b.text // "") | startswith("Stop hook feedback")) then
        .out += [rec("hook"; $who; $b.text | sub("^Stop hook feedback:\\s*"; "stop hook: "))]
      else . end);

def on_event($e):
  if $e.type == "system" and $e.subtype == "init" then
    .out += [rec("start"; "lead"; "session start · \($e.model // "?") · claude \($e.claude_code_version // "?")")]
  elif $e.type == "system" and $e.subtype == "task_started" then
    # A teammate is a local_agent task; anything else (a backgrounded command) belongs to
    # the agent that started it: its parent task, or the Lead.
    .tasks[$e.task_id // ""] =
      (if $e.task_type == "local_agent" then
         {who: (.names[$e.tool_use_id // ""] // $e.subagent_type // $e.description // "agent"), bg: false}
       else
         {who: (.tasks[$e.parent_task_id // ""].who // "lead"), bg: true}
       end)
  elif $e.type == "system" and $e.subtype == "task_notification" then
    (.tasks[$e.task_id // ""] // {who: (.names[$e.tool_use_id // ""] // "agent"), bg: false}) as $tk
    | .out += [rec("status"; $tk.who; "[\($e.status // "?")] " + (if $tk.bg then "bg: " else "" end) + ($e.summary // ""))]
  elif $e.type == "rate_limit_event" then
    # Report changes only: every turn re-sends the same status.
    [($e.rate_limit_info.status // "allowed"), ($e.rate_limit_info.rateLimitType // "?")] as $l
    | if $l[0] != "allowed" and $l != .lastlimit then
        .out += [rec("limit"; "lead"; "rate limit \($l[0]) (\($l[1]))")] | .lastlimit = $l
      elif $l[0] == "allowed" then .lastlimit = null
      else . end
  elif $e.type == "result" then
    .out += [rec("end"; "lead";
      "end \($e.subtype // "?") · \($e.num_turns // "?") turns · \((($e.duration_ms // 0) / 1000) | dur) · $\((($e.total_cost_usd // 0) * 100 | round) / 100)"
      + (($e.result // "") | split("\n")[0] | if . != "" then " · " + . else "" end))
      + {sev: (if $e.is_error == true or $e.subtype != "success" then "fail" else "ok" end)}]
  elif $e.type == "assistant" then on_assistant($e)
  elif $e.type == "user" then on_user($e)
  else . end;

def pad($n): if ($n - length) > 0 then . + (" " * ($n - length)) else . end;

def hhmmss:
  if . == null then "--:--:--"
  else (sub("\\.\\d+Z$"; "Z") | try (fromdateiso8601 | strflocaltime("%H:%M:%S")) catch "--:--:--") end;

def paint($code): if $color then "\u001b[\($code)m\(.)\u001b[0m" else . end;

def kind_color:
  if .kind == "phase" then "1;33"
  elif .kind == "escalation" or .kind == "hook" or .kind == "limit" then "1;31"
  elif .kind == "start" or .kind == "end" then (if .sev == "fail" then "1;31" else "1;35" end)
  elif .kind == "gate" or .kind == "commit" then (if .sev == "ok" then "32" elif .sev == "fail" then "1;31" else "36" end)
  elif .kind == "spawn" then "34"
  elif .kind == "msg" then "36"
  elif .kind == "status" or .kind == "tool" then "2"
  else "0" end;

def render:
  kind_color as $kc
  | (.t | hhmmss) as $time
  | (.who | pad(14)) as $who
  | ("\($time)  \($who)  ") as $prefix
  | (if $full then .text | gsub("\\n"; "\n" + (" " * ($prefix | length)))
     else .text | gsub("\\s*\\n\\s*"; " ⏎ ")
       | if $width > 0 and length > ($width - ($prefix | length)) then .[0:($width - ($prefix | length) - 1)] + "…" else . end
     end) as $text
  | $time + "  " + (if .who == "lead" then $who | paint("1") else $who end) + "  " + ($text | paint($kc));

foreach (inputs | fromjson? | select(type == "object")) as $e (
  {names: {}, tasks: {}, commits: {}, phase: null, t: null, lastgate: null, lastlimit: null, out: []};
  .out = [] | .t = ($e.timestamp // .t) | on_event($e);
  .t as $t | .out[] | . + {t: $t}
)
| . as $r
| select(any($rules[]; (.who == null or .who == $r.who) and (.kinds | index($r.kind))))
| render
