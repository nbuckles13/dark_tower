#!/usr/bin/env bash
# mh.test.sh — hermetic self-test for infra/kind/scripts/mh.sh (story 4 task I-5).
#
# mh.sh is the MH pool tool whose machine contract
# (docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt) is consumed by
# deploy.sh, the env-test mh_pool fixture and the ops runbook. Nothing calls it before I-2, so
# this suite is its only exerciser. No cluster: a STATEFUL PATH-stub kubectl (scripts/layer7.test.sh
# style) serves one JSON state file per case. Its whole behaviour is one jq program (STUB_JQ
# below); it advances one "tick" per call, so pods go Ready, get SIGTERM and disappear after a
# per-case number of calls. The stub is LOUD: an unknown verb (rc 97), a call without the
# expected --context (rc 98), and any --force / --grace-period / pod delete (rc 96) each drop a
# violation file that the final checks fail on.
#
# TIMING RULE: a wait bound may be small ONLY when the awaited condition can never hold (the
# stub never transitions); every positive arm runs with generous margins and finishes on ticks.
#
# Every mh.sh invocation goes through run_mh, which asserts the stream contract (exactly one
# MH_RESULT line and it is the last stderr line; with --json exactly one JSON object whose "ok"
# matches STATUS; without --json an empty stdout) and records REASON + rc for the
# token-completeness check (mh.sh's MH_TOKENS == emitted tokens == contract §3, both directions).
#
# Wired into scripts/layer3.sh. Consumes scripts/lang/_test_helpers.sh.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/../../.." && pwd)"
# shellcheck source=../../../scripts/lang/_test_helpers.sh
source "${REPO_ROOT}/scripts/lang/_test_helpers.sh"
set +e

MH="${__here}/mh.sh"
CONTRACT="${REPO_ROOT}/docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
EMITTED="${WORK}/emitted"     # "<token> <rc>" per mh.sh run
: >"$EMITTED"
BASH_BIN="$(command -v bash)"

# --- The stub kubectl ---------------------------------------------------------------

