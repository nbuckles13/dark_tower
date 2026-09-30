#!/usr/bin/env bash
# validate-dev-cluster-verbs.sh — the devloop cluster VOCABULARY drift guard (ADR-0038 step 3).
#
# THE PROBLEM. The commands that manage the dev cluster — the helper's verbs (`dev-cluster
# provision`, `dev-cluster deploy`, …) and the Kind scripts' flags (`deploy.sh`, `setup.sh
# --provision-org`, …) — are spelled as REMEDIES in three languages that cannot share a
# constant: Rust (crates/env-tests/src/fixtures/kube.rs's REDEPLOY_HINT), shell (deploy.sh's
# blueprint/migration remedies, layer7.sh's precondition fixes) and markdown (the runbooks,
# docs/runbooks/devloop-validation.md §6.7 above all). ADR-0038 step 3 retired `setup`,
# `rebuild`, `rebuild-all`, `deploy <svc>`, `--only`, `--skip-build` and `--rebuild-all`; a
# remedy still naming one sends an operator to a command that is now REJECTED. So instead of
# pinning each site's text, this guard pins the VOCABULARY every site draws from:
#
#   (1) SSoT: `VERBS` in crates/devloop-helper/src/protocol.rs (the helper's wire allowlist,
#       pinned against parse_command by an in-crate unit test). The `dev-cluster` client's own
#       one-line `VERBS=` must equal it exactly — a second copy of the allowlist, checked.
#   (2) every `dev-cluster <verb>` spelled in live code/docs names a verb in that set;
#   (3) every `{setup,provision,deploy}.sh --<flag>` spelled there is a flag that script's own
#       argument parser accepts;
#   (4) ANTI-VACUITY: each enumerated remedy site yields at least one match (>= 1 — never an
#       exact count, which would be a second encoding of the site's prose), every scan root
#       exists (a missing root is a FAILURE, never "0 hits"), and neither verb set is empty.
#
# COMMAND POSITIONS ONLY (prose like "the dev-cluster helper" is not a command):
#   - markdown: inside backtick spans — `(^|[[:space:]/;&|(])dev-cluster <word>`; on a ``` fenced
#     line only at its start (`$ `/path allowed) or after `&& `, `; `, `| `;
#   - code (everything else): `(["'`(]|/devloop/|&& |; |\| )dev-cluster <word>`, and on a comment
#     line (`#`, `//`) only in quotes or backticks.
#   (5) RETIRED SPELLINGS: bare prose ("after `rebuild-all`") escapes rules (2)/(3), so a
#       denylist of retired tokens (RETIRED below) is swept over the same scope. A live text that
#       MUST name one (the rejection tests, the ADR-0030 retirement record, the helpers that still
#       own `diff_touches_path`) is admitted ONLY by an ALLOW entry `path|token|context|reason`:
#       the hit's path and token must match exactly and its line must contain `context`. It is
#       not a free-form suppression — an entry that no longer matches any line is itself a
#       violation (stale allow), so the list can only shrink with the tree.
#
# SCOPE: live code and docs. EXCLUDED as history (dated records quote what was true then):
# docs/devloop-outputs/, docs/user-stories/, docs/debates/, docs/TODO.md and docs/decisions/ —
# EXCEPT docs/decisions/adr-0030-host-side-cluster-helper.md, whose verb table is the normative
# statement of the allowlist and IS swept. Also excluded: this guard and its self-test.
#
# Wired into Layer 3: auto-discovered by scripts/guards/run-guards.sh; its self-test
# (scripts/guards/validate-dev-cluster-verbs.test.sh) is wired explicitly in scripts/layer3.sh.
#
# Exit: 0 when every spelling is live; 1 on any violation.

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # scripts/guards/simple
__real_root="$(cd "${__here}/../../.." && pwd)"

# shellcheck source=../common.sh
source "${__here}/../common.sh"
# Scan-root seam (DT_VERBS_GUARD_ROOT), DEVLOOP_TEST-gated — rationale on guard_seam_root.
REPO_ROOT="$(guard_seam_root DT_VERBS_GUARD_ROOT "$__real_root")"

readonly PROTOCOL_RS="crates/devloop-helper/src/protocol.rs"
readonly CLIENT="infra/devloop/dev-cluster"
readonly KIND_SCRIPTS=(setup provision deploy)
# Scan roots — every one must exist.
readonly ROOTS=(
  crates scripts infra packages .claude
  docs/runbooks docs/specialist-knowledge docs/LOCAL_DEVELOPMENT.md
  docs/decisions/adr-0030-host-side-cluster-helper.md
)
readonly EXCLUDES=(
  ':(exclude)docs/devloop-outputs/'
  ':(exclude)docs/user-stories/'
  ':(exclude)docs/debates/'
  ':(exclude)scripts/guards/simple/validate-dev-cluster-verbs.sh'
  ':(exclude)scripts/guards/validate-dev-cluster-verbs.test.sh'
)
# Retired spellings (ADR-0038 step 3), fixed strings. `rebuild-all` also covers `--rebuild-all`,
# `skip-observability` covers `--skip-observability`.
readonly RETIRED=(
  "rebuild-all" "--skip-build" "diff_touches_path" "observability-apply-failed"
  "FRESH_CLUSTER_HINT" "setup_sh_command" "cluster-setup-failed" "cluster-rebuild-failed"
  "Setup in progress" "skip-observability" "setup-poll-timeout"
)
# `path|token|context|reason` — the ONLY admission for a retired token in live scope. Kept as a
# data file INSIDE the scanned root (the media-telemetry-deny.yaml precedent), so a synthetic
# self-test tree carries its own list; a missing file is a violation, never "nothing allowed".
readonly ALLOW_FILE="scripts/guards/simple/dev-cluster-verbs-allow.txt"

# Remedy sites that must each spell at least one live command (`<path>|<fixed needle>`).
readonly SITES=(
  "crates/env-tests/src/fixtures/kube.rs|dev-cluster deploy"
  "docs/runbooks/devloop-validation.md|dev-cluster provision"
  "docs/runbooks/devloop-validation.md|dev-cluster deploy"
  "infra/kind/scripts/deploy.sh|dev-cluster provision"
  "scripts/layer7.sh|dev-cluster provision"
)

violations=0
violation() { printf 'VIOLATION: %s\n' "$*"; violations=$((violations + 1)); }

printf 'dev-cluster vocabulary drift guard\n'

# --- (1) the two allowlist copies ----------------------------------------------------------
helper_verbs="$(grep -m1 -E '^pub const VERBS: &\[&str\] = &\[' "${REPO_ROOT}/${PROTOCOL_RS}" 2>/dev/null \
  | grep -oE '"[a-z][a-z-]*"' | tr -d '"' | sort -u || true)"
client_verbs="$(grep -m1 -E '^VERBS="[a-z -]+"$' "${REPO_ROOT}/${CLIENT}" 2>/dev/null \
  | sed -E 's/^VERBS="(.*)"$/\1/' | tr ' ' '\n' | grep . | sort -u || true)"
if [[ -z "$helper_verbs" ]]; then
  violation "no one-line 'pub const VERBS: &[&str] = &[...]' in ${PROTOCOL_RS} — the SSoT could not be read, so every check below would be vacuous"
fi
if [[ -z "$client_verbs" ]]; then
  violation "no one-line VERBS=\"...\" in ${CLIENT} — the client's allowlist copy could not be read"
fi
if [[ -n "$helper_verbs" && -n "$client_verbs" && "$helper_verbs" != "$client_verbs" ]]; then
  violation "the client's VERBS (${CLIENT}) differ from the helper's (${PROTOCOL_RS}): helper=[$(tr '\n' ' ' <<<"$helper_verbs")] client=[$(tr '\n' ' ' <<<"$client_verbs")]"
fi