STUB_BIN="${WORK}/bin"
mkdir -p "$STUB_BIN"
STUB_JQ="${WORK}/stub.jq"
cat >"$STUB_JQ" <<'JQ'
def LS: "dark-tower.io/mh-slot";
def lbl($k): (.metadata.labels // {})[$k];
def ok($o): {state: ., rc: 0, out: $o, err: ""};
def ok: ok("");
def bad($e): {state: ., rc: 1, out: "", err: $e};
def iso: todate;
def advance:
  .tick as $t
  | .pods |= map(if ._sigterm_at != null and ._sigterm_at >= 0 and $t >= ._sigterm_at and .metadata.deletionTimestamp == null
                 then .metadata.deletionTimestamp = (($now + (.spec.terminationGracePeriodSeconds // 30)) | iso) else . end)
  | .pods |= map(select((._gone_at != null and ._gone_at >= 0 and $t >= ._gone_at) | not))
  | .deploys |= map(select((._vanish_at != null and ._vanish_at >= 0 and $t >= ._vanish_at) | not))
  | . as $st
  | .deploys |= map(select(.metadata.deletionTimestamp == null or ._hold
                           or (lbl(LS) as $k | any($st.pods[]; lbl(LS) == $k))));
def podview: .tick as $t | .pods | map(.status.conditions = [{type: "Ready",
    status: (if ._ready_at >= 0 and $t >= ._ready_at then "True" else "False" end)}]
  | .status.ephemeralContainerStatuses = [(._injects // [])[] | {name, state:
      (if .waiting != null then {waiting: {reason: .waiting}}
       elif .done_at >= 0 and $t >= .done_at then {terminated: {exitCode: .exit}}
       else {running: {}} end)}]);
def depview: podview as $pv | .deploys | map(lbl(LS) as $k
    | ([$pv[] | select(lbl(LS) == $k and .metadata.deletionTimestamp == null and .status.conditions[0].status == "True")] | length) as $a
    | .metadata.generation = 1
    | .status = {observedGeneration: 1, availableReplicas: $a,
                 conditions: [{type: "Available", status: (if $a > 0 then "True" else "False" end)}]});
def newpod($d; $ready_at):
  {metadata: {name: ($d.metadata.name + "-p\(.tick)"), uid: ("uid-\($d.metadata.name)-\(.tick)"),
              labels: $d.spec.template.metadata.labels, creationTimestamp: ($now | iso)},
   spec: {terminationGracePeriodSeconds: ($d.spec.template.spec.terminationGracePeriodSeconds // 30)},
   status: {podIP: "10.244.0.\(.tick % 250)", containerStatuses: [{name: $d.spec.template.spec.containers[0].name, restartCount: 0}]},
   _ready_at: $ready_at};
def create_deploy($b):
  ($b.metadata.labels[LS]) as $k
  | if any(.deploys[]; .metadata.name == $b.metadata.name) then
      bad("Error from server (AlreadyExists): deployments.apps \"\($b.metadata.name)\" already exists")
    elif ((.knobs.create_err // []) | index($k)) != null then
      bad("Error from server (Forbidden): deployments.apps is forbidden: denied by stub")
    elif ((.knobs.race // []) | index($k)) != null then
      # a concurrent creator won this slot: materialise its MH, then report AlreadyExists
      ($b | .metadata.labels["dark-tower.io/mh-owner"] = "racer" | .spec.template.metadata.labels["dark-tower.io/mh-owner"] = "racer") as $r
      | .deploys += [$r] | .pods += [newpod($r; 0)]
      | bad("Error from server (AlreadyExists): deployments.apps \"\($b.metadata.name)\" already exists")
    else
      # vanish_after: a third party deletes the new MH (Deployment + pod) N ticks after create
      (if (.knobs.vanish_after // -1) >= 0 then .tick + .knobs.vanish_after else -1 end) as $van
      | .created += [$b] | .deploys += [$b | ._vanish_at = $van]
      | .pods += [newpod($b; (if .knobs.ready_after < 0 then -1 else .tick + .knobs.ready_after end)) | ._gone_at = $van]
      | ok("deployment.apps/\($b.metadata.name) created")
    end;
def delete_deploy($n):
  (first(.deploys[] | select(.metadata.name == $n)) // null) as $d
  | if $d == null then bad("Error from server (NotFound): deployments.apps \"\($n)\" not found")
    else ($d | lbl(LS)) as $k | .tick as $t | .knobs as $kn
      | .deploys |= map(if .metadata.name == $n then .metadata.deletionTimestamp = ($now | iso) else . end)
      | .pods |= map(if lbl(LS) == $k and .metadata.deletionTimestamp == null then
            ._sigterm_at = (if $kn.sigterm_after < 0 then -1 else $t + $kn.sigterm_after end)
            | ._gone_at = (if $kn.gone_after < 0 then -1 else $t + $kn.gone_after end)
            | (if $kn.sigterm_after == 0 then .metadata.deletionTimestamp = (($now + (.spec.terminationGracePeriodSeconds // 30)) | iso) else . end)
          else . end)
      | ok("deployment.apps \"\($n)\" deleted")
    end;
# signal's calls: `kubectl debug` adds an ephemeral container whose outcome is knobs.inject.
def debug_pod($a):
  ($a[1]) as $pn
  | ([$a[] | select(startswith("--container=")) | ltrimstr("--container=")] | .[0]) as $cn
  | if any(.pods[]; .metadata.name == $pn) | not then bad("Error from server (NotFound): pods \"\($pn)\" not found")
    else (.knobs.inject // {}) as $i | .tick as $t
      | .debugs += [{pod: $pn, container: $cn, argv: $a}]
      | .pods |= map(if .metadata.name == $pn then
            .spec.ephemeralContainers = ((.spec.ephemeralContainers // []) + [{name: $cn}])
            | ._injects = ((._injects // []) + [{name: $cn, exit: ($i.exit // 0), logs: ($i.logs // ""), waiting: ($i.waiting // null),
                                                 done_at: (if ($i.after // 1) < 0 then -1 else $t + ($i.after // 1) end)}])
          else . end)
      | ok("")
    end;
def lease_get:
  if .lease == null then bad("Error from server (NotFound): leases.coordination.k8s.io \"mh-pool\" not found")
  else (if .knobs.lease_steal and (.lease.spec.holderIdentity // "") != "thief@stub:1" then
          .lease.spec.holderIdentity = "thief@stub:1"
          | .lease.metadata.resourceVersion = ((.lease.metadata.resourceVersion | tonumber) + 1 | tostring)
        else . end) | ok(.lease | tojson) end;
def lease_create($b):
  if .lease != null then bad("Error from server (AlreadyExists): leases.coordination.k8s.io \"mh-pool\" already exists")
  else .lease = ($b | .metadata.resourceVersion = "1") | ok(.lease | tojson) end;
def lease_replace($b):
  if .lease == null then bad("Error from server (NotFound): leases.coordination.k8s.io \"mh-pool\" not found")
  elif .knobs.lease_conflict or ($b.metadata.resourceVersion != .lease.metadata.resourceVersion) then
    bad("Error from server (Conflict): Operation cannot be fulfilled on leases.coordination.k8s.io \"mh-pool\": the object has been modified")
  else .lease = ($b | .metadata.resourceVersion = (($b.metadata.resourceVersion | tonumber) + 1 | tostring)) | ok(.lease | tojson) end;
def dispatch($a):
  ($a | join(" ")) as $s
  | if $s == "config get-contexts -o name" then ok($contexts)
    elif $s == "get configmap mh-template -o json" then
      (if .knobs.template_err then bad("Unable to connect to the server: dial tcp 127.0.0.1:6443: connect: connection refused")
       elif .template == null then bad("Error from server (NotFound): configmaps \"mh-template\" not found")
       else ok(.template | tojson) end)
    elif $s == "get configmaps -l dark-tower.io/mh-slot -o json" then ok({items: .slotcms} | tojson)
    elif $s == "get deployments -l dark-tower.io/mh-slot -o json" then ok({items: depview} | tojson)
    elif $s == "get pods -l dark-tower.io/mh-slot -o json" then ok({items: podview} | tojson)
    elif ($a | length) == 5 and $a[0] == "get" and $a[1] == "deployment" and $a[3:] == ["-o", "json"] then
      (.polled += [$a[2]] | (first(depview[] | select(.metadata.name == $a[2])) // null) as $d
       | if $d == null then bad("Error from server (NotFound): deployments.apps \"\($a[2])\" not found") else ok($d | tojson) end)
    elif ($a | length) == 5 and $a[0] == "get" and $a[1] == "pod" and $a[3:] == ["-o", "json"] then
      ((first(podview[] | select(.metadata.name == $a[2])) // null) as $p
       | if $p == null then bad("Error from server (NotFound): pods \"\($a[2])\" not found")
         else ok($p | del(._injects, ._ready_at, ._gone_at, ._sigterm_at) | tojson) end)
    elif $a[0] == "debug" and ($a | length) > 2 then debug_pod($a)
    elif ($a | length) == 4 and $a[0] == "logs" and $a[2] == "-c" then
      ((first(.pods[] | select(.metadata.name == $a[1])) // null) as $p
       | (first(($p._injects // [])[] | select(.name == $a[3])) // null) as $c
       | if $c == null then bad("Error from server (BadRequest): container \($a[3]) is not valid") else ok($c.logs) end)
    elif $s == "get lease mh-pool -o json" then lease_get
    elif ($a[0] == "create" or $a[0] == "replace") and $a[1] == "-f" and ($a[3:] == [] or $a[3:] == ["-o", "json"]) then
      (if $a[0] == "create" and $body.kind == "Deployment" then create_deploy($body)
       elif $a[0] == "create" and $body.kind == "Lease" then lease_create($body)
       elif $a[0] == "replace" and $body.kind == "Lease" then lease_replace($body)
       else {state: ., rc: 97, out: "", err: "stub: unsupported \($a[0]) of \($body.kind)", violation: "unknown"} end)
    elif ($a | length) == 5 and $a[0] == "delete" and $a[1] == "deployment" and $a[3:] == ["--cascade=foreground", "--wait=false"] then
      delete_deploy($a[2])
    else {state: ., rc: 97, out: "", err: "stub kubectl: unrecognised call: \($s)", violation: "unknown"} end;
.tick += 1 | advance
| $argv as $a
| ($a | join(" ")) as $s
| (.knobs.fail // null) as $f
| (if $f != null and ($s | startswith($f.match)) then .failcount += 1 else . end)
| if $f != null and ($s | startswith($f.match)) and .failcount >= $f.after then
    bad("Error from server (InternalError): an error on the server (\"stub\") has prevented the request from succeeding")
  else dispatch($a) end
| (.state | tojson), (.rc | tostring), (.violation // ""), (.out | @base64), (.err | @base64)
JQ

cat >"${STUB_BIN}/kubectl" <<'STUB'
#!/usr/bin/env bash
# Stateful kubectl stub for mh.test.sh. Never permissive: see the suite header.
S="${STUB_STATE:?STUB_STATE unset}"
line="$*"; printf '%s\n' "${line//$'\n'/\\n}" >>"$S/calls.log"
for a in "$@"; do
  case "$a" in
    --custom=*) cat "${a#--custom=}" >>"$S/debug-custom.json"; echo >>"$S/debug-custom.json" ;;
  esac
  case "$a" in
    --force|--force=*|--grace-period|--grace-period=*|--now)
      printf '%s\n' "$*" >>"$S/violation.force"; echo "stub kubectl: forbidden flag ${a}" >&2; exit 96 ;;
  esac
done
ctx=""; rto=""; args=()
while (( $# )); do
  case "$1" in
    --context) ctx="${2:-}"; shift 2 ;;
    --request-timeout=*) rto="$1"; shift ;;
    -n) shift 2 ;;
    *) args+=("$1"); shift ;;
  esac
done
if [[ -z "$ctx" || "$ctx" != "${STUB_EXPECT_CONTEXT:-}" ]]; then
  printf '%s\n' "${args[*]}" >>"$S/violation.context"; echo "stub kubectl: call without --context ${STUB_EXPECT_CONTEXT:-}" >&2; exit 98
fi
if [[ -z "$rto" ]]; then
  printf '%s\n' "${args[*]}" >>"$S/violation.request-timeout"; echo "stub kubectl: call without --request-timeout" >&2; exit 95
fi
# A slow apiserver: "<argv-prefix>|<seconds>" in $S/slow delays matching calls.
if [[ -f "$S/slow" ]]; then
  IFS='|' read -r slow_match slow_secs <"$S/slow"
  [[ "${args[*]}" == "${slow_match}"* ]] && sleep "$slow_secs"
fi
# A pod delete in ANY resource position after `delete` (po|pod|pods, bare or /<name>; after
# flags; in a multi-resource delete). Applies to every verb's calls; a Deployment delete passes.
if [[ "${args[0]:-}" == "delete" ]]; then
  for a in "${args[@]:1}"; do
    if [[ "$a" =~ ^(po|pod|pods)(/.*)?$ || "$a" =~ ,(po|pod|pods)(,|$) || "$a" =~ ^(po|pod|pods), ]]; then
      printf '%s\n' "${args[*]}" >>"$S/violation.force"; echo "stub kubectl: pod delete forbidden" >&2; exit 96
    fi
  done
fi
body="null"
for (( i = 0; i < ${#args[@]}; i++ )); do
  [[ "${args[$i]}" == "-f" ]] && body="$(cat "${args[$((i + 1))]}")"
done
# argv as a JSON array, built in bash (jq 1.6's --args would parse a later `-o` as its own option).
argv="["; sep=""
for a in "${args[@]}"; do a="${a//\\/\\\\}"; a="${a//\"/\\\"}"; a="${a//$'\n'/\\n}"; a="${a//$'\t'/\\t}"; argv+="${sep}\"${a}\""; sep=","; done
argv+="]"
mapfile -t res < <(jq -r --argjson body "$body" --argjson argv "$argv" --argjson now "$(date -u +%s)" \
  --arg contexts "${STUB_CONTEXTS:-${STUB_EXPECT_CONTEXT}}" -f "$STUB_JQ" "$S/state.json")
if (( ${#res[@]} != 5 )); then echo "stub kubectl: internal stub error" >&2; touch "$S/violation.stub"; exit 99; fi
printf '%s\n' "${res[0]}" >"$S/state.json.new" && mv "$S/state.json.new" "$S/state.json"
[[ -n "${res[2]}" ]] && printf '%s\n' "${args[*]}" >>"$S/violation.${res[2]}"
printf '%s' "${res[3]}" | base64 -d
printf '%s' "${res[4]}" | base64 -d >&2
[[ -n "${res[4]}" ]] && echo >&2
exit "${res[1]}"
STUB
chmod +x "${STUB_BIN}/kubectl"
export STUB_JQ

# --- Fixtures -------------------------------------------------------------------------

REV="rev2"        # the template revision
OLD="rev1"        # a stale revision

# template_deployment <variant>: the rendered MH Deployment. Variants pick which probe is the
# max (the derived `up` bound must be the MAX over probes, not the first one).
#   readiness-max: startup 1, readiness 2, liveness 1    startup-max: startup 2, readiness 1, liveness 1, minReadySeconds 1
#   no-app-label:  pod labels lack the selector's app label    no-probes: no probes at all (window 0)
template_deployment() {
  local v="${1:-readiness-max}" s=1 r=2
  [[ "$v" == "startup-max" ]] && { s=2; r=1; }
  jq -cn --argjson s "$s" --argjson r "$r" --arg v "$v" '
    def probe($ft): {httpGet: {path: "/ready", port: 8083}, initialDelaySeconds: 0, periodSeconds: 1, failureThreshold: $ft, timeoutSeconds: 0};
    {apiVersion: "apps/v1", kind: "Deployment",
     metadata: {name: "mh-template-rendered", labels: {app: "mh-service", component: "media", "app.kubernetes.io/part-of": "dark-tower"}},
     spec: {replicas: 1, minReadySeconds: (if $v == "startup-max" then 1 else 0 end), selector: {matchLabels: {app: "mh-service", component: "media"}},
       template: {metadata: {labels: (if $v == "no-app-label" then {component: "media"} else {app: "mh-service", component: "media", "app.kubernetes.io/version": "test"} end)},
         spec: {terminationGracePeriodSeconds: 1,
           containers: [{name: "mh-service", image: "localhost/mh-service:sha-test",
             ports: [{name: "health", containerPort: 8083}, {name: "grpc", containerPort: 50053},
                     {name: "webtransport", containerPort: 4434, protocol: "UDP"}],
             env: [{name: "MH_REGION", valueFrom: {configMapKeyRef: {name: "mh-service-config-abc", key: "MH_REGION"}}},
                   {name: "MH_WEBTRANSPORT_ADVERTISE_ADDRESS", valueFrom: {configMapKeyRef: {name: "REPLACED-PER-SLOT", key: "MH_WEBTRANSPORT_ADVERTISE_ADDRESS"}}}],
             startupProbe: probe($s), readinessProbe: probe($r), livenessProbe: probe(1)}
             | if $v == "no-probes" then del(.startupProbe, .readinessProbe, .livenessProbe) else . end]}}}}'
}

# new_case <label> [slots-csv (default 0..5)] [template-variant]: fresh state; slot ConfigMaps
# are written in a SCRAMBLED order (the stub returns them as stored) to prove mh.sh sorts.
new_case() {
  CASE_LABEL="$1"
  local slots="${2:-0,1,2,3,4,5}" variant="${3:-readiness-max}"
  CASE="${WORK}/case-${GROUP_ID}-$(( ++CASE_N ))"
  mkdir -p "$CASE/tmp"
  : >"$CASE/calls.log"
  jq -n --arg slots "$slots" --arg rev "$REV" --argjson dep "$(template_deployment "$variant")" '
    ($slots | split(",") | map(tonumber) | reverse) as $ks
    | {tick: 0, failcount: 0, created: [], polled: [], deploys: [], pods: [], lease: null,
       knobs: {ready_after: 1, sigterm_after: 0, gone_after: 2, race: [], create_err: [], lease_steal: false, lease_conflict: false},
       template: {apiVersion: "v1", kind: "ConfigMap", metadata: {name: "mh-template"},
                  data: {"deployment.json": ($dep | tojson), revision: $rev, baseline: "2"}},
       slotcms: [$ks[] | {apiVersion: "v1", kind: "ConfigMap",
                 metadata: {name: "mh-slot-\(.)-config-h4sh", labels: {"dark-tower.io/mh-slot": (tostring)}},
                 data: {MH_WEBTRANSPORT_ADVERTISE_ADDRESS: "https://127.0.0.1:\(30420 + .)", MH_HOST_PORT: "\(30420 + .)"}}]}' \
    >"$CASE/state.json"
}
CASE_N=0
GROUP_ID="main"

state_edit() { jq "$@" "$CASE/state.json" >"$CASE/state.json.new" && mv "$CASE/state.json.new" "$CASE/state.json"; }
state_q() { jq "$@" "$CASE/state.json"; }
knob() { state_edit --argjson v "$2" ".knobs.$1 = \$v"; }

# seed_mh <slot> <owner> <rev> <state> [replaces]: an existing MH. States:
#   serving | starting | draining (deleting, pod within grace 10s, gone after 10 ticks)
#   draining-forever (never gone) | stuck (pod past grace) | finalizing (Deployment deleting, no pod)
seed_mh() {
  local k="$1" owner="$2" rev="$3" st="$4" rep="${5:-}"
  state_edit --argjson k "$k" --arg o "$owner" --arg r "$rev" --arg st "$st" --arg rep "$rep" --argjson now "$(date -u +%s)" '
    ({"dark-tower.io/mh-slot": ($k | tostring), "dark-tower.io/mh-revision": $r, "dark-tower.io/mh-owner": $o,
      app: "mh-service", component: "media"}) as $lab
    | ("mh-s\($k)") as $n
    | ($st == "draining" or $st == "draining-forever" or $st == "stuck" or $st == "finalizing") as $del
    | .deploys += [{apiVersion: "apps/v1", kind: "Deployment",
        _hold: ($st == "finalizing"),
        metadata: ({name: $n, labels: $lab, annotations: (if $rep == "" then {} else {"dark-tower.io/mh-replaces": $rep} end)}
                   + (if $del then {deletionTimestamp: (($now - 5) | todate)} else {} end)),
        spec: {template: {metadata: {labels: $lab}, spec: {terminationGracePeriodSeconds: 1, containers: [{name: "mh-service"}]}}}}]
    | if $st == "finalizing" then . else
      .pods += [{metadata: ({name: "\($n)-seed", uid: "uid-seed-\($k)", labels: $lab, creationTimestamp: (($now - 60) | todate)}
                  + (if $st == "stuck" then {deletionTimestamp: (($now - 100) | todate)}
                     elif $del then {deletionTimestamp: (($now + 300) | todate)} else {} end)),
                 spec: {terminationGracePeriodSeconds: (if $st == "draining" then 10 else 1 end),
                        shareProcessNamespace: true,
                        containers: [{name: "mh-service", livenessProbe: {httpGet: {path: "/health", port: 8083},
                          initialDelaySeconds: 10, periodSeconds: 10, timeoutSeconds: 5, successThreshold: 1, failureThreshold: 3}}]},
                 status: {podIP: "10.244.1.\($k)", containerStatuses: [{name: "mh-service", restartCount: 2}]},
                 _ready_at: (if $st == "serving" then 0 else -1 end),
                 _gone_at: (if $st == "draining" then .tick + 10 else -1 end)}] end'
}

# seed_lease <holder> <renew-offset-s> [duration]: an existing mh-pool Lease.
seed_lease() {
  state_edit --arg h "$1" --argjson off "$2" --argjson d "${3:-30}" --argjson now "$(date -u +%s)" '
    .lease = {apiVersion: "coordination.k8s.io/v1", kind: "Lease", metadata: {name: "mh-pool", resourceVersion: "7"},
              spec: ({leaseDurationSeconds: $d, renewTime: ((($now + $off) | todate) | sub("Z$"; ".000000Z"))}
                     + (if $h == "" then {} else {holderIdentity: $h} end))}'
}

calls() { cat "$CASE/calls.log"; }
# assert_lease_released_before_wait <mutation-pattern> <poll-pattern>: exactly one Lease release
# lies between the mutation and the first matching poll after it (the wait runs outside the Lease).
assert_lease_released_before_wait() {
  local mut poll rel
  mut="$(grep -nE -e "$1" "$CASE/calls.log" | tail -n 1 | cut -d: -f1)"
  poll="$(tail -n +"$(( ${mut:-1} + 1 ))" "$CASE/calls.log" | grep -nE -e "$2" | head -n 1 | cut -d: -f1)"
  poll=$(( ${mut:-0} + ${poll:-999999} ))
  rel="$(head -n "$poll" "$CASE/calls.log" | tail -n +"${mut:-1}" | grep -c 'replace -f .*lease-rel.json')"
  assert_rc "${CASE_LABEL}: Lease released after '$1' and before the wait ('$2')" 1 "$rel"
}
# The create/delete sequence a case issued (Lease calls excluded), e.g. "create -f …;delete deployment mh-s0;".
upgrade_order() { grep -oE '(create -f [^ ]*create.json|delete deployment mh-s[0-9]+)' "$CASE/calls.log" | tr '\n' ';'; }
count_calls() { grep -cE -e "$1" "$CASE/calls.log"; }

# run_mh [VAR=value...] <args...>: run mh.sh against the case state; sets OUT, ERR, RC, REASON,
# DETAIL, LAST. Asserts the stream contract on EVERY run. The margins default to a GENEROUS 30s so
# a positive arm never races the wall clock under load; only a case that asserts an exact derived
# bound passes MH_UP_MARGIN_S=0 / MH_DRAIN_MARGIN_S=0 (and those arms never transition).
MH_CLUSTER="mhtest"
run_mh() {
  local env_extra=()
  while [[ "${1:-}" == *=* && "${1:-}" != --* ]]; do env_extra+=("$1"); shift; done
  local json=0 a
  for a in "$@"; do [[ "$a" == "--json" ]] && json=1; done
  env -u DT_PORT_MAP -u DT_HOST_GATEWAY_IP \
    PATH="${STUB_BIN}:${PATH}" STUB_STATE="$CASE" STUB_EXPECT_CONTEXT="kind-${MH_CLUSTER}" \
    DT_CLUSTER_NAME="$MH_CLUSTER" KUBECONFIG="$WORK/kubeconfig" TMPDIR="$CASE/tmp" \
    MH_POLL_INTERVAL_S=0.01 MH_UP_MARGIN_S=30 MH_DRAIN_MARGIN_S=30 "${env_extra[@]}" \
    "$BASH_BIN" "$MH" "$@" >"$CASE/out" 2>"$CASE/err"
  RC=$?
  record_run "$json"
}

# record_run <json>: the per-run stream assertions + the emitted-token collector.
record_run() {
  local json="$1" n
  OUT="$(cat "$CASE/out")"; ERR="$(cat "$CASE/err")"
  LAST="$(tail -n 1 "$CASE/err")"
  n="$(grep -c '^MH_RESULT ' "$CASE/err")"
  assert_rc "${CASE_LABEL}: exactly one MH_RESULT line" 1 "$n"
  assert_status "${CASE_LABEL}: MH_RESULT is the last stderr line" "MH_RESULT STATUS=" "${LAST:0:18}"
  REASON="$(sed -n 's/^MH_RESULT STATUS=[A-Z]* REASON=\([a-z-]*\).*/\1/p' <<<"$LAST")"
  DETAIL="$(sed -n 's/.* DETAIL=//p' <<<"$LAST")"
  printf '%s %s\n' "${REASON:-<none>}" "$RC" >>"$EMITTED"
  if (( json )); then
    local objs okv
    objs="$(jq -s 'length' <<<"$OUT" 2>/dev/null)"
    assert_rc "${CASE_LABEL}: --json stdout is exactly one JSON value" 1 "${objs:-0}"
    okv="$(jq -r '.ok' <<<"$OUT" 2>/dev/null)"
    if [[ "$LAST" == "MH_RESULT STATUS=OK"* ]]; then
      assert_rc "${CASE_LABEL}: ok==true on STATUS=OK" true "$okv"
    else
      assert_rc "${CASE_LABEL}: ok==false on STATUS=FAIL" false "$okv"
      assert_rc "${CASE_LABEL}: failure JSON reason == REASON" "$REASON" "$(jq -r '.reason' <<<"$OUT" 2>/dev/null)"
    fi
  else
    assert_rc "${CASE_LABEL}: stdout empty without --json" 0 "${#OUT}"
  fi
}

expect() { # expect <reason> <rc>
  assert_rc "${CASE_LABEL}: rc" "$2" "$RC"
  assert_rc "${CASE_LABEL}: REASON" "$1" "$REASON"
}
jout() { jq -r "$@" <<<"$OUT"; }

# --- list -------------------------------------------------------------------------------

GOLDEN_TOP='["baseline","ok","schema_version","slots","template_revision"]'
GOLDEN_SLOT='["address","deployment","host_port","owner","pod","pod_ip","pod_uid","restart_count","revision","slot","stale","state","terminating_past_grace"]'

group_list() {
new_case "list empty pool"
run_mh list --json
expect ok 0
assert_rc "$CASE_LABEL: top-level key set (exact)" "$GOLDEN_TOP" "$(jout -c 'keys')"
assert_rc "$CASE_LABEL: per-slot key set (exact, every slot)" "[$GOLDEN_SLOT]" "$(jout -c '[.slots[] | keys] | unique')"
assert_rc "$CASE_LABEL: every slot listed, sorted" "[0,1,2,3,4,5]" "$(jout -c '[.slots[].slot]')"
assert_rc "$CASE_LABEL: schema_version 1" 1 "$(jout '.schema_version')"
assert_rc "$CASE_LABEL: template_revision + baseline" "$REV 2" "$(jout '"\(.template_revision) \(.baseline)"')"
assert_rc "$CASE_LABEL: free slot nulls" '[null,null,null,null,null,null,null,"free",false,false]' \
  "$(jout -c '.slots[0] | [.pod, .pod_ip, .pod_uid, .deployment, .revision, .owner, .restart_count, .state, .stale, .terminating_past_grace]')"
assert_rc "$CASE_LABEL: address from the slot ConfigMap" "https://127.0.0.1:30423 30423" "$(jout '.slots[3] | "\(.address) \(.host_port)"')"
assert_rc "$CASE_LABEL: list takes no Lease" 0 "$(count_calls lease)"

new_case "list every state"
seed_mh 0 baseline "$OLD" serving
seed_mh 1 baseline "$REV" starting
seed_mh 2 manual "$REV" draining-forever
seed_mh 3 manual "$REV" stuck
seed_mh 4 manual "$REV" finalizing
seed_lease "other@host:1" 0
run_mh list --json
expect ok 0
assert_rc "$CASE_LABEL: states" '["serving","starting","draining","terminating","terminating","free"]' "$(jout -c '[.slots[].state]')"
assert_rc "$CASE_LABEL: terminating_past_grace" '[false,false,false,true,false,false]' "$(jout -c '[.slots[].terminating_past_grace]')"
assert_rc "$CASE_LABEL: stale" '[true,false,false,false,false,false]' "$(jout -c '[.slots[].stale]')"
assert_rc "$CASE_LABEL: populated pod fields" '["10.244.1.0","uid-seed-0","mh-s0-seed","mh-s0",2,"baseline"]' \
  "$(jout -c '.slots[0] | [.pod_ip, .pod_uid, .pod, .deployment, .restart_count, .owner]')"
assert_status "$CASE_LABEL: busy Lease does not block list" "STATUS=OK" "$LAST"

}

group_nextfit() {
# --- up: next-fit -------------------------------------------------------------------------

nextfit_case() { # nextfit_case <label> <slots-csv> <occupied k[:state]...> -- <expected>
  local label="$1" slots="$2" occ; shift 2
  new_case "next-fit ${label}" "$slots"
  while [[ "$1" != "--" ]]; do
    occ="$1"; [[ "$occ" == *:* ]] || occ="${occ}:serving"
    seed_mh "${occ%%:*}" baseline "$REV" "${occ#*:}"; shift
  done
  run_mh up --json
  expect ok 0
  assert_rc "$CASE_LABEL: chosen slot" "$2" "$(jout '.slot')"
  assert_rc "$CASE_LABEL: created" true "$(jout '.created')"
}
nextfit_case "{0,1}->2" 0,1,2,3,4,5 0 1 -- 2
nextfit_case "{1,3}->4 (not lowest-free 0)" 0,1,2,3,4,5 1 3 -- 4
nextfit_case "{5}->0 (wrap)" 0,1,2,3,4,5 5 -- 0
nextfit_case "{0,5}->1 (wrap)" 0,1,2,3,4,5 0 5 -- 1
nextfit_case "empty->lowest" 0,1,2,3,4,5 -- 0
nextfit_case "K=4 discovered" 0,1,2,3 3 -- 0
nextfit_case "non-contiguous {2}->5" 0,2,5 2 -- 5
nextfit_case "non-contiguous {5}->0" 0,2,5 5 -- 0
# A non-serving MH still occupies its slot AND can be the highest occupied one.
nextfit_case "{0 serving, 3 draining}->4" 0,1,2,3,4,5 0 3:draining-forever -- 4
nextfit_case "{4 stuck}->5" 0,1,2,3,4,5 4:stuck -- 5
nextfit_case "{4 finalizing}->5" 0,1,2,3,4,5 4:finalizing -- 5
nextfit_case "{2 starting}->3" 0,1,2,3,4,5 2:starting -- 3

}

group_up_object() {
new_case "up created object"
seed_mh 0 baseline "$REV" serving
run_mh up --owner e2e-0123456789abcdef --json
expect ok 0
CREATED="$(state_q -c '.created[0]')"
cq() { jq -r "$@" <<<"$CREATED"; }
assert_rc "$CASE_LABEL: name + replicas" "mh-s1 1" "$(cq '"\(.metadata.name) \(.spec.replicas)"')"
assert_rc "$CASE_LABEL: template pod labels survive (app, component)" "mh-service media" \
  "$(cq '.spec.template.metadata.labels | "\(.app) \(.component)"')"
# Every template label survives, derived from the fixture's own template (not retyped).
TPL_DEP="$(state_q -c '.template.data["deployment.json"] | fromjson')"
assert_rc "$CASE_LABEL: control: the template has pod labels to keep" true \
  "$(jq -r '(.spec.template.metadata.labels | length) > 2 and (.metadata.labels | length) > 2' <<<"$TPL_DEP")"
assert_rc "$CASE_LABEL: every template pod label survives" 0 \
  "$(jq -n --argjson t "$TPL_DEP" --argjson c "$CREATED" '($t.spec.template.metadata.labels | to_entries) - ($c.spec.template.metadata.labels | to_entries) | length')"
assert_rc "$CASE_LABEL: every template Deployment label survives" 0 \
  "$(jq -n --argjson t "$TPL_DEP" --argjson c "$CREATED" '($t.metadata.labels | to_entries) - ($c.metadata.labels | to_entries) | length')"
assert_rc "$CASE_LABEL: three pool labels on the pod template" "1 $REV e2e-0123456789abcdef" \
  "$(cq '.spec.template.metadata.labels | "\(.["dark-tower.io/mh-slot"]) \(.["dark-tower.io/mh-revision"]) \(.["dark-tower.io/mh-owner"])"')"
assert_rc "$CASE_LABEL: three pool labels on the Deployment" "1 $REV e2e-0123456789abcdef mh-service" \
  "$(cq '.metadata.labels | "\(.["dark-tower.io/mh-slot"]) \(.["dark-tower.io/mh-revision"]) \(.["dark-tower.io/mh-owner"]) \(.app)"')"
assert_rc "$CASE_LABEL: selector keeps the template's and gains only mh-slot" '{"app":"mh-service","component":"media","dark-tower.io/mh-slot":"1"}' \
  "$(cq -c '.spec.selector.matchLabels')"
assert_rc "$CASE_LABEL: webtransport hostPort = the slot's MH_HOST_PORT" 30421 \
  "$(cq '.spec.template.spec.containers[0].ports[] | select(.name == "webtransport") | .hostPort')"
assert_rc "$CASE_LABEL: other ports get no hostPort" null \
  "$(cq '[.spec.template.spec.containers[0].ports[] | select(.name != "webtransport") | .hostPort] | unique | .[0]')"
assert_rc "$CASE_LABEL: advertise-address ref = slot ConfigMap" "mh-slot-1-config-h4sh" \
  "$(cq '.spec.template.spec.containers[0].env[] | select(.name == "MH_WEBTRANSPORT_ADVERTISE_ADDRESS") | .valueFrom.configMapKeyRef.name')"
assert_rc "$CASE_LABEL: other configMapKeyRefs untouched" "mh-service-config-abc" \
  "$(cq '.spec.template.spec.containers[0].env[] | select(.name == "MH_REGION") | .valueFrom.configMapKeyRef.name')"
assert_rc "$CASE_LABEL: no replaces annotation outside upgrade" null "$(cq '.metadata.annotations["dark-tower.io/mh-replaces"]')"
assert_rc "$CASE_LABEL: output address from the slot ConfigMap" "https://127.0.0.1:30421" "$(jout '.address')"
assert_status "$CASE_LABEL: output pod_ip populated from status.podIP" "10.244.0." "$(jout '.pod_ip')"
assert_rc "$CASE_LABEL: output per-slot keys + created + ok" "$(jq -c '. + ["created","ok"] | sort' <<<"$GOLDEN_SLOT")" "$(jout -c 'keys')"
# Lease ordering: released BEFORE the Available poll (the wait runs outside the Lease).
rel_line="$(grep -n 'replace -f' "$CASE/calls.log" | tail -n 1 | cut -d: -f1)"
poll_line="$(grep -n 'get deployment mh-s1 ' "$CASE/calls.log" | head -n 1 | cut -d: -f1)"
assert_rc "$CASE_LABEL: Lease released before the Ready wait" true "$( (( ${rel_line:-999} < ${poll_line:-0} )) && echo true || echo false)"
assert_rc "$CASE_LABEL: Lease holder cleared after success" null "$(state_q -r '.lease.spec.holderIdentity')"
assert_lease_released_before_wait 'create -f .*create.json' "get deployment mh-s1 "

}

group_up_capacity() {
# --- up: capacity, races, idempotence --------------------------------------------------------

new_case "AlreadyExists race moves to the next slot"
seed_mh 0 baseline "$REV" serving; seed_mh 1 baseline "$REV" serving
knob race '["2"]'
run_mh up --json
expect ok 0
assert_rc "$CASE_LABEL: landed on 3" 3 "$(jout '.slot')"
assert_rc "$CASE_LABEL: tried 2 then 3" "mh-s2 mh-s3" "$(state_q -r '[.deploys[].metadata.name] | .[2:] | join(" ")')"
assert_status "$CASE_LABEL: race is reported" "lost a race" "$ERR"

new_case "AlreadyExists on every candidate"
seed_mh 0 baseline "$REV" serving
knob race '["1","2","3","4","5"]'
run_mh up --json
expect mh-no-free-slot 3

new_case "full pool"
for k in 0 1 2 3 4 5; do seed_mh "$k" baseline "$REV" serving; done
run_mh up --json
expect mh-no-free-slot 3
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"
assert_rc "$CASE_LABEL: Lease holder cleared after a refusal" null "$(state_q -r '.lease.spec.holderIdentity')"

new_case "create fails with foreign wording"
seed_mh 0 baseline "$REV" serving
knob create_err '["1"]'
run_mh up --json
expect kubectl-failed 1
assert_rc "$CASE_LABEL: no next-slot attempt" 1 "$(count_calls 'create -f .*create.json')"
assert_status "$CASE_LABEL: DETAIL names the op" "op=create-deployment" "$DETAIL"

}

group_up_idempotent() {
new_case "idempotent up --slot same owner"
seed_mh 2 e2e-aaaa "$REV" serving
run_mh up --slot 2 --owner e2e-aaaa --json
expect ok 0
assert_rc "$CASE_LABEL: created false" false "$(jout '.created')"
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"

new_case "idempotent up --slot same owner, still starting -> waits, created:false"
seed_mh 2 e2e-aaaa "$REV" starting
state_edit '.pods |= map(._ready_at = 12)'
run_mh up --slot 2 --owner e2e-aaaa --json
expect ok 0
assert_rc "$CASE_LABEL: created false" false "$(jout '.created')"
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"
assert_rc "$CASE_LABEL: waited for Available (polled)" true "$( (( $(count_calls 'get deployment mh-s2 ') >= 1 )) && echo true || echo false)"
assert_rc "$CASE_LABEL: returned serving" serving "$(jout -r '.state')"

new_case "up --slot occupied by another owner"
seed_mh 2 baseline "$REV" serving
run_mh up --slot 2 --owner e2e-aaaa --json
expect mh-no-free-slot 3
assert_status "$CASE_LABEL: slot-occupied" "slot-occupied" "$DETAIL"
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"

new_case "up --slot occupied by a stale same-owner MH"
seed_mh 2 e2e-aaaa "$OLD" serving
run_mh up --slot 2 --owner e2e-aaaa --json
expect mh-no-free-slot 3

new_case "up --slot free"
run_mh up --slot 4 --json
expect ok 0
assert_rc "$CASE_LABEL: on the requested slot, default owner manual" "4 manual" "$(jout '"\(.slot) \(.owner)"')"

}

group_up_derived() {
# --- up: derived bound + not-available-timeout ---------------------------------------------

for variant in readiness-max startup-max; do
  new_case "up derived timeout (${variant})" 0,1,2,3,4,5 "$variant"
  knob ready_after -1
  want=2; [[ "$variant" == "startup-max" ]] && want=3
  run_mh MH_UP_MARGIN_S=0 up --json
  expect not-available-timeout 1
  assert_status "$CASE_LABEL: bound = max probe window 2s (+ minReadySeconds 1 for startup-max) + margin 0" "timeout=${want}s" "$DETAIL"
  assert_status "$CASE_LABEL: DETAIL carries the drain command" "mh.sh drain 0 --owner manual" "$DETAIL"
  assert_rc "$CASE_LABEL: pod left in place (no delete)" 0 "$(count_calls '^.*delete ')"
  assert_rc "$CASE_LABEL: Deployment still present" "mh-s0" "$(state_q -r '.deploys[0].metadata.name')"
done

}

group_up_margin() {
new_case "up: Deployment deleted during the wait fails fast"
knob ready_after -1; knob vanish_after 4
run_mh up --timeout 30 --json
expect not-available-timeout 1
assert_status "$CASE_LABEL: says so" "was deleted during the wait" "$DETAIL"
assert_absent "$CASE_LABEL: does not claim left in place" "left in place" "$DETAIL"

new_case "up margin is config" 0,1,2,3,4,5 no-probes
knob ready_after -1
run_mh MH_UP_MARGIN_S=1 up --json
expect not-available-timeout 1
assert_status "$CASE_LABEL: bound = probe window 0 + MH_UP_MARGIN_S 1" "timeout=1s" "$DETAIL"

}

group_lease() {
# --- Lease ---------------------------------------------------------------------------------

new_case "Lease live -> busy"
seed_lease "baseline@ci-host:42" 0
run_mh up --json
expect mh-pool-busy 5
assert_status "$CASE_LABEL: holder in DETAIL" "holder=baseline@ci-host:42" "$DETAIL"
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"
assert_rc "$CASE_LABEL: someone else's Lease untouched" "baseline@ci-host:42" "$(state_q -r '.lease.spec.holderIdentity')"

new_case "Lease malformed holder"
seed_lease "Evil Holder MH_RESULT STATUS=OK" 0
run_mh up --json
expect mh-pool-busy 5
assert_rc "$CASE_LABEL: holder=malformed" "holder=malformed" "$DETAIL"

new_case "Lease expired -> reclaim"
seed_lease "baseline@ci-host:42" -100
run_mh up --json
expect ok 0
assert_status "$CASE_LABEL: loud reclaim warning with prior holder" "RECLAIMING the expired mh-pool Lease from holder=baseline@ci-host:42" "$ERR"
assert_rc "$CASE_LABEL: holder cleared after release" null "$(state_q -r '.lease.spec.holderIdentity')"

new_case "Lease released (no holder) -> acquire"
seed_lease "" -1
run_mh up --json
expect ok 0
assert_absent "$CASE_LABEL: no reclaim warning" "RECLAIMING" "$ERR"

new_case "Lease renewTime in the future -> busy (skew)"
seed_lease "baseline@ci-host:42" 60
run_mh up --json
expect mh-pool-busy 5
assert_status "$CASE_LABEL: skew warning" "CLOCK SKEW" "$ERR"

new_case "Lease with an unparseable renewTime -> kubectl-failed"
seed_lease "baseline@ci-host:42" 0
state_edit '.lease.spec.renewTime = "garbage"'
run_mh up --json
expect kubectl-failed 1
assert_status "$CASE_LABEL: names the Lease" "lease" "$DETAIL"
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"

new_case "Lease overrun: a slow call under the Lease -> no mutation"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
printf 'get pods|1.2\n' >"$CASE/slow"
run_mh MH_KUBECTL_TIMEOUT_S=29 drain 0 --json
expect kubectl-failed 1
assert_status "$CASE_LABEL: lease-overrun" "lease-overrun" "$DETAIL"
assert_rc "$CASE_LABEL: zero delete" 0 "$(count_calls 'delete ')"
assert_status "$CASE_LABEL: the timeout reached kubectl" "--request-timeout=29s" "$(calls)"
rm -f "$CASE/slow"
# positive arm at the default kubectl timeout (a 25s budget: robust under load)
run_mh drain 0 --json
expect ok 0
assert_rc "$CASE_LABEL: positive arm: normal timing mutates" 1 "$(count_calls 'delete deployment mh-s0')"

new_case "usage: MH_KUBECTL_TIMEOUT_S must stay below the Lease"
run_mh MH_KUBECTL_TIMEOUT_S=30 list
expect usage 2

new_case "Lease reclaim Conflict -> busy"
seed_lease "baseline@ci-host:42" -100
knob lease_conflict true
run_mh up --json
expect mh-pool-busy 5
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"

new_case "Lease taken over while held -> not cleared"
seed_mh 0 baseline "$REV" serving
knob lease_steal true
run_mh up --json
expect ok 0
assert_status "$CASE_LABEL: release warns" "no longer ours" "$ERR"
assert_rc "$CASE_LABEL: the new holder's Lease kept" "thief@stub:1" "$(state_q -r '.lease.spec.holderIdentity')"
assert_rc "$CASE_LABEL: no Lease delete" 0 "$(count_calls 'delete lease')"

}

group_drain_basic() {
# --- drain ---------------------------------------------------------------------------------

new_case "drain empty slot"
run_mh drain 3 --json
expect ok 0
assert_rc "$CASE_LABEL: drained false" false "$(jout '.drained')"
assert_rc "$CASE_LABEL: no delete" 0 "$(count_calls 'delete ')"

new_case "drain serving (two serving)"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
run_mh drain 0 --json
expect ok 0
assert_rc "$CASE_LABEL: drained true, pre-drain entry" "true mh-s0-seed" "$(jout '"\(.drained) \(.pod)"')"
assert_rc "$CASE_LABEL: one foreground Deployment delete" 1 "$(count_calls 'delete deployment mh-s0 --cascade=foreground --wait=false')"
assert_rc "$CASE_LABEL: gone" "[]" "$(state_q -c '[.deploys[] | select(.metadata.name == "mh-s0")]')"
assert_rc "$CASE_LABEL: holder cleared" null "$(state_q -r '.lease.spec.holderIdentity')"
assert_rc "$CASE_LABEL: output keys = list entry + drained + ok" "$(jq -c '. + ["drained","ok"] | sort' <<<"$GOLDEN_SLOT")" "$(jout -c 'keys')"
assert_lease_released_before_wait 'delete deployment mh-s0' 'get pods'

new_case "drain joins a draining slot"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" draining
run_mh drain 1 --json
expect ok 0
assert_rc "$CASE_LABEL: joined" "joined" "$DETAIL"
assert_rc "$CASE_LABEL: no second delete" 0 "$(count_calls 'delete ')"

}

group_drain_timeouts() {
new_case "drain timeout derived from the pod's grace"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
knob gone_after -1
run_mh MH_DRAIN_MARGIN_S=0 drain 0 --json
expect drain-timeout 1
assert_status "$CASE_LABEL: bound = pod grace 1s + margin 0" "bound=1s" "$DETAIL"
assert_status "$CASE_LABEL: names the pod" "pod=mh-s0-seed" "$DETAIL"
assert_absent "$CASE_LABEL: no --force hint" "force" "$DETAIL"

new_case "drain margin is config"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
state_edit '.pods |= map(.spec.terminationGracePeriodSeconds = 0)'
knob gone_after -1
run_mh MH_DRAIN_MARGIN_S=1 drain 0 --json
expect drain-timeout 1
assert_status "$CASE_LABEL: bound = pod grace 0 + MH_DRAIN_MARGIN_S 1" "bound=1s" "$DETAIL"

new_case "drain --no-wait returns at SIGTERM (pod never gone)"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
knob gone_after -1; knob sigterm_after 12
run_mh drain 0 --no-wait --json
expect ok 0
assert_rc "$CASE_LABEL: signalled" "signalled" "$DETAIL"
del_line="$(grep -n 'delete deployment' "$CASE/calls.log" | cut -d: -f1)"
pods_after="$(tail -n +"${del_line:-1}" "$CASE/calls.log" | grep -c 'get pods')"
# 12 ticks after the delete: the release takes 2, each poll 2 (deployments + pods), so the pod is
# marked on the 5th pod read; returning at the delete call would show 0-1.
assert_rc "$CASE_LABEL: waited for pod-level SIGTERM (>=5 pod reads after the delete)" true "$( (( pods_after >= 5 )) && echo true || echo false)"
assert_rc "$CASE_LABEL: the pod carries a deletionTimestamp at return" true \
  "$(state_q -r '[.pods[] | select(.metadata.name == "mh-s0-seed") | .metadata.deletionTimestamp != null] | .[0]')"
assert_rc "$CASE_LABEL: pod still present (returned before gone)" 1 "$(state_q '[.pods[] | select(.metadata.name == "mh-s0-seed")] | length')"

new_case "drain --no-wait never signalled -> drain-timeout"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
knob gone_after -1; knob sigterm_after -1
run_mh drain 0 --no-wait --timeout 1 --json
expect drain-timeout 1

}

group_drain_refusals() {
new_case "foreign-owner"
seed_mh 0 baseline "$REV" serving; seed_mh 1 baseline "$REV" serving
run_mh drain 0 --json
expect foreign-owner 4
assert_rc "$CASE_LABEL: failure object keys" '["detail","ok","reason","slot"]' "$(jout -c 'keys')"
assert_rc "$CASE_LABEL: failure slot is a number" 0 "$(jout '.slot')"
assert_rc "$CASE_LABEL: no delete" 0 "$(count_calls 'delete ')"
run_mh drain 0 --owner baseline --json
expect ok 0
new_case "foreign-owner --force"
seed_mh 0 baseline "$REV" serving; seed_mh 1 baseline "$REV" serving
run_mh drain 0 --force --json
expect ok 0
assert_absent "$CASE_LABEL: --force never reaches kubectl" "--force" "$(calls)"

new_case "last-serving-mh"
seed_mh 0 manual "$REV" serving
run_mh drain 0 --json
expect last-serving-mh 4
assert_rc "$CASE_LABEL: no delete" 0 "$(count_calls 'delete ')"
run_mh drain 0 --force --json
expect ok 0
new_case "last-serving-mh: serving + starting still refuses"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" starting
run_mh drain 0 --json
expect last-serving-mh 4
run_mh drain 1 --json
expect ok 0

new_case "stuck-terminating elsewhere, even with --force"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving; seed_mh 3 manual "$REV" stuck
run_mh drain 0 --json
expect stuck-terminating 4
assert_rc "$CASE_LABEL: SLOT is the stuck one" "MH_RESULT STATUS=FAIL REASON=stuck-terminating SLOT=3" "${LAST%% DETAIL=*}"
assert_status "$CASE_LABEL: investigate, don't force" "investigate, don't force" "$DETAIL"
run_mh drain 0 --force --json
expect stuck-terminating 4
assert_rc "$CASE_LABEL: no delete" 0 "$(count_calls 'delete ')"
new_case "within grace is not stuck"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving; seed_mh 3 manual "$REV" draining-forever
run_mh drain 0 --json
expect ok 0

# drain --all
}

group_drain_all() {
new_case "drain --all drains only the owner"
seed_mh 0 baseline "$REV" serving; seed_mh 1 e2e-run1 "$REV" serving; seed_mh 2 e2e-run1 "$REV" starting
run_mh drain --all --owner e2e-run1 --json
expect ok 0
assert_rc "$CASE_LABEL: drained" "[1,2]" "$(jout -c '.drained')"
assert_rc "$CASE_LABEL: output keys" '["drained","ok"]' "$(jout -c 'keys')"
assert_rc "$CASE_LABEL: baseline untouched" 0 "$(count_calls 'delete deployment mh-s0')"

new_case "drain --all: owner holds every serving MH"
seed_mh 0 e2e-run1 "$REV" serving; seed_mh 1 e2e-run1 "$REV" serving; seed_mh 2 e2e-run1 "$REV" serving
run_mh drain --all --owner e2e-run1 --json
expect last-serving-mh 4
assert_rc "$CASE_LABEL: N-1 deletes" 2 "$(count_calls 'delete deployment')"
assert_rc "$CASE_LABEL: the others drained" 2 "$(jout '.drained | length')"
assert_rc "$CASE_LABEL: failure keys + drained" '["detail","drained","ok","reason","slot"]' "$(jout -c 'keys')"
assert_status "$CASE_LABEL: SLOT names the refused one" "SLOT=2" "$LAST"

new_case "drain --all: nothing owned"
seed_mh 0 baseline "$REV" serving
run_mh drain --all --owner e2e-run1 --json
expect ok 0
assert_rc "$CASE_LABEL: drained []" "[]" "$(jout -c '.drained')"

new_case "drain --all: stuck anywhere refuses all"
seed_mh 0 e2e-run1 "$REV" serving; seed_mh 1 baseline "$REV" serving; seed_mh 4 baseline "$REV" stuck
run_mh drain --all --owner e2e-run1 --json
expect stuck-terminating 4
assert_rc "$CASE_LABEL: no delete" 0 "$(count_calls 'delete ')"

}

group_upgrade_a() {
# --- upgrade -------------------------------------------------------------------------------


new_case "upgrade forward, two stale, serial"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$OLD" serving
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: replaced" '[{"old":0,"new":2},{"old":1,"new":3}]' "$(jout -c '.replaced')"
assert_rc "$CASE_LABEL: strictly serial create/drain" \
  "create -f $CASE/tmp/X/create.json;delete deployment mh-s0;create -f $CASE/tmp/X/create.json;delete deployment mh-s1;" \
  "$(upgrade_order | sed -E 's#/tmp/mh-sh\.[^/]*/#/tmp/X/#g')"
assert_rc "$CASE_LABEL: output keys" '["ok","replaced"]' "$(jout -c 'keys')"
assert_rc "$CASE_LABEL: replaced[] keys" '[["new","old"]]' "$(jout -c '[.replaced[] | keys] | unique')"
assert_lease_released_before_wait 'delete deployment mh-s1' 'get pods'
assert_rc "$CASE_LABEL: owner inherited + replaces annotated" "baseline 0" \
  "$(state_q -r '.created[0] | "\(.metadata.labels["dark-tower.io/mh-owner"]) \(.metadata.annotations["dark-tower.io/mh-replaces"])"')"

new_case "upgrade second step fails -> reports the steps done"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$OLD" serving
state_edit '.slotcms |= map(select(.metadata.labels["dark-tower.io/mh-slot"] | IN("0","1","2")))'
knob create_err '["0"]'
run_mh upgrade --json
expect kubectl-failed 1
assert_rc "$CASE_LABEL: failure JSON carries replaced" '[{"old":0,"new":2}]' "$(jout -c '.replaced')"
assert_status "$CASE_LABEL: DETAIL: steps done + resume" 'replaced so far=[{"old":0,"new":2}]; rerun mh.sh upgrade to resume' "$DETAIL"

new_case "upgrade two stale, ONE free slot"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$OLD" serving
state_edit '.slotcms |= map(select(.metadata.labels["dark-tower.io/mh-slot"] | IN("0","1","2")))'
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: second reuses the first old slot" '[{"old":0,"new":2},{"old":1,"new":0}]' "$(jout -c '.replaced')"

new_case "upgrade resume (Ready replacement)"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$REV" serving 0
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: no create" 0 "$(count_calls 'create -f .*create.json')"
assert_rc "$CASE_LABEL: drains only the old" "delete deployment mh-s0;" "$(upgrade_order)"

new_case "upgrade resume on a FULL pool"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$REV" serving 0
state_edit '.slotcms |= map(select(.metadata.labels["dark-tower.io/mh-slot"] | IN("0","1")))'
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: drains the old" "delete deployment mh-s0;" "$(upgrade_order)"

new_case "upgrade resume, old already draining -> joins"
seed_mh 0 baseline "$OLD" draining; seed_mh 1 baseline "$REV" serving 0
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: no create, no delete" "" "$(upgrade_order)"

new_case "upgrade resume, replacement NOT Ready"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$REV" starting 0
run_mh upgrade --timeout 1 --json
expect mh-rollout-failed 1
assert_rc "$CASE_LABEL: no create, no delete (never stacks)" "" "$(upgrade_order)"
assert_status "$CASE_LABEL: names both slots" "old=0 new=1" "$DETAIL"

}

group_upgrade_b() {
new_case "upgrade replacement never Available"
seed_mh 0 baseline "$OLD" serving
knob ready_after -1
run_mh upgrade --timeout 1 --json
expect mh-rollout-failed 1
assert_rc "$CASE_LABEL: old untouched" 0 "$(count_calls 'delete ')"
assert_status "$CASE_LABEL: --timeout honoured" "timeout=1s" "$DETAIL"
assert_status "$CASE_LABEL: reclaim command for the replacement" "mh.sh drain 1 --owner baseline" "$DETAIL"

new_case "upgrade no free slot -> refuse before touching"
seed_mh 0 baseline "$OLD" serving; seed_mh 1 baseline "$OLD" serving
state_edit '.slotcms |= map(select(.metadata.labels["dark-tower.io/mh-slot"] | IN("0","1")))'
run_mh upgrade --json
expect mh-no-free-slot 3
assert_rc "$CASE_LABEL: zero create, zero delete" "" "$(upgrade_order)"

new_case "upgrade: replacement deleted during the wait fails fast"
seed_mh 0 baseline "$OLD" serving
knob ready_after -1; knob vanish_after 4
run_mh upgrade --timeout 30 --json
expect mh-rollout-failed 1
assert_status "$CASE_LABEL: says so" "was deleted during the wait" "$DETAIL"
assert_rc "$CASE_LABEL: old untouched" 0 "$(count_calls 'delete ')"

new_case "upgrade nothing stale"
seed_mh 0 baseline "$REV" serving
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: noop" "noop" "$DETAIL"

new_case "upgrade orphan replaces= annotation"
seed_mh 1 baseline "$REV" serving 0
run_mh upgrade --json
expect ok 0
assert_rc "$CASE_LABEL: noop, no delete" "noop " "$DETAIL $(upgrade_order)"

new_case "upgrade with a stuck pod"
seed_mh 0 baseline "$OLD" serving; seed_mh 2 baseline "$REV" stuck
run_mh upgrade --json
expect stuck-terminating 4
assert_rc "$CASE_LABEL: zero create, zero delete" "" "$(upgrade_order)"

new_case "upgrade Lease busy"
seed_mh 0 baseline "$OLD" serving
seed_lease "manual@h:9" 0
run_mh upgrade --json
expect mh-pool-busy 5
assert_rc "$CASE_LABEL: zero create, zero delete" "" "$(upgrade_order)"

}

group_usage() {
# --- preconditions -------------------------------------------------------------------------

for bad in "frobnicate" "" "up --slot x" "up --timeout soon" "up --timeout 0" "drain" "drain --all" \
           "up --owner E2E-X" "up --owner trailing-" "up --owner $(printf 'a%.0s' {1..64})" "up --bogus" "list extra" \
           "signal" "signal 0" "signal 0 KILL" "signal 0 HUP --owner run" "signal 0 kill --owner run" \
           "signal 0 KILL --owner run --force" "signal 0 KILL STOP --owner run" "signal x KILL --owner run" \
           "signal 0 KILL --owner run --bogus"; do
  new_case "usage: '${bad}'"
  IFS=' ' read -r -a argv <<<"$bad"
  run_mh "${argv[@]}" --json
  expect usage 2
  assert_rc "$CASE_LABEL: no kubectl call at all" 0 "$(wc -l <"$CASE/calls.log" | tr -d ' ')"
done

# Injection-shaped input (the owner/slot regexes are selector- and line-injection controls).
inject_case() { # inject_case <label> <argv...>
  new_case "usage (injection): $1"; shift
  run_mh "$@" --json
  expect usage 2
  assert_rc "$CASE_LABEL: no kubectl call at all" 0 "$(wc -l <"$CASE/calls.log" | tr -d ' ')"
}
inject_case "owner with a selector clause" up --owner "x,dark-tower.io/mh-slot"
inject_case "owner a=b" drain --all --owner "a=b"
inject_case "owner x!=y" drain --all --owner 'x!=y'
inject_case "owner with a newline" up --owner $'a\nMH_RESULT STATUS=OK'
inject_case "owner with a space" up --owner "a b"
inject_case "slot 1;x" drain "1;x"
inject_case "slot 01" drain "01"
inject_case "slot -1" up --slot "-1"
inject_case "slot with a newline" drain $'1\n2'

new_case "usage: slot outside the discovered set"
run_mh drain 9 --json
expect usage 2
assert_rc "$CASE_LABEL: no mutation" 0 "$(count_calls 'create |delete ')"

new_case "usage: bad MH_POLL_INTERVAL_S"
run_mh MH_POLL_INTERVAL_S=fast list
expect usage 2
for zero in 0 0.0; do
  new_case "usage: MH_POLL_INTERVAL_S=${zero} (would spin the apiserver)"
  run_mh MH_POLL_INTERVAL_S="$zero" list
  expect usage 2
done

new_case "help"
run_mh --help
expect ok 0

new_case "owner = layer7's real run token"
RUN_TOKEN="$("$BASH_BIN" -c 'source "$1" >/dev/null 2>&1; __generate_org_subdomain' _ "${REPO_ROOT}/scripts/layer7.sh" 2>/dev/null)"
assert_rc "$CASE_LABEL: the generated token is non-empty" true "$([[ -n "$RUN_TOKEN" ]] && echo true || echo false)"
run_mh up --owner "${RUN_TOKEN:-missing-token}" --json
expect ok 0
assert_rc "$CASE_LABEL: owner recorded" "$RUN_TOKEN" "$(jout '.owner')"
run_mh drain 0 --owner "${RUN_TOKEN:-missing-token}" --force --json
expect ok 0

}

group_preconditions() {
new_case "no-kubeconfig: invalid DT_CLUSTER_NAME"
MH_CLUSTER="Bad_Name" run_mh list --json
expect no-kubeconfig 2
assert_rc "$CASE_LABEL: no kubectl call" 0 "$(wc -l <"$CASE/calls.log" | tr -d ' ')"

new_case "no-kubeconfig: context absent from the kubeconfig"
run_mh STUB_CONTEXTS=kind-other list --json
expect no-kubeconfig 2

new_case "no-kubeconfig: apiserver unreachable"
knob template_err true
run_mh list --json
expect no-kubeconfig 2

new_case "context pinned: DT_CLUSTER_NAME=foo -> kind-foo"
MH_CLUSTER="foo" run_mh list --json
expect ok 0
assert_status "$CASE_LABEL: every call carried --context kind-foo" "--context kind-foo" "$(head -n 1 "$CASE/calls.log")"
assert_rc "$CASE_LABEL: no call without it" "$(wc -l <"$CASE/calls.log" | tr -d ' ')" "$(count_calls '--context kind-foo -n dark-tower')"

new_case "stub rejects a context-less call (control)"
PATH="${STUB_BIN}:${PATH}" STUB_STATE="$CASE" STUB_EXPECT_CONTEXT="kind-x" kubectl get pods >/dev/null 2>&1
assert_rc "$CASE_LABEL: rc 98" 98 "$?"
assert_marker "$CASE_LABEL: violation recorded" "$CASE" "violation.context"
rm -f "$CASE/violation.context"   # the control's own violation, not mh.sh's

new_case "no-template: absent"
state_edit '.template = null'
run_mh list --json
expect no-template 2
new_case "no-template: pod labels lack the selector's app label" 0,1,2,3,4,5 no-app-label
run_mh up --json
expect no-template 2
assert_rc "$CASE_LABEL: no create (never ships an unscraped MH)" 0 "$(count_calls 'create ')"
new_case "no-template: duplicate slot ConfigMap"
state_edit '.slotcms += [.slotcms[0] | .metadata.name = "mh-slot-dup"]'
run_mh list --json
expect no-template 2
assert_status "$CASE_LABEL: DETAIL" "duplicate-slot-configmap" "$DETAIL"
new_case "no-template: non-decimal slot label"
state_edit '.slotcms[0].metadata.labels["dark-tower.io/mh-slot"] = "a"'
run_mh list --json
expect no-template 2
new_case "no-template: no slot ConfigMaps"
state_edit '.slotcms = []'
run_mh list --json
expect no-template 2

# prerequisite-missing: a PATH holding only the coreutils mh.sh needs, without jq / kubectl.
MINI="${WORK}/mini"
mkdir -p "$MINI"
for t in dirname mktemp rm cat head grep sed; do ln -sf "$(command -v "$t")" "$MINI/$t"; done
for missing in jq kubectl; do
  new_case "prerequisite-missing: ${missing}"
  rm -f "$MINI/kubectl" "$MINI/jq"
  [[ "$missing" == "jq" ]] && ln -sf "${STUB_BIN}/kubectl" "$MINI/kubectl"
  [[ "$missing" == "kubectl" ]] && ln -sf "$(command -v jq)" "$MINI/jq"
  env -i PATH="$MINI" HOME="$WORK" STUB_STATE="$CASE" TMPDIR="$CASE/tmp" DT_CLUSTER_NAME=mhtest \
    "$BASH_BIN" "$MH" list >"$CASE/out" 2>"$CASE/err"
  RC=$?
  record_run 0
  expect prerequisite-missing 2
  assert_status "$CASE_LABEL: DETAIL" "missing=${missing}" "$DETAIL"
done

}

group_signal() {
# --- signal (TEST-ONLY injector) ---------------------------------------------------------------
RUN="e2e-0123456789abcdef"
# The pod a seeded slot k runs (seed_mh's naming).
debug_argv() { jq -r '.debugs[-1].argv | join(" ")' "$CASE/state.json"; }

new_case "signal STOP: ok, budget, entry + injector keys"
seed_mh 0 "$RUN" "$REV" serving; seed_mh 1 baseline "$REV" serving
knob inject '{"logs":"DT_INJECT pid=42 state=T"}'
run_mh signal 0 STOP --owner "$RUN" --json
expect ok 0
assert_rc "$CASE_LABEL: key set = list entry + injector keys (exact)" \
  "$(jq -c '. + ["debug_container","liveness_budget_seconds","ok","pid","process_state","restart_budget_seconds","signal"] | sort' <<<"$GOLDEN_SLOT")" "$(jout -c 'keys')"
assert_rc "$CASE_LABEL: pid / state / signal" "42 T STOP" "$(jout '"\(.pid) \(.process_state) \(.signal)"')"
assert_rc "$CASE_LABEL: liveness budget = (3-1)x10 + 5" 25 "$(jout '.liveness_budget_seconds')"
assert_rc "$CASE_LABEL: no restart budget on STOP (key present, null)" "true null" "$(jout '"\(has("restart_budget_seconds")) \(.restart_budget_seconds)"')"
assert_rc "$CASE_LABEL: entry read BEFORE the signal" "uid-seed-0 2 $RUN" "$(jout '"\(.pod_uid) \(.restart_count) \(.owner)"')"
assert_rc "$CASE_LABEL: debug container name" "dt-inject-stop-0" "$(jout '.debug_container')"
assert_status "$CASE_LABEL: SLOT" "MH_RESULT STATUS=OK REASON=ok SLOT=0" "$LAST"
assert_rc "$CASE_LABEL: signal takes NO Lease" 0 "$(count_calls lease)"
assert_rc "$CASE_LABEL: no create/delete" 0 "$(count_calls '(^| )(create|delete|replace) ')"
A="$(debug_argv)"
for want in "debug mh-s0-seed " " --attach=false " " --target=mh-service " " --container=dt-inject-stop-0 " \
            " --image=$("$BASH_BIN" -c 'source "$1"; printf %s "$MH_DEBUG_IMAGE"' _ "$MH") " \
            " --image-pull-policy=IfNotPresent " " --profile=restricted " " --custom=" " -- sh -c " " dt-inject-stop-0 STOP mh-service"; do
  assert_status "$CASE_LABEL: debug argv has '${want}'" "$want" "$A "
done
assert_absent "$CASE_LABEL: never --profile=general (SYS_PTRACE)" "--profile=general" "$A"
C="$(tail -n 1 "$CASE/debug-custom.json" 2>/dev/null)"
assert_rc "$CASE_LABEL: --custom securityContext pinned" \
  '{"allowPrivilegeEscalation":false,"capabilities":{"drop":["ALL"]},"privileged":false,"runAsGroup":65532,"runAsNonRoot":true,"runAsUser":65532}' \
  "$(jq -cS '.securityContext' <<<"$C" 2>/dev/null)"
assert_rc "$CASE_LABEL: --custom adds no capability, nothing beyond securityContext" '[] ["securityContext"]' \
  "$(jq -c '[.securityContext.capabilities.add // empty | .[]]' <<<"$C" 2>/dev/null) $(jq -c 'keys' <<<"$C" 2>/dev/null)"

new_case "signal STOP then CONT: names increment, CONT reaches a not-Ready (frozen) MH"
seed_mh 0 "$RUN" "$REV" starting; seed_mh 1 baseline "$REV" serving
knob inject '{"logs":"DT_INJECT pid=42 state=T"}'
run_mh signal 0 STOP --owner "$RUN" --json
expect ok 0
knob inject '{"logs":"DT_INJECT pid=42 state=S"}'
run_mh signal 0 CONT --owner "$RUN" --json
expect ok 0
assert_rc "$CASE_LABEL: second container" "dt-inject-cont-1 S null null" "$(jout '"\(.debug_container) \(.process_state) \(.liveness_budget_seconds) \(.restart_budget_seconds)"')"

new_case "signal KILL: process gone, no budget"
seed_mh 0 "$RUN" "$REV" serving
knob inject '{"logs":"some sh noise\nDT_INJECT pid=42 state=gone"}'
run_mh signal 0 KILL --owner "$RUN" --json
expect ok 0
assert_rc "$CASE_LABEL: gone + null budget" "gone null dt-inject-kill-0" "$(jout '"\(.process_state) \(.liveness_budget_seconds) \(.debug_container)"')"
assert_rc "$CASE_LABEL: restart budget = up's derived bound (probe window 2 + margin 30)" 32 "$(jout '.restart_budget_seconds')"
new_case "signal KILL: restart budget follows MH_UP_MARGIN_S and the template probes"
seed_mh 0 "$RUN" "$REV" serving
knob inject '{"logs":"DT_INJECT pid=42 state=gone"}'
run_mh MH_UP_MARGIN_S=0 signal 0 KILL --owner "$RUN" --json
expect ok 0
assert_rc "$CASE_LABEL: margin 0 -> the probe window alone" 2 "$(jout '.restart_budget_seconds')"

budget_case() { # budget_case <label> <probe-jq-edit> <want>
  new_case "signal STOP budget: $1"
  seed_mh 0 "$RUN" "$REV" serving
  state_edit ".pods[0].spec.containers[0].livenessProbe |= ($2)"
  knob inject '{"logs":"DT_INJECT pid=42 state=T"}'
  run_mh signal 0 STOP --owner "$RUN" --json
  expect ok 0
  assert_rc "$CASE_LABEL: budget" "$3" "$(jout '.liveness_budget_seconds')"
}
budget_case "(5-1)x7 + 2" '.periodSeconds = 7 | .failureThreshold = 5 | .timeoutSeconds = 2' 30
budget_case "failureThreshold 1 -> timeout only" '.failureThreshold = 1' 5
budget_case "initialDelaySeconds does not count" '.initialDelaySeconds = 999' 25

for f in periodSeconds failureThreshold timeoutSeconds; do
  new_case "signal STOP: absent livenessProbe.${f} fails (no k8s default)"
  seed_mh 0 "$RUN" "$REV" serving
  state_edit ".pods[0].spec.containers[0].livenessProbe |= del(.${f})"
  run_mh signal 0 STOP --owner "$RUN" --json
  expect kubectl-failed 1
  assert_status "$CASE_LABEL: names the field" "livenessProbe.${f}" "$DETAIL"
  assert_rc "$CASE_LABEL: nothing injected" 0 "$(count_calls '^.* debug ')"
done
new_case "signal STOP: no livenessProbe at all fails"
seed_mh 0 "$RUN" "$REV" serving
state_edit '.pods[0].spec.containers[0] |= del(.livenessProbe)'
run_mh signal 0 STOP --owner "$RUN" --json
expect kubectl-failed 1
assert_rc "$CASE_LABEL: nothing injected" 0 "$(count_calls ' debug ')"
new_case "signal KILL: needs no livenessProbe"
seed_mh 0 "$RUN" "$REV" serving
state_edit '.pods[0].spec.containers[0] |= del(.livenessProbe)'
knob inject '{"logs":"DT_INJECT pid=42 state=gone"}'
run_mh signal 0 KILL --owner "$RUN" --json
expect ok 0

# --- refusals: no injection, ever ---
refuse_case() { # refuse_case <label> <reason> <rc> <setup-fn> <argv...>
  local label="$1" reason="$2" rc="$3" setup="$4"; shift 4
  new_case "signal refused: $label"
  "$setup"
  run_mh "$@" --json
  expect "$reason" "$rc"
  assert_rc "$CASE_LABEL: no debug call" 0 "$(count_calls ' debug ')"
  assert_rc "$CASE_LABEL: no Lease" 0 "$(count_calls lease)"
}
s_baseline() { seed_mh 0 baseline "$REV" serving; seed_mh 1 baseline "$REV" serving; }
s_manual() { seed_mh 0 manual "$REV" serving; }
s_free() { :; }
s_draining() { seed_mh 0 "$RUN" "$REV" draining-forever; }
s_stuck() { seed_mh 0 "$RUN" "$REV" stuck; }
s_finalizing() { seed_mh 0 "$RUN" "$REV" finalizing; }
s_nospn() { seed_mh 0 "$RUN" "$REV" serving; state_edit '.pods[0].spec |= del(.shareProcessNamespace)'; }
s_spnfalse() { seed_mh 0 "$RUN" "$REV" serving; state_edit '.pods[0].spec.shareProcessNamespace = false'; }
refuse_case "baseline MH (foreign-owner, unforceable)" foreign-owner 4 s_baseline signal 0 KILL --owner "$RUN"
refuse_case "manual MH (foreign-owner)" foreign-owner 4 s_manual signal 0 STOP --owner "$RUN"
refuse_case "free slot" mh-not-serving 4 s_free signal 0 KILL --owner "$RUN"
refuse_case "draining MH" mh-not-serving 4 s_draining signal 0 STOP --owner "$RUN"
refuse_case "stuck MH" mh-not-serving 4 s_stuck signal 0 CONT --owner "$RUN"
refuse_case "finalizing (no pod)" mh-not-serving 4 s_finalizing signal 0 KILL --owner "$RUN"
refuse_case "pod without shareProcessNamespace" mh-no-share-process-namespace 2 s_nospn signal 0 STOP --owner "$RUN"
assert_rc "$CASE_LABEL: the exact operator message" \
  "pod mh-s0-seed lacks shareProcessNamespace: mh-template is stale or not Kind-rendered; re-run deploy (infra/kind/scripts/deploy.sh)" "$DETAIL"
refuse_case "shareProcessNamespace=false" mh-no-share-process-namespace 2 s_spnfalse signal 0 KILL --owner "$RUN"

# --- injector failures (validated DETAIL only) ---
fail_case() { # fail_case <label> <reason> <inject-knob-json> <signal> [timeout]
  new_case "signal fails: $1"
  seed_mh 0 "$RUN" "$REV" serving
  knob inject "$3"
  if [[ -n "${5:-}" ]]; then run_mh signal 0 "$4" --owner "$RUN" --timeout "$5" --json
  else run_mh signal 0 "$4" --owner "$RUN" --json; fi
  expect "$2" 1
}
fail_case "pid not unique (container exit 11)" signal-failed '{"exit":11,"logs":"DT_INJECT error=pid-count count=2"}' KILL
assert_status "$CASE_LABEL: DETAIL carries the validated result" "exit=11 error=pid-count count=2" "$DETAIL"
fail_case "raw container output never reaches DETAIL" signal-failed '{"exit":12,"logs":"Error from server: SECRET-abc /proc/1/environ MH_CLIENT_SECRET=x\nDT_INJECT error=pid-one"}' STOP
assert_status "$CASE_LABEL: the validated line" "error=pid-one" "$DETAIL"
assert_absent "$CASE_LABEL: no raw text in DETAIL" "SECRET" "$DETAIL"
assert_absent "$CASE_LABEL: no raw text anywhere on stderr" "SECRET" "$ERR"
assert_absent "$CASE_LABEL: no raw text in the JSON" "SECRET" "$OUT"
fail_case "non-zero exit, unrecognised output" signal-failed '{"exit":1,"logs":"sh: kill: Operation not permitted"}' KILL
assert_status "$CASE_LABEL: says so" "exit=1 no result line" "$DETAIL"
assert_absent "$CASE_LABEL: raw text not relayed" "Operation not permitted" "$ERR"
fail_case "exit 0 without a result line" signal-failed '{"logs":"hello"}' KILL
fail_case "STOP did not take effect" signal-failed '{"logs":"DT_INJECT pid=42 state=S"}' STOP
fail_case "STOP reported tracing-stop t, not T" signal-failed '{"logs":"DT_INJECT pid=42 state=t"}' STOP
fail_case "CONT left it stopped" signal-failed '{"logs":"DT_INJECT pid=42 state=T"}' CONT
fail_case "KILL left it running" signal-failed '{"logs":"DT_INJECT pid=42 state=R"}' KILL
fail_case "reported pid 1" signal-failed '{"logs":"DT_INJECT pid=1 state=T"}' STOP
fail_case "never terminates (bounded)" debug-container-timeout '{"after":-1}' STOP 1
assert_status "$CASE_LABEL: names the container" "container=dt-inject-stop-0 not terminated within 1s" "$DETAIL"
fail_case "image pull fails fast" debug-container-timeout '{"after":-1,"waiting":"ImagePullBackOff"}' KILL
assert_status "$CASE_LABEL: names the waiting reason" "waiting=ImagePullBackOff" "$DETAIL"

# --- the ONE in-container script, run for real against a local process ---
# A bash loop that renames ITSELF to a unique comm stands in for mh-service (the comm is the
# script's $2). It writes /proc/self/comm rather than running a renamed copy of a binary: a
# multicall coreutils (Ubuntu 26.04's) picks its utility from its own file name, so a renamed
# copy of sleep exits at once. Its `sleep` children keep comm `sleep` and never match.
COMM="dtinj$$x"; COMM="${COMM:0:15}"
new_case "in-container script: STOP / CONT / KILL on a real process"
STANDIN='printf %s "$1" > /proc/self/comm; while :; do sleep 1; done'
SCRIPT="$("$BASH_BIN" -c 'source "$1"; printf %s "$MH_INJECT_SCRIPT"' _ "$MH")"
pstate() { sed -n 's/^[0-9]* (.*) \([A-Za-z]\) .*/\1/p' "/proc/$1/stat" 2>/dev/null; }
# wait_comm <pid>: bounded poll (5 s, 0.05 s steps) until the process IS $COMM; loud at the bound.
wait_comm() {
  local i
  for (( i = 0; i < 100; i++ )); do
    [[ "$(cat "/proc/$1/comm" 2>/dev/null)" == "$COMM" ]] && return 0
    sleep 0.05
  done
  assert_rc "$CASE_LABEL: process $1 never showed comm $COMM within 5s" "$COMM" "$(cat "/proc/$1/comm" 2>/dev/null)"
  return 1
}
# Every stand-in is SIGKILLed at case end (assertions never abort, so this always runs); KILL
# also ends a STOPped one, so nothing is left behind.
TPIDS=()
reap_standins() { local p; for p in "${TPIDS[@]}"; do kill -KILL "$p" 2>/dev/null; wait "$p" 2>/dev/null; done; TPIDS=(); }
"$BASH_BIN" -c "$STANDIN" _ "$COMM" & tpid=$!; TPIDS+=("$tpid")
wait_comm "$tpid"
o="$(sh -c "$SCRIPT" dt-inject STOP "$COMM")"; r=$?
assert_rc "$CASE_LABEL: STOP rc" 0 "$r"; assert_rc "$CASE_LABEL: STOP line" "DT_INJECT pid=${tpid} state=T" "$o"
assert_rc "$CASE_LABEL: really stopped" T "$(pstate "$tpid")"
o="$(sh -c "$SCRIPT" dt-inject CONT "$COMM")"; r=$?
assert_rc "$CASE_LABEL: CONT rc" 0 "$r"; assert_rc "$CASE_LABEL: really running" S "$(pstate "$tpid")"
o="$(sh -c "$SCRIPT" dt-inject HUP "$COMM")"; r=$?
assert_rc "$CASE_LABEL: closed set re-checked in-container" "10 DT_INJECT error=bad-signal" "$r $o"
assert_rc "$CASE_LABEL: HUP never sent" S "$(pstate "$tpid")"
"$BASH_BIN" -c "$STANDIN" _ "$COMM" & tpid2=$!; TPIDS+=("$tpid2")
wait_comm "$tpid2"
o="$(sh -c "$SCRIPT" dt-inject KILL "$COMM")"; r=$?
assert_rc "$CASE_LABEL: two matches refused" "11 DT_INJECT error=pid-count count=2" "$r $o"
kill "$tpid2" 2>/dev/null; wait "$tpid2" 2>/dev/null
o="$(sh -c "$SCRIPT" dt-inject KILL "$COMM")"; r=$?
assert_rc "$CASE_LABEL: KILL" "0 DT_INJECT pid=${tpid} state=gone" "$r $o"
wait "$tpid" 2>/dev/null
o="$(sh -c "$SCRIPT" dt-inject KILL "$COMM")"; r=$?
assert_rc "$CASE_LABEL: no match refused" "11 DT_INJECT error=pid-count count=0" "$r $o"
assert_rc "$CASE_LABEL: exactly one kill, of \$p by name (never -1/0/a group)" 1 "$(grep -c 'kill -s "\$1" "\$p"' <<<"$SCRIPT")"
assert_rc "$CASE_LABEL: no other kill in the script" 1 "$(grep -c 'kill ' <<<"$SCRIPT")"
assert_status "$CASE_LABEL: pid > 1 checked before kill" '[ "$p" -gt 1 ]' "$SCRIPT"
reap_standins

# --- constants ---
new_case "MH_DEBUG_IMAGE is digest-pinned"
IMG="$("$BASH_BIN" -c 'source "$1"; printf %s "$MH_DEBUG_IMAGE"' _ "$MH")"
assert_rc "$CASE_LABEL: @sha256:<64 hex> at the end" true "$([[ "$IMG" =~ @sha256:[0-9a-f]{64}$ ]] && echo true || echo false)"

# --- the widened pod-delete matcher: negative controls (the stub must flag each) ---
for bad in "delete deployment x pod/y" "delete po y" "delete -l a=b pods" "delete pods,deployments -l a=b"; do
  new_case "stub flags a pod delete (control): ${bad}"
  IFS=' ' read -r -a argv <<<"$bad"
  PATH="${STUB_BIN}:${PATH}" STUB_STATE="$CASE" STUB_EXPECT_CONTEXT="kind-x" \
    kubectl --context kind-x --request-timeout=5s -n dark-tower "${argv[@]}" >/dev/null 2>&1
  assert_rc "$CASE_LABEL: rc 96" 96 "$?"
  assert_marker "$CASE_LABEL: violation recorded" "$CASE" "violation.force"
  rm -f "$CASE/violation.force"   # the control's own violation, not mh.sh's
  : >"$CASE/calls.log"            # ... and its own argv (the global argv checks are about mh.sh)
done
new_case "stub passes drain's Deployment delete (control)"
PATH="${STUB_BIN}:${PATH}" STUB_STATE="$CASE" STUB_EXPECT_CONTEXT="kind-x" \
  kubectl --context kind-x --request-timeout=5s -n dark-tower delete deployment mh-s0 --cascade=foreground --wait=false >/dev/null 2>&1
assert_no_marker "$CASE_LABEL: not flagged" "$CASE" "violation.*"
}

group_failures() {
# --- kubectl-failed / internal-error ---------------------------------------------------------

new_case "kubectl-failed: API error mid-operation"
seed_mh 0 manual "$REV" serving; seed_mh 1 manual "$REV" serving
knob fail '{"match":"get pods","after":1}'
run_mh drain 0 --json
expect kubectl-failed 1
assert_status "$CASE_LABEL: DETAIL names the op only" "op=get-pods" "$DETAIL"
assert_absent "$CASE_LABEL: no raw kubectl text in DETAIL" "Error from server" "$DETAIL"
assert_status "$CASE_LABEL: kubectl's words on a human line" "kubectl: Error from server (InternalError)" "$ERR"
assert_rc "$CASE_LABEL: Lease still released on the failure path" null "$(state_q -r '.lease.spec.holderIdentity')"

new_case "kubectl-failed: malformed owner read back"
seed_mh 0 "Bad Owner" "$REV" serving
run_mh list --json
expect kubectl-failed 1
assert_status "$CASE_LABEL: names the field" "owner@slot0" "$DETAIL"

new_case "internal-error: SIGTERM mid-wait"
knob ready_after -1
env -u DT_PORT_MAP -u DT_HOST_GATEWAY_IP PATH="${STUB_BIN}:${PATH}" STUB_STATE="$CASE" STUB_EXPECT_CONTEXT="kind-${MH_CLUSTER}" \
  DT_CLUSTER_NAME="$MH_CLUSTER" KUBECONFIG="$WORK/kubeconfig" TMPDIR="$CASE/tmp" \
  MH_POLL_INTERVAL_S=0.01 MH_UP_MARGIN_S=30 MH_DRAIN_MARGIN_S=30 \
  "$BASH_BIN" "$MH" up --timeout 60 --json >"$CASE/out" 2>"$CASE/err" &
pid=$!
waited=0
until [[ "$(state_q -r '.polled | length')" -gt 0 ]]; do
  (( waited++ > 500 )) && break
  sleep 0.02
done
assert_rc "$CASE_LABEL: reached the Ready wait (bounded)" true "$([[ "$(state_q -r '.polled | length')" -gt 0 ]] && echo true || echo false)"
kill -TERM "$pid" 2>/dev/null
wait "$pid"
RC=$?
record_run 1
expect internal-error 1
assert_status "$CASE_LABEL: DETAIL signal" "signal=TERM" "$DETAIL"
assert_status "$CASE_LABEL: SLOT names the created MH" "SLOT=0" "$LAST"
assert_status "$CASE_LABEL: reclaim command" "reclaim with: mh.sh drain 0 --owner manual" "$DETAIL"
assert_rc "$CASE_LABEL: temp dir cleaned by the trap" 0 "$(find "$CASE/tmp" -mindepth 1 | wc -l | tr -d ' ')"

new_case "internal-error: a mutation without the Lease fails closed"
PATH="${STUB_BIN}:${PATH}" STUB_STATE="$CASE" STUB_EXPECT_CONTEXT="kind-x" \
  "$BASH_BIN" -c 'source "$1"; MH_TMP="$2"; KUBE_CONTEXT=kind-x; MH_KUBECTL_TIMEOUT_S=5; MH_LEASE_HELD=0; delete_mh 0' \
  _ "$MH" "$CASE/tmp" >"$CASE/out" 2>"$CASE/err"
RC=$?
record_run 0
expect internal-error 1
assert_status "$CASE_LABEL: says why" "mutation without the mh-pool Lease" "$DETAIL"
assert_rc "$CASE_LABEL: no kubectl call at all" 0 "$(wc -l <"$CASE/calls.log" | tr -d ' ')"

new_case "internal-error: unknown token in the emitter"
"$BASH_BIN" -c 'source "$1"; MH_TMP=""; mh_finish not-a-token' _ "$MH" >"$CASE/out" 2>"$CASE/err"
RC=$?
record_run 0
expect internal-error 1

}

# --- Run the groups CONCURRENTLY ------------------------------------------------------------
# Each group is independent (its own case dirs under $WORK, its own collector file), so the
# suite's wall time is its longest group, not the sum (the timeout cases wait real seconds).
# A group's counters come back through a result file; a group that dies without one is a FAIL.
MH_TEST_GROUPS=(group_list group_nextfit group_up_object group_up_capacity group_up_idempotent group_up_derived group_up_margin group_lease group_drain_basic group_drain_timeouts group_drain_refusals group_drain_all group_upgrade_a group_upgrade_b group_usage group_preconditions group_failures group_signal)
for g in "${MH_TEST_GROUPS[@]}"; do
  (
    GROUP_ID="${g#group_}"; PASS=0; FAIL=0; FAILURES=()
    EMITTED="${WORK}/emitted.${GROUP_ID}"; : >"$EMITTED"
    "$g"
    { printf '%s\n%s\n' "$PASS" "$FAIL"; printf '%s\0' "${FAILURES[@]}"; } >"${WORK}/result.${GROUP_ID}"
  ) &
done
wait
for g in "${MH_TEST_GROUPS[@]}"; do
  r="${WORK}/result.${g#group_}"
  if [[ ! -s "$r" ]]; then
    FAIL=$((FAIL + 1)); FAILURES+=("[${g}] group produced no result file (crashed)"); continue
  fi
  PASS=$(( PASS + $(sed -n 1p "$r") )); FAIL=$(( FAIL + $(sed -n 2p "$r") ))
  while IFS= read -r -d '' f; do [[ -n "$f" ]] && FAILURES+=("$f"); done < <(tail -n +3 "$r")
done
cat "${WORK}"/emitted.* >"$EMITTED"
# A recorded failure can never be reported as "0 failed".
(( ${#FAILURES[@]} > FAIL )) && FAIL=${#FAILURES[@]}

# --- Global properties ---------------------------------------------------------------------

CASE_LABEL="global"

# Token completeness, both directions: MH_TOKENS (sourced) == contract §3 == emitted.
ARRAY="$("$BASH_BIN" -c 'source "$1"; printf "%s\n" "${MH_TOKENS[@]}"' _ "$MH" | sort)"
CONTRACT_TOKENS="$(awk '/^3\. EXIT CODES/{f=1; next} /^4\. /{f=0} f' "$CONTRACT" \
  | sed -nE 's/^ +([0-5]) [A-Z]+ +(.*)$/\1 \2/p' | sed -E 's/\([^)]*\)//g' \
  | awk '{c=$1; $1=""; n=split($0, t, ","); for (i=1;i<=n;i++){gsub(/ /,"",t[i]); if (t[i]!="") print t[i] ":" c}}' | sort)"
EMITTED_SET="$(awk '{print $1}' "$EMITTED" | sort -u)"
assert_rc "global: MH_TOKENS extraction non-vacuous (>= 15)" true "$( (( $(grep -c . <<<"$ARRAY") >= 15 )) && echo true || echo false)"
assert_rc "global: contract §3 extraction non-vacuous (>= 15)" true "$( (( $(grep -c . <<<"$CONTRACT_TOKENS") >= 15 )) && echo true || echo false)"
assert_rc "global: MH_TOKENS == contract §3 (token:code, both directions)" "$CONTRACT_TOKENS" "$ARRAY"
assert_rc "global: every listed token emitted by some case" "$(cut -d: -f1 <<<"$ARRAY" | sort -u)" "$EMITTED_SET"
while IFS=' ' read -r tok rc; do
  want="$(grep -E "^${tok}:" <<<"$ARRAY" | cut -d: -f2)"
  assert_rc "global: ${tok} exits ${want:-<unlisted>}" "${want:-unlisted}" "$rc"
done < <(sort -u "$EMITTED")

# The force/zero-grace ban, over every argv every case sent (+ a positive control).
ALL_CALLS="$(cat "$WORK"/case-*/calls.log)"
assert_absent "global: no --grace-period ever" "--grace-period" "$ALL_CALLS"
assert_absent "global: no --force ever" "--force" "$ALL_CALLS"
assert_rc "global: no pod delete ever" 0 "$(grep -cE '(^| )delete pods?( |$)' <<<"$ALL_CALLS")"
assert_rc "global: control: foreground Deployment deletes were issued" true \
  "$(grep -q 'delete deployment mh-s[0-9]* --cascade=foreground --wait=false' <<<"$ALL_CALLS" && echo true || echo false)"
assert_rc "global: control: signal's restricted debug calls were issued (and recorded)" true \
  "$(grep -qE '(^| )debug mh-s[0-9]+-seed .*--profile=restricted' <<<"$ALL_CALLS" && echo true || echo false)"
assert_no_marker "global: no stub violation in any case" "$WORK" "case-*/violation.*"

# Static checks on the script body (comments excluded), each with a positive control.
BODY="$(grep -vE '^[[:space:]]*#' "$MH")"
assert_absent "global: no --grace-period literal" "grace-period" "$BODY"
assert_rc "global: Prometheus-free (no curl/wget/prometheus/:9090//metrics)" 0 "$(grep -ciE 'curl|wget|prometheus|:9090|/metrics' <<<"$BODY")"
assert_status "global: control: the body is non-empty (MH_RESULT present)" "MH_RESULT" "$BODY"
assert_rc "global: exit codes only via MH_TOKENS (one exit, in mh_finish)" 1 "$(grep -cE '(^|[^_[:alnum:]])exit[[:space:]]' <<<"$BODY")"

report_results "infra/kind/scripts/mh.test.sh"