# The flags each Kind script's argument parser accepts (its `--flag)` case arms).
declare -A FLAGS=()
for s in "${KIND_SCRIPTS[@]}"; do
  f="${REPO_ROOT}/infra/kind/scripts/${s}.sh"
  if [[ ! -f "$f" ]]; then
    violation "infra/kind/scripts/${s}.sh is missing — its flags cannot be read"
    continue
  fi
  FLAGS[$s]=" $(grep -oE '^[[:space:]]+--[a-z][a-z-]*\)' "$f" | tr -d ' \t)' | tr '\n' ' ')"
  [[ "${FLAGS[$s]}" == *" --help "* ]] || violation "could not read the flags infra/kind/scripts/${s}.sh accepts (no '--help)' arm found)"
done

# --- scope ---------------------------------------------------------------------------------
if ! git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  violation "REPO_ROOT='${REPO_ROOT}' is not a git work tree — the sweep cannot run (VACUOUS, not evidence)"
fi
for r in "${ROOTS[@]}"; do
  [[ -e "${REPO_ROOT}/${r}" ]] || violation "scan root '${r}' does not exist — a missing root is a failure, never 0 hits"
done

# Candidate files (tracked + untracked-not-ignored), narrowed by a cheap pre-filter.
mapfile -t files < <(
  git -C "$REPO_ROOT" grep --untracked -I -l -E 'dev-cluster [a-z]|(setup|provision|deploy)\.sh --' \
    -- "${ROOTS[@]}" "${EXCLUDES[@]}" 2>/dev/null || true
)

# Emit the command-position text of one file (see header).
command_text() {
  local f="$1"
  case "$f" in
    *.md)
      # Inline spans: any command position inside the span. Fenced lines: only a line that
      # STARTS with the command (optionally `$ ` / a path) or chains it (`&&`, `;`, `|`) — a
      # fenced ASCII diagram ("└── dev-cluster client") is not a command.
      awk '
        /^[[:space:]]*```/ { fence = !fence; next }
        fence { print "F\t" $0; next }
        { n = split($0, parts, "`"); for (i = 2; i <= n; i += 2) print "S\t" parts[i] }
      ' "${REPO_ROOT}/${f}" | awk -F '\t' '
        $1 == "S" { print $2; next }
        { if (match($2, /^[[:space:]]*(\$ )?([^[:space:]]*\/)?dev-cluster [a-z]/)) print $2
          else if (match($2, /(&& |; |\| )dev-cluster [a-z]/)) print substr($2, RSTART) }
      ' | grep -oE '(^|[[:space:]/;&|(])dev-cluster [a-z][a-z-]*' || true ;;
    *)
      # Comment lines (`#`, `//`): only a quoted/backticked command. Code lines: a quote,
      # backtick, paren, a /devloop/ path or a chain operator before the command.
      awk '
        /^[[:space:]]*(#|\/\/)/ { print "C\t" $0; next }
        { print "L\t" $0 }
      ' "${REPO_ROOT}/${f}" | awk -F '\t' '
        $1 == "C" { while (match($2, /["\047`]dev-cluster [a-z][a-z-]*/)) { print substr($2, RSTART + 1, RLENGTH - 1); $2 = substr($2, RSTART + RLENGTH) } next }
        { while (match($2, /(["\047`(]|\/devloop\/|&& |; |\| )dev-cluster [a-z][a-z-]*/)) { m = substr($2, RSTART, RLENGTH); sub(/^.*dev-cluster /, "dev-cluster ", m); print m; $2 = substr($2, RSTART + RLENGTH) } }
      ' || true ;;
  esac
}

checked=0
for f in "${files[@]}"; do
  [[ -n "$f" ]] || continue
  # (2) verbs
  while IFS= read -r m; do
    [[ -n "$m" ]] || continue
    checked=$((checked + 1))
    verb="${m##*dev-cluster }"
    if ! grep -qxF "$verb" <<<"$helper_verbs"; then
      violation "${f}: 'dev-cluster ${verb}' is not a helper verb (valid: $(tr '\n' ' ' <<<"$helper_verbs"))"
    fi
  done < <(command_text "$f")
  # (3) Kind script flags
  while IFS= read -r m; do
    [[ -n "$m" ]] || continue
    checked=$((checked + 1))
    script="$(sed -E 's/.*(setup|provision|deploy)\.sh --.*/\1/' <<<"$m")"
    flag="--${m##*.sh --}"
    if [[ "${FLAGS[$script]:-}" != *" ${flag} "* ]]; then
      violation "${f}: '${script}.sh ${flag}' is not a flag infra/kind/scripts/${script}.sh accepts (accepts:${FLAGS[$script]:-<unread>})"
    fi
  done < <(grep -oE '(^|[^a-zA-Z0-9_.-])(setup|provision|deploy)\.sh --[a-z][a-z-]*' "${REPO_ROOT}/${f}" || true)
done

# --- (5) retired spellings, admitted only by an exact ALLOW entry -------------------------------
declare -A ALLOW_USED=()
ALLOW=()
if [[ -f "${REPO_ROOT}/${ALLOW_FILE}" ]]; then
  mapfile -t ALLOW < <(grep -vE '^[[:space:]]*(#|$)' "${REPO_ROOT}/${ALLOW_FILE}")
else
  violation "${ALLOW_FILE} is missing — the retired-spelling allowlist cannot be read"
fi
for entry in "${ALLOW[@]}"; do
  IFS='|' read -r a_path a_tok a_ctx a_reason <<<"$entry"
  [[ -n "$a_path" && -n "$a_tok" && -n "$a_ctx" && -n "$a_reason" ]] \
    || violation "malformed ALLOW entry (need path|token|context|reason, all non-empty): ${entry}"
done
grep_args=()
for tok in "${RETIRED[@]}"; do grep_args+=(-e "$tok"); done
while IFS= read -r hit; do
  [[ -n "$hit" ]] || continue
  path="${hit%%:*}"; rest="${hit#*:}"; lineno="${rest%%:*}"; text="${rest#*:}"
  for tok in "${RETIRED[@]}"; do
    [[ "$text" == *"$tok"* ]] || continue
    allowed=0
    for idx in "${!ALLOW[@]}"; do
      IFS='|' read -r a_path a_tok a_ctx _ <<<"${ALLOW[$idx]}"
      if [[ "$a_path" == "$path" && "$a_tok" == "$tok" && "$text" == *"$a_ctx"* ]]; then
        allowed=1; ALLOW_USED[$idx]=1
      fi
    done
    (( allowed )) || violation "${path}:${lineno}: retired spelling '${tok}' in live text — use the live verb/flag/token (ADR-0038 step 3), or, if the text must name it, add a narrow entry to ${ALLOW_FILE}"
  done
done < <(git -C "$REPO_ROOT" grep --untracked -I -n -F "${grep_args[@]}" -- "${ROOTS[@]}" "${EXCLUDES[@]}" ":(exclude)${ALLOW_FILE}" 2>/dev/null || true)
for idx in "${!ALLOW[@]}"; do
  [[ -n "${ALLOW_USED[$idx]:-}" ]] || violation "stale ALLOW entry (matches no line any more — delete it): ${ALLOW[$idx]}"
done

# --- (4) anti-vacuity: each remedy site spells >= 1 live command ---------------------------
for site in "${SITES[@]}"; do
  path="${site%%|*}"; needle="${site#*|}"
  if [[ ! -f "${REPO_ROOT}/${path}" ]]; then
    violation "remedy site ${path} is missing"
  elif ! grep -qF -- "$needle" "${REPO_ROOT}/${path}"; then
    violation "remedy site ${path} no longer spells '${needle}' — either the remedy moved (update SITES) or it names a command this guard cannot see"
  fi
done
[[ "$checked" -gt 0 ]] || violation "the sweep matched ZERO command spellings — the extraction broke; this guard is VACUOUS until repaired"

printf '\n'
if [[ "$violations" -eq 0 ]]; then
  printf '%d command spelling(s) checked in %d file(s); every one names a live verb/flag.\n' "$checked" "${#files[@]}"
  printf 'STATUS=OK REASON=dev-cluster-vocabulary-live\n'
  exit 0
fi
printf '%d violation(s). The SSoT for verbs is VERBS in %s; for flags, each Kind script'"'"'s parser.\n' "$violations" "$PROTOCOL_RS"
printf 'STATUS=FAIL REASON=dev-cluster-vocabulary-drift\n'
exit 1
