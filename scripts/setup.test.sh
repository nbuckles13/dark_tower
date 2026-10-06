#!/usr/bin/env bash
# setup.test.sh — self-test for the Kind scripts: infra/kind/scripts/{setup,provision,deploy}.sh
# and lib/common.sh (ADR-0038 step 3 split setup.sh by lifecycle).
#
# Groups:
#   (A) the disk precondition guard (deploy.sh);
#   (P) provision.sh — the blueprint render / check / record;
#   (D) deploy.sh — content-tagged images, the migration Job, the one environment root;
#   (B) `--provision-org` (setup.sh), the one mode invoked from INSIDE the devloop container
#       (scripts/layer7.sh Phase 1h, story R-7). scripts/layer7.test.sh structurally cannot
#       reach it: setup.sh is STUBBED there through the DEVLOOP_SETUP_SH seam, so the real
#       script's early dispatch, its subdomain validation and its psql shapes are invisible
#       to that tier. This file runs the REAL script against PATH-stubbed
#       kubectl/psql/kind/docker/podman.
#
# The keystone of this devloop (.dockerignore + cleanup GC) is self-validated by Gate-2
# Layer 7's cluster rebuild — but that rebuild only exercises the disk guard's PASS path
# (the host has space → the guard returns 0). The guard's WHOLE POINT — emitting
# `PRECONDITION_FAILURE: … REASON=insufficient-disk` and `exit 2` — is a documented operator
# contract (docs/runbooks/devloop-validation.md §6.7 + the §4 two-token convention) that
# Gate 2 never reaches. Without this test a future refactor could drop/rename the token or
# break exit 2 unnoticed.
#
# REAL-input test (ADR-0034 — no mocked internals): we force DEVLOOP_MIN_DISK_GB absurdly
# high so the comparison trips on any real filesystem, then assert exit 2 + the line-anchored
# token. One case covers override-read + graphroot fallback + df + arithmetic + comparison +
# banner + exit. setup.sh's `BASH_SOURCE[0]==$0` guard lets us source it without running main.
#
# Wired into scripts/layer3.sh (there is no *.test.sh auto-runner), so it runs every devloop
# + CI. Consumes scripts/lang/_test_helpers.sh (assert_rc/assert_status/report_results).
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${__here}/.." && pwd)"
SETUP="${REPO_ROOT}/infra/kind/scripts/setup.sh"
DEPLOY="${REPO_ROOT}/infra/kind/scripts/deploy.sh"
PROVISION="${REPO_ROOT}/infra/kind/scripts/provision.sh"
LIB_COMMON="${REPO_ROOT}/infra/kind/scripts/lib/common.sh"
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

# The guard deliberately `exit 2`s on the trip lane; run each case in its own `bash -c`
# subshell (fresh _DT_DISK_CHECKED sentinel + isolated exit) and capture rc/output, so the
# harness's set -e must not abort. report_results sets the final code.
set +e

# A copy of the repo inputs deploy.sh reads, for tests that mutate them: `infra/` plus the
# ONE list of root files deploy.sh reads (Cargo.lock: sqlx-cli pin; rust-toolchain.toml: image
# RUST_VERSION). A new root input is added HERE, once, not at each copy site.
deploy_tree_copy() {
  mkdir -p "$1"; cp -r "${REPO_ROOT}/infra" "$1/"
  cp "${REPO_ROOT}/Cargo.lock" "${REPO_ROOT}/rust-toolchain.toml" "$1/"
}

# src_run [-u VAR]... <script> <snippet> [args...]: the ONE way this file calls a Kind script's
# functions directly. Runs <snippet> in ONE fresh `bash -c` with <script> SOURCED (each Kind
# script's `BASH_SOURCE[0]==$0` guard suppresses its main; `_` is $0, never the script).
#   - `set --` clears the positionals BEFORE the source, so the caller's args never reach the
#     sourced script's option parser; the snippet gets them as "${ARGS[@]}".
#   - Source-time noise is discarded; the snippet's own stdout/stderr pass through.
#   - The snippet runs under the SOURCED script's shell options (Kind scripts: `set -euo
#     pipefail`) and its top-level logic (provision.sh: AUTO_YES from `[[ -t 0 ]]`) — a snippet
#     that needs a failing rc must capture it with `|| rc=$?`.
#   - `-u VAR` unsets VAR for the run (e.g. an ambient DT_HOST_GATEWAY_IP must not leak in).
#   - Env for the run is passed as temporary assignments: `FOO=1 src_run …`.
src_run() {
  local -a unset_vars=()
  while [[ "${1:-}" == "-u" ]]; do unset_vars+=("$2"); shift 2; done
  local sp="$1" snip="$2"; shift 2
  (
    for v in "${unset_vars[@]}"; do unset "$v"; done
    exec bash -c 'sp="$1"; snip="$2"; shift 2; ARGS=("$@"); set --; source "$sp" >/dev/null 2>&1; eval "$snip"' \
      _ "$sp" "$snip" "$@"
  )
}

# Source deploy.sh (src_run; its BASH_SOURCE guard suppresses main), then call the guard with
# the runtime arg. The guard's stderr banner is what we capture.
RUN_GUARD='check_build_disk_space "${ARGS[0]}"'

# === (1) forced TRIP: min-disk floor absurdly high → comparison trips on the real fs ========
# Real df on the real graphroot (or its PROJECT_ROOT fallback — always df-able, so avail_gb
# is always populated and the huge floor always trips, even on a host without podman).
trip_out="$(DEVLOOP_MIN_DISK_GB=999999999 src_run "$DEPLOY" "$RUN_GUARD" podman 2>&1)"
trip_rc=$?
assert_rc "trip-exit2" 2 "$trip_rc"
# The operator contract: line-anchored, greppable by the §4 one-pass scan.
grep -Eq '^PRECONDITION_FAILURE:.*REASON=insufficient-disk' <<<"$trip_out"
assert_rc "trip-banner-anchored" 0 $?
assert_status "trip-remediation-hint" "podman image prune -f" "$trip_out"

# === (2) PASS path: a 0 GB floor cannot trip (avail_gb >= 0) → exit 0, no banner ============
# Guards against a regression where the guard trips unconditionally (which would still pass
# case 1). Proves the comparison is real, not always-fail.
pass_out="$(DEVLOOP_MIN_DISK_GB=0 src_run "$DEPLOY" "$RUN_GUARD" podman 2>&1)"
pass_rc=$?
assert_rc "pass-exit0" 0 "$pass_rc"
grep -Eq 'PRECONDITION_FAILURE' <<<"$pass_out"
assert_rc "pass-no-banner" 1 $?  # grep exit 1 == token NOT present (correct)

# =============================================================================================
# === (B) --provision-org: per-run organization provisioning (story R-7, task #3) ==============
# =============================================================================================

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
STUB_BIN="${WORK}/bin"
MARK="${WORK}/marks"
mkdir -p "$STUB_BIN" "$MARK"

# A SEALED PATH for everything below: every executable on the outer PATH, symlinked into
# one directory, EXCEPT the container runtimes and kind. Each case prefixes its own stub
# dir, so a stub that is absent or deliberately removed (the provider case moves `podman`
# away) must leave the tool absent, not fall through to the host's real one.
TOOLBOX="${WORK}/toolbox"
mkdir -p "$TOOLBOX"
IFS=: read -r -a __path_dirs <<<"$PATH"
for __d in "${__path_dirs[@]}"; do
  [[ -d "$__d" ]] || continue
  for __f in "$__d"/*; do
    [[ -f "$__f" && -x "$__f" ]] || continue
    __n="${__f##*/}"
    case "$__n" in podman|docker|kind) continue ;; esac
    [[ -e "$TOOLBOX/$__n" ]] || ln -s "$__f" "$TOOLBOX/$__n"
  done
done
unset __path_dirs __d __f __n
export PATH="$TOOLBOX"

# --- PATH stubs --------------------------------------------------------------
# Each stub drops a marker when invoked. Markers, not exit codes, are the evidence here: a
# rejection case passes trivially on exit code alone, because a BROKEN harness also exits
# non-zero. "The psql stub did not run" is what actually distinguishes "the value was
# rejected before reaching SQL" from "the value reached SQL and something else went wrong".
#
# Every stub is LOUD on an invocation it does not model (non-zero + a named message on
# stderr) rather than silently exiting 0. A permissive stub is a vacuous-pass generator:
# setup.sh could start calling something entirely different and every case here would stay
# green.

# kubectl: models exactly two invocations — the kubeconfig context read, and `exec`.
# On `exec` it runs the in-pod command (everything after `--`) LOCALLY, so the PATH-stubbed
# `psql` is what actually executes and can be observed. Without that, `psql` never runs as a
# process at all (it runs inside the pod) and no psql assertion would mean anything.
cat > "${STUB_BIN}/kubectl" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kubectl.calls"
: > "${MARK}/ran.kubectl"
args=("\$@")
for ((i=0; i<\${#args[@]}; i++)); do
  case "\${args[i]}" in
    config)
      if [[ "\${args[i+1]:-}" == "get-contexts" ]]; then
        printf '%s\n' "\${STUB_CONTEXTS:-kind-devloop-fixture}"
        exit 0
      fi ;;
    exec)
      for ((j=i; j<\${#args[@]}; j++)); do
        if [[ "\${args[j]}" == "--" ]]; then exec "\${args[@]:j+1}"; fi
      done
      echo "kubectl stub: 'exec' with no '--' separator: \$*" >&2; exit 90 ;;
  esac
done
echo "kubectl stub: unmodelled invocation: \$*" >&2
exit 91
EOF

# psql: answers the two statements provision_run_org issues, discriminating on the SQL it
# receives ON STDIN. Reading stdin is itself an assertion: setup.sh passes SQL via
# `kubectl exec -i` + a QUOTED heredoc with --set variables, never `psql -c "…"`. `-c`
# performs no `:'var'` interpolation at all (verified on the live pod:
# `psql -v sub=abc -tAc "SELECT :'sub'"` -> `ERROR: syntax error at or near ":"`), so a stub
# that looked for the SQL in argv would be modelling an interface that does not work.
cat > "${STUB_BIN}/psql" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/psql.calls"
: > "${MARK}/ran.psql"
sql="\$(cat)"
printf '%s\n---\n' "\$sql" >> "${MARK}/psql.sql"
if [[ "\$sql" == *"INSERT INTO organizations"* ]]; then
  printf '%s\n' "\${STUB_ORG_ID:-11111111-2222-3333-4444-555555555555}"
elif [[ "\$sql" == *"SELECT count(*)"* ]]; then
  printf '%s\n' "\${STUB_READBACK_COUNT:-1}"
else
  echo "psql stub: unmodelled SQL: \$sql" >&2; exit 92
fi
EOF

# kind / docker / podman: present on PATH but must NEVER be reached on this path. See the
# early-dispatch case for why that is a containment control, not a performance one.
for tool in kind docker podman; do
  cat > "${STUB_BIN}/${tool}" <<EOF
#!/usr/bin/env bash
: > "${MARK}/ran.${tool}"
printf '%s\n' "\$*" >> "${MARK}/${tool}.calls"
exit 0
EOF
done
chmod +x "${STUB_BIN}"/*

reset_marks() { rm -rf "$MARK"; mkdir -p "$MARK"; }

# Invoke the REAL setup.sh with the stubs in front of PATH. Sets global PROV_RC / PROV_OUT.
#
# LC_ALL=C is pinned for the whole group so the harness is DETERMINISTIC — not because any
# value below is currently locale-sensitive.
#
# CLAIM CORRECTED 2026-08-15 (@code-reviewer N3). An earlier version of this comment asserted
# that the Cyrillic row "would otherwise pass because of the ENVIRONMENT rather than because of
# the control it names". That does not reproduce, and it is corrected rather than deleted
# because a comment naming a control that is not there is exactly what the `psql -c`
# interpolation correction in this changeset exists to prevent. Measured: running the real
# `subdomain_re` over аbc / Ä / ábc / ABC / aBc / abc under LC_ALL=C and LC_ALL=C.utf8 gives
# IDENTICAL verdicts — the pin discriminates on nothing here. This image ships only C, C.utf8
# and POSIX (`locale -a`), and `[a-z]` already rejects uppercase in all three, so the locales
# where range-vs-collation actually bites are not installed and the question cannot be settled
# on this host either way.
#
# `setup.sh`'s own `local LC_ALL=C` above `subdomain_re` therefore stands as cheap,
# correct-in-principle insurance at a SQL trust boundary rather than as a control any currently
# testable value demonstrates. B4 below asserts its PRESENCE statically, which is what keeps it
# from being deleted as apparently-redundant — a harness that pins its own locale structurally
# cannot prove the script pins its own.
run_provision() {  # $1 = subdomain argument (passed verbatim, including empty)
  PROV_OUT="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C DT_CLUSTER_NAME=devloop-fixture \
    bash "$SETUP" --provision-org "$1" 2>&1)"
  PROV_RC=$?
}

# === (B1) HAPPY PATH + EARLY DISPATCH =========================================================
# The single most important assertion in this file is a NEGATIVE one: `kind`, `docker` and
# `podman` must not be touched.
#
# Two independent reasons, and the second is the stronger:
#   1. Correctness — those three are ABSENT in the devloop container, so falling through to
#      check_prerequisites()/detect_container_runtime() dies on "Neither Podman nor Docker
#      found": the wrong problem, and unfixable from inside the container.
#   2. CREDENTIAL CONTAINMENT — falling through also reaches seed_test_data() and
#      create_ac_secrets(), which put a `client_secret_hash` and a master key into psql/kubectl
#      arguments. Under Layer 7 that output is captured and relayed, landing in
#      ${DEVLOOP_TMP}/layer-7*.log. A `--provision-org` that merely set a flag without
#      short-circuiting main() would rebuild the cluster and leak on EVERY run while every
#      hermetic layer7 case stayed green — which is precisely why this assertion cannot live
#      in the layer7 tier, where setup.sh is stubbed out.
reset_marks
run_provision "e2e-0123456789abcdef"
assert_rc     "provision-happy-exit0" 0 "$PROV_RC"
assert_status "provision-happy-token" "PROVISIONED_ORG org_id=11111111-2222-3333-4444-555555555555 subdomain=e2e-0123456789abcdef" "$PROV_OUT"
assert_marker    "provision-happy-psql-ran"   "$MARK" 'ran.psql'
assert_no_marker "provision-happy-no-kind"    "$MARK" 'ran.kind'
assert_no_marker "provision-happy-no-docker"  "$MARK" 'ran.docker'
assert_no_marker "provision-happy-no-podman"  "$MARK" 'ran.podman'
# Exactly TWO psql invocations, and one of each kind.
#
# NB — main.md's test plan says "exactly once"; the SHIPPED function issues two statements
# (the INSERT, then the readback that item 9 requires), so "once" would red against correct
# code. Corrected to the real shape rather than relaxed to ">= 1", which would stop
# distinguishing "the readback ran" from "the readback was dropped".
assert_rc "provision-happy-psql-call-count" 2 "$(wc -l < "${MARK}/psql.calls")"
assert_status "provision-happy-insert-ran"   "INSERT INTO organizations" "$(cat "${MARK}/psql.sql")"
assert_status "provision-happy-readback-ran" "SELECT count(*) FROM organizations" "$(cat "${MARK}/psql.sql")"
# The values reach SQL as psql --set variables, never concatenated into the statement text.
assert_status "provision-happy-set-sub" "--set=sub=e2e-0123456789abcdef" "$(cat "${MARK}/psql.calls")"
assert_status "provision-happy-on-error-stop" "ON_ERROR_STOP=1" "$(cat "${MARK}/psql.calls")"
# `kubectl exec -i` — without `-i` psql reads an EMPTY stdin and exits 0, a silent no-op.
assert_status "provision-happy-kubectl-exec-i" "exec -i" "$(cat "${MARK}/kubectl.calls")"
# max_participants_per_meeting is never named, so it keeps the schema default of 100 that
# crates/env-tests/tests/23_meeting_creation.rs asserts and GC's LEAST() would silently cap.
assert_absent "provision-happy-omits-max-participants" "max_participants_per_meeting" "$(grep -A3 -F 'INSERT INTO organizations' "${MARK}/psql.sql" || true)"

# CREDENTIAL CONTAINMENT, ASSERTED BY NAME RATHER THAN BY PROXY (@security F2). The marker
# assertions above detect a fall-through VIA check_prerequisites, which is only one route. They
# would not catch a `create_ac_secrets` call added to the provision path "for the secrets it
# needs", a reordering of main() that lands the branch after a seeding step, or a future
# --provision-org variant that also refreshes credentials — in all of which kind/docker/podman
# stay untouched and the psql call-count is the last line of defence, and that count looks like
# an arithmetic detail so whoever adds the statement will simply update it.
#
# These four assert the property the comment above states in words: NO credential material
# appears in anything this mode emits or executes. Named in the language of the thing being
# protected, so they survive a refactor that renames the proxy.
prov_all="$(cat "${MARK}/psql.sql" "${MARK}/psql.calls" "${MARK}/kubectl.calls" 2>/dev/null; printf '%s' "$PROV_OUT")"
assert_absent "provision-happy-no-client-secret-hash"  "client_secret_hash"  "$prov_all"
assert_absent "provision-happy-no-service-credentials" "service_credentials" "$prov_all"
assert_absent "provision-happy-no-master-key"          "AC_MASTER_KEY"       "$prov_all"
assert_absent "provision-happy-no-mh-secret"           "MH_CLIENT_SECRET"    "$prov_all"

# === (B2) READBACK ASSERTS ON OUTPUT, NOT EXIT CODE ==========================================
# A SELECT matching ZERO rows exits 0. "The readback ran and exited 0" — the natural thing to
# write — would move the success-looking-no-op bug one step later instead of killing it. The
# stub returns count=0 with exit 0; provisioning must still FAIL.
reset_marks
PROV_OUT="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C DT_CLUSTER_NAME=devloop-fixture \
  STUB_READBACK_COUNT=0 bash "$SETUP" --provision-org "e2e-0123456789abcdef" 2>&1)"; PROV_RC=$?
if [[ "$PROV_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[provision-readback-zero-rows] a readback returning count=0 with exit 0 was accepted as success — the readback must assert on OUTPUT, since psql exits 0 on an empty result set")
fi
assert_absent "provision-readback-zero-no-success-line" "PROVISIONED_ORG" "$PROV_OUT"

# === (B3) REJECTION TABLE ====================================================================
# Merged @security/@test corpus. EVERY row asserts BOTH halves:
#   - a non-zero exit, and
#   - assert_no_marker on the psql stub.
# The no-marker half is load-bearing. An exit-code-only assertion passes even when the hostile
# value reached psql and the DATABASE rejected it — i.e. it would keep passing after the
# in-script validation was deleted, which is the one regression it exists to catch.
#
# The classes are not five spellings of one idea:
#   shell  — would matter if the value were ever interpolated into a shell command string;
#   psql   — psql's own metacharacters (`:var` is RAW TEXT SUBSTITUTION, not quoting; only
#            `:'var'` quotes, which is why the shipped SQL uses the quoted form throughout);
#   argv   — a LEADING-HYPHEN value, which a bare `-z "${2:-}"` argument check does
#            NOT catch. `--provision-org --yes` silently consumes the next flag as the
#            subdomain; the anchored regex is the only thing that rejects it. This is the row
#            that stops that hole being copied into a second mode;
#   format — the schema CHECK's own boundaries, including a 64-char all-lowercase value that
#            must be REJECTED rather than outsourced to VARCHAR(63) truncation (truncation
#            would provision an org under a DIFFERENT subdomain than the one exported to the
#            suites, and both suites would then fail at AC token acquisition);
#   unicode — Cyrillic `а` (U+0430) is not ASCII `a`.
reject_case() {  # $1=label  $2=value
  local label="$1" value="$2"
  reset_marks
  run_provision "$value"
  if [[ "$PROV_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
    FAIL=$((FAIL+1))
    FAILURES+=("[reject-${label}-exit] setup.sh --provision-org ACCEPTED the rejected value ${value@Q} (exit 0)")
  fi
  assert_no_marker "reject-${label}-no-psql" "$MARK" 'ran.psql'
}

# shell class
reject_case "sql-injection"   "x'; DROP TABLE organizations;--"
reject_case "command-sub"     '$(id)'
reject_case "backticks"       '`id`'
reject_case "embedded-newline" 'e2e-abc
devtest'
# psql class
reject_case "single-quote"    "'"
reject_case "double-quote-pair" "''"
reject_case "psql-var-bare"   ':sub'
reject_case "psql-var-quoted" ":'other'"
reject_case "backslash"       '\'
# argv class
reject_case "leading-hyphen-flag"  "-x"
reject_case "leading-hyphen-known" "--yes"
# format class
reject_case "uppercase"       "UPPER"
reject_case "leading-hyphen"  "-leading"
reject_case "trailing-hyphen" "trailing-"
reject_case "empty"           ""
reject_case "64-chars"        "0123456789012345678901234567890123456789012345678901234567890123"
# unicode
reject_case "cyrillic-a"      "аbc"

# === (B4) STATIC: the locale pin is in the SCRIPT, not only in this harness ===================
# This harness pins LC_ALL=C for determinism, which structurally prevents it from proving that
# setup.sh pins its own. Asserted textually instead: `local LC_ALL=C` must immediately precede
# the `subdomain_re` assignment. Without it the anchored pattern LOOKS structural while
# `[a-z]` silently means "whatever this locale collates", and the Cyrillic row above would
# invert on a UTF-8 host.
locale_pin="$(grep -A1 -F 'local LC_ALL=C' "$SETUP" | grep -c "subdomain_re=" || true)"
if [[ "$locale_pin" -ge 1 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[provision-locale-pin] setup.sh's subdomain_re is no longer immediately preceded by 'local LC_ALL=C' — bracket ranges become collation-dependent and the subdomain validator stops meaning ASCII")
fi

# === (B5) DT_CLUSTER_NAME has NO fallback on this path ========================================
# The global default (${DT_CLUSTER_NAME:-dark-tower}) stays for host-side runs, but here a
# fallback is a hazard: on a workstation that also has a manual `dark-tower` cluster,
# `kind-dark-tower` RESOLVES, and the row would be written to the wrong database while the
# suites ran against the devloop one — a silent cross-cluster write surfacing later as an
# unexplained auth failure.
reset_marks
PROV_OUT="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C env -u DT_CLUSTER_NAME \
  bash "$SETUP" --provision-org "e2e-0123456789abcdef" 2>&1)"; PROV_RC=$?
if [[ "$PROV_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[provision-requires-cluster-name] --provision-org ran without DT_CLUSTER_NAME set, so it fell back to the 'dark-tower' default and could write to the wrong cluster")
fi
assert_no_marker "provision-requires-cluster-name-no-psql" "$MARK" 'ran.psql'

# === (B6) --provision-org rejects an unknown trailing flag (the retired --skip-build) =========
reset_marks
PROV_OUT="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C DT_CLUSTER_NAME=devloop-fixture \
  bash "$SETUP" --provision-org "e2e-0123456789abcdef" --skip-build 2>&1)"; PROV_RC=$?
if [[ "$PROV_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[provision-rejects-retired-flag] --provision-org was silently combined with the retired --skip-build instead of failing")
fi
assert_status "provision-rejects-retired-flag-unknown" "Unknown option '--skip-build'" "$PROV_OUT"
assert_no_marker "provision-rejects-retired-flag-no-psql" "$MARK" 'ran.psql'

# === (B7) CONTEXT MISMATCH: a kubeconfig without kind-${CLUSTER_NAME} must NOT write =========
# Found by @infrastructure at Gate 3 and mutation-confirmed here before writing the case:
# replacing provision_run_org's context check with `if false; then` left this file at 63/0.
# B1-B6 all pass through a kubeconfig that HAPPENS to contain the expected context, so the
# fail-closed branch — step (3), the whole point of which is that a wrong-cluster write reads
# as "wrong cluster" rather than as a provisioning bug — had zero coverage. The stub was already
# parameterised (`${STUB_CONTEXTS:-kind-devloop-fixture}`); nothing ever overrode it, which is
# the quieter half of the same vacuity class: a seam built for a case that was never written.
#
# WHY IT MATTERS beyond the branch: without this check a cross-cluster write SUCCEEDS. The org
# lands in whatever database the ambient context points at, Phase 1h reports PROVISIONED_ORG,
# and both Phase-2 suites then fail at AC token acquisition against a cluster that never got the
# row — a provisioning fault surfacing as a suite failure two phases later, which is precisely
# the lane confusion R-7 exists to remove.
#
# `run_provision` cannot drive this (it hardcodes DT_CLUSTER_NAME=devloop-fixture, matching the
# stub's default context), so the invocation is spelled out.
reset_marks
PROV_OUT="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C DT_CLUSTER_NAME=devloop-fixture \
  STUB_CONTEXTS=kind-some-other-cluster \
  bash "$SETUP" --provision-org "e2e-0123456789abcdef" 2>&1)"; PROV_RC=$?
if [[ "$PROV_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[provision-context-mismatch] --provision-org wrote to a kubeconfig that does NOT hold kind-devloop-fixture — a silent cross-cluster write; the suites would then fail at AC token acquisition against a cluster that never received the row")
fi
# The no-marker half is the load-bearing one: an exit-code-only assertion also passes when the
# INSERT reached psql and something downstream happened to fail.
assert_no_marker "provision-context-mismatch-no-psql" "$MARK" 'ran.psql'
# The diagnostic must name the context that was SOUGHT and the ones that were AVAILABLE, or the
# operator cannot tell "wrong cluster" from "provisioning bug" — which is this branch's entire
# reason to exist.
assert_status "provision-context-mismatch-names-expected"  "kind-devloop-fixture"    "$PROV_OUT"
assert_status "provision-context-mismatch-names-available" "kind-some-other-cluster" "$PROV_OUT"
# And it must NOT report success.
assert_absent "provision-context-mismatch-no-success-line" "PROVISIONED_ORG" "$PROV_OUT"

# =============================================================================================
# === (C) #1 name-length — CLUSTER_NAME cap 49 (lib/common.sh + teardown.sh), slug cap 41 (devloop.sh)
# =============================================================================================
# The cap is DERIVED from the 63-char DNS-label limit (see lib/common.sh's validate_cluster_name).
# A literal `49` at a slug site is the specific bug that passes review while leaving the hole
# open, so these boundary cases pin 49/50 (cluster) and 41/42 (slug). Reject cases assert the
# create/delete verb did NOT run (marker), not exit-code alone — a broken fixture also exits
# non-zero.
TEARDOWN="${REPO_ROOT}/infra/kind/scripts/teardown.sh"
DEVLOOP="${REPO_ROOT}/infra/devloop/devloop.sh"
NAME50="$(printf 'a%.0s' {1..50})"
NAME49="$(printf 'a%.0s' {1..49})"
SLUG42="$(printf 'a%.0s' {1..42})"
SLUG41="$(printf 'a%.0s' {1..41})"

# --- lib/common.sh (definitions only): source + call validate_cluster_name directly. The
#     function has NO cluster calls, so a reject cannot create anything — "before creation" is
#     structural. provision.sh reaches it through dt_init_cluster_env, pinned below.
SETUP_VALIDATE='validate_cluster_name "${ARGS[0]}"'

c_rej_out="$(src_run "$LIB_COMMON" "$SETUP_VALIDATE" "$NAME50" 2>&1)"; c_rej_rc=$?
assert_rc     "namelen-setup-reject-exit1"  1 "$c_rej_rc"
assert_status "namelen-setup-reject-token"  "CLUSTER_NAME_TOO_LONG SUBJECT=cluster-name NAME=${NAME50} LEN=50 MAX=49" "$c_rej_out"

c_acc_out="$(src_run "$LIB_COMMON" "$SETUP_VALIDATE" "$NAME49" 2>&1)"; c_acc_rc=$?
assert_rc     "namelen-setup-accept-exit0"    0 "$c_acc_rc"
assert_absent "namelen-setup-accept-no-token" "CLUSTER_NAME_TOO_LONG" "$c_acc_out"   # not rejecting on some other axis
# The entry points really apply it: provision.sh with a 50-char DT_CLUSTER_NAME is rejected
# before it creates anything.
prov_long="$(DT_CLUSTER_NAME="$NAME50" bash "$PROVISION" --check 2>&1)"
assert_status "namelen-provision-entry-rejects" "CLUSTER_NAME_TOO_LONG SUBJECT=cluster-name NAME=${NAME50}" "$prov_long"
# Sourcing the lib runs nothing (teardown.sh relies on it: its validation deliberately differs).
# NOT src_run, deliberately: this case asserts on what SOURCING itself prints, which src_run
# discards by design.
lib_src_out="$(DT_CLUSTER_NAME="$NAME50" bash -c 'source "$1"; echo sourced-ok' _ "$LIB_COMMON" 2>&1)"
assert_status "lib-common-is-definitions-only" "sourced-ok" "$lib_src_out"
assert_absent "lib-common-sourcing-validates-nothing" "CLUSTER_NAME_TOO_LONG" "$lib_src_out"

# --- teardown.sh DELIBERATELY has NO length cap (inverse precondition, @paired-operations):
#     it must be able to DELETE a pre-existing orphan cluster whose name is >49 chars. So a
#     50-char name must NOT be rejected — it must REACH `kind delete`. (Charset is still
#     enforced; the charset-in-sync check below covers that.) Run the REAL script (no
#     source-guard); the `get clusters` stub reports the orphan so teardown proceeds to delete.
cat > "${STUB_BIN}/kind" <<EOF
#!/usr/bin/env bash
case "\$1 \$2" in
  "get clusters")   [[ -z "\${STUB_KIND_LIST_FAIL:-}" ]] || exit 1
                    echo "\${DT_CLUSTER_NAME}" ;;   # report the orphan cluster exists
  "delete cluster") touch "${MARK}/ran.kind_delete" ;;
  *)                : ;;
esac
exit 0
EOF
chmod +x "${STUB_BIN}/kind"
reset_marks
td_out="$(PATH="${STUB_BIN}:${PATH}" DT_CLUSTER_NAME="$NAME50" bash "$TEARDOWN" 2>&1)"; td_rc=$?
# Positive control (@dry-reviewer): a 50-char name is ACCEPTED (exit 0), emits NO token, and
# REACHES `kind delete` (the marker — exit-code-only would pass if teardown returned 0 for an
# unrelated reason). This assertion IS the ruling: teardown must not strand a long-named orphan.
assert_rc     "namelen-teardown-accepts-long-exit0"          0 "$td_rc"
assert_absent "namelen-teardown-accepts-long-no-token"       "CLUSTER_NAME_TOO_LONG" "$td_out"
assert_marker "namelen-teardown-accepts-long-reaches-delete" "$MARK" "ran.kind_delete"
# teardown STILL enforces charset (the injection control): a malformed name is rejected before
# any delete, so the length-cap divergence didn't drop the format check.
reset_marks
tdbad_out="$(PATH="${STUB_BIN}:${PATH}" DT_CLUSTER_NAME='Bad_Name' bash "$TEARDOWN" 2>&1)"; tdbad_rc=$?
assert_rc        "namelen-teardown-charset-reject-exit1"        1 "$tdbad_rc"
assert_status    "namelen-teardown-charset-reject-msg"          "Invalid cluster name" "$tdbad_out"
assert_no_marker "namelen-teardown-charset-reject-before-delete" "$MARK" "ran.kind_delete"
# teardown uses lib/common.sh:cluster_exists: `kind get clusters` FAILING is not "absent" — it
# fails loudly instead of reporting a teardown that deleted nothing.
reset_marks
tdlist_out="$(PATH="${STUB_BIN}:${PATH}" DT_CLUSTER_NAME=pcluster STUB_KIND_LIST_FAIL=1 bash "$TEARDOWN" 2>&1)"; tdlist_rc=$?
assert_rc        "teardown-kind-list-fail-rc"        1 "$tdlist_rc"
assert_status    "teardown-kind-list-fail-says-so"   "'kind get clusters' failed" "$tdlist_out"
assert_absent    "teardown-kind-list-fail-not-absent" "does not exist" "$tdlist_out"
assert_no_marker "teardown-kind-list-fail-no-delete" "$MARK" "ran.kind_delete"

# --- CHARSET regex in sync (@dry-reviewer): pin the CANONICAL literal absolutely (not just
#     compare the two files — a change applied to BOTH would pass a mutual compare). The charset
#     rule is the one thing lib/common.sh + teardown.sh must still agree on after the length-cap
#     divergence; a teardown charset STRICTER than provision's would strand a cluster it could
#     create. `grep -c -F` the exact `=~`-anchored condition in EACH; require >= 1 in BOTH (zero
#     is drift — renamed fn / edited literal / deleted file — never a pass). NOTE a THIRD charset
#     site exists at infra/devloop/devloop.sh (TASK_SLUG) — deliberately NOT pinned here: it
#     validates the SLUG (different subject), and drift there surfaces loudly at setup.
# Needle is the `=~`-anchored PATTERN only (not the `[[ ! "${name}" … ]]` wrapper), so a
# reindent or a local-variable rename can't falsely red this (@dry-reviewer).
readonly CHARSET_NEEDLE='=~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$'
setup_n="$(grep -c -F -- "$CHARSET_NEEDLE" "$LIB_COMMON" || true)"
teardown_n="$(grep -c -F -- "$CHARSET_NEEDLE" "$TEARDOWN" || true)"
if (( setup_n >= 1 && teardown_n >= 1 )); then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[charset-validators-in-sync] lib/common.sh(${setup_n})/teardown.sh(${teardown_n}) must EACH contain the canonical charset check — they must share the CHARSET rule (their LENGTH caps deliberately differ: teardown has none, by design; see its header)")
fi

# --- Guard the single-control-plane-node ASSUMPTION the 49/41 caps derive from (@test).
#     The caps subtract len("-control-plane")=14; a SECOND control-plane node makes KIND emit
#     `-control-plane2` (suffix 15) → real cap 48/40, but every pinned cap test above stays
#     green (they pin the caps, not the config). CLAUDE.md SSoT: two places encode one fact →
#     guard the drift. Match the node-list ENTRY precisely (anchored `-  role: control-plane`),
#     NOT a bare `grep 'role: control-plane'` — the back-reference NOTE comment contains that
#     string, so a naive count returns 2 and the guard would be vacuous. Require EXACTLY 1 in
#     BOTH configs; 0 (extractor drifted / node renamed) is a FAILURE, never "fine".
CP_NEEDLE='^[[:space:]]*-[[:space:]]+role:[[:space:]]*control-plane'
KIND_STATIC="${REPO_ROOT}/infra/kind/kind-config.yaml"
KIND_TMPL="${REPO_ROOT}/infra/kind/kind-config.yaml.tmpl"
cp_static="$(grep -cE "$CP_NEEDLE" "$KIND_STATIC" || true)"
cp_tmpl="$(grep -cE "$CP_NEEDLE" "$KIND_TMPL" || true)"
if [[ "$cp_static" == "1" && "$cp_tmpl" == "1" ]]; then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[single-control-plane-node] kind-config.yaml(${cp_static})/kind-config.yaml.tmpl(${cp_tmpl}) must EACH declare EXACTLY ONE control-plane node — the 49/41 name-length caps assume it (a 2nd shifts the real cap to 48/40; the pinned cap tests would stay green). Adjust the caps at all four sites if this changes.")
fi

# --- devloop.sh launch-time slug cap 41. Reject exits BEFORE any container/cluster creation:
#     run in a NON-git temp dir ($WORK) so a passing slug can't proceed to real work, and prove
#     the podman stub never ran on the reject.
cat > "${STUB_BIN}/podman" <<EOF
#!/usr/bin/env bash
touch "${MARK}/ran.podman"
exit 0
EOF
chmod +x "${STUB_BIN}/podman"
reset_marks
dl_rej_out="$(cd "$WORK" && PATH="${STUB_BIN}:${PATH}" bash "$DEVLOOP" "$SLUG42" 2>&1)"; dl_rej_rc=$?
assert_rc        "namelen-devloop-reject-exit1"           1 "$dl_rej_rc"
assert_status    "namelen-devloop-reject-token"           "CLUSTER_NAME_TOO_LONG SUBJECT=slug NAME=${SLUG42} LEN=42 MAX=41" "$dl_rej_out"
assert_no_marker "namelen-devloop-reject-before-creation" "$MARK" "ran.podman"

# accept: a 41-char slug passes the length gate (it then fails later at the git step in the
# non-repo dir — WITHOUT the token, which is the boundary proof).
reset_marks
dl_acc_out="$(cd "$WORK" && PATH="${STUB_BIN}:${PATH}" bash "$DEVLOOP" "$SLUG41" 2>&1)"
assert_absent "namelen-devloop-accept-no-token" "CLUSTER_NAME_TOO_LONG" "$dl_acc_out"

# --- devloop.sh detect_orphan_clusters: a FAILED listing is "unknown", never "no orphans" ------
# devloop.sh has no source guard (sourcing it runs the launcher), so the REAL function body is
# extracted from the file and run against PATH stubs — never a re-typed copy.
ORPHAN_FN="$(sed -n '/^detect_orphan_clusters() {/,/^}/p' "$DEVLOOP")"
assert_rc "orphan-scan-fn-extracted" 0 "$([[ "$ORPHAN_FN" == 'detect_orphan_clusters() {'* && "$ORPHAN_FN" == *$'\n}' && "$ORPHAN_FN" == *'kind get clusters'* ]] && echo 0 || echo 1)"
OS_BIN="${WORK}/orphanbin"; mkdir -p "$OS_BIN"
cat > "${OS_BIN}/kind" <<'STUB'
#!/usr/bin/env bash
[[ -z "${STUB_KIND_LIST_FAIL:-}" ]] || exit 1
printf 'devloop-zz-orphan-probe\nother-cluster\n'
STUB
printf '#!/usr/bin/env bash\nexit 0\n' > "${OS_BIN}/podman"   # no dev container is running
chmod +x "${OS_BIN}/kind" "${OS_BIN}/podman"
orphan_scan() {
  PATH="${OS_BIN}:${PATH}" bash -c 'eval "$1"; CLUSTER_PREFIX="devloop-"; is_helper_process_alive() { return 1; }; detect_orphan_clusters' _ "$ORPHAN_FN" 2>&1 </dev/null
}
os_out="$(STUB_KIND_LIST_FAIL=1 orphan_scan)"; os_rc=$?
assert_rc     "orphan-scan-list-fail-rc0-advisory"   0 "$os_rc"
assert_status "orphan-scan-list-fail-warns"          "'kind get clusters' failed; cannot scan for orphaned devloop clusters" "$os_out"
# Positive control: with a listing, the same function DOES report the prefixed orphan (so the
# warning above is the failure path, not a scan that never looks).
os_out="$(orphan_scan)"; os_rc=$?
assert_rc     "orphan-scan-listing-rc0"              0 "$os_rc"
assert_status "orphan-scan-listing-reports-orphan"   "Orphaned Kind cluster: devloop-zz-orphan-probe" "$os_out"
assert_absent "orphan-scan-listing-prefix-only"      "other-cluster" "$os_out"

# =============================================================================================
# === (P) provision.sh — the blueprint (ADR-0038 §1, step 3) ===================================
# =============================================================================================
# provision.sh runs against a COPY of the tree (so a case can edit a PROVISION_INPUT) with
# PATH-stubbed kind/kubectl/podman and a stubbed cert recipe. The "cluster" lives in $PSTATE:
# `clusters` (one name per line, what `kind get clusters` prints) and `rec.hash`/`rec.manifest`
# (the kube-system/devloop-blueprint record). Knobs (env):
#   STUB_KIND_VERSION      what `kind version` prints
#   STUB_KIND_LIST_FAIL=1  `kind get clusters` fails (cannot tell whether a cluster exists)
#   STUB_RECORD_READ_FAIL=1  `kubectl get configmap devloop-blueprint` fails (unreadable record)
#   STUB_FAIL_AT=create|calico|secret|record  that build stage fails
#   STUB_MUTATE_ON_CREATE=<rel>  `kind create` edits that tree file (a tree edit mid-build)
#   STUB_RENEWAL_RC        the recipe's --check-renewal exit (0 fresh, 10 due)
#   STUB_RENEW_WRITES=1    a normal recipe run rewrites mc-webtransport.crt (a renewal)
#   STUB_RUNTIME_INFO=fail|hang   `podman info` fails / hangs (the shared env classifier)
#   STUB_READYZ=fail|hang  `kubectl get --raw /readyz` fails / hangs
#   STUB_NODE_IMAGE        what `podman images` lists (default: nothing)
#   STUB_NODE_RUN=fail|hang  the node-container probe (`podman run … kindest/node…`) fails / hangs
#   STUB_CALICO_BYTES      what the curl stub serves for the Calico manifest (the copy's pin is
#                          the sha256 of the default, "CALICO FIXTURE v1")
#
# SHAPE (why this group is not ~75 end-to-end runs). A case runs provision.sh END TO END only
# where the behaviour under test IS main's orchestration — step order, the decision -> action
# mapping, the EXIT trap's one-line invariant, mode dispatch, tty detection. Everything else
# calls the sourced functions directly (src_run via psrc), and a failure path runs under the
# PRODUCTION mechanism itself — `trap __provision_exit_trap EXIT; pstep <fn>` (ptrap) — so its
# STEP/REASON line comes from the real trap and the real shared classifier, never a copy.
# `pstep` is never run under if/||/&&/! there: set -e would be suspended and the trap would see
# rc 0. A cluster "with a recorded blueprint" is SEEDED by the real blueprint_decide +
# record_blueprint (seed_record), not by a whole provision.
PTREE="${WORK}/ptree"; PTEMPLATE="${WORK}/ptree.template"; PBIN="${WORK}/pbin"; PSTATE="${WORK}/pstate"
mkdir -p "$PBIN"
# The pristine tree copy is built ONCE; each case resets it with one `cp -a`.
mkdir -p "$PTEMPLATE/infra/kind/scripts/lib" "$PTEMPLATE/scripts" \
  "$PTEMPLATE/infra/services/postgres" "$PTEMPLATE/infra/docker/certs"
cp "${REPO_ROOT}/infra/kind/scripts/provision.sh" "${REPO_ROOT}/infra/kind/scripts/deploy.sh" "$PTEMPLATE/infra/kind/scripts/"
cp "${REPO_ROOT}/infra/kind/scripts/lib/common.sh" "$PTEMPLATE/infra/kind/scripts/lib/"
# The real kind config WITHOUT its `hostPort` lines: those fixed ports (8443, 9090, ...) are
# held on any host running its dev cluster, and check_host_ports would then fail every
# end-to-end case with port-held. The port-held cases below write their own config.
sed '/^[[:space:]]*hostPort:/d' "${REPO_ROOT}/infra/kind/kind-config.yaml" > "$PTEMPLATE/infra/kind/kind-config.yaml"
cp "${REPO_ROOT}/infra/services/postgres/secret.yaml" "$PTEMPLATE/infra/services/postgres/"
printf 'MC PUBLIC CERT v1\n' > "$PTEMPLATE/infra/docker/certs/mc-webtransport.crt"
printf 'MH PUBLIC CERT v1\n' > "$PTEMPLATE/infra/docker/certs/mh-webtransport.crt"
printf 'NOT-HASHED KEY\n' | tee "$PTEMPLATE/infra/docker/certs/mc-webtransport.key" > "$PTEMPLATE/infra/docker/certs/mh-webtransport.key"
cat > "$PTEMPLATE/scripts/generate-dev-certs.sh" <<EOF
#!/usr/bin/env bash
if [[ "\${1:-}" == "--check-renewal" ]]; then : > "${MARK}/ran.renewal-check"; exit "\${STUB_RENEWAL_RC:-0}"; fi
: > "${MARK}/ran.recipe"
[[ -z "\${STUB_RENEW_WRITES:-}" ]] || printf 'MC PUBLIC CERT v2\n' > "$PTREE/infra/docker/certs/mc-webtransport.crt"
exit 0
EOF
chmod +x "$PTEMPLATE/scripts/generate-dev-certs.sh" "$PTEMPLATE/infra/kind/scripts/"*.sh
# The copy pins the fixture manifest's sha256 (the real pin names the real artifact).
sed -i -E "s/^CALICO_MANIFEST_SHA256=\"[0-9a-f]+\"/CALICO_MANIFEST_SHA256=\"$(printf 'CALICO FIXTURE v1\n' | sha256sum | cut -d' ' -f1)\"/" \
  "$PTEMPLATE/infra/kind/scripts/provision.sh"
make_ptree() { rm -rf "$PTREE" "$PSTATE"; cp -a "$PTEMPLATE" "$PTREE"; mkdir -p "$PSTATE"; }
PROV_COPY="${PTREE}/infra/kind/scripts/provision.sh"

cat > "${PBIN}/kind" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kind.calls"
case "\$1 \${2:-}" in
  "version "*) printf '%s\n' "\${STUB_KIND_VERSION:-kind v0.23.0 go1.22 linux/amd64}" ;;
  "get clusters")
    [[ -z "\${STUB_KIND_LIST_FAIL:-}" ]] || { echo "kind stub: cannot list" >&2; exit 1; }
    cat "${PSTATE}/clusters" 2>/dev/null || true ;;
  "create cluster")
    [[ "\${STUB_FAIL_AT:-}" != create ]] || { echo "kind stub: create failed" >&2; exit 1; }
    for a in "\$@"; do [[ "\$a" == --name=* ]] && printf '%s\n' "\${a#--name=}" >> "${PSTATE}/clusters"; done
    : > "${MARK}/ran.kind_create"
    [[ -z "\${STUB_MUTATE_ON_CREATE:-}" ]] || printf '# edited mid-build\n' >> "${PTREE}/\${STUB_MUTATE_ON_CREATE}" ;;
  "delete cluster")
    printf '%s\n' "\$*" >> "${MARK}/kind_delete.calls"; : > "${MARK}/ran.kind_delete"
    rm -f "${PSTATE}/clusters" "${PSTATE}/rec.hash" "${PSTATE}/rec.manifest" ;;
  "export kubeconfig") : > "${MARK}/ran.kind_export" ;;
  *) echo "kind stub (P): unmodelled '\$*'" >&2; exit 3 ;;
esac
EOF
cat > "${PBIN}/kubectl" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kubectl.calls"
: > "${MARK}/ran.kubectl"
args=("\$@")
if [[ "\${args[0]:-}" == "--context" ]]; then args=("\${args[@]:2}"); fi
case "\${args[0]} \${args[1]:-}" in
  "get --raw")
    : > "${MARK}/ran.readyz"
    case "\${STUB_READYZ:-}" in
      fail) exit 1 ;;
      hang) sleep 30; exit 0 ;;
    esac
    echo ok; exit 0 ;;
  "get configmap")
    [[ -z "\${STUB_RECORD_READ_FAIL:-}" ]] || { echo "kubectl stub: the server is unreachable" >&2; exit 1; }
    case "\${args[*]}" in
      *'{.data.hash}'*) cat "${PSTATE}/rec.hash" 2>/dev/null || true ;;
      *'{.data.manifest}'*) cat "${PSTATE}/rec.manifest" 2>/dev/null || true ;;
    esac
    exit 0 ;;
  "create configmap")
    [[ "\${STUB_FAIL_AT:-}" != record ]] || { echo "kubectl stub: record failed" >&2; exit 1; }
    for a in "\${args[@]}"; do
      case "\$a" in
        --from-literal=hash=*) printf '%s' "\${a#--from-literal=hash=}" > "${PSTATE}/rec.hash" ;;
        --from-file=manifest=*) cat "\${a#--from-file=manifest=}" > "${PSTATE}/rec.manifest" ;;
      esac
    done
    : > "${MARK}/ran.record"; exit 0 ;;
  "create -f")
    [[ "\${STUB_FAIL_AT:-}" != calico ]] || { echo "kubectl stub: calico failed" >&2; exit 1; }
    cp "\${args[2]}" "${MARK}/applied.calico" 2>/dev/null; : > "${MARK}/ran.calico-create"
    exit 0 ;;
  "create secret")
    [[ "\${STUB_FAIL_AT:-}" != secret ]] || { echo "kubectl stub: secret failed" >&2; exit 1; }
    printf '%s\n' "\${args[*]}" >> "${MARK}/secrets.calls"; echo "apiVersion: v1"; exit 0 ;;
  "create namespace") echo "apiVersion: v1"; exit 0 ;;
  "apply -f"|"wait --for=condition=Ready") cat >/dev/null 2>&1 || true; exit 0 ;;
  "get pods") echo "calico-node-x 1/1 Running"; exit 0 ;;
esac
echo "kubectl stub (P): unmodelled invocation: \$*" >&2
exit 91
EOF
cat > "${PBIN}/curl" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/curl.calls"
out=""; prev=""
for a in "\$@"; do [[ "\$prev" == "-o" ]] && out="\$a"; prev="\$a"; done
[[ -n "\$out" ]] || { echo "curl stub: no -o" >&2; exit 3; }
printf '%s\n' "\${STUB_CALICO_BYTES:-CALICO FIXTURE v1}" > "\$out"
EOF
cat > "${PBIN}/podman" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/podman.calls"
if [[ "\$1" == info ]]; then
  : > "${MARK}/ran.runtime-info"
  case "\${STUB_RUNTIME_INFO:-}" in
    fail) exit 1 ;;
    hang) sleep 30; exit 0 ;;
  esac
fi
if [[ "\$1" == images ]]; then
  [[ -z "\${STUB_NODE_IMAGE:-}" ]] || printf 'docker.io/library/busybox:latest\n%s\n' "\${STUB_NODE_IMAGE}"
fi
if [[ "\$1" == run ]]; then
  : > "${MARK}/ran.node-probe"
  case "\${STUB_NODE_RUN:-}" in
    fail) exit 126 ;;
    hang) sleep 30; exit 0 ;;
  esac
fi
exit 0
EOF
chmod +x "${PBIN}"/*

RWORK_P="${WORK}/prender"; mkdir -p "$RWORK_P"
# prov [args...]: run the copy's provision.sh END TO END. Sets P_RC / P_OUT. DT_CLUSTER_NAME
# defaults to `pcluster` (P_CLUSTER overrides); stub knobs are temporary assignments.
prov() {
  P_OUT="$(PATH="${PBIN}:${PATH}" TMPDIR="$RWORK_P" DT_CLUSTER_NAME="${P_CLUSTER-pcluster}" \
    bash "$PROV_COPY" "$@" 2>&1 </dev/null)"; P_RC=$?
}
# psrc '<snippet>' [args...]: src_run over the copy's provision.sh, same env as prov (stdin
# /dev/null, so AUTO_YES=true as for prov). Output passes through; the caller captures it.
psrc() {
  PATH="${PBIN}:${PATH}" TMPDIR="$RWORK_P" DT_CLUSTER_NAME="${P_CLUSTER-pcluster}" \
    src_run "$PROV_COPY" "$@" </dev/null
}
# psrc_rt '<snippet>' [args...]: psrc with the container runtime detected first (what main does
# before anything renders: blueprint_render needs KIND_EXPERIMENTAL_PROVIDER).
psrc_rt() { local snip="$1"; shift; psrc "detect_container_runtime >/dev/null; ${snip}" "$@"; }
# ptrap <fn> [args...]: run ONE provision step the way main does — the real EXIT trap, then
# `pstep <fn>` under the sourced script's set -e. P_CREATED=true models "this run created the
# cluster" (the trap then probes the apiserver). Sets P_RC / P_OUT.
ptrap() {
  P_OUT="$(psrc_rt 'trap __provision_exit_trap EXIT; PROVISION_CLUSTER_CREATED="${P_CREATED:-false}"; pstep "${ARGS[@]}"' "$@" 2>&1)"; P_RC=$?
}
# decide: the real blueprint_decide against the stubbed cluster. Sets P_RC / P_OUT, whose last
# line is `DECIDE REASON=<r> CHANGED=<c> RECORDED=<hash|none>`.
decide() {
  P_OUT="$(psrc_rt 'blueprint_decide; echo "DECIDE REASON=${BP_REASON} CHANGED=${BP_CHANGED} RECORDED=${BP_RECORDED:-none}"' 2>&1)"; P_RC=$?
}
decide_line() { grep -m1 '^DECIDE ' <<< "$P_OUT"; }
# seed_record: a cluster named P_CLUSTER (default pcluster) with the CURRENT tree's blueprint
# recorded, by the real blueprint_decide + record_blueprint; marks reset afterwards.
seed_record() {
  printf '%s\n' "${P_CLUSTER-pcluster}" > "$PSTATE/clusters"
  psrc_rt 'blueprint_decide; record_blueprint' >/dev/null 2>&1
  local rc=$?
  if [[ $rc -ne 0 || ! -s "$PSTATE/rec.hash" ]]; then
    FAIL=$((FAIL + 1)); FAILURES+=("[prov-seed-record] seeding a recorded blueprint failed (rc ${rc}) — every case built on it is unproven")
  fi
  reset_marks
}
# The rendered blueprint of the copy (stdout only). Sets P_RC / P_RENDER.
prender() { P_RENDER="$(psrc_rt 'blueprint_render' 2>/dev/null)"; P_RC=$?; }
# One fingerprint of the whole provision state (the cluster record AND the tree copy).
pstate_sum() {
  (cd "$WORK" && find pstate ptree -type f -print0 | sort -z | xargs -0 sha256sum) | sha256sum | cut -d' ' -f1
}
bp_line() { grep -m1 '^BLUEPRINT ' <<< "$P_OUT"; }
one_failed_line() {  # $1 = label: exactly ONE PROVISION_FAILED line in P_OUT
  assert_rc "$1" 1 "$(grep -c '^PROVISION_FAILED' <<< "$P_OUT")"
}

make_ptree; reset_marks
PRISTINE_STATE="$(pstate_sum)"

# --- (P1) fresh: no cluster -> build, record LAST; the decision line is emitted ------------
prov --yes
assert_rc "prov-fresh-rc" 0 "$P_RC"
assert_status "prov-fresh-decision-line" "BLUEPRINT ACTION=rebuild REASON=missing RECORDED=none CURRENT=" "$P_OUT"
assert_marker "prov-fresh-created" "$MARK" "ran.kind_create"
assert_no_marker "prov-fresh-nothing-to-delete" "$MARK" "ran.kind_delete"
assert_marker "prov-fresh-recorded" "$MARK" "ran.record"
assert_marker "prov-fresh-materialized-certs" "$MARK" "ran.recipe"
assert_rc "prov-fresh-record-is-sha256" 0 "$([[ "$(cat "$PSTATE/rec.hash" 2>/dev/null)" =~ ^[0-9a-f]{64}$ ]] && echo 0 || echo 1)"
# The record is written AFTER every build stage: it is the last kubectl call.
assert_status "prov-fresh-record-is-last-write" "create configmap devloop-blueprint" "$(tail -n1 "${MARK}/kubectl.calls" 2>/dev/null)"
# The AC DATABASE_URL is DERIVED from the Postgres Secret (T1), never a second literal.
# Positive control first: the secret stage was reached at all (a vacuous-pass guard).
assert_rc "prov-fresh-secrets-created" 0 "$([[ -s "${MARK}/secrets.calls" ]] && echo 0 || echo 1)"
pg_pw="$(awk '/^  POSTGRES_PASSWORD:/{print $2}' "$PTREE/infra/services/postgres/secret.yaml")"
assert_status "prov-ac-db-url-derived" "DATABASE_URL=postgresql://darktower:${pg_pw}@postgres" "$(cat "${MARK}/secrets.calls" 2>/dev/null)"
assert_absent "prov-ac-db-url-no-literal-in-script" "dev_password_change_in_production" "$(grep -v '^[[:space:]]*#' "${REPO_ROOT}/infra/kind/scripts/provision.sh")"
first_hash="$(cat "$PSTATE/rec.hash")"

# --- (P2) unchanged -> no-op (no delete, no create, no new record) ------------------------
reset_marks
prov --yes
assert_rc "prov-noop-rc" 0 "$P_RC"
assert_status "prov-noop-decision" "BLUEPRINT ACTION=none REASON=match RECORDED=${first_hash:0:12} CURRENT=${first_hash:0:12} CHANGED=-" "$P_OUT"
assert_no_marker "prov-noop-no-delete" "$MARK" "ran.kind_delete"
assert_no_marker "prov-noop-no-create" "$MARK" "ran.kind_create"
assert_no_marker "prov-noop-no-record" "$MARK" "ran.record"
assert_marker "prov-noop-still-exports-kubeconfig" "$MARK" "ran.kind_export"

# --- (P3, test d) determinism + sensitivity: each section class changed once ---------------
# Determinism compares the render (STDOUT) only: the old combined-stream compare included a
# timestamped log line and flaked whenever the two runs straddled a second.
prender; m1="$P_RENDER"
prender; m2="$P_RENDER"
assert_rc "prov-render-deterministic" 0 "$([[ -n "$m1" && "$m1" == "$m2" ]] && echo 0 || echo 1)"
assert_rc "prov-render-lists-every-input" 0 "$([[ "$(grep -c '^file:' <<< "$m1")" -eq 4 ]] && echo 0 || echo "1 ($(grep -c '^file:' <<< "$m1"))")"
# The render names public certs only — never a key file, never a line of any input — on ANY
# stream (stdout AND stderr).
m_all="$(psrc 'detect_container_runtime; blueprint_render' 2>&1)"; m_all_rc=$?
# Positive control: m_all IS a render (an error line alone would pass every absence below).
assert_rc "prov-render-all-streams-is-a-render" 0 "$([[ $m_all_rc -eq 0 && "$m_all" == *"dt-blueprint v1"* && "$(grep -c ' sha256:' <<< "$m_all")" -eq 7 ]] && echo 0 || echo "1 (rc=${m_all_rc})")"
assert_absent "prov-render-no-key-file" ".key" "$m_all"
assert_absent "prov-render-no-input-lines" "POSTGRES_PASSWORD" "$m_all"
# The secret VALUE (read from the tree copy, never typed), not only its key name.
pg_pw_needle="$(awk '/^  POSTGRES_PASSWORD:/{print $2}' "$PTREE/infra/services/postgres/secret.yaml")"
assert_rc "prov-render-password-needle-read" 0 "$([[ -n "$pg_pw_needle" ]] && echo 0 || echo "1 (no POSTGRES_PASSWORD read from the tree copy's secret.yaml)")"
assert_absent "prov-render-no-postgres-password" "${pg_pw_needle:-<unread-password-needle>}" "$m_all"
# One digest per labelled input, taken BY POSITION from one sha256sum call: every rendered
# digest must equal the per-file `sha256sum < file` of the file its label names (a
# misalignment would pass every other case here).
render_digests_aligned() {  # $1 = render; prints mismatches, empty when every digest is right
  local label val path
  # IFS: this file runs with IFS=$'\n\t'; a render line splits on its SPACE.
  while IFS=' ' read -r label val; do
    [[ "$val" == sha256:* ]] || continue
    case "$label" in
      kind-config) path="$PTREE/infra/kind/kind-config.yaml" ;;
      file:*) path="$PTREE/${label#file:}" ;;
      tls-cert:*) path="$PTREE/infra/docker/certs/${label#tls-cert:}" ;;
      *) echo "unknown-label:${label}"; continue ;;
    esac
    [[ "${val#sha256:}" == "$(sha256sum < "$path" | cut -d' ' -f1)" ]] || echo "misaligned:${label}"
  done <<< "$1"
}
mis="$(render_digests_aligned "$m1")"
assert_rc "prov-render-digest-per-label" 0 "$([[ -z "$mis" && "$(grep -c ' sha256:' <<< "$m1")" -eq 7 ]] && echo 0 || echo "1 (${mis//$'\n'/ } n=$(grep -c ' sha256:' <<< "$m1"))")"
# An ABSENT TLS cert still renders (`missing`, rc 0) and the digests after it stay aligned.
mv "$PTREE/infra/docker/certs/mc-webtransport.crt" "$WORK/mc.crt.aside"
prender
assert_rc "prov-render-absent-cert-rc" 0 "$P_RC"
assert_status "prov-render-absent-cert-missing-line" "tls-cert:mc-webtransport.crt missing" "$P_RENDER"
mis="$(render_digests_aligned "$P_RENDER")"
assert_rc "prov-render-absent-cert-still-aligned" 0 "$([[ -z "$mis" && "$(grep -c ' sha256:' <<< "$P_RENDER")" -eq 6 ]] && echo 0 || echo "1 (${mis})")"
mv "$WORK/mc.crt.aside" "$PTREE/infra/docker/certs/mc-webtransport.crt"
# An input that CANNOT be hashed fails the render — never an empty digest.
mv "$PTREE/infra/services/postgres/secret.yaml" "$WORK/secret.aside"
P_OUT="$(psrc_rt 'blueprint_render' 2>&1)"; P_RC=$?
assert_rc "prov-render-unhashable-input-fails" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-render-unhashable-input-names-file" "infra/services/postgres/secret.yaml" "$P_OUT"
assert_rc "prov-render-unhashable-no-empty-digest" 0 "$(grep -qE ' sha256:$' <<< "$P_OUT" && echo 1 || echo 0)"
# ...end to end: the build fails at the decision (the classified line), destroys and records
# nothing, and prints no decision line; --check fails too. A cluster with a record exists
# first, so a delete WOULD be possible.
mv "$WORK/secret.aside" "$PTREE/infra/services/postgres/secret.yaml"
seed_record
mv "$PTREE/infra/services/postgres/secret.yaml" "$WORK/secret.aside"
prov --yes
assert_rc "prov-unhashable-input-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-unhashable-input-classified" "PROVISION_FAILED REASON=step-failed STEP=blueprint_decide" "$P_OUT"
one_failed_line "prov-unhashable-input-one-line"
assert_absent "prov-unhashable-input-no-decision-line" "BLUEPRINT ACTION=" "$P_OUT"
assert_no_marker "prov-unhashable-input-no-delete" "$MARK" "ran.kind_delete"
assert_no_marker "prov-unhashable-input-no-create" "$MARK" "ran.kind_create"
assert_no_marker "prov-unhashable-input-no-record" "$MARK" "ran.record"
reset_marks; prov --check
assert_rc "prov-unhashable-input-check-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_absent "prov-unhashable-input-check-no-decision-line" "BLUEPRINT ACTION=" "$P_OUT"
mv "$WORK/secret.aside" "$PTREE/infra/services/postgres/secret.yaml"

# Sensitivity: each section class changed once, by the real blueprint_decide against a seeded
# record. Each case starts from a `match` positive control (a failed restore cannot bleed into
# the next case) and restores its mutation. That a CHANGED section rebuilds and deletes is
# main's section-independent mapping, asserted end to end by the kind-config case (P9 below).
make_ptree; reset_marks; seed_record
sens_direct() {  # $1=label $2=expected CHANGED= section; the mutation has been applied by the caller
  decide
  assert_status "prov-sensitive-${1}-changed" "DECIDE REASON=changed" "$(decide_line)"
  assert_rc "prov-sensitive-${1}-names-exactly-that-section" 0 "$([[ "$(decide_line)" == *" CHANGED=${2} RECORDED="* ]] && echo 0 || echo "1 ($(decide_line))")"
}
sens_baseline() {  # $1=label: the positive control — the tree matches the record before mutating
  decide
  assert_status "prov-sensitive-${1}-baseline-match" "DECIDE REASON=match CHANGED=-" "$(decide_line)"
}
sens_baseline kind-version
STUB_KIND_VERSION="kind v0.99.0" sens_direct kind-version "kind-version"
sens_baseline lib-file
cp "$PTREE/infra/kind/scripts/lib/common.sh" "$WORK/common.aside"
printf '# edit\n' >> "$PTREE/infra/kind/scripts/lib/common.sh"; sens_direct lib-file "file:infra/kind/scripts/lib/common.sh"
cp "$WORK/common.aside" "$PTREE/infra/kind/scripts/lib/common.sh"
sens_baseline tls-cert
printf 'MH PUBLIC CERT v2\n' > "$PTREE/infra/docker/certs/mh-webtransport.crt"; sens_direct tls-cert "tls-cert:mh-webtransport.crt"
printf 'MH PUBLIC CERT v1\n' > "$PTREE/infra/docker/certs/mh-webtransport.crt"
sens_baseline provider
# provider: detect_container_runtime prefers podman; with only docker on PATH it is docker.
mv "${PBIN}/podman" "${PBIN}/podman.off"; cp "${PBIN}/podman.off" "${PBIN}/docker"
sens_direct provider "provider"
rm -f "${PBIN}/docker"; mv "${PBIN}/podman.off" "${PBIN}/podman"
# THE POINT OF THE FILE SPLIT: a deploy.sh-only edit leaves the blueprint unchanged.
sens_baseline deploy-only-edit
printf '# edit\n' >> "$PTREE/infra/kind/scripts/deploy.sh"
decide
assert_status "prov-deploy-only-edit-matches" "DECIDE REASON=match CHANGED=-" "$(decide_line)"

# --- (P4, test e) --blueprint is PURE: no recipe run, no cluster write ----------------------
reset_marks; prov --blueprint
assert_rc "prov-blueprint-rc" 0 "$P_RC"
assert_no_marker "prov-blueprint-no-recipe" "$MARK" "ran.recipe"
assert_no_marker "prov-blueprint-no-renewal-probe" "$MARK" "ran.renewal-check"
assert_no_marker "prov-blueprint-no-kubectl" "$MARK" "ran.kubectl"
assert_no_marker "prov-blueprint-no-create" "$MARK" "ran.kind_create"

# --- (P5, test b) a failure at ANY build stage leaves NO record; the next run rebuilds ------
# Each stage failure runs END TO END (main's trap + one-line invariant + "no record"). The
# create stage also runs with a silent apiserver: a PRE-create step never reports
# apiserver-unreachable (there is no apiserver yet), and the trap never even asks.
declare -A STAGE_LINE=([create]="kind stub: create failed" [calico]="kubectl stub: calico failed" [secret]="kubectl stub: secret failed" [record]="kubectl stub: record failed")
declare -A STAGE_STATE=()
for stage in create calico secret record; do
  make_ptree; reset_marks
  if [[ "$stage" == create ]]; then STUB_READYZ=fail STUB_FAIL_AT="$stage" prov --yes; else STUB_FAIL_AT="$stage" prov --yes; fi
  assert_rc "prov-fail-${stage}-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
  # The injected stage was REACHED (not an earlier, unrelated failure).
  assert_status "prov-fail-${stage}-reached" "${STAGE_LINE[$stage]}" "$P_OUT"
  assert_absent "prov-fail-${stage}-no-unmodelled-call" "unmodelled" "$P_OUT"
  # Both probes answer (the stubs' defaults), so the stage failure is the tree's.
  case "$stage" in
    create) want_step=create_cluster ;; calico) want_step=install_calico ;;
    secret) want_step=create_ac_secrets ;; record) want_step=record_blueprint ;;
  esac
  assert_status "prov-fail-${stage}-classified-line" "PROVISION_FAILED REASON=step-failed STEP=${want_step}" "$P_OUT"
  one_failed_line "prov-fail-${stage}-one-line"
  assert_rc "prov-fail-${stage}-no-record" 1 "$([[ -s "$PSTATE/rec.hash" ]] && echo 0 || echo 1)"
  if [[ "$stage" == create ]]; then
    assert_absent "prov-classify-pre-create-no-apiserver-token" "apiserver-unreachable" "$P_OUT"
    assert_no_marker "prov-classify-pre-create-readyz-not-asked" "$MARK" "ran.readyz"
  else
    # After the cluster exists the trap DOES probe the apiserver (main set CLUSTER_CREATED).
    assert_marker "prov-fail-${stage}-post-create-readyz-asked" "$MARK" "ran.readyz"
  fi
  # The next run sees the half-built cluster as MISSING (never `match`)...
  decide
  assert_status "prov-fail-${stage}-next-run-sees-missing" "DECIDE REASON=missing" "$(decide_line)"
  STAGE_STATE[$stage]="$(pstate_sum)"
done
# ...and the state each failure leaves is ONE of two shapes, so one recovery run each proves
# "the next run recovers" for all four. create: nothing was made (== the pristine tree, which
# P1 builds from). calico/secret/record: the cluster exists with no record — asserted
# identical, with a positive control that the state really is "listed, unrecorded".
assert_rc "prov-fail-create-state-equals-pristine" 0 "$([[ "${STAGE_STATE[create]}" == "$PRISTINE_STATE" ]] && echo 0 || echo 1)"
assert_rc "prov-fail-calico-state-equals-record-failure-state" 0 "$([[ "${STAGE_STATE[calico]}" == "${STAGE_STATE[record]}" ]] && echo 0 || echo 1)"
assert_rc "prov-fail-secret-state-equals-record-failure-state" 0 "$([[ "${STAGE_STATE[secret]}" == "${STAGE_STATE[record]}" ]] && echo 0 || echo 1)"
assert_rc "prov-fail-record-state-is-listed-unrecorded" 0 "$([[ "$(cat "$PSTATE/clusters" 2>/dev/null)" == pcluster && ! -e "$PSTATE/rec.hash" ]] && echo 0 || echo 1)"
# (P8) a MISSING record on an existing cluster (half-built / pre-ADR-0038) rebuilds — this IS
# the post-record-failure state, so its recovery run is that case.
reset_marks; prov --yes
assert_rc "prov-fail-record-next-run-recovers" 0 "$P_RC"
assert_status "prov-no-record-rebuilds" "ACTION=rebuild REASON=missing RECORDED=none" "$(bp_line)"
assert_marker "prov-no-record-deletes" "$MARK" "ran.kind_delete"
assert_marker "prov-fail-record-next-run-records" "$MARK" "ran.record"
# An UNPARSEABLE record is missing too — rebuild, never skip.
printf 'not-a-hash' > "$PSTATE/rec.hash"; decide
assert_status "prov-garbage-record-is-missing" "DECIDE REASON=missing" "$(decide_line)"

# --- (P6, test c) a tree edit DURING the build fails loudly and records nothing ------------
make_ptree; reset_marks
STUB_MUTATE_ON_CREATE="infra/kind/scripts/lib/common.sh" prov --yes
assert_rc "prov-midbuild-edit-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-midbuild-edit-says-so" "changed DURING the build" "$P_OUT"
assert_no_marker "prov-midbuild-edit-no-record" "$MARK" "ran.record"

# --- (P6b, SEC-1) the Calico manifest is VERIFIED before it is applied with cluster-admin --
# install_calico directly, under the trap. (A failed calico stage leaving no record is P5.)
make_ptree; reset_marks
P_CREATED=true STUB_CALICO_BYTES="CALICO TAMPERED" ptrap install_calico
assert_rc "prov-calico-tampered-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-calico-tampered-says-so" "Calico manifest integrity check FAILED" "$P_OUT"
assert_no_marker "prov-calico-tampered-not-applied" "$MARK" "ran.calico-create"
reset_marks
P_CREATED=true ptrap install_calico
assert_rc "prov-calico-verified-rc" 0 "$P_RC"
assert_marker "prov-calico-verified-applied" "$MARK" "ran.calico-create"
assert_status "prov-calico-applied-the-verified-bytes" "CALICO FIXTURE v1" "$(cat "${MARK}/applied.calico" 2>/dev/null)"
assert_status "prov-calico-https-only" "--proto =https" "$(cat "${MARK}/curl.calls" 2>/dev/null)"
# The real pin is a 64-hex sha256 in provision.sh itself (so it is part of the blueprint).
assert_rc "prov-calico-real-pin-shape" 0 "$(grep -qE '^CALICO_MANIFEST_SHA256="[0-9a-f]{64}"$' "${REPO_ROOT}/infra/kind/scripts/provision.sh" && echo 0 || echo 1)"

# --- (P6c, T9) provision failures are classified at the source by the SHARED bounded probes -
# (lib/common.sh:classify_env_failure), through the real trap. A POST-create step with a
# silent apiserver is the environment; the PRE-create case is in P5 (create stage).
make_ptree; reset_marks
P_CREATED=true STUB_FAIL_AT=calico STUB_READYZ=fail ptrap install_calico
assert_rc "prov-classify-post-create-apiserver-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-classify-post-create-apiserver" "PROVISION_FAILED REASON=apiserver-unreachable STEP=install_calico" "$P_OUT"
one_failed_line "prov-classify-post-create-apiserver-one-line"
assert_marker "prov-classify-post-create-readyz-reached" "$MARK" "ran.readyz"
reset_marks
STUB_FAIL_AT=create STUB_RUNTIME_INFO=fail ptrap create_cluster
assert_rc "prov-classify-runtime-unreachable-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-classify-runtime-unreachable" "PROVISION_FAILED REASON=runtime-unreachable STEP=create_cluster" "$P_OUT"
one_failed_line "prov-classify-runtime-unreachable-one-line"
assert_marker "prov-classify-runtime-info-reached" "$MARK" "ran.runtime-info"
# HANGS: each probe is bounded (DT_ENV_PROBE_TIMEOUT=1 against a 30s stub).
for probe in runtime readyz; do
  reset_marks
  t0=$SECONDS
  if [[ "$probe" == runtime ]]; then
    P_CREATED=true DT_ENV_PROBE_TIMEOUT=1 STUB_FAIL_AT=calico STUB_RUNTIME_INFO=hang ptrap install_calico; want=runtime-unreachable
  else
    P_CREATED=true DT_ENV_PROBE_TIMEOUT=1 STUB_FAIL_AT=calico STUB_READYZ=hang ptrap install_calico; want=apiserver-unreachable
  fi
  took=$(( SECONDS - t0 ))
  assert_rc "prov-classify-${probe}-hang-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
  assert_status "prov-classify-${probe}-hang-token" "PROVISION_FAILED REASON=${want} STEP=install_calico" "$P_OUT"
  one_failed_line "prov-classify-${probe}-hang-one-line"
  assert_rc "prov-classify-${probe}-hang-bounded" 0 "$([[ "$took" -lt 15 ]] && echo 0 || echo "1 (${took}s)")"
done
# (P7, test a) an UNREADABLE record never destroys — directly classified, end to end.
make_ptree; seed_record
STUB_RECORD_READ_FAIL=1 prov --yes
assert_rc "prov-unreadable-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-unreadable-token" "BLUEPRINT ACTION=refuse REASON=unreadable" "$(bp_line)"
assert_status "prov-unreadable-classified" "PROVISION_FAILED REASON=blueprint-unreadable" "$P_OUT"
one_failed_line "prov-unreadable-one-line"
assert_marker "prov-unreadable-kubectl-reached" "$MARK" "ran.kubectl"
assert_no_marker "prov-unreadable-no-delete" "$MARK" "ran.kind_delete"
assert_no_marker "prov-unreadable-no-create" "$MARK" "ran.kind_create"
# `kind get clusters` failing is ALSO unreadable, never "missing" (which would rebuild); main's
# unreadable -> refuse mapping is the case just above.
STUB_KIND_LIST_FAIL=1 decide
assert_status "prov-kind-list-fail-is-unreadable" "DECIDE REASON=unreadable" "$(decide_line)"
# (P9, T4) a non-interactive destroy of an UNNAMED cluster refuses (operator-declined).
make_ptree; printf 'dark-tower\n' > "$PSTATE/clusters"; reset_marks
P_OUT="$(PATH="${PBIN}:${PATH}" TMPDIR="$RWORK_P" env -u DT_CLUSTER_NAME bash "$PROV_COPY" --yes 2>&1 </dev/null)"; P_RC=$?
assert_rc "prov-unnamed-destroy-refused-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
assert_status "prov-unnamed-destroy-operator-declined" "PROVISION_FAILED REASON=operator-declined" "$P_OUT"
assert_status "prov-unnamed-destroy-refused-says-why" "never falls back to the default name" "$P_OUT"
assert_no_marker "prov-unnamed-destroy-no-delete" "$MARK" "ran.kind_delete"
# ...but CREATING the default cluster (nothing to destroy) still works for the host.
make_ptree; reset_marks
P_OUT="$(PATH="${PBIN}:${PATH}" TMPDIR="$RWORK_P" env -u DT_CLUSTER_NAME bash "$PROV_COPY" --yes 2>&1 </dev/null)"; P_RC=$?
assert_rc "prov-unnamed-create-ok" 0 "$P_RC"

# port-held: a real listener on a host port the kind config maps → the environment, before
# `kind create`. listen <tcp|udp> <addr>: binds <addr>:0 (the kernel picks a free port — no
# pick-then-rebind race), reports the port through a file, and is polled for readiness with a
# BOUND; on timeout it FAILS loudly and returns 1 — a case never runs without its listener.
# Sets LISTENER (pid) and LPORT.
listen() {
  local pf="${WORK}/listener.port" i
  rm -f "$pf" "$pf.tmp"
  python3 -c '
import socket, sys, os, time
proto, addr, pf = sys.argv[1], sys.argv[2], sys.argv[3]
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM if proto == "tcp" else socket.SOCK_DGRAM)
s.bind((addr, 0))
if proto == "tcp":
    s.listen(4)
with open(pf + ".tmp", "w") as f:
    f.write(str(s.getsockname()[1]))
os.rename(pf + ".tmp", pf)
time.sleep(20)' "$1" "$2" "$pf" &
  LISTENER=$!
  for ((i = 0; i < 100; i++)); do [[ -s "$pf" ]] && break; sleep 0.05; done
  if [[ ! -s "$pf" ]]; then
    kill "$LISTENER" 2>/dev/null; wait "$LISTENER" 2>/dev/null
    FAIL=$((FAIL + 1)); FAILURES+=("[prov-listener-${1}-${2}] the ${1} listener on ${2} did not come up within 5s — the port-held cases it serves did NOT run; this is a FAIL, not a skip")
    return 1
  fi
  LPORT="$(cat "$pf")"
}
unlisten() { kill "$LISTENER" 2>/dev/null; wait "$LISTENER" 2>/dev/null; }
port_config() {  # $1=TCP|UDP $2=port: the copy's kind config maps exactly that host port on 127.0.0.1
  printf 'kind: Cluster\nnodes:\n  - role: control-plane\n    extraPortMappings:\n      - containerPort: 30082\n        hostPort: %s\n        listenAddress: "127.0.0.1"\n        protocol: %s\n' "$2" "$1" > "$PTREE/infra/kind/kind-config.yaml"
}
# TCP, END TO END: held -> port-held, and main never reaches `kind create`.
make_ptree; reset_marks
if listen tcp 127.0.0.1; then
  port_config TCP "$LPORT"
  prov --yes
  unlisten
  assert_status "prov-port-held-classified" "PROVISION_FAILED REASON=port-held STEP=check_host_ports" "$P_OUT"
  assert_status "prov-port-held-names-port" "127.0.0.1:${LPORT}" "$P_OUT"
  assert_no_marker "prov-port-held-never-creates" "$MARK" "ran.kind_create"
  # ...and the same config with the port free passes the check (positive control).
  reset_marks; ptrap check_host_ports
  assert_rc "prov-port-free-passes-check" 0 "$P_RC"
  assert_absent "prov-port-free-no-failed-line" "PROVISION_FAILED" "$P_OUT"
fi
# UDP host ports, directly: read from the bound-socket table (no connect to observe). A real
# UDP socket on the mapped port is held; one on a NON-overlapping address is not. (That a
# held port stops main before `kind create` is protocol-independent: the TCP case above.)
if listen udp 127.0.0.1; then
  port_config UDP "$LPORT"; reset_marks; ptrap check_host_ports; unlisten
  assert_rc "prov-udp-port-held-rc" 1 "$([[ "$P_RC" -ne 0 ]] && echo 1 || echo 0)"
  assert_status "prov-udp-port-held-classified" "PROVISION_FAILED REASON=port-held STEP=check_host_ports" "$P_OUT"
  assert_status "prov-udp-port-held-names-port" "127.0.0.1:${LPORT}/udp" "$P_OUT"
  one_failed_line "prov-udp-port-held-one-line"
fi
if listen udp 0.0.0.0; then
  port_config UDP "$LPORT"; reset_marks; ptrap check_host_ports; unlisten
  assert_status "prov-udp-wildcard-holder-overlaps" "PROVISION_FAILED REASON=port-held STEP=check_host_ports" "$P_OUT"
fi
if listen udp 127.0.0.2; then
  port_config UDP "$LPORT"; reset_marks; ptrap check_host_ports; unlisten
  assert_absent "prov-udp-other-address-not-held" "port-held" "$P_OUT"
  # (also the positive control: the check lets a non-conflicting config through)
  assert_rc "prov-udp-other-address-passes-check" 0 "$P_RC"
fi

# udp_port_bound over tables that are partly ABSENT (an IPv4-only host has no udp6; mawk
# aborts on an unopenable file, which would read as "not held").
UDP_T="${WORK}/udp-tables"; mkdir -p "$UDP_T"
printf '  sl  local_address rem_address   st\n   0: 0100007F:1F90 00000000:0000 07\n' > "$UDP_T/udp"
udp_bound() {
  T4="$UDP_T/udp" T6="$UDP_T/udp6-absent" psrc 'UDP_SOCKET_TABLES=("$T4" "$T6"); udp_port_bound "${ARGS[@]}"' "$1" "$2"
}
udp_bound 127.0.0.1 8080; assert_rc "udp-table-missing-udp6-still-held" 0 "$?"
udp_bound 127.0.0.2 8080; assert_rc "udp-table-other-address-not-held" 1 "$?"
udp_bound 0.0.0.0 8080;   assert_rc "udp-table-wildcard-want-held" 0 "$?"
udp_bound 127.0.0.1 8081; assert_rc "udp-table-other-port-not-held" 1 "$?"

# runtime-incapable (docs/TODO.md §G): `kind create` fails and the SAME container shape on
# the node image fails too -> the host. The probe keyed on exit status; every other outcome
# stays `step-failed` (the tree's lane — the conservative direction). END TO END once (the
# specific line + the trap's one-line invariant through main); the other outcomes directly.
NODE_IMG="docker.io/kindest/node:v1.31.0"
make_ptree; reset_marks
STUB_FAIL_AT=create STUB_NODE_IMAGE="$NODE_IMG" STUB_NODE_RUN=fail prov --yes
assert_status "prov-runtime-incapable-classified" "PROVISION_FAILED REASON=runtime-incapable STEP=create_cluster" "$P_OUT"
one_failed_line "prov-runtime-incapable-one-line"
assert_status "prov-runtime-incapable-probe-shape" "run --rm --pull=never --privileged --hostname dt-node-probe --entrypoint /bin/true ${NODE_IMG}" "$(cat "$MARK/podman.calls")"
reset_marks
STUB_FAIL_AT=create STUB_NODE_IMAGE="$NODE_IMG" ptrap create_cluster
assert_status "prov-node-probe-passes-stays-tree" "PROVISION_FAILED REASON=step-failed STEP=create_cluster" "$P_OUT"
one_failed_line "prov-node-probe-passes-one-line"
assert_marker "prov-node-probe-passes-reached" "$MARK" "ran.node-probe"
reset_marks
STUB_FAIL_AT=create STUB_NODE_RUN=fail ptrap create_cluster
assert_status "prov-node-probe-no-image-stays-tree" "PROVISION_FAILED REASON=step-failed STEP=create_cluster" "$P_OUT"
one_failed_line "prov-node-probe-no-image-one-line"
assert_status "prov-node-probe-no-image-warns" "No local kind node image" "$P_OUT"
assert_no_marker "prov-node-probe-no-image-not-run" "$MARK" "ran.node-probe"
reset_marks; t0=$SECONDS
DT_ENV_PROBE_TIMEOUT=1 STUB_FAIL_AT=create STUB_NODE_IMAGE="$NODE_IMG" STUB_NODE_RUN=hang ptrap create_cluster
took=$(( SECONDS - t0 ))
assert_status "prov-node-probe-hang-not-incapable" "PROVISION_FAILED REASON=step-failed STEP=create_cluster" "$P_OUT"
one_failed_line "prov-node-probe-hang-one-line"
assert_rc "prov-node-probe-hang-bounded" 0 "$([[ "$took" -lt 15 ]] && echo 0 || echo "1 (${took}s)")"
# A kind create that SUCCEEDS never runs the probe.
make_ptree; reset_marks
STUB_NODE_IMAGE="$NODE_IMG" STUB_NODE_RUN=fail ptrap create_cluster
assert_rc "prov-node-probe-create-ok-rc" 0 "$P_RC"
assert_no_marker "prov-node-probe-only-on-create-failure" "$MARK" "ran.node-probe"
# No `kind` on the host: a PATH of ONLY the plain tools (no stubs, no host kind/kubectl/
# runtime), so a real host install cannot leak in and the case always runs.
PNOKIND_BIN="${WORK}/pbin-nokind"; rm -rf "$PNOKIND_BIN"; mkdir -p "$PNOKIND_BIN"
for t in bash awk grep sed sort mktemp cat cut head tail tr sha256sum date dirname basename rm cp mv env realpath wc sleep tee uname ls mkdir chmod timeout; do
  ln -sf "$(command -v "$t")" "$PNOKIND_BIN/$t"
done
make_ptree; reset_marks
P_OUT="$(PATH="${PNOKIND_BIN}" TMPDIR="$RWORK_P" DT_CLUSTER_NAME=pcluster "$PNOKIND_BIN/bash" "$PROV_COPY" --yes 2>&1 </dev/null)"; P_RC=$?
assert_rc "prov-prerequisite-missing-rc" 1 "$P_RC"
assert_status "prov-prerequisite-missing-token" "PROVISION_FAILED REASON=prerequisite-missing STEP=provision_check_prerequisites" "$P_OUT"
one_failed_line "prov-prerequisite-missing-one-line"
# ...and no `timeout` is a named prerequisite for provision too (stubs present, timeout absent).
PNOTO_BIN="${WORK}/pbin-notimeout"; rm -rf "$PNOTO_BIN"; mkdir -p "$PNOTO_BIN"
cp "${PBIN}/kind" "${PBIN}/kubectl" "${PBIN}/podman" "${PBIN}/curl" "$PNOTO_BIN/"
for t in bash awk grep sed sort mktemp cat cut head tail tr sha256sum date dirname basename rm cp mv env realpath wc sleep tee uname ls mkdir chmod; do
  ln -sf "$(command -v "$t")" "$PNOTO_BIN/$t"
done
make_ptree; reset_marks
P_OUT="$(PATH="${PNOTO_BIN}" TMPDIR="$RWORK_P" DT_CLUSTER_NAME=pcluster "$PNOTO_BIN/bash" "$PROV_COPY" --yes 2>&1 </dev/null)"; P_RC=$?
assert_status "prov-no-timeout-prerequisite-missing" "PROVISION_FAILED REASON=prerequisite-missing STEP=provision_check_prerequisites" "$P_OUT"
assert_status "prov-no-timeout-names-it" "timeout is not installed" "$P_OUT"
assert_absent "prov-no-timeout-never-runtime-unreachable" "runtime-unreachable" "$P_OUT"

# --- (P9, T4) a CHANGED blueprint rebuilds and destroys EXACTLY DT_CLUSTER_NAME, once --------
# END TO END (main's section-independent changed -> rebuild + delete mapping; the per-section
# sensitivity is P3's direct cases).
make_ptree; P_CLUSTER=named-cluster; seed_record
printf '# edit\n' >> "$PTREE/infra/kind/kind-config.yaml"; reset_marks; prov --yes
assert_status "prov-sensitive-kind-config-rebuilds" "ACTION=rebuild REASON=changed" "$(bp_line)"
assert_rc "prov-sensitive-kind-config-names-exactly-that-section" 0 "$([[ "$(bp_line)" == *" CHANGED=kind-config" ]] && echo 0 || echo "1 ($(bp_line))")"
assert_marker "prov-sensitive-kind-config-deleted" "$MARK" "ran.kind_delete"
assert_status "prov-destroy-exact-name" "delete cluster --name named-cluster" "$(cat "${MARK}/kind_delete.calls" 2>/dev/null)"
assert_rc "prov-destroy-exact-name-only-once" 1 "$(wc -l < "${MARK}/kind_delete.calls" 2>/dev/null)"
unset P_CLUSTER

# --- (P10, test a) interactive `N` never destroys ---------------------------------------------
make_ptree; seed_record
printf '# edit\n' >> "$PTREE/infra/kind/kind-config.yaml"; reset_marks
P_OUT="$(PATH="${PBIN}:${PATH}" TMPDIR="$RWORK_P" DT_CLUSTER_NAME=pcluster \
  script -qec "bash '$PROV_COPY'" /dev/null <<< "n" 2>&1)"; P_RC=$?
if command -v script >/dev/null 2>&1; then
  assert_status "prov-tty-no-refuses" "Not rebuilding" "$P_OUT"
  assert_status "prov-tty-no-operator-declined" "PROVISION_FAILED REASON=operator-declined" "$P_OUT"
  assert_marker "prov-tty-no-kubectl-reached" "$MARK" "ran.kubectl"
  assert_no_marker "prov-tty-no-no-delete" "$MARK" "ran.kind_delete"
else
  FAIL=$((FAIL + 1)); FAILURES+=("[prov-tty-no] util-linux 'script' is not on PATH — the interactive-N case cannot run; this is a FAIL, not a skip")
fi

# --- (P11) TLS renewal: one cause, one CHANGED= token, on BOTH paths ------------------------
make_ptree; seed_record
STUB_RENEWAL_RC=10 prov --check
assert_rc "prov-check-renewal-due-rc" 1 "$P_RC"
for m in ran.kind_create ran.kind_delete ran.record ran.recipe ran.calico-create; do
  assert_no_marker "prov-check-renewal-due-never-writes-${m}" "$MARK" "$m"
done
assert_status "prov-check-renewal-due" "BLUEPRINT ACTION=check REASON=changed" "$(bp_line)"
assert_status "prov-check-renewal-due-token" "tls-renewal-due" "$(bp_line)"
reset_marks
STUB_RENEWAL_RC=10 STUB_RENEW_WRITES=1 prov --yes
assert_status "prov-real-renewal-token" "tls-renewal-due" "$(bp_line)"
assert_status "prov-real-renewal-names-cert" "tls-cert:mc-webtransport.crt" "$(bp_line)"
assert_status "prov-real-renewal-rebuilds" "ACTION=rebuild" "$(bp_line)"

# --- (P12) --check: match rc 0, missing/stale rc 1, never a write ---------------------------
make_ptree; reset_marks; prov --check
assert_rc "prov-check-missing-rc" 1 "$P_RC"
assert_status "prov-check-missing" "BLUEPRINT ACTION=check REASON=missing" "$(bp_line)"
for m in ran.kind_create ran.kind_delete ran.record ran.recipe ran.calico-create; do
  assert_no_marker "prov-check-missing-never-writes-${m}" "$MARK" "$m"
done
seed_record; printf '# edit\n' >> "$PTREE/infra/kind/kind-config.yaml"; reset_marks; prov --check
assert_rc "prov-check-stale-rc" 1 "$P_RC"
assert_status "prov-check-stale" "BLUEPRINT ACTION=check REASON=changed" "$(bp_line)"
for m in ran.kind_create ran.kind_delete ran.record ran.recipe ran.calico-create; do
  assert_no_marker "prov-check-stale-never-writes-${m}" "$MARK" "$m"
done
seed_record; prov --check
assert_rc "prov-check-match-rc" 0 "$P_RC"
assert_status "prov-check-match" "BLUEPRINT ACTION=check REASON=match" "$(bp_line)"
assert_no_marker "prov-check-never-creates" "$MARK" "ran.kind_create"
assert_no_marker "prov-check-never-records" "$MARK" "ran.record"
assert_no_marker "prov-check-never-materializes" "$MARK" "ran.recipe"

# --- (P13, test g) provision reaches NO file except through provision_input -----------------
# Every line that sources/executes/reads a path must go through provision_input (or be one of
# the tagged accessor lines / the two root definitions). Proven able to go red on a fixture.
provision_reach_violations() {
  grep -nE '(^|[;&|[:space:]("])(source|\.)[[:space:]]+[^[:space:]]|(^|[;&|[:space:]("])(bash|sh)[[:space:]]+[^-[:space:]]|\$\{?(PROJECT_ROOT|SCRIPT_DIR)\}?/|\$\(dirname|(^|[[:space:]"(])\./[A-Za-z]' "$1" \
    | grep -vE '^[0-9]+:[[:space:]]*#' \
    | grep -vE 'provision_input|# provision-input-accessor|^[0-9]+:(SCRIPT_DIR|PROJECT_ROOT)=' || true
}
viol="$(provision_reach_violations "${REPO_ROOT}/infra/kind/scripts/provision.sh")"
assert_rc "prov-inputs-only-through-accessor" 0 "$([[ -z "$viol" ]] && echo 0 || echo "1 (${viol})")"
PFIX="${WORK}/provision.fixture.sh"
{ cat "${REPO_ROOT}/infra/kind/scripts/provision.sh"; echo 'source "${SCRIPT_DIR}/unlisted.sh"'; } > "$PFIX"
assert_rc "prov-inputs-pin-can-go-red" 0 "$([[ -n "$(provision_reach_violations "$PFIX")" ]] && echo 0 || echo 1)"
{ cat "${REPO_ROOT}/infra/kind/scripts/provision.sh"; echo 'bash ./helper.sh'; } > "$PFIX"
assert_rc "prov-inputs-pin-catches-bash-exec" 0 "$([[ -n "$(provision_reach_violations "$PFIX")" ]] && echo 0 || echo 1)"
# Positive control: the array is non-empty and every entry exists in the tree.
inputs="$(bash -c 'src="$1"; eval "$(grep -E "^(PROVISION_SH_REL|LIB_COMMON_REL|CERTS_RECIPE_REL|PG_SECRET_REL|PROVISION_INPUTS)=" "$src")"; printf "%s\n" "${PROVISION_INPUTS[@]}"' _ "${REPO_ROOT}/infra/kind/scripts/provision.sh")"
n_inputs="$(grep -c . <<< "$inputs")"
missing_inputs="$(while IFS= read -r f; do [[ -f "${REPO_ROOT}/${f}" ]] || echo "$f"; done <<< "$inputs")"
assert_rc "prov-inputs-nonvacuous-and-exist" 0 "$([[ "$n_inputs" -ge 1 && -z "$missing_inputs" ]] && echo 0 || echo "1 (n=${n_inputs} missing=${missing_inputs})")"
# Every entry but provision.sh itself (hashed as the running implementation) is CONSUMED
# through provision_input — an array entry nothing reads is a dead input, not a pin.
PROV_SRC="${REPO_ROOT}/infra/kind/scripts/provision.sh"
input_vars="$(grep -m1 -E '^PROVISION_INPUTS=\(' "$PROV_SRC" | grep -oE '\$\{[A-Z_]+\}' | tr -d '${}' | grep -vx PROVISION_SH_REL)"
unconsumed="$(while IFS= read -r v; do grep -qF "provision_input \"\${${v}}\"" "$PROV_SRC" || echo "$v"; done <<< "$input_vars")"
assert_rc "prov-inputs-every-entry-consumed" 0 "$([[ -n "$input_vars" && -z "$unconsumed" ]] && echo 0 || echo "1 (vars=${input_vars//$'\n'/ } unconsumed=${unconsumed})")"
# The `# provision-input-accessor` tag is NOT a free-form suppression: exactly the three
# accessor lines carry it (provision_input's printf, KIND_CONFIG, tls_file's printf), each of
# which is rendered as its own blueprint section.
tagged_ok() {  # $1 = file; rc 0 iff the tagged lines are exactly the known three
  local t
  t="$(grep -F '# provision-input-accessor' "$1" | grep -vE '^[[:space:]]*#')"
  [[ "$(grep -c . <<< "$t")" -eq 3 ]] \
    && grep -qE "printf '%s/%s\\\\n' \"\\\$\{PROJECT_ROOT\}\" \"\\\$\{rel\}\"" <<< "$t" \
    && grep -qE '^KIND_CONFIG=' <<< "$t" \
    && grep -qF 'infra/docker/certs/%s.%s' <<< "$t"
}
assert_rc "prov-accessor-tag-exactly-known-lines" 0 "$(tagged_ok "$PROV_SRC" && echo 0 || echo 1)"
{ cat "$PROV_SRC"; echo 'extra="$(cat "${PROJECT_ROOT}/infra/other.yaml")"   # provision-input-accessor'; } > "$PFIX"
assert_rc "prov-accessor-tag-cannot-launder-a-read" 1 "$(tagged_ok "$PFIX" && echo 0 || echo 1)"
# ONE render: exactly one definition, and check/build/deploy all go through it.
assert_rc "prov-one-render-definition" 0 "$([[ "$(grep -c '^blueprint_render()' "${REPO_ROOT}/infra/kind/scripts/provision.sh")" -eq 1 && "$(grep -rl '^blueprint_render()' "${REPO_ROOT}/infra" "${REPO_ROOT}/scripts" 2>/dev/null | grep -vc 'setup.test.sh')" -eq 1 ]] && echo 0 || echo 1)"
assert_status "deploy-guard-uses-provision-check" '"${PROVISION_SH}" --check' "$(cat "${REPO_ROOT}/infra/kind/scripts/deploy.sh")"

# --- (P14) the recipe's --check-renewal is READ-ONLY and uses the one predicate ------------
CR="${WORK}/certrepo"; rm -rf "$CR"; mkdir -p "$CR/scripts"; cp "${REPO_ROOT}/scripts/generate-dev-certs.sh" "$CR/scripts/"
bash "$CR/scripts/generate-dev-certs.sh" --check-renewal >/dev/null 2>&1; cr_rc=$?
assert_rc "certs-check-renewal-due-when-none" 10 "$cr_rc"
assert_rc "certs-check-renewal-writes-nothing" 1 "$([[ -e "$CR/infra/docker/certs" ]] && echo 0 || echo 1)"
if command -v openssl >/dev/null 2>&1; then
  bash "$CR/scripts/generate-dev-certs.sh" >/dev/null 2>&1
  bash "$CR/scripts/generate-dev-certs.sh" --check-renewal >/dev/null 2>&1; cr_rc=$?
  assert_rc "certs-check-renewal-fresh-after-mint" 0 "$cr_rc"
else
  FAIL=$((FAIL + 1)); FAILURES+=("[certs-check-renewal-fresh] openssl not on PATH — cannot mint certs; FAIL, not skip")
fi

# --- (P15) lib/common.sh helpers every Kind script runs ---------------------------------------
# cluster_exists: an exact-LINE match (it decides which cluster a destroy targets). A near-miss
# name must not match, the name is literal (never a glob), the last line matches without a
# trailing newline, and a failed listing is rc 2 — never "absent".
CE_BIN="${WORK}/cebin"; mkdir -p "$CE_BIN"
cat > "${CE_BIN}/kind" <<'STUB'
#!/usr/bin/env bash
[[ -z "${STUB_KIND_LIST_FAIL:-}" ]] || exit 1
printf '%b' "${STUB_CLUSTERS:-}"
STUB
chmod +x "${CE_BIN}/kind"
ce() {  # $1 = listing (printf %b), $2 = CLUSTER_NAME; prints cluster_exists's rc
  local rc=0
  PATH="${CE_BIN}:${PATH}" STUB_CLUSTERS="$1" src_run "$LIB_COMMON" 'CLUSTER_NAME="${ARGS[0]}"; rc=0; cluster_exists || rc=$?; echo "$rc"' "$2" 2>/dev/null || rc=$?
  (( rc == 0 )) || echo "harness-rc-${rc}"
}
assert_rc "cluster-exists-exact" 0 "$(ce 'pcluster\n' pcluster)"
assert_rc "cluster-exists-last-line-no-newline" 0 "$(ce 'other\npcluster' pcluster)"
assert_rc "cluster-exists-near-miss-suffix" 1 "$(ce 'pcluster-2\n' pcluster)"
assert_rc "cluster-exists-near-miss-prefix" 1 "$(ce 'xpcluster\n' pcluster)"
assert_rc "cluster-exists-substring-of-a-line" 1 "$(ce 'a pcluster b\n' pcluster)"
assert_rc "cluster-exists-name-is-literal-not-glob" 1 "$(ce 'pcluster\n' 'p*')"
assert_rc "cluster-exists-empty-listing" 1 "$(ce '' pcluster)"
assert_rc "cluster-exists-list-failure-is-2" 2 "$(STUB_KIND_LIST_FAIL=1 ce 'pcluster\n' pcluster)"
# ...and it is the ONE existence check: every executed `kind get clusters` under infra/ is
# cluster_exists's own, plus devloop.sh's orphan scan (a different query: every cluster of this
# project, by prefix; its listing failure is checked on its own). An inline
# `kind get clusters | grep -q "^name$"` reads a failed listing as "absent". The needle is the
# command wherever it is NOT quoted — with or without a stderr redirect; log messages quote it
# ('kind get clusters' failed). One entry per occurrence, so a second copy in either file shows.
kgc_sites_in() {  # $@ = files or dirs; prints `<path>:` per unquoted, uncommented occurrence
  grep -rnE "(^|[^'])kind get clusters" "$@" --include='*.sh' | grep -v '\.test\.sh:' \
    | grep -vE '^[^:]+:[0-9]+:[[:space:]]*#' | sed -E "s#^${REPO_ROOT}/##; s#^${WORK}/##; s#:[0-9]+:.*#:#" | sort | tr '\n' ' '
}
kgc_sites="$(kgc_sites_in "${REPO_ROOT}/infra")"
assert_rc "cluster-exists-is-the-one-existence-check" 0 "$([[ "$kgc_sites" == "infra/devloop/devloop.sh: infra/kind/scripts/lib/common.sh: " ]] && echo 0 || echo "1 (${kgc_sites})")"
# Positive control: the needle catches the redirect-free inline copy, and not the quoted message.
KGC_FIX="${WORK}/kgc-fixture"; mkdir -p "$KGC_FIX"
cp "${REPO_ROOT}/infra/kind/scripts/teardown.sh" "$KGC_FIX/teardown.sh"
printf '%s\n' 'if kind get clusters | grep -q "^x$"; then :; fi' >> "$KGC_FIX/teardown.sh"
assert_rc "cluster-exists-guard-catches-inline-copy" 0 "$([[ "$(kgc_sites_in "$KGC_FIX")" == "kgc-fixture/teardown.sh: " ]] && echo 0 || echo "1 ($(kgc_sites_in "$KGC_FIX"))")"
# The log line shape (the timestamp is a bash builtin, not a `date` fork): colours stripped,
# `[HH:MM:SS LEVEL] msg`; info/step on stdout, warn/error on STDERR only.
strip_colour() { sed -E $'s/\x1b\\[[0-9;]*m//g'; }
LOG_SHAPE='^\[[0-2][0-9]:[0-5][0-9]:[0-5][0-9] '
log_out="$(src_run "$LIB_COMMON" 'log_info "hello info"; log_step "hello step"' 2>/dev/null | strip_colour)"
assert_rc "log-line-shape-info" 0 "$(grep -qE "${LOG_SHAPE}INFO\] hello info$" <<< "$log_out" && echo 0 || echo "1 (${log_out})")"
assert_rc "log-line-shape-step" 0 "$(grep -qE "${LOG_SHAPE}STEP\] hello step$" <<< "$log_out" && echo 0 || echo "1 (${log_out})")"
log_err="$(src_run "$LIB_COMMON" 'log_warn "hello warn"; log_error "hello error"' 2>&1 >/dev/null | strip_colour)"
log_err_stdout="$(src_run "$LIB_COMMON" 'log_warn "hello warn"; log_error "hello error"' 2>/dev/null)"
assert_rc "log-line-shape-warn-stderr" 0 "$(grep -qE "${LOG_SHAPE}WARN\] hello warn$" <<< "$log_err" && echo 0 || echo "1 (${log_err})")"
assert_rc "log-line-shape-error-stderr" 0 "$(grep -qE "${LOG_SHAPE}ERROR\] hello error$" <<< "$log_err" && echo 0 || echo "1 (${log_err})")"
assert_rc "log-warn-error-never-stdout" 0 "$([[ -z "$log_err_stdout" ]] && echo 0 || echo "1 (${log_err_stdout})")"

# =============================================================================================
# === (D) The environment root + content-addressed ConfigMaps (ADR-0038 devloop 1) =============
# =============================================================================================
# These cases RENDER with the REAL kubectl (its embedded kustomize), never a stub: a stubbed
# render would pass vacuously. kubectl is guaranteed in the devloop image
# (infra/devloop/Dockerfile) and on CI runners; absent, the render cases FAIL loudly here.
REAL_KUBECTL="$(command -v kubectl || true)"
if [[ -z "$REAL_KUBECTL" ]]; then
  FAIL=$((FAIL + 1))
  FAILURES+=("[render-kubectl-present] kubectl not on PATH — the (D) render cases cannot run; this is a FAIL, not a skip")
fi
ROOT_DIR="${REPO_ROOT}/infra/kubernetes/overlays/kind"
RWORK="${WORK}/render"
mkdir -p "$RWORK"

# render <dir> <out-dir>: kustomize-render <dir> into <out-dir>.yaml and split it into one file
# per document (on the `---` separator kustomize emits), so documents compare byte-for-byte.
render() {
  local src="$1" out="$2"
  rm -rf "$out"; mkdir -p "$out"
  "$REAL_KUBECTL" kustomize "$src" > "${out}.yaml" || return 1
  awk -v d="$out" 'BEGIN{n=0; f=sprintf("%s/%04d", d, n)} /^---$/{close(f); n++; f=sprintf("%s/%04d", d, n); next} {print > f}' "${out}.yaml"
}
# doc_sums <split-dir>: sha256 of every document, sorted.
doc_sums() { local f; for f in "$1"/*; do [[ -f "$f" ]] && sha256sum < "$f" | cut -d' ' -f1; done | sort; }
# cm_names <split-dir>: ConfigMap metadata.name values, sorted.
cm_names() {
  local f
  for f in "$1"/*; do
    grep -q '^kind: ConfigMap$' "$f" && awk '/^metadata:/{m=1;next} /^[^ ]/{m=0} m&&/^  name: /{print $2}' "$f"
  done | sort
}
# kind_docs <split-dir> <kind>: concatenated documents of one kind (for identity checks).
kind_docs() { local f; for f in "$1"/*; do grep -q "^kind: $2\$" "$f" && cat "$f"; done; }

if [[ -n "$REAL_KUBECTL" ]]; then
  render "$ROOT_DIR" "${RWORK}/root"; root_rc=$?
  assert_rc "root-renders" 0 "$root_rc"
  assert_status "root-render-has-deployments" "kind: Deployment" "$(cat "${RWORK}/root.yaml")"
  root_sums="$(doc_sums "${RWORK}/root")"

  # --- (D1) Pre-applied sub-overlays are byte-identical SUBSETS of the root render ------------
  # deploy.sh applies postgres/redis/otel-collector ahead of the root for ordering only; if their
  # render ever differed from the root's, the root apply would flip those objects back and forth.
  # Non-empty + expected kind first, so "every doc of an empty set is in root" cannot pass.
  for pair in "postgres:StatefulSet" "redis:StatefulSet" "otel-collector:Deployment"; do
    sub="${pair%%:*}"; want_kind="${pair#*:}"
    render "${ROOT_DIR}/services/${sub}" "${RWORK}/sub-${sub}"; sub_rc=$?
    assert_rc "subset-${sub}-renders" 0 "$sub_rc"
    assert_status "subset-${sub}-nonempty-has-${want_kind}" "kind: ${want_kind}" "$(cat "${RWORK}/sub-${sub}.yaml")"
    missing="$(comm -23 <(doc_sums "${RWORK}/sub-${sub}") <(printf '%s\n' "$root_sums"))"
    if [[ -z "$missing" ]]; then PASS=$((PASS + 1)); else
      FAIL=$((FAIL + 1)); FAILURES+=("[subset-${sub}-in-root] a document of overlays/kind/services/${sub} is not byte-identical in the root render — the root apply would flip it"); fi
  done

  # --- (D2) The devloop wrapper: advertise values land, and ONLY those four ConfigMaps change --
  # Every render needs resolved content-tagged refs (ADR-0038 devloop 2); the fixture ref is
  # a well-formed sha tag, set for exactly the repos the root runs.
  TEST_TAG="sha-0123456789abcdef"
  SET_REFS='for r in $(root_repos); do IMAGE_REFS[$r]="$r:'"${TEST_TAG}"'"; done'
  RUN_RENDER='cache_renders; '"${SET_REFS}"'; render_env_overlay "${ARGS[0]}"'
  WRAP="${RWORK}/wrap"; mkdir -p "$WRAP"
  DT_HOST_GATEWAY_IP=10.1.2.3 MC_0_WEBTRANSPORT_PORT=24433 MC_1_WEBTRANSPORT_PORT=24435 \
    MH_0_WEBTRANSPORT_PORT=24434 MH_1_WEBTRANSPORT_PORT=24436 src_run "$DEPLOY" "$RUN_RENDER" "$WRAP" >/dev/null 2>&1
  assert_rc "wrapper-renders" 0 $?
  render "$WRAP" "${RWORK}/wrapped"; assert_rc "wrapper-kustomize-builds" 0 $?
  wrapped="$(cat "${RWORK}/wrapped.yaml")"
  assert_status "wrapper-mc-0-advertise" "MC_WEBTRANSPORT_ADVERTISE_ADDRESS: https://10.1.2.3:24433" "$wrapped"
  assert_status "wrapper-mc-1-advertise" "MC_WEBTRANSPORT_ADVERTISE_ADDRESS: https://10.1.2.3:24435" "$wrapped"
  assert_status "wrapper-mh-0-advertise" "MH_WEBTRANSPORT_ADVERTISE_ADDRESS: https://10.1.2.3:24434" "$wrapped"
  assert_status "wrapper-mh-1-advertise" "MH_WEBTRANSPORT_ADVERTISE_ADDRESS: https://10.1.2.3:24436" "$wrapped"
  # Exactly the four per-instance ConfigMap names change (their hash); every other ConfigMap
  # name, e.g. ac-service-config's, is unchanged, which is "rolls exactly those pods".
  changed="$(comm -3 <(cm_names "${RWORK}/root") <(cm_names "${RWORK}/wrapped") | tr -d '\t' \
    | sed -E 's/-[^-]+$//' | sort -u | tr '\n' ' ')"
  assert_rc "wrapper-changes-exactly-the-four-instance-configmaps" 0 \
    "$([[ "$changed" == "mc-0-config mc-1-config mh-0-config mh-1-config " ]] && echo 0 || echo "1 (changed: ${changed})")"
  ac_root="$(cm_names "${RWORK}/root" | grep '^ac-service-config-')"
  assert_rc "wrapper-ac-config-present" 0 "$([[ -n "$ac_root" ]] && echo 0 || echo 1)"
  assert_status "wrapper-ac-config-hash-unchanged" "$ac_root" "$(cm_names "${RWORK}/wrapped")"
  # Security 2c: the wrapper must not touch Secrets.
  assert_rc "wrapper-secrets-identical" 0 \
    "$([[ "$(kind_docs "${RWORK}/root" Secret)" == "$(kind_docs "${RWORK}/wrapped" Secret)" ]] && echo 0 || echo 1)"
  # DRY N1: the wrapper's instance set is DERIVED; it must equal the per-instance generators
  # the root actually renders, so a new instance can never miss its override silently.
  inst_wrapper="$(src_run "$DEPLOY" 'advertise_instances' | tr '\n' ' ')"
  inst_root="$(cm_names "${RWORK}/root" | grep -E '^m[ch]-[0-9]+-config-' | sed -E 's/-config-[^-]+$//' | sort | tr '\n' ' ')"
  assert_rc "wrapper-instances-equal-root-generators" 0 \
    "$([[ -n "$inst_root" && "$inst_wrapper" == "$inst_root" ]] && echo 0 || echo "1 (wrapper='${inst_wrapper}' root='${inst_root}')")"
  # Invalid inputs fail the render (the caller then refuses to apply anything).
  bad="$(DT_HOST_GATEWAY_IP=10.1.2.3 MC_0_WEBTRANSPORT_PORT=24433 MC_1_WEBTRANSPORT_PORT=70000 \
    MH_0_WEBTRANSPORT_PORT=24434 MH_1_WEBTRANSPORT_PORT=24436 src_run "$DEPLOY" "$RUN_RENDER" "$WRAP" 2>&1)"; bad_rc=$?
  assert_rc "wrapper-bad-port-rejected" 1 "$bad_rc"
  assert_status "wrapper-bad-port-named" "MC_1_WEBTRANSPORT_PORT" "$bad"
  DT_HOST_GATEWAY_IP=10.1.2.3 src_run "$DEPLOY" "$RUN_RENDER" "$WRAP" >/dev/null 2>&1
  assert_rc "wrapper-missing-port-rejected" 1 $?

  # --- (D2b) Content-tagged images: the wrapper ALWAYS carries the tags (ADR-0038 §2) -------
  STATIC="${RWORK}/static"; mkdir -p "$STATIC"
  src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_RENDER" "$STATIC" >/dev/null 2>&1
  assert_rc "static-wrapper-renders" 0 $?
  render "$STATIC" "${RWORK}/static-r"; assert_rc "static-wrapper-kustomize-builds" 0 $?
  static_r="$(cat "${RWORK}/static-r.yaml")"
  for repo in ac-service gc-service mc-service mh-service; do
    assert_status "wrapper-every-repo-tagged-${repo}" "image: localhost/${repo}:${TEST_TAG}" "$static_r"
  done
  # Nothing first-party may survive untagged — neither `:latest` nor the placeholder.
  assert_rc "wrapper-no-latest-survives" 0 "$(grep -q 'localhost/[a-z-]*:latest' <<< "$static_r" && echo 1 || echo 0)"
  assert_absent "wrapper-no-placeholder-survives" ":render-required" "$static_r"
  assert_rc "wrapper-no-latest-survives-gateway" 0 "$(grep -q 'localhost/[a-z-]*:latest' <<< "$wrapped" && echo 1 || echo 0)"
  assert_absent "wrapper-no-placeholder-survives-gateway" ":render-required" "$wrapped"
  # Advertise merges only with a gateway: the static render carries none of them.
  assert_absent "wrapper-advertise-only-with-gateway" "behavior: merge" "$(cat "${STATIC}/kustomization.yaml")"
  # S4a: a Kind-loaded first-party image is never pulled.
  pull_policies="$(grep -A1 'image: localhost/' <<< "$static_r" | grep -o 'imagePullPolicy: [A-Za-z]*' | sort -u)"
  assert_rc "wrapper-pullpolicy-never" 0 "$([[ "$pull_policies" == "imagePullPolicy: Never" ]] && echo 0 || echo "1 (${pull_policies})")"
  # The repo set is DERIVED — exactly the four services + the migrations image.
  repos_out="$(src_run "$DEPLOY" 'cache_renders; first_party_repos' 2>/dev/null | tr '\n' ' ')"
  assert_rc "repos-derived-exact-set" 0 "$([[ "$repos_out" == "localhost/ac-service localhost/db-migrate localhost/gc-service localhost/mc-service localhost/mh-service " ]] && echo 0 || echo "1 (${repos_out})")"
  # A render with any root repo lacking a resolved ref refuses (never a placeholder apply).
  NOREF="${RWORK}/noref"; mkdir -p "$NOREF"
  out="$(src_run "$DEPLOY" 'cache_renders; IMAGE_REFS[localhost/ac-service]="localhost/ac-service:sha-0123456789abcdef"; render_env_overlay "${ARGS[0]}"' "$NOREF" 2>&1)"
  assert_rc "wrapper-missing-ref-refused" 1 "$?"
  assert_status "wrapper-missing-ref-named" "no content-tagged ref resolved for localhost/gc-service" "$out"

  # --- (D2c) The migration Job render (ADR-0038 §2 step 3) ------------------------------------
  RUN_JOB='render_migration_job "${ARGS[0]}" "${ARGS[1]}"'
  J1="${RWORK}/job1"; J2="${RWORK}/job2"; J3="${RWORK}/job3"; mkdir -p "$J1" "$J2" "$J3"
  name1="$(src_run "$DEPLOY" "$RUN_JOB" "$J1" "localhost/db-migrate:${TEST_TAG}" 2>/dev/null)"
  name2="$(src_run "$DEPLOY" "$RUN_JOB" "$J2" "localhost/db-migrate:${TEST_TAG}" 2>/dev/null)"
  name3="$(src_run "$DEPLOY" "$RUN_JOB" "$J3" "localhost/db-migrate:sha-fedcba9876543210" 2>/dev/null)"
  assert_rc "job-render-named-by-hash" 0 "$([[ "$name1" =~ ^db-migrate-[0-9a-f]{10}$ ]] && echo 0 || echo "1 (${name1})")"
  assert_rc "job-name-stable-when-unchanged" 0 "$([[ -n "$name1" && "$name1" == "$name2" ]] && echo 0 || echo 1)"
  assert_rc "job-name-changes-with-tag" 0 "$([[ -n "$name3" && "$name1" != "$name3" ]] && echo 0 || echo 1)"
  job_r="$("$REAL_KUBECTL" kustomize "$J1")"
  assert_status "job-render-carries-name" "name: ${name1}" "$job_r"
  assert_status "job-render-carries-tag" "image: localhost/db-migrate:${TEST_TAG}" "$job_r"
  assert_absent "job-render-no-placeholder" ":render-required" "$job_r"
  # S2: no literal credential — only secretKeyRefs into postgres-secret (positive control).
  # The needle is READ from the artifact that owns the value, never typed from memory: a
  # rotated password (or a new spelling) must still be what this looks for.
  pg_password="$(awk '/^  POSTGRES_PASSWORD:/{print $2}' "${REPO_ROOT}/infra/services/postgres/secret.yaml")"
  assert_rc "job-render-password-needle-read" 0 "$([[ -n "$pg_password" ]] && echo 0 || echo "1 (no POSTGRES_PASSWORD read from infra/services/postgres/secret.yaml)")"
  assert_absent "job-render-no-literal-password" "${pg_password:-<unread-password-needle>}" "$job_r"
  assert_absent "job-render-no-literal-user" "$(awk '/^  POSTGRES_USER:/{print "value: "$2}' "${REPO_ROOT}/infra/services/postgres/secret.yaml")" "$job_r"
  assert_rc "job-render-has-secretkeyref" 0 "$(grep -A3 'name: PGPASSWORD' <<< "$job_r" | grep -q 'key: POSTGRES_PASSWORD' && grep -A4 'name: PGPASSWORD' <<< "$job_r" | grep -q 'name: postgres-secret' && echo 0 || echo 1)"
  assert_absent "job-render-url-has-no-userinfo" "@postgres" "$(grep -A1 'name: DATABASE_URL' <<< "$job_r")"
  assert_absent "job-render-no-sslmode" "sslmode" "$job_r"
  # A changed Job SPEC renames it too (immutable pod template): copy the tree, edit job.yaml.
  JCP="${RWORK}/jobcopy"; deploy_tree_copy "$JCP"
  sed -i 's/backoffLimit: 1/backoffLimit: 2/' "$JCP/infra/services/db-migrate/job.yaml"
  J4="${RWORK}/job4"; mkdir -p "$J4"
  name4="$(src_run "$JCP/infra/kind/scripts/deploy.sh" "$RUN_JOB" "$J4" "localhost/db-migrate:${TEST_TAG}" 2>/dev/null)"
  assert_rc "job-name-changes-with-spec" 0 "$([[ -n "$name4" && "$name1" != "$name4" ]] && echo 0 || echo "1 (${name1} vs ${name4})")"
  # A placeholder / :latest ref never renders a Job.
  src_run "$DEPLOY" "$RUN_JOB" "${RWORK}/job5" "localhost/db-migrate:latest" >/dev/null 2>&1
  assert_rc "job-render-rejects-latest" 1 $?
  # S3: the postgres ingress rule for the Job is ONE `from` element (AND), not two (OR).
  pg_np="$(awk '/app: db-migrate/{print NR}' "${REPO_ROOT}/infra/services/postgres/network-policy.yaml")"
  pg_from="$(awk -v t="$pg_np" 'NR<t && /^  - from:/{f=NR} NR<t && /^    - /{last=NR} END{print f" "last}' "${REPO_ROOT}/infra/services/postgres/network-policy.yaml")"
  assert_rc "postgres-ingress-db-migrate-single-from-element" 0 "$(sed -n "$(cut -d' ' -f1 <<< "$pg_from"),${pg_np}p" "${REPO_ROOT}/infra/services/postgres/network-policy.yaml" | grep -c '^    - ' | grep -qx 1 && echo 0 || echo 1)"

  # --- (D3) A data change changes ONLY its consumer's pod template ----------------------------
  # Copy infra/ (the root references bases by relative path), change one key in mc-0's
  # generator source, re-render: the only workload whose document differs must be mc-0.
  CP="${RWORK}/copy"; mkdir -p "$CP"; cp -r "${REPO_ROOT}/infra" "$CP/"
  sed -i -E 's#^(MC_WEBTRANSPORT_ADVERTISE_ADDRESS=).*#\1https://127.0.0.1:1#' "$CP/infra/services/mc-service/mc-0-config.env"
  render "$CP/infra/kubernetes/overlays/kind" "${RWORK}/mutated"; assert_rc "mutated-renders" 0 $?
  wl_changed=""
  for f in "${RWORK}/mutated"/*; do
    grep -qE '^kind: (Deployment|StatefulSet|DaemonSet)$' "$f" || continue
    if ! grep -qxF "$(sha256sum < "$f" | cut -d' ' -f1)" <<< "$root_sums"; then
      wl_changed+="$(awk '/^metadata:/{m=1;next} /^[^ ]/{m=0} m&&/^  name: /{print $2}' "$f") "
    fi
  done
  assert_rc "one-key-change-rolls-only-its-consumer" 0 \
    "$([[ "$wl_changed" == "mc-0 " ]] && echo 0 || echo "1 (changed workloads: '${wl_changed}')")"
fi

# --- (D4) apply_env_root: ONE apply path, ALWAYS the rendered wrapper ------------------------
# A PATH-stubbed kubectl models the cluster reads/writes the converge makes; `kustomize` passes
# through to the REAL kubectl (renders are never stubbed). Knobs (env):
#   STUB_DEPLOYED_TAG        tag every first-party workload runs (default sha-aaaaaaaaaaaaaaaa)
#   STUB_DEPLOYED_OVERRIDE   "<res>=<image>" — one workload runs <image> instead
#   STUB_GET_FAIL=1          workload/job `get` fails (an unreadable cluster)
#   STUB_JOBS                lines `<succeeded> <image>` for `get jobs -l app=db-migrate`
#   STUB_JOB_COND_INITIAL    the migration Job's condition before any apply ("" = absent)
#   STUB_JOB_COND_AFTER_APPLY  its condition once a migration wrapper is applied
#   STUB_ROLLOUT_FAIL        a `<kind>/<name>` whose `rollout status` fails
#   STUB_NODE_HAS            space-separated refs the Kind node already holds (crictl inspecti)
#   STUB_NODE_IMAGES         `crictl images` output in the node (IMAGE TAG ID lines)
#   STUB_RS_ROWS / STUB_CR_ROWS  `<revision> <owner> <image…>` rows for `get replicasets` /
#                            `get controllerrevisions` (the rollout history the prune keeps by)
#   STUB_APPLY_FAIL=root     the environment-root `apply -k` is rejected by the apiserver
#   STUB_KIND_LIST_FAIL=1    `kind get clusters` fails
#   STUB_READYZ_FAIL=1       the apiserver's `/readyz` does not answer (main's EXIT-trap probe)
#   STUB_KUSTOMIZE_FAIL=<dir>  `kubectl kustomize <dir>` fails (every other render is REAL)
D4_BIN="${WORK}/d4bin"; mkdir -p "$D4_BIN"
cat > "${D4_BIN}/kubectl" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kubectl.calls"
args=("\$@")
# Drop a leading --context <ctx>.
if [[ "\${args[0]:-}" == "--context" ]]; then args=("\${args[@]:2}"); fi
case "\${args[0]:-}" in
  kustomize)
    [[ -n "\${STUB_KUSTOMIZE_FAIL:-}" && "\${args[1]%/}" == "\${STUB_KUSTOMIZE_FAIL%/}" ]] && { echo "kubectl stub: kustomize of \${args[1]} failed" >&2; exit 1; }
    exec "${REAL_KUBECTL:-/nonexistent-kubectl}" "\${args[@]}" ;;
  apply)
    if [[ "\${args[1]:-}" == "-k" ]]; then
      d="\${args[2]}"; printf '%s\n' "\$d" >> "${MARK}/applied"
      if [[ -f "\$d/kustomization.yaml" ]]; then
        if grep -q 'render_migration_job' "\$d/kustomization.yaml"; then
          cp "\$d/kustomization.yaml" "${MARK}/applied.migration.yaml"
          printf '%s' "\${STUB_JOB_COND_AFTER_APPLY-Complete }" > "${MARK}/jobcond"
        elif grep -q 'render_env_overlay' "\$d/kustomization.yaml"; then
          [[ "\${STUB_APPLY_FAIL:-}" != root ]] || { echo "kubectl stub: the apiserver rejected a manifest" >&2; exit 1; }
          cp "\$d/kustomization.yaml" "${MARK}/applied.kustomization.yaml"
        fi
      fi
    fi
    exit 0 ;;
  delete)
    printf '%s\n' "\${args[*]}" >> "${MARK}/deleted"
    exit 0 ;;
  rollout)
    case "\${args[1]:-}" in
      restart) printf '%s\n' "\${args[2]}" >> "${MARK}/restarted"; exit 0 ;;
      status)
        [[ -n "\${STUB_ROLLOUT_FAIL:-}" && "\${args[2]:-}" == "\${STUB_ROLLOUT_FAIL}" ]] && { echo "kubectl stub: rollout of \${args[2]} timed out" >&2; exit 1; }
        exit 0 ;;
    esac ;;
  wait|describe|create) exit 0 ;;
  logs) printf 'Applied 20260322000001/migrate add participant tracking (postgres://darktower:hunter2@postgres:5432/x)\n'; exit 0 ;;
  exec) : > "${MARK}/ran.kubectl-exec"; exit 0 ;;
  get)
    if [[ "\${args[1]:-}" == "--raw" ]]; then
      : > "${MARK}/ran.readyz"
      [[ -z "\${STUB_READYZ_FAIL:-}" ]] || { echo "kubectl stub: readyz timed out" >&2; exit 1; }
      [[ -z "\${STUB_READYZ_HANG:-}" ]] || { sleep 30; exit 0; }
      echo ok; exit 0
    fi
    [[ -n "\${STUB_GET_FAIL:-}" && "\${args[1]:-}" != "events" ]] && { echo "kubectl stub: get failed" >&2; exit 1; }
    case "\${args[1]:-}" in
      jobs) printf '%b' "\${STUB_JOBS:-}"; exit 0 ;;
      job)
        if [[ -f "${MARK}/jobcond" ]]; then cat "${MARK}/jobcond"; else printf '%s' "\${STUB_JOB_COND_INITIAL:-}"; fi
        printf '\n'; exit 0 ;;
      events) exit 0 ;;
      pods) printf 'calico-node-x 1/1 Running\n'; exit 0 ;;
      replicasets) [[ -z "\${STUB_RS_FAIL:-}" ]] || { echo "kubectl stub: rs read failed" >&2; exit 1; }; printf '%b' "\${STUB_RS_ROWS:-}"; exit 0 ;;
      controllerrevisions) printf '%b' "\${STUB_CR_ROWS:-}"; exit 0 ;;
      */*)
        if [[ "\$*" == *go-template* ]]; then printf 'app=%s,' "\${args[1]#*/}"; exit 0; fi
        res="\${args[1]}"
        if [[ -n "\${STUB_DEPLOYED_OVERRIDE:-}" && "\${STUB_DEPLOYED_OVERRIDE%%=*}" == "\$res" ]]; then
          printf '%s\n' "\${STUB_DEPLOYED_OVERRIDE#*=}"; exit 0
        fi
        n="\${res#*/}"
        case "\$n" in
          ac-service|gc-service) svc="\$n" ;;
          mc-*) svc=mc-service ;;
          mh-*) svc=mh-service ;;
          *) printf 'thirdparty/image:1\n'; exit 0 ;;
        esac
        printf 'localhost/%s:%s\n' "\$svc" "\${STUB_DEPLOYED_TAG:-sha-aaaaaaaaaaaaaaaa}"; exit 0 ;;
    esac ;;
esac
echo "kubectl stub (D4): unmodelled invocation: \$*" >&2
exit 91
STUB
for tool in docker podman; do
  cat > "${D4_BIN}/${tool}" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/${tool}.calls"
case "\$1" in
  build)
    shift
    while [[ \$# -gt 0 ]]; do
      if [[ "\$1" == "--iidfile" ]]; then printf '%s' "\${STUB_IID-sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb}" > "\$2"; fi
      shift
    done ;;
  save) for a in "\$@"; do :; done; : > "\${a}" ;;
  info)
    : > "${MARK}/ran.runtime-info"
    [[ "\${STUB_RUNTIME_INFO:-}" != fail ]] || exit 1
    [[ "\${STUB_RUNTIME_INFO:-}" != hang ]] || { sleep 30; exit 0; } ;;
  exec)
    # exec <node> crictl <verb> ...
    case "\$4" in
      inspecti) for r in \${STUB_NODE_HAS:-}; do [[ "\$r" == "\${@: -1}" ]] && exit 0; done; exit 1 ;;
      images) printf '%b' "\${STUB_NODE_IMAGES:-}"; exit 0 ;;
      rmi) printf '%s\n' "\$5" >> "${MARK}/node-rmi"; exit 0 ;;
    esac ;;
esac
exit 0
STUB
done
cat > "${D4_BIN}/kind" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kind.calls"
case "\$1 \$2" in
  "get clusters") [[ -z "\${STUB_KIND_LIST_FAIL:-}" ]] || exit 1; echo "d4cluster" ;;
  "get nodes") echo "d4cluster-control-plane" ;;
esac
exit 0
STUB
# The retired host path must never run: a PATH `sqlx` that records any call.
cat > "${D4_BIN}/sqlx" <<STUB
#!/usr/bin/env bash
: > "${MARK}/ran.host-sqlx"
exit 0
STUB
chmod +x "${D4_BIN}/kubectl" "${D4_BIN}/docker" "${D4_BIN}/podman" "${D4_BIN}/kind" "${D4_BIN}/sqlx"
BUILT_TAG="sha-bbbbbbbbbbbbbbbb"
DEPLOYED_TAG="sha-aaaaaaaaaaaaaaaa"
D4_REFS='for r in $(root_repos); do IMAGE_REFS[$r]="$r:sha-0123456789abcdef"; done'
RUN_APPLY='cache_renders; '"${D4_REFS}"'; apply_env_root'

reset_marks
PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_APPLY" >/dev/null 2>&1
assert_rc "apply-root-plain-rc" 0 $?
assert_absent "apply-root-plain-never-the-bare-root" "$ROOT_DIR" "$(cat "${MARK}/applied" 2>/dev/null)"
assert_status "apply-root-plain-applied-the-tagged-wrapper" "newTag: sha-" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_absent "apply-root-plain-no-advertise-merge" "behavior: merge" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
# The retired grafana-sidecar RBAC delete is gone: every cluster is rebuilt by its first
# provision after ADR-0038 step 3 (no blueprint record), so nothing can carry that Role forward.
assert_no_marker "apply-root-deletes-nothing" "$MARK" "deleted"
assert_rc "apply-root-plain-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-env-root.*" >/dev/null && echo 0 || echo 1)"

reset_marks
PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_HOST_GATEWAY_IP=10.1.2.3 MC_0_WEBTRANSPORT_PORT=24433 \
  MC_1_WEBTRANSPORT_PORT=24435 MH_0_WEBTRANSPORT_PORT=24434 MH_1_WEBTRANSPORT_PORT=24436 \
  src_run "$DEPLOY" "$RUN_APPLY" >/dev/null 2>&1
assert_rc "apply-root-gateway-rc" 0 $?
assert_absent "apply-root-gateway-not-the-plain-root" "$ROOT_DIR" "$(cat "${MARK}/applied" 2>/dev/null)"
assert_status "apply-root-gateway-applied-the-wrapper" "behavior: merge" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_status "apply-root-gateway-wrapper-tagged" "newTag: sha-" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_rc "apply-root-gateway-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-env-root.*" >/dev/null && echo 0 || echo 1)"

reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_HOST_GATEWAY_IP=10.1.2.3 \
  src_run "$DEPLOY" "$RUN_APPLY" 2>&1)"; rc=$?
assert_rc "apply-root-render-failure-aborts" 1 "$rc"
assert_no_marker "apply-root-render-failure-applies-nothing" "$MARK" "applied"
assert_status "apply-root-render-failure-says-no-fallback" "NOT applying" "$out"
assert_rc "apply-root-failure-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-env-root.*" >/dev/null && echo 0 || echo 1)"

# --- (D5) content_tag: the ONE tag derivation (pure) -------------------------------------------
ct() { src_run "$DEPLOY" 'content_tag "${ARGS[0]}"' "$1" 2>/dev/null; }
hex_a="$(printf 'a%.0s' {1..64})"
id_a="sha256:${hex_a}"; id_b="sha256:${hex_a:0:15}b${hex_a:16}"
ta1="$(ct "$id_a")"; ta2="$(ct "$id_a")"; tb="$(ct "$id_b")"
assert_rc "content-tag-same-id-same-tag" 0 "$([[ -n "$ta1" && "$ta1" == "$ta2" ]] && echo 0 || echo 1)"
assert_rc "content-tag-different-id-different-tag" 0 "$([[ -n "$tb" && "$ta1" != "$tb" ]] && echo 0 || echo "1 (${ta1} ${tb})")"
assert_rc "content-tag-shape" 0 "$([[ "$ta1" == "sha-aaaaaaaaaaaaaaaa" ]] && echo 0 || echo "1 (${ta1})")"
assert_rc "content-tag-accepts-bare-id" 0 "$([[ "$(ct "$hex_a")" == "$ta1" ]] && echo 0 || echo 1)"
ct "sha256:not-hex" >/dev/null; assert_rc "content-tag-rejects-non-sha256" 1 $?
ct "" >/dev/null; assert_rc "content-tag-rejects-empty" 1 $?
ct "sha256:abc123" >/dev/null; assert_rc "content-tag-rejects-short" 1 $?

# A derivation called before cache_renders fails LOUDLY, naming the render (never printing it),
# and first_party_repos does not mask it behind the other render's repos.
out="$(src_run "$DEPLOY" 'first_party_repos' 2>&1)"; rc=$?
assert_rc "cache-unprimed-first-party-repos-fails" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
assert_status "cache-unprimed-names-the-render" "the root render is not cached" "$out"
assert_absent "cache-unprimed-never-prints-the-render" "kind: " "$out"
out="$(src_run "$DEPLOY" 'env_root_workloads' 2>&1)"; rc=$?
assert_rc "cache-unprimed-env-root-workloads-fails" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
out="$(src_run "$DEPLOY" 'cache_renders; first_party_repos' 2>&1)"; rc=$?
assert_rc "cache-primed-first-party-repos-rc" 0 "$rc"
assert_status "cache-primed-first-party-repos-derives" "localhost/db-migrate" "$out"

# --- (D6) Ref resolution: every ref is the one BUILT this run; the deployed one is remembered --
# deploy builds every first-party image, so there is no deployed-ref fallback (the retired
# IMAGE_UNRESOLVED path). The pre-converge ref is still READ — it is the previous generation the
# prune keeps — and an unreadable cluster fails with its own token.
RUN_RES='cache_renders; for r in $(first_party_repos); do [[ "$r" == "${SKIP_REPO:-}" ]] || BUILT_REFS[$r]="$r:'"${BUILT_TAG:-sha-bbbbbbbbbbbbbbbb}"'"; done; resolve_image_refs || exit 1; for r in "${!IMAGE_REFS[@]}"; do echo "$r=${IMAGE_REFS[$r]} prev=${DEPLOYED_REFS[$r]:-none}"; done | sort'
res() { PATH="${D4_BIN}:${PATH}" DT_CLUSTER_NAME=d4cluster src_run "$DEPLOY" "$RUN_RES" 2>&1; }
JOBS_OK="1 localhost/db-migrate:${DEPLOYED_TAG}\n"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" res)"; rc=$?
assert_rc "resolve-rc" 0 "$rc"
assert_status "resolve-uses-built" "localhost/gc-service=localhost/gc-service:${BUILT_TAG} prev=localhost/gc-service:${DEPLOYED_TAG}" "$out"
assert_status "resolve-remembers-deployed-migration-job" "localhost/db-migrate=localhost/db-migrate:${BUILT_TAG} prev=localhost/db-migrate:${DEPLOYED_TAG}" "$out"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_DEPLOYED_TAG=latest res)"; rc=$?
assert_rc "resolve-deployed-latest-not-remembered-rc" 0 "$rc"
assert_status "resolve-deployed-latest-not-remembered" "localhost/ac-service=localhost/ac-service:${BUILT_TAG} prev=none" "$out"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" SKIP_REPO=localhost/mh-service res)"; rc=$?
assert_rc "resolve-unbuilt-repo-is-a-defect" 1 "$rc"
assert_status "resolve-unbuilt-repo-says-defect" "was not built in this run" "$out"
assert_absent "resolve-no-image-unresolved-left" "IMAGE_UNRESOLVED" "$(grep -v '^[[:space:]]*#' "$DEPLOY")"
reset_marks
out="$(STUB_GET_FAIL=1 res)"; rc=$?
assert_rc "resolve-kubectl-read-failure-rc" 1 "$rc"
assert_status "resolve-kubectl-read-failure-distinct" "REASON=image-ref-read-failed" "$out"

# --- (D7) run_migration_job: fails LOUDLY; an unchanged set is a no-op -----------------------
RUN_MIG='IMAGE_REFS[localhost/db-migrate]="localhost/db-migrate:'"${DEPLOYED_TAG}"'"; run_migration_job'
mig() { PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DT_JOB_POLL_SECONDS=0 src_run "${1:-$DEPLOY}" "$RUN_MIG" 2>&1; }
reset_marks
out="$(mig)"; rc=$?
assert_rc "migrate-complete-rc0" 0 "$rc"
assert_status "migrate-applies-the-job-wrapper" "render_migration_job" "$(cat "${MARK}/applied.migration.yaml" 2>/dev/null)"
assert_status "migrate-success-log-printed" "Applied 20260322000001" "$out"
assert_status "migrate-success-says-applied" "Migrations applied (job/db-migrate-" "$out"
assert_status "migrate-log-redacts-userinfo" "postgres://<redacted>@postgres" "$out"
assert_absent "migrate-log-no-password" "hunter2" "$out"
mig_name="$(grep -o 'value: db-migrate-[0-9a-f]*' "${MARK}/applied.migration.yaml" 2>/dev/null | cut -d' ' -f2)"
assert_rc "migrate-name-captured" 0 "$([[ -n "$mig_name" ]] && echo 0 || echo 1)"
# The interim Succeeded-pod delete is GONE (ADR-0038 step 3): the pod and its logs stay with
# their Job. Positive control: the stubbed kubectl was reached for the Job itself.
assert_absent "migrate-no-succeeded-pod-delete" "delete pods" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_status "migrate-stub-reached-the-job" "get job db-migrate-" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
assert_status "migrate-prune-excludes-current" "metadata.name!=${mig_name}" "$(grep '^delete jobs' "${MARK}/deleted" 2>/dev/null)"
assert_rc "migrate-prune-never-current" 0 "$(grep '^delete job' "${MARK}/deleted" 2>/dev/null | grep -v "metadata.name!=" | grep -q "${mig_name:-NONE}" && echo 1 || echo 0)"
assert_rc "migrate-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-migrate.*" >/dev/null && echo 0 || echo 1)"

reset_marks
out="$(STUB_JOB_COND_AFTER_APPLY="Failed BackoffLimitExceeded" mig)"; rc=$?
assert_rc "migrate-failed-rc" 1 "$rc"
assert_rc "migrate-failed-token" 0 "$(grep -Eq '^MIGRATION_FAILURE: job/db-migrate-[0-9a-f]+ failed \(reason=BackoffLimitExceeded\) REASON=migration-failed' <<< "$out" && echo 0 || echo "1 (${out})")"
assert_status "migrate-failed-logs-all-attempts" "--all-containers --prefix --tail=-1" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
assert_status "migrate-failed-logs-dumped" "Applied 20260322000001" "$out"
assert_absent "migrate-failed-log-no-password" "hunter2" "$out"
assert_status "migrate-failed-names-checksum-remedy" "previously applied but has been modified" "$out"
assert_no_marker "migrate-failed-deletes-nothing" "$MARK" "deleted"

# The Job's OWN deadline ends as Failed/DeadlineExceeded — the common timeout path. It is a
# timeout, not a SQL failure: timeout token + unreachable/not-loaded advice, never migration-failed.
reset_marks
out="$(STUB_JOB_COND_AFTER_APPLY="Failed DeadlineExceeded" mig)"; rc=$?
assert_rc "migrate-deadline-exceeded-rc" 1 "$rc"
assert_rc "migrate-deadline-exceeded-is-timeout" 0 "$(grep -Eq '^MIGRATION_FAILURE: .*reason=DeadlineExceeded.* REASON=migration-timeout' <<< "$out" && echo 0 || echo "1 (${out})")"
assert_absent "migrate-deadline-exceeded-not-failed-token" "REASON=migration-failed" "$out"
assert_absent "migrate-deadline-exceeded-not-sqlx-advice" "carry sqlx's error" "$out"
assert_status "migrate-deadline-exceeded-hint" "ErrImageNeverPull" "$out"

# The derived wait: a copy of the tree with a 1s activeDeadlineSeconds and a 0s margin.
MCP="${RWORK}/migcopy"; rm -rf "$MCP"; deploy_tree_copy "$MCP"
sed -i 's/activeDeadlineSeconds: 300/activeDeadlineSeconds: 1/' "$MCP/infra/services/db-migrate/job.yaml"
reset_marks
out="$(STUB_JOB_COND_AFTER_APPLY="" DT_JOB_WAIT_MARGIN_SECONDS=0 mig "$MCP/infra/kind/scripts/deploy.sh")"; rc=$?
assert_rc "migrate-timeout-rc" 1 "$rc"
assert_rc "migrate-timeout-distinct-token" 0 "$(grep -Eq '^MIGRATION_FAILURE: .* REASON=migration-timeout' <<< "$out" && echo 0 || echo "1 (${out})")"
assert_absent "migrate-timeout-not-the-failed-token" "REASON=migration-failed" "$out"
assert_status "migrate-deadline-derived" "within 1s (activeDeadlineSeconds 1 + 0s" "$out"
sed -i '/activeDeadlineSeconds/d' "$MCP/infra/services/db-migrate/job.yaml"
reset_marks
out="$(mig "$MCP/infra/kind/scripts/deploy.sh")"; rc=$?
assert_rc "migrate-no-deadline-fails" 1 "$rc"
assert_status "migrate-no-deadline-says-so" "no activeDeadlineSeconds" "$out"
assert_no_marker "migrate-no-deadline-applies-nothing" "$MARK" "applied"

reset_marks
out="$(STUB_JOB_COND_INITIAL="Failed BackoffLimitExceeded" mig)"; rc=$?
assert_rc "migrate-existing-failed-retried-rc" 0 "$rc"
assert_status "migrate-existing-failed-deleted" "delete job db-migrate-" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_status "migrate-existing-failed-recreated" "render_migration_job" "$(cat "${MARK}/applied.migration.yaml" 2>/dev/null)"

reset_marks
out="$(STUB_JOB_COND_INITIAL="Complete " mig)"; rc=$?
assert_rc "migrate-existing-complete-noop-rc" 0 "$rc"
assert_no_marker "migrate-existing-complete-no-apply" "$MARK" "applied"
assert_absent "migrate-existing-complete-no-job-delete" "delete job db-migrate-" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_status "migrate-existing-complete-condition-read" "get job db-migrate-" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
assert_status "migrate-existing-complete-says-so" "Migrations unchanged (job/db-migrate-" "$out"
assert_absent "migrate-existing-complete-never-claims-applied" "Migrations applied" "$out"
assert_rc "migrate-existing-complete-no-log-dump" 0 "$(grep -Eq '(^| )logs ' "${MARK}/kubectl.calls" 2>/dev/null && echo 1 || echo 0)"
assert_absent "migrate-existing-complete-no-pod-delete" "delete pods" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_status "migrate-existing-complete-still-prunes-older" "metadata.name!=db-migrate-" "$(cat "${MARK}/deleted" 2>/dev/null)"

# --- (D8) deploy.sh main: every run builds everything, applies the one root, restarts nothing --
# main() runs SOURCED with check_blueprint overridden (the guard itself is pinned just below with
# a stubbed provision.sh) — a harness-side override, not a production seam.
builds() { grep '^build ' "${MARK}/podman.calls" 2>/dev/null | grep -o 'infra/docker/[a-z-]*/Dockerfile' | sort | tr '\n' ' '; }
RUN_DEPLOY='check_blueprint() { : > "'"${MARK}"'/ran.blueprint-check"; }; main'
dep() {
  PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 DT_JOB_POLL_SECONDS=0 \
    src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_DEPLOY" 2>&1
}
ALL_DOCKERFILES="infra/docker/ac-service/Dockerfile infra/docker/db-migrate/Dockerfile infra/docker/gc-service/Dockerfile infra/docker/mc-service/Dockerfile infra/docker/mh-service/Dockerfile "
reset_marks
GC_NODE_IMAGES="IMAGE TAG ID\nlocalhost/gc-service ${BUILT_TAG} x\nlocalhost/gc-service ${DEPLOYED_TAG} y\nlocalhost/gc-service sha-cccccccccccccccc z\nlocalhost/mc-service ${BUILT_TAG} w\n"
# gc-service's rollout history: current (BUILT), previous (DEPLOYED), older (cccc).
GC_RS_ROWS="3 gc-service localhost/gc-service:${BUILT_TAG}\n2 gc-service localhost/gc-service:${DEPLOYED_TAG}\n1 gc-service localhost/gc-service:sha-cccccccccccccccc\n"
out="$(STUB_JOBS="$JOBS_OK" STUB_NODE_IMAGES="$GC_NODE_IMAGES" STUB_RS_ROWS="$GC_RS_ROWS" dep)"; rc=$?
assert_rc "deploy-rc" 0 "$rc"
assert_marker "deploy-runs-the-blueprint-check" "$MARK" "ran.blueprint-check"
assert_rc "deploy-builds-every-repo" 0 "$([[ "$(builds)" == "$ALL_DOCKERFILES" ]] && echo 0 || echo "1 ($(builds))")"
assert_status "deploy-db-migrate-gets-sqlx-version" "SQLX_CLI_VERSION=" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
# EVERY image build carries the reader's RUST_VERSION (none of the Dockerfiles has a default), and
# every SERVICE build its CARGO_CHEF_VERSION — counted per build line against the Dockerfile set,
# so one hit cannot satisfy it and a build path that bypasses image_build_args trips it.
D8_RUST="$(src_run "${REPO_ROOT}/infra/lib/rust-toolchain.sh" 'rust_toolchain_version "${ARGS[0]}"' "${REPO_ROOT}/rust-toolchain.toml")"
d8_builds="$(grep -c '^build ' "${MARK}/podman.calls" 2>/dev/null)"
d8_rust="$(grep '^build ' "${MARK}/podman.calls" 2>/dev/null | grep -c -- "--build-arg RUST_VERSION=${D8_RUST} ")"
d8_chef="$(grep '^build ' "${MARK}/podman.calls" 2>/dev/null | grep -- '-service/Dockerfile' | grep -c -- '--build-arg CARGO_CHEF_VERSION=[0-9]')"
d8_files="$(wc -w <<< "$ALL_DOCKERFILES")"
assert_rc "deploy-every-build-gets-rust-version" 0 "$([[ -n "$D8_RUST" && "$d8_builds" -eq "$d8_files" && "$d8_rust" -eq "$d8_files" ]] && echo 0 || echo "1 (builds=${d8_builds} with-rust=${d8_rust} dockerfiles=${d8_files} rust=${D8_RUST})")"
assert_rc "deploy-every-service-build-gets-cargo-chef" 0 "$([[ "$d8_chef" -eq $((d8_files - 1)) ]] && echo 0 || echo "1 (${d8_chef} service builds with CARGO_CHEF_VERSION)")"
assert_status "deploy-tags-by-content" "tag sha256:bbbb" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
# ONE render of the environment root per converge (cache_renders): the derivations read the
# snapshot, never re-render it (the stub logs every kubectl call; kustomize passes through).
root_renders="$(grep -cE "^kustomize ${ROOT_DIR}/?\$" "${MARK}/kubectl.calls" 2>/dev/null)"
assert_rc "deploy-renders-the-root-once" 0 "$([[ "$root_renders" == 1 ]] && echo 0 || echo "1 (${root_renders} root renders)")"
assert_status "deploy-root-carries-built-tags" "name: localhost/mc-service"$'\n'"    newTag: ${BUILT_TAG}" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_no_marker "deploy-restarts-nothing" "$MARK" "restarted"
assert_status "deploy-runs-the-migration-job" "render_migration_job" "$(cat "${MARK}/applied.migration.yaml" 2>/dev/null)"
assert_marker "deploy-seeds-after-migrations" "$MARK" "ran.kubectl-exec"
assert_status "deploy-waits-for-every-root-workload" "statefulset/redis" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
# The collector pre-apply (and its rollout wait) precedes the root apply.
applied_order="$(tr '\n' ' ' < "${MARK}/applied" 2>/dev/null)"
assert_rc "deploy-collector-before-root" 0 "$([[ "$applied_order" =~ services/otel-collector/.*dt-env-root ]] && echo 0 || echo "1 (${applied_order})")"
# Loaded into Kind (the node held none of the built refs).
assert_status "deploy-loads-missing-refs" "save localhost/gc-service:${BUILT_TAG}" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
# Prune keeps TWO generations (this deploy's + the previous one) and evicts only older refs.
assert_status "prune-evicts-older-from-node" "localhost/gc-service:sha-cccccccccccccccc" "$(cat "${MARK}/node-rmi" 2>/dev/null)"
assert_status "prune-evicts-older-from-host" "rmi localhost/gc-service:sha-cccccccccccccccc" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_absent "prune-keeps-previous-generation" "${DEPLOYED_TAG}" "$(cat "${MARK}/node-rmi" 2>/dev/null)"
assert_absent "prune-keeps-current" "${BUILT_TAG}" "$(cat "${MARK}/node-rmi" 2>/dev/null)"
assert_no_marker "deploy-host-sqlx-never-invoked" "$MARK" "ran.host-sqlx"
# PostgreSQL and Redis are waited on by `rollout status`, never a label-selector pod wait
# (which matches the OLD Ready pod on a warm cluster).
assert_status "deploy-postgres-rollout-status" "rollout status statefulset/postgres" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
assert_status "deploy-redis-rollout-status" "rollout status statefulset/redis" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
assert_absent "deploy-no-label-selector-pod-wait" "wait --for=condition=Ready pod -l app=postgres" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"

# UNCHANGED redeploy (ops F1): current == what ran before (B), rollout history still names A
# as previous. A must be KEPT (it is what `rollout undo` reaches); only older refs go.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_DEPLOYED_TAG="${BUILT_TAG}" STUB_NODE_IMAGES="$GC_NODE_IMAGES" STUB_RS_ROWS="$GC_RS_ROWS" dep)"; rc=$?
assert_rc "prune-unchanged-redeploy-rc" 0 "$rc"
assert_absent "prune-unchanged-redeploy-keeps-rollback-target" "${DEPLOYED_TAG}" "$(cat "${MARK}/node-rmi" 2>/dev/null)"
assert_status "prune-unchanged-redeploy-evicts-only-older" "localhost/gc-service:sha-cccccccccccccccc" "$(cat "${MARK}/node-rmi" 2>/dev/null)"
# An unreadable rollout history never evicts (hygiene must not delete on uncertainty).
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_NODE_IMAGES="$GC_NODE_IMAGES" STUB_RS_ROWS="$GC_RS_ROWS" STUB_RS_FAIL=1 dep)"; rc=$?
assert_rc "prune-history-unreadable-deploy-still-ok" 0 "$rc"
assert_status "prune-history-unreadable-warns" "PRUNE_WARN REASON=image-prune-failed REF=- WHERE=history" "$out"
assert_absent "prune-history-unreadable-evicts-nothing-of-gc" "localhost/gc-service" "$(cat "${MARK}/node-rmi" 2>/dev/null)"

# The environment root cannot be RENDERED: the converge stops at cache_renders with ONE
# classified line, before building or applying anything.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_KUSTOMIZE_FAIL="$ROOT_DIR" dep)"; rc=$?
assert_rc "deploy-root-render-failure-rc" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
assert_status "deploy-root-render-failure-reached" "kubectl stub: kustomize of" "$out"
assert_status "deploy-root-render-failure-token" "DEPLOY_FAILED REASON=step-failed STEP=cache_renders WORKLOADS=-" "$out"
assert_rc "deploy-root-render-failure-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"
assert_no_marker "deploy-root-render-failure-applies-nothing" "$MARK" "applied"
assert_rc "deploy-root-render-failure-builds-nothing" 0 "$([[ -z "$(builds)" ]] && echo 0 || echo "1 ($(builds))")"

# A PostgreSQL rollout that does not finish stops deploy BEFORE the migration Job.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_ROLLOUT_FAIL=statefulset/postgres dep)"; rc=$?
assert_rc "deploy-postgres-rollout-failure-rc" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
assert_status "deploy-postgres-rollout-failure-token" "DEPLOY_FAILED REASON=rollout-failed WORKLOADS=dark-tower/statefulset/postgres" "$out"
assert_no_marker "deploy-postgres-rollout-failure-no-migration" "$MARK" "applied.migration.yaml"
assert_rc "deploy-postgres-rollout-failure-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"

# Observability F1: a failure WITHOUT a specific line (the apiserver rejects a manifest) still
# ends with ONE DEPLOY_FAILED line naming the step (main's EXIT trap).
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_APPLY_FAIL=root dep)"; rc=$?
assert_rc "deploy-step-failed-rc" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
assert_status "deploy-step-failed-token" "DEPLOY_FAILED REASON=step-failed STEP=apply_env_root WORKLOADS=-" "$out"
assert_rc "deploy-step-failed-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"
assert_marker "deploy-step-failed-probed-the-apiserver" "$MARK" "ran.readyz"
# The same failure while the apiserver does NOT answer is the environment, not the tree.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_APPLY_FAIL=root STUB_READYZ_FAIL=1 dep)"; rc=$?
assert_status "deploy-apiserver-unreachable-token" "DEPLOY_FAILED REASON=apiserver-unreachable STEP=apply_env_root WORKLOADS=-" "$out"
assert_absent "deploy-apiserver-unreachable-not-step-failed" "REASON=step-failed" "$out"
assert_marker "deploy-apiserver-unreachable-probe-reached" "$MARK" "ran.readyz"
# A dead container runtime during the build is the environment, not the tree.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_APPLY_FAIL=root STUB_RUNTIME_INFO=fail dep)"; rc=$?
assert_status "deploy-runtime-unreachable-token" "DEPLOY_FAILED REASON=runtime-unreachable STEP=apply_env_root WORKLOADS=-" "$out"
assert_marker "deploy-runtime-unreachable-probe-reached" "$MARK" "ran.runtime-info"
# Bounded: hung probes still classify within the bound.
for probe in runtime readyz; do
  reset_marks
  t0=$SECONDS
  if [[ "$probe" == runtime ]]; then
    out="$(DT_ENV_PROBE_TIMEOUT=1 STUB_JOBS="$JOBS_OK" STUB_APPLY_FAIL=root STUB_RUNTIME_INFO=hang dep)"; want=runtime-unreachable
  else
    out="$(DT_ENV_PROBE_TIMEOUT=1 STUB_JOBS="$JOBS_OK" STUB_APPLY_FAIL=root STUB_READYZ_HANG=1 dep)"; want=apiserver-unreachable
  fi
  took=$(( SECONDS - t0 ))
  assert_status "deploy-classify-${probe}-hang-token" "DEPLOY_FAILED REASON=${want} " "$out"
  assert_rc "deploy-classify-${probe}-hang-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"
  assert_rc "deploy-classify-${probe}-hang-bounded" 0 "$([[ "$took" -lt 20 ]] && echo 0 || echo "1 (${took}s)")"
done
# Environment reasons are classified at the source, each with its own token.
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=999999999 DT_JOB_POLL_SECONDS=0 \
  STUB_JOBS="$JOBS_OK" src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_DEPLOY" 2>&1)"; rc=$?
assert_status "deploy-insufficient-disk-token" "DEPLOY_FAILED REASON=insufficient-disk WORKLOADS=-" "$out"
assert_rc "deploy-insufficient-disk-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_GET_FAIL=1 dep)"; rc=$?
assert_status "deploy-image-ref-read-failed-token" "DEPLOY_FAILED REASON=image-ref-read-failed WORKLOADS=-" "$out"
assert_rc "deploy-image-ref-read-failed-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"
# No container runtime: a PATH of ONLY the stubs (minus podman/docker) and the plain tools
# deploy.sh needs, so a real host runtime cannot leak in and the case always runs.
NORT_BIN="${WORK}/d4bin-noruntime"; rm -rf "$NORT_BIN"; mkdir -p "$NORT_BIN"
cp "${D4_BIN}/kubectl" "${D4_BIN}/kind" "${D4_BIN}/sqlx" "$NORT_BIN/"
for t in bash awk grep sed sort mktemp cat cut head tail tr sha256sum date dirname basename rm cp mv env realpath wc sleep tee uname ls mkdir chmod timeout; do
  ln -sf "$(command -v "$t")" "$NORT_BIN/$t"
done
reset_marks
out="$(PATH="${NORT_BIN}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 \
  src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_DEPLOY" 2>&1)"; rc=$?
assert_status "deploy-prerequisite-missing-token" "DEPLOY_FAILED REASON=prerequisite-missing WORKLOADS=-" "$out"
assert_rc "deploy-prerequisite-missing-one-line" 1 "$(grep -c '^DEPLOY_FAILED' <<< "$out")"
assert_status "deploy-prerequisite-missing-names-runtime" "a container runtime (podman or docker) is not installed" "$out"
# No `timeout` (stock macOS): every bounded probe would exit 127 and read as "unreachable".
# It is a named prerequisite, and the classifier itself names it rather than probing.
NOTO_BIN="${WORK}/d4bin-notimeout"; rm -rf "$NOTO_BIN"; mkdir -p "$NOTO_BIN"
cp "${D4_BIN}/kubectl" "${D4_BIN}/kind" "${D4_BIN}/sqlx" "${D4_BIN}/podman" "$NOTO_BIN/"
for t in bash awk grep sed sort mktemp cat cut head tail tr sha256sum date dirname basename rm cp mv env realpath wc sleep tee uname ls mkdir chmod; do
  ln -sf "$(command -v "$t")" "$NOTO_BIN/$t"
done
reset_marks
out="$(PATH="${NOTO_BIN}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 \
  src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_DEPLOY" 2>&1)"; rc=$?
assert_status "deploy-no-timeout-prerequisite-missing" "DEPLOY_FAILED REASON=prerequisite-missing WORKLOADS=-" "$out"
assert_status "deploy-no-timeout-names-it" "timeout is not installed" "$out"
assert_absent "deploy-no-timeout-never-runtime-unreachable" "runtime-unreachable" "$out"
out="$(PATH="${NOTO_BIN}" src_run "$LIB_COMMON" 'classify_env_failure apiserver' 2>&1)"
assert_rc "classify-no-timeout-names-prerequisite" 0 "$([[ "$out" == prerequisite-missing ]] && echo 0 || echo "1 (${out})")"
out="$(PATH="${NOTO_BIN}:$(dirname "$(command -v timeout)")" src_run "$LIB_COMMON" 'classify_env_failure apiserver' 2>&1)"
assert_absent "classify-with-timeout-probes-control" "prerequisite-missing" "$out"
# ...and a migration failure carries BOTH its own banner and the step line.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_JOB_COND_AFTER_APPLY="Failed BackoffLimitExceeded" dep)"; rc=$?
assert_status "deploy-migration-failure-step-line" "DEPLOY_FAILED REASON=step-failed STEP=run_migration_job" "$out"
assert_status "deploy-migration-failure-own-banner" "REASON=migration-failed" "$out"
# `kind get clusters` failing is UNREADABLE, never "missing".
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster STUB_KIND_LIST_FAIL=1 bash "$DEPLOY" 2>&1)"; rc=$?
assert_rc "deploy-kind-list-fail-rc" 1 "$rc"
assert_status "deploy-kind-list-fail-unreadable" "DEPLOY_FAILED REASON=blueprint-unreadable" "$out"
assert_absent "deploy-kind-list-fail-not-missing" "blueprint-missing" "$out"

# An image the node already holds is NOT reloaded (content-addressed: same ref, same bytes).
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_NODE_HAS="localhost/gc-service:${BUILT_TAG}" dep)"; rc=$?
assert_rc "deploy-node-has-ref-rc" 0 "$rc"
assert_absent "deploy-node-has-ref-not-reloaded" "save localhost/gc-service:${BUILT_TAG}" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_status "deploy-node-has-ref-others-still-loaded" "save localhost/mc-service:${BUILT_TAG}" "$(cat "${MARK}/podman.calls" 2>/dev/null)"

# A collector that does not roll out STOPS deploy before the root, naming the collector.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_ROLLOUT_FAIL=deployment/otel-collector dep)"; rc=$?
assert_rc "deploy-collector-failure-rc" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
assert_status "deploy-collector-failure-token" "DEPLOY_FAILED REASON=collector-rollout-failed WORKLOADS=dark-tower/deployment/otel-collector" "$out"
assert_no_marker "deploy-collector-failure-root-not-applied" "$MARK" "applied.kustomization.yaml"
# A root workload that does not roll out: ONE line naming it.
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_ROLLOUT_FAIL=deployment/mc-0 dep)"; rc=$?
assert_rc "deploy-rollout-failure-rc" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
assert_status "deploy-rollout-failure-token" "DEPLOY_FAILED REASON=rollout-failed WORKLOADS=dark-tower/deployment/mc-0" "$out"

# (test f) deploy's blueprint guard: a stale/missing platform fails LOUDLY naming provision,
# with ZERO build/apply calls. The guard runs the real check_blueprint against a stub
# provision.sh that answers like provision.sh --check.
PSTUB="${WORK}/provision-stub.sh"
cat > "$PSTUB" <<'STUB'
#!/usr/bin/env bash
[[ "$1" == "--check" ]] || { echo "provision stub: expected --check, got $*" >&2; exit 9; }
echo "BLUEPRINT ACTION=check REASON=${STUB_BP_REASON:-changed} RECORDED=aaaaaaaaaaaa CURRENT=bbbbbbbbbbbb CHANGED=kind-config"
[[ "${STUB_BP_REASON:-changed}" == match ]]
STUB
chmod +x "$PSTUB"
RUN_GUARDED='PROVISION_SH="${ARGS[0]}"; main'
for reason in changed missing unreadable; do
  reset_marks
  # Run as the devloop helper does, so the remedy is the container's command.
  out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster STUB_BP_REASON="$reason" \
    DT_CALLER=devloop-helper src_run "$DEPLOY" "$RUN_GUARDED" "$PSTUB" 2>&1)"; rc=$?
  assert_rc "deploy-guard-${reason}-rc" 1 "$([[ "$rc" -ne 0 ]] && echo 1 || echo 0)"
  assert_status "deploy-guard-${reason}-token" "DEPLOY_FAILED REASON=blueprint-${reason}" "$out"
  assert_status "deploy-guard-${reason}-names-provision" "dev-cluster provision" "$out"
  assert_rc "deploy-guard-${reason}-builds-nothing" 0 "$([[ -z "$(builds)" ]] && echo 0 || echo "1 ($(builds))")"
  assert_no_marker "deploy-guard-${reason}-applies-nothing" "$MARK" "applied"
done
# The same guard run by a person on the host names the host command, not the container's.
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster STUB_BP_REASON=changed \
  src_run -u DT_CALLER "$DEPLOY" "$RUN_GUARDED" "$PSTUB" 2>&1)"
assert_status "deploy-guard-host-names-setup" "./infra/kind/scripts/setup.sh" "$out"
assert_absent "deploy-guard-host-not-container-cmd" "dev-cluster provision" "$out"
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 DT_JOB_POLL_SECONDS=0 STUB_BP_REASON=match STUB_JOBS="$JOBS_OK" \
  src_run -u DT_HOST_GATEWAY_IP "$DEPLOY" "$RUN_GUARDED" "$PSTUB" 2>&1)"; rc=$?
assert_rc "deploy-guard-match-proceeds" 0 "$rc"
# No cluster at all: fails before the check, still naming provision, building nothing.
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=nosuchcluster bash "$DEPLOY" 2>&1)"; rc=$?
assert_rc "deploy-no-cluster-rc" 1 "$rc"
assert_status "deploy-no-cluster-token" "DEPLOY_FAILED REASON=blueprint-missing" "$out"
assert_rc "deploy-no-cluster-builds-nothing" 0 "$([[ -z "$(builds)" ]] && echo 0 || echo "1 ($(builds))")"
# The retired flags are rejected, never silently ignored.
for f in --only --skip-build --rebuild-all; do
  bash "$DEPLOY" "$f" >/dev/null 2>&1; assert_rc "deploy-rejects-retired${f}" 1 $?
  bash "$SETUP" "$f" >/dev/null 2>&1; assert_rc "setup-rejects-retired${f}" 1 $?
done

# The imperative Secrets/namespaces (provision.sh) are created as
# `create --dry-run=client -o yaml | apply -f -`: an idempotent shape, so a hand-run of a
# build step on a live cluster cannot fail with AlreadyExists. (A HALF-built cluster is never
# reused at all — provision records its blueprint only after a successful build.)
for obj in "secret generic ac-service-secrets" 'secret tls "${svc}-service-tls"' "namespace dark-tower "; do
  line="$(grep -A5 -F "create ${obj}" "$PROVISION" | tr '\n' ' ')"
  assert_rc "reuse-safe-create-${obj//[ \"\$\{\}]/-}" 0 "$([[ "$line" == *"--dry-run=client -o yaml | \${KUBECTL} apply -f -"* ]] && echo 0 || echo 1)"
done
# deploy never creates, regenerates or overwrites Secret/TLS material.
assert_absent "deploy-creates-no-secret" "create secret" "$(grep -v '^[[:space:]]*#' "$DEPLOY")"
assert_absent "deploy-runs-no-cert-recipe" "generate-dev-certs" "$(grep -v '^[[:space:]]*#' "$DEPLOY")"

# A FAILED migration Job stops deploy BEFORE the seeds and the root apply, and the host sqlx is
# never invoked (the retired host path).
reset_marks
out="$(STUB_JOB_COND_AFTER_APPLY="Failed BackoffLimitExceeded" dep)"; rc=$?
assert_rc "main-migrate-failure-rc" 1 "$rc"
assert_status "main-migrate-failure-token" "REASON=migration-failed" "$out"
assert_status "main-migrate-failure-reached-the-job" "render_migration_job" "$(cat "${MARK}/applied.migration.yaml" 2>/dev/null)"
assert_no_marker "main-migrate-failure-stops-before-seeds" "$MARK" "ran.kubectl-exec"
assert_no_marker "main-migrate-failure-stops-before-root" "$MARK" "applied.kustomization.yaml"
assert_no_marker "main-host-sqlx-never-invoked" "$MARK" "ran.host-sqlx"
for script in "$SETUP" "$DEPLOY" "$PROVISION"; do
  code="$(grep -vE '^[[:space:]]*#' "$script")"
  nm="$(basename "$script" .sh)"
  assert_absent "${nm}-no-sqlx-cli-branch-left" "sqlx-cli not installed" "$code"
  assert_absent "${nm}-no-host-sqlx-migrate-left" "sqlx migrate" "$code"
  assert_absent "${nm}-no-port-forward-migration-left" "run_migrations()" "$code"
  # The imperative ConfigMap patch and apply --prune (which could delete the imperatively-
  # created Secrets) must not come back.
  assert_absent "${nm}-has-no-configmap-patch" "patch configmap" "$code"
  assert_absent "${nm}-has-no-prune" "--prune" "$code"
done
# No unconditional restart anywhere a converge runs (setup.sh's access info still PRINTS the
# host-only "reset runtime state" commands for a human; it runs none).
for script in "$DEPLOY" "$PROVISION"; do
  assert_absent "$(basename "$script" .sh)-no-unconditional-restart" "rollout restart" "$(grep -vE '^[[:space:]]*#' "$script")"
done

# === (E) the cargo parallelism cap reaches every cargo invocation, incl. the IMAGE builds =======
# The image builds' release dependency cook + service build are the heaviest builds on the box
# and ran uncapped (nproc=32 jobs each) while the pipeline was capped — two devloops building
# images at once exhausted the 16GB WSL VM (2026-09-27). The cap now lives ONLY in
# .cargo/config.toml (build.jobs), which cargo reads everywhere in the workspace.

# The config sets a positive build.jobs under [build] (the single home of the number).
cfg_jobs="$(awk '/^\[build\]/{b=1; next} /^\[/{b=0} b && /^jobs[[:space:]]*=/{gsub(/[^0-9]/,""); print}' "${REPO_ROOT}/.cargo/config.toml")"
if [[ "$cfg_jobs" =~ ^[1-9][0-9]*$ ]]; then PASS=$((PASS + 1)); else
  FAIL=$((FAIL + 1)); FAILURES+=("[cargo-config-jobs] .cargo/config.toml must set a positive build.jobs under [build]; got '${cfg_jobs}'"); fi
# ...and nothing else restates it (a second copy of the number is how caps drift).
restated="$(grep -rlE 'CARGO_BUILD_JOBS[:=]-?[0-9]|--build-arg CARGO_BUILD_JOBS' "${REPO_ROOT}/scripts" "${REPO_ROOT}/infra" 2>/dev/null | grep -v '/setup.test.sh$' || true)"
if [ -z "$restated" ]; then PASS=$((PASS + 1)); else
  FAIL=$((FAIL + 1)); FAILURES+=("[cargo-cap-not-restated] the cap is restated outside .cargo/config.toml: ${restated}"); fi

# Every Dockerfile STAGE that compiles copies .cargo/ in BEFORE its first cargo compile (the
# dependency cook runs before `COPY . .`), so a new service's Dockerfile cannot build uncapped.
# `cargo install` counts too (the db-migrate image compiles sqlx-cli; ADR-0038 devloop 2).
df_count=0
for df in "${REPO_ROOT}"/infra/docker/*/Dockerfile; do
  df_count=$((df_count + 1))
  bad="$(awk '
    /^FROM /                        { stage=$0; cfg=0; next }
    /^COPY (\.cargo\/? \.cargo\/?|\. \.)/ { cfg=1 }
    /^RUN cargo (chef cook|build|install)/  { if (!cfg) print stage }
  ' "$df" | sort -u)"
  if [ -z "$bad" ]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[dockerfile-cargo-config-copied] ${df#"${REPO_ROOT}"/}: a cargo compile runs in stage '${bad}' before .cargo/ is copied in, so it ignores the build.jobs cap"); fi
done
# Positive control: the loop must have checked the four service Dockerfiles, not zero.
if [ "$df_count" -ge 4 ]; then PASS=$((PASS + 1)); else
  FAIL=$((FAIL + 1)); FAILURES+=("[dockerfile-cap-nonvacuous] expected >=4 service Dockerfiles, found ${df_count}"); fi

# ---- extract_manifest_images: a comment must never become an image ----------------
# Regression for Gate 2 attempt 1: a ConfigMap-literal comment "# image: without this"
# was extracted as an image and `podman pull docker.io/without` broke bring-up.
emi() { src_run "$DEPLOY" 'printf "%s" "${ARGS[0]}" | extract_manifest_images' "$1"; }
# Value-equality check on the extractor's OUTPUT, feeding the shared PASS/FAIL
# counters. An exit code cannot express "extracted nothing" vs "extracted the wrong
# thing", and extracting a wrong thing is precisely the regression.
emi_expect() {
  local label="$1" expected="$2" actual="$3"
  if [[ "$actual" == "$expected" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[${label}] expected=${expected@Q} actual=${actual@Q}"); fi
}

emi_out=$(emi $'      containers:\n      - name: c\n        image: otel/opentelemetry-collector-contrib:0.161.0\n')
emi_expect "emi-real-image-key"         "otel/opentelemetry-collector-contrib:0.161.0" "$emi_out"
emi_out=$(emi $'      - image: redis:7.2-alpine\n')
emi_expect "emi-list-item-image-key"    "redis:7.2-alpine" "$emi_out"
emi_out=$(emi $'      # image: without this, the second writer is dropped\n')
emi_expect "emi-comment-never-matches"  "" "$emi_out"
emi_out=$(emi $'    # Measured on the pinned image: without this, the total reads 29\n')
emi_expect "emi-midline-prose-never-matches" "" "$emi_out"
emi_out=$(emi $'  data:\n    note: the image: field is set elsewhere\n')
emi_expect "emi-value-text-never-matches" "" "$emi_out"

# === (F) ADR-0038 devloop 2: shared pins and retired paths ======================================

# --- cargo_lock_version: the ONE Cargo.lock reader (sqlx-cli pin for both images) -----------
LIB="${REPO_ROOT}/infra/lib/cargo-lock-version.sh"
clv() { src_run "$LIB" 'cargo_lock_version "${ARGS[@]}"' "$1" "$2" 2>&1; }
LOCKS="${WORK}/locks"; mkdir -p "$LOCKS"
printf '[[package]]\nname = "sqlx-core"\nversion = "9.9.9"\n\n[[package]]\nname = "sqlx"\nversion = "0.8.6"\n' > "$LOCKS/one.lock"
printf '[[package]]\nname = "sqlx-core"\nversion = "0.8.6"\n' > "$LOCKS/none.lock"
printf '[[package]]\nname = "sqlx"\nversion = "0.7.4"\n\n[[package]]\nname = "sqlx"\nversion = "0.8.6"\n' > "$LOCKS/two.lock"
out="$(clv "$LOCKS/one.lock" sqlx)"; rc=$?
assert_rc "cargo-lock-version-exact-match-rc" 0 "$rc"
emi_expect "cargo-lock-version-exact-match" "0.8.6" "$out"
emi_expect "cargo-lock-version-not-fooled-by-sqlx-core" "0.8.6" "$out"
out="$(clv "$LOCKS/none.lock" sqlx)"; rc=$?
assert_rc "cargo-lock-version-missing-fails" 1 "$rc"
assert_status "cargo-lock-version-missing-says-so" "no \`sqlx\` package" "$out"
out="$(clv "$LOCKS/two.lock" sqlx)"; rc=$?
assert_rc "cargo-lock-version-ambiguous-fails" 1 "$rc"
assert_status "cargo-lock-version-ambiguous-says-so" "more than one version" "$out"
out="$(clv "${REPO_ROOT}/Cargo.lock" sqlx)"; rc=$?
assert_rc "cargo-lock-version-real-lock" 0 "$([[ $rc -eq 0 && "$out" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] && echo 0 || echo "1 (${out})")"
# Both images take the pin from that reader — never a hand-typed version.
assert_status "devloop-sh-derives-sqlx-cli-version" 'cargo_lock_version "${script_dir}/../../Cargo.lock" sqlx' "$(cat "${REPO_ROOT}/infra/devloop/devloop.sh")"
assert_status "devloop-dockerfile-pins-sqlx-cli" '--version "=${SQLX_CLI_VERSION}"' "$(cat "${REPO_ROOT}/infra/devloop/Dockerfile")"
assert_status "db-migrate-dockerfile-pins-sqlx-cli" '--version "=${SQLX_CLI_VERSION}"' "$(cat "${REPO_ROOT}/infra/docker/db-migrate/Dockerfile")"

# --- Rust version: rust-toolchain.toml is the ONE source; every site derives from it ----------
# rustup reads the file on every cargo/rustc call (CI, devloop container, host helper build);
# the images take `ARG RUST_VERSION` (no default) from the ONE reader, infra/lib/rust-toolchain.sh,
# via devloop.sh and deploy.sh:image_build_args. These checks prove DERIVATION (a sentinel value
# comes out of every consumer) and that no literal copy is left anywhere it could drift.
RTLIB="${REPO_ROOT}/infra/lib/rust-toolchain.sh"
rtv() { src_run "$RTLIB" 'rust_toolchain_version "${ARGS[@]}"' "$1" 2>&1; }
RTS="${WORK}/rt"; mkdir -p "$RTS"
rt_file() { printf '%b' "$2" > "$RTS/$1.toml"; }
rt_file good      '[toolchain]\nchannel = "9.8.7"\nprofile = "minimal"\ncomponents = ["rustfmt", "clippy"]\n'
rt_file commented '# header\n\n[toolchain]\nchannel = "9.8.7"\n'
rt_file nochannel '[toolchain]\nprofile = "minimal"\n'
rt_file dup       '[toolchain]\nchannel = "9.8.7"\nchannel = "9.8.7"\n'
rt_file stable    '[toolchain]\nchannel = "stable"\n'
rt_file nightly   '[toolchain]\nchannel = "nightly"\n'
rt_file twopart   '[toolchain]\nchannel = "1.99"\n'
rt_file trailing  '[toolchain]\nchannel = "9.8.7" # pinned\n'
rt_file squote    "[toolchain]\nchannel = '9.8.7'\n"
rt_file path      '[toolchain]\nchannel = "9.8.7"\npath = "/tmp/evil"\n'
rt_file unknown   '[toolchain]\nchannel = "9.8.7"\ntargets = ["wasm32-unknown-unknown"]\n'
rt_file twotables '[toolchain]\nchannel = "9.8.7"\n[other]\nchannel = "1.0.0"\n'
rt_file notable   'channel = "9.8.7"\n'
out="$(rtv "$RTS/good.toml")"; rc=$?
assert_rc  "rust-toolchain-exact-rc" 0 "$rc"
emi_expect "rust-toolchain-exact" "9.8.7" "$out"
out="$(rtv "$RTS/commented.toml")"; emi_expect "rust-toolchain-comments-and-blanks-ok" "9.8.7" "$out"
for bad in nochannel dup stable nightly twopart trailing squote path unknown twotables notable; do
  out="$(rtv "$RTS/${bad}.toml")"; rc=$?
  assert_rc "rust-toolchain-${bad}-rejected" 1 "$rc"
  assert_status "rust-toolchain-${bad}-says-why" "ERROR: rust_toolchain_version" "$out"
done
out="$(rtv "$RTS/path.toml")";    assert_status "rust-toolchain-path-names-allow-list" "allow-list" "$out"
out="$(rtv "$RTS/absent.toml")"; rc=$?
assert_rc "rust-toolchain-unreadable-fails" 1 "$rc"
RUST_PIN="$(rtv "${REPO_ROOT}/rust-toolchain.toml")"; rc=$?
assert_rc "rust-toolchain-real-file" 0 "$([[ $rc -eq 0 && "$RUST_PIN" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] && echo 0 || echo "1 (${RUST_PIN})")"
# The ONLY toolchain file in the tree is the root rust-toolchain.toml. rustup resolves the
# override by walking UP from the cwd, so a nested `rust-toolchain[.toml]` (e.g. under a fuzz
# crate) or a legacy plain-text root `rust-toolchain` would win for every cargo call made there,
# without ever passing through the reader's allow-list / X.Y.Z check.
toolchain_files() {  # $1 = root; prints every toolchain file, relative to the root
  find "$1" \( -name rust-toolchain -o -name rust-toolchain.toml \) \
    -not -path '*/target/*' -not -path '*/node_modules/*' -not -path '*/.git/*' | sed "s#^$1/##" | sort
}
tf="$(toolchain_files "$REPO_ROOT")"
emi_expect "only-root-rust-toolchain-file" "rust-toolchain.toml" "$tf"
TFROOT="${WORK}/tfroot"; mkdir -p "$TFROOT/crates/x/fuzz"; : > "$TFROOT/rust-toolchain.toml"
: > "$TFROOT/crates/x/fuzz/rust-toolchain.toml"
tf="$(toolchain_files "$TFROOT")"
assert_rc "nested-rust-toolchain-file-trips" 0 "$([[ "$tf" != "rust-toolchain.toml" ]] && echo 0 || echo 1)"
rm "$TFROOT/crates/x/fuzz/rust-toolchain.toml"; : > "$TFROOT/rust-toolchain"
tf="$(toolchain_files "$TFROOT")"
assert_rc "legacy-rust-toolchain-file-trips" 0 "$([[ "$tf" != "rust-toolchain.toml" ]] && echo 0 || echo 1)"

# Derivation, not agreement: deploy.sh:image_build_args against a root whose file says 9.8.7.
RTROOT="${WORK}/rtroot"; mkdir -p "$RTROOT/infra"; cp "$RTS/good.toml" "$RTROOT/rust-toolchain.toml"; cp "${REPO_ROOT}/Cargo.lock" "$RTROOT/"
printf 'cargo-chef 7.6.5\n' > "$RTROOT/infra/cargo-tools.versions"
iba() { src_run "$DEPLOY" 'PROJECT_ROOT="${ARGS[0]}"; image_build_args "${ARGS[1]}"' "$RTROOT" "$1" 2>&1 | tr '\n' ' '; }
out="$(iba localhost/ac-service)"
assert_status "image-build-args-service-derives-rust" "--build-arg RUST_VERSION=9.8.7 " "$out"
assert_status "image-build-args-service-derives-cargo-chef" "--build-arg CARGO_CHEF_VERSION=7.6.5 " "$out"
out="$(iba localhost/db-migrate)"
assert_status "image-build-args-migrate-derives-rust" "--build-arg RUST_VERSION=9.8.7 " "$out"
assert_status "image-build-args-migrate-keeps-sqlx" "--build-arg SQLX_CLI_VERSION=" "$out"
rm "$RTROOT/rust-toolchain.toml"
out="$(src_run "$DEPLOY" 'PROJECT_ROOT="${ARGS[0]}"; image_build_args "${ARGS[1]}"' "$RTROOT" localhost/ac-service 2>&1)"; rc=$?
assert_rc "image-build-args-missing-toolchain-fails" 1 "$rc"
# devloop.sh: ONE build site (build_devloop_image), reached from both the image-only
# `--rebuild` path and the launch path; its RUST_VERSION build-arg is a variable assigned
# from the reader, never a literal.
DLSH="$(cat "${REPO_ROOT}/infra/devloop/devloop.sh")"
assert_status "devloop-sh-derives-rust-version" 'rust_toolchain_version "${script_dir}/../../rust-toolchain.toml"' "$DLSH"
dl_builds="$(grep -c 'podman build ' <<< "$DLSH")"
dl_callers="$(grep -cE '^[[:space:]]+build_devloop_image "\$SCRIPT_DIR" "\$IMAGE"$' <<< "$DLSH")"
dl_rust_var="$(grep -c -- '--build-arg "RUST_VERSION=\${RUST_VERSION}"' <<< "$DLSH")"
dl_rust_any="$(grep -c -- '--build-arg "RUST_VERSION=' <<< "$DLSH")"
dl_rust_set="$(grep -cF 'RUST_VERSION="$(read_rust_version "$script_dir")"' <<< "$DLSH")"
assert_rc "devloop-sh-one-build-site" 0 "$([[ "$dl_builds" -eq 1 && "$dl_callers" -eq 2 && "$dl_rust_var" -eq 1 && "$dl_rust_any" -eq 1 && "$dl_rust_set" -eq 1 ]] && echo 0 || echo "1 (builds=${dl_builds} callers=${dl_callers} var=${dl_rust_var} any=${dl_rust_any} from-reader=${dl_rust_set})")"
assert_rc "devloop-sh-no-literal-rust-assignment" 0 "$(grep -qE '(^|[^_A-Z])RUST_VERSION=["'"'"']?[0-9]' <<< "$DLSH" && echo 1 || echo 0)"
assert_status "devloop-sh-helper-built-from-repo-root" '(cd "$REPO_ROOT" && cargo build --release -p devloop-helper' "$DLSH"

# No literal Rust version anywhere it could drift. Positive controls keep the scans non-vacuous.
rust_literal_sites() {  # $1 = root; prints one line per literal Rust-version site
  local root="$1"
  grep -HnE '^ARG RUST_VERSION=' "${root}"/infra/docker/*/Dockerfile "${root}/infra/devloop/Dockerfile" 2>/dev/null
  grep -HniE '^[[:space:]]*FROM[[:space:]].*rust:[0-9]' "${root}"/infra/docker/*/Dockerfile "${root}/infra/devloop/Dockerfile" 2>/dev/null
  grep -HnE 'rustup (toolchain install|default) [0-9]|rustup default' "${root}"/.github/workflows/*.yml 2>/dev/null
}
rl="$(rust_literal_sites "$REPO_ROOT")"
assert_rc "rust-version-no-literal-sites" 0 "$([[ -z "$rl" ]] && echo 0 || echo "1 (${rl//$'\n'/ | })")"
rv_files="$(ls "${REPO_ROOT}"/infra/docker/*/Dockerfile "${REPO_ROOT}/infra/devloop/Dockerfile" | wc -l)"
rv_from="$(cat "${REPO_ROOT}"/infra/docker/*/Dockerfile "${REPO_ROOT}/infra/devloop/Dockerfile" | grep -cE '^FROM docker\.io/library/rust:\$\{RUST_VERSION\}-')"
rv_arg="$(cat "${REPO_ROOT}"/infra/docker/*/Dockerfile "${REPO_ROOT}/infra/devloop/Dockerfile" | grep -cx 'ARG RUST_VERSION')"
assert_rc "rust-version-dockerfiles-nonvacuous" 0 "$([[ "$rv_files" -ge 6 && "$rv_from" -eq "$rv_files" && "$rv_arg" -eq "$rv_files" ]] && echo 0 || echo "1 (files=${rv_files} from=${rv_from} arg=${rv_arg})")"
rv_install="$(cat "${REPO_ROOT}"/.github/workflows/*.yml | grep -cE '^[[:space:]]+rustup toolchain install$')"
assert_rc "rust-version-workflows-read-the-file" 0 "$([[ "$rv_install" -ge 4 ]] && echo 0 || echo "1 (${rv_install} no-arg installs)")"
assert_rc "dockerignore-excludes-rust-toolchain" 0 "$(grep -qx 'rust-toolchain.toml' "${REPO_ROOT}/.dockerignore" && echo 0 || echo 1)"
# cargo-chef: ONE value (infra/cargo-tools.versions, read by deploy.sh), no Dockerfile default,
# never an unpinned install.
chef_sites="$(cat "${REPO_ROOT}"/infra/docker/*-service/Dockerfile | grep -c 'cargo install cargo-chef --locked --version "=${CARGO_CHEF_VERSION}"')"
chef_files="$(ls "${REPO_ROOT}"/infra/docker/*-service/Dockerfile | wc -l)"
assert_rc "cargo-chef-pinned-in-every-service-image" 0 "$([[ "$chef_files" -ge 4 && "$chef_sites" -eq "$chef_files" ]] && echo 0 || echo "1 (${chef_sites} of ${chef_files})")"
assert_rc "cargo-chef-no-dockerfile-default" 0 "$(grep -qE '^ARG CARGO_CHEF_VERSION=' "${REPO_ROOT}"/infra/docker/*/Dockerfile && echo 1 || echo 0)"
assert_rc "cargo-chef-no-unpinned-install" 0 "$(grep -hE 'cargo install cargo-chef' "${REPO_ROOT}"/infra/docker/*/Dockerfile | grep -qv -- '--version' && echo 1 || echo 0)"
# MSRV (`rust-version`) is deliberately NOT declared: this is an application workspace, so the
# minimum Rust IS the pinned toolchain and a `rust-version` would be a second copy of it with no
# consumer (resolver 2 — not MSRV-aware). See docs/devloop-outputs/2026-10-05-rust-latest-stable/.
msrv="$(find "${REPO_ROOT}" -name Cargo.toml -not -path '*/target/*' -not -path '*/node_modules/*' -exec grep -lE '^[[:space:]]*rust-version[[:space:]]*=' {} + 2>/dev/null)"
assert_rc "no-rust-version-msrv-declared" 0 "$([[ -z "$msrv" ]] && echo 0 || echo "1 (${msrv//$'\n'/ })")"
# Negative controls: each literal kind trips the scan.
RVROOT="${WORK}/rvroot"; mkdir -p "$RVROOT/infra/docker/x" "$RVROOT/infra/devloop" "$RVROOT/.github/workflows"
printf 'ARG RUST_VERSION\nFROM docker.io/library/rust:${RUST_VERSION}-slim-bookworm\n' > "$RVROOT/infra/devloop/Dockerfile"
printf 'ARG RUST_VERSION=1.99.0\n' > "$RVROOT/infra/docker/x/Dockerfile"
assert_rc "rust-version-arg-default-trips" 0 "$([[ -n "$(rust_literal_sites "$RVROOT")" ]] && echo 0 || echo 1)"
printf 'FROM docker.io/library/rust:1.99.0-slim-bookworm\n' > "$RVROOT/infra/docker/x/Dockerfile"
assert_rc "rust-version-from-literal-trips" 0 "$([[ -n "$(rust_literal_sites "$RVROOT")" ]] && echo 0 || echo 1)"
printf 'from --platform=linux/amd64 docker.io/library/rust:1.99.0-slim-bookworm AS extra\n' > "$RVROOT/infra/docker/x/Dockerfile"
assert_rc "rust-version-from-flag-stage-literal-trips" 0 "$([[ -n "$(rust_literal_sites "$RVROOT")" ]] && echo 0 || echo 1)"
printf 'ARG RUST_VERSION\n' > "$RVROOT/infra/docker/x/Dockerfile"
assert_rc "rust-version-clean-root-passes" 0 "$([[ -z "$(rust_literal_sites "$RVROOT")" ]] && echo 0 || echo 1)"
printf '        run: rustup toolchain install 1.99.0 --profile minimal\n' > "$RVROOT/.github/workflows/w.yml"
assert_rc "rust-version-workflow-install-literal-trips" 0 "$([[ -n "$(rust_literal_sites "$RVROOT")" ]] && echo 0 || echo 1)"
printf '        run: |\n          rustup toolchain install\n          rustup default 1.99.0\n' > "$RVROOT/.github/workflows/w.yml"
assert_rc "rust-version-workflow-default-trips" 0 "$([[ -n "$(rust_literal_sites "$RVROOT")" ]] && echo 0 || echo 1)"

# --- cargo tool pins: infra/cargo-tools.versions is the ONE source (devloop image + every workflow)
# cargo-nextest / cargo-audit / cargo-llvm-cov / cargo-fuzz are read through the ONE reader,
# infra/lib/cargo-tools.sh; sqlx-cli's pin is Cargo.lock's `sqlx` (checked above).
CTLIB="${REPO_ROOT}/infra/lib/cargo-tools.sh"
ctv() { src_run "$CTLIB" 'cargo_tool_version "${ARGS[@]}"' "$@" 2>&1; }
CTS="${WORK}/ct"; mkdir -p "$CTS"
printf '# header\n\ncargo-nextest 9.8.7\ncargo-audit 1.2.3\n' > "$CTS/good"
printf 'cargo-nextest 9.8\n'                         > "$CTS/twopart"
printf 'cargo-nextest 9.8.7 # pinned\n'              > "$CTS/trailing"
printf 'cargo-nextest 9.8.7\ncargo-nextest 9.8.8\n'  > "$CTS/dup"
printf 'cargo-audit 1.2.3\n'                         > "$CTS/absent"
out="$(ctv "$CTS/good" cargo-nextest)"; rc=$?
assert_rc  "cargo-tools-exact-rc" 0 "$rc"
emi_expect "cargo-tools-exact" "9.8.7" "$out"
emi_expect "cargo-tools-second-entry" "1.2.3" "$(ctv "$CTS/good" cargo-audit)"
for bad in twopart trailing dup absent; do
  out="$(ctv "$CTS/${bad}" cargo-nextest)"; rc=$?
  assert_rc     "cargo-tools-${bad}-rejected"  1 "$rc"
  assert_status "cargo-tools-${bad}-says-why"  "ERROR: cargo_tool_version" "$out"
done
out="$(ctv "$CTS/missing-file" cargo-nextest)"; rc=$?
assert_rc "cargo-tools-unreadable-fails" 1 "$rc"
for tool in cargo-nextest cargo-audit cargo-llvm-cov cargo-fuzz cargo-chef; do
  out="$(ctv "${REPO_ROOT}/infra/cargo-tools.versions" "$tool")"; rc=$?
  assert_rc "cargo-tools-real-${tool}" 0 "$([[ $rc -eq 0 && "$out" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] && echo 0 || echo "1 (${out})")"
done

# devloop.sh: each image tool pin is read through the reader and passed as its build arg;
# the Dockerfile has no defaults for them and installs exactly those versions.
DLDF="$(cat "${REPO_ROOT}/infra/devloop/Dockerfile")"
for pair in CARGO_NEXTEST_VERSION:cargo-nextest CARGO_LLVM_COV_VERSION:cargo-llvm-cov CARGO_AUDIT_VERSION:cargo-audit; do
  var="${pair%%:*}"; tool="${pair#*:}"
  assert_status "devloop-sh-reads-${tool}"      "${var}=\"\$(read_cargo_tool_version \"\$script_dir\" ${tool})\"" "$DLSH"
  assert_status "devloop-sh-passes-${tool}"     "--build-arg \"${var}=\${${var}}\"" "$DLSH"
  assert_rc     "devloop-dockerfile-no-default-${tool}" 0 "$(grep -qE "^ARG ${var}=" <<< "$DLDF" && echo 1 || echo 0)"
  assert_status "devloop-dockerfile-installs-${tool}" "cargo install ${tool} --locked --version \"=\${${var}}\"" "$DLDF"
done

# --- Playwright pin: pnpm-lock.yaml is the ONE source (devloop image) ------------------------
# The devloop image's baked Chromium must match the `playwright` package the tests run. The
# Dockerfile takes `ARG PLAYWRIGHT_VERSION` (no default) from the ONE reader,
# infra/lib/pnpm-lock-version.sh, via devloop.sh. Reader contract + derivation + the
# package.json pins agreeing with the lockfile.
PLLIB="${REPO_ROOT}/infra/lib/pnpm-lock-version.sh"
plv() { src_run "$PLLIB" 'pnpm_lock_version "${ARGS[@]}"' "$1" "$2" 2>&1; }
PLS="${WORK}/pnpm-locks"; mkdir -p "$PLS"
pl_file() { printf '%b' "$2" > "$PLS/$1.yaml"; }
# importers + snapshots carry DIFFERENT playwright versions (9.9.9): only packages: counts.
pl_file good "lockfileVersion: '9.0'\n\nimporters:\n\n  packages/web-app:\n    devDependencies:\n      playwright:\n        specifier: 9.9.9\n        version: 9.9.9\n\npackages:\n\n  playwright-core@9.9.9:\n    resolution: {integrity: sha512-x}\n\n  playwright@1.62.1:\n    resolution: {integrity: sha512-y}\n\n  '@vitest/browser-playwright@4.1.10':\n    resolution: {integrity: sha512-z}\n\nsnapshots:\n\n  playwright@9.9.9:\n    dependencies:\n      playwright-core: 9.9.9\n\n  '@vitest/browser-playwright@4.1.10(playwright@9.9.9)':\n    dependencies:\n      playwright: 9.9.9\n"
pl_file quoted "packages:\n\n  'playwright@1.62.1':\n    resolution: {integrity: sha512-y}\n"
pl_file peer   "packages:\n\n  playwright@1.62.1(foo@1.0.0):\n    resolution: {integrity: sha512-y}\n"
pl_file pre    "packages:\n\n  playwright@1.63.0-alpha.2:\n    resolution: {integrity: sha512-y}\n"
pl_file two    "packages:\n\n  playwright@1.62.1:\n    resolution: {}\n\n  playwright@1.55.0:\n    resolution: {}\n"
pl_file none   "packages:\n\n  playwright-core@1.62.1:\n    resolution: {}\n\nsnapshots:\n\n  playwright@1.62.1: {}\n"
pl_file badver "packages:\n\n  playwright@1.62.1;rm:\n    resolution: {}\n"
out="$(plv "$PLS/good.yaml" playwright)"; rc=$?
assert_rc  "pnpm-lock-version-exact-rc" 0 "$rc"
emi_expect "pnpm-lock-version-packages-section-only" "1.62.1" "$out"
emi_expect "pnpm-lock-version-not-fooled-by-playwright-core" "9.9.9" "$(plv "$PLS/good.yaml" playwright-core)"
emi_expect "pnpm-lock-version-scoped-quoted-key" "4.1.10" "$(plv "$PLS/good.yaml" @vitest/browser-playwright)"
emi_expect "pnpm-lock-version-quoted-key"  "1.62.1" "$(plv "$PLS/quoted.yaml" playwright)"
emi_expect "pnpm-lock-version-peer-suffix" "1.62.1" "$(plv "$PLS/peer.yaml" playwright)"
emi_expect "pnpm-lock-version-prerelease"  "1.63.0-alpha.2" "$(plv "$PLS/pre.yaml" playwright)"
out="$(plv "$PLS/two.yaml" playwright)"; rc=$?
assert_rc     "pnpm-lock-version-ambiguous-fails" 1 "$rc"
assert_status "pnpm-lock-version-ambiguous-says-so" "more than one version" "$out"
assert_status "pnpm-lock-version-ambiguous-lists-versions" "1.55.0 1.62.1" "$out"
out="$(plv "$PLS/none.yaml" playwright)"; rc=$?
assert_rc     "pnpm-lock-version-missing-fails" 1 "$rc"
assert_status "pnpm-lock-version-missing-says-so" "no \`playwright\` package" "$out"
out="$(plv "$PLS/badver.yaml" playwright)"; rc=$?
assert_rc     "pnpm-lock-version-bad-shape-fails" 1 "$rc"
assert_status "pnpm-lock-version-bad-shape-says-so" "is not X.Y.Z" "$out"
out="$(plv "$PLS/absent.yaml" playwright)"; rc=$?
assert_rc "pnpm-lock-version-unreadable-fails" 1 "$rc"
re_lib="$(sed -n "s/^__PNPM_LOCK_VERSION_RE='\(.*\)'$/\1/p" "$PLLIB")"
PW_PIN="$(plv "${REPO_ROOT}/pnpm-lock.yaml" playwright)"; rc=$?
assert_rc "pnpm-lock-version-real-lock" 0 "$([[ $rc -eq 0 && -n "$re_lib" && "$PW_PIN" =~ $re_lib ]] && echo 0 || echo "1 (${PW_PIN})")"
# Every workspace package declaring `playwright` pins EXACTLY the version the lockfile
# resolves — so the packages agree with each other AND with the image. Set derived by glob;
# non-vacuous (web-app + sdk-svelte today).
pw_pins=0; pw_bad=""
for pj in "${REPO_ROOT}"/packages/*/package.json; do
  pin="$(grep -oE '"playwright"[[:space:]]*:[[:space:]]*"[^"]*"' "$pj" | sed -E 's/.*"([^"]*)"$/\1/')"
  [[ -n "$pin" ]] || continue
  pw_pins=$((pw_pins + 1))
  [[ "$pin" == "$PW_PIN" ]] || pw_bad+=" ${pj#"${REPO_ROOT}/"}=${pin}"
done
assert_rc "playwright-package-pins-match-lock" 0 "$([[ "$pw_pins" -ge 2 && -z "$pw_bad" ]] && echo 0 || echo "1 (pins=${pw_pins} lock=${PW_PIN}${pw_bad})")"
# devloop.sh derives + passes it; the Dockerfile has no default and no literal version.
assert_status "devloop-sh-derives-playwright-version" 'pnpm_lock_version "${script_dir}/../../pnpm-lock.yaml" playwright' "$DLSH"
assert_status "devloop-sh-reads-playwright"  'PLAYWRIGHT_VERSION="$(read_playwright_version "$script_dir")"' "$DLSH"
assert_status "devloop-sh-passes-playwright" '--build-arg "PLAYWRIGHT_VERSION=${PLAYWRIGHT_VERSION}"' "$DLSH"
assert_rc     "devloop-dockerfile-bare-playwright-arg" 0 "$(grep -cx 'ARG PLAYWRIGHT_VERSION' <<< "$DLDF" | grep -qx 1 && echo 0 || echo 1)"
assert_status "devloop-dockerfile-runs-playwright-install" 'playwright-install.sh "${PLAYWRIGHT_VERSION}"' "$DLDF"
pw_lit="$(cat "${REPO_ROOT}/infra/devloop/Dockerfile" "${REPO_ROOT}/infra/devloop/devloop.sh" | grep -nE 'playwright@[0-9]|PLAYWRIGHT_VERSION=[0-9]' || true)"
assert_rc "playwright-no-literal-version" 0 "$([[ -z "$pw_lit" ]] && echo 0 || echo "1 (${pw_lit//$'\n'/ | })")"

# --- playwright-install.sh: bounded + retried browser download (hermetic, stub npx) ----------
PWI="${REPO_ROOT}/infra/devloop/playwright-install.sh"
# Version-shape regex: two byte-identical copies (the image build context can't source the
# reader) — drift guard. Non-vacuous: both extractions must be non-empty.
re_pwi="$(sed -n "s/^__PLAYWRIGHT_VERSION_RE='\(.*\)'$/\1/p" "$PWI")"
assert_rc "playwright-version-regex-copies-identical" 0 "$([[ -n "$re_lib" && "$re_lib" == "$re_pwi" ]] && echo 0 || echo "1 (lib=${re_lib@Q} install=${re_pwi@Q})")"
PWS="${WORK}/pwi"; mkdir -p "$PWS/bin"
# Stub npx: counts invocations in $PWS/count; behaviour from $PWS/mode:
#   ok | fail | hang | hang-ignore-term | fail-then-ok. Each attempt plants a partial browser
#   dir and records whether the previous attempt's dir was still present.
cat > "$PWS/bin/npx" <<'STUB'
#!/usr/bin/env bash
n=$(( $(cat "$PWI_DIR/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$PWI_DIR/count"
[[ -e "$PLAYWRIGHT_BROWSERS_PATH/partial" ]] && echo "stale-on-attempt-$n" >> "$PWI_DIR/stale"
mkdir -p "$PLAYWRIGHT_BROWSERS_PATH"; : > "$PLAYWRIGHT_BROWSERS_PATH/partial"
echo "$*" > "$PWI_DIR/args"
case "$(cat "$PWI_DIR/mode")" in
  ok) exit 0 ;;
  fail) exit 3 ;;
  hang) exec sleep 30 ;;
  hang-ignore-term) trap '' TERM; sleep 30 & wait; sleep 30 ;;
  fail-then-ok) [[ "$n" -ge 2 ]] && exit 0 || exit 3 ;;
esac
STUB
chmod +x "$PWS/bin/npx"
pwi_run() {  # $1 = mode, rest = args; sets out/rc/attempts
  rm -rf "$PWS/count" "$PWS/stale" "$PWS/args" "$PWS/browsers"; echo "$1" > "$PWS/mode"; shift
  out="$(PATH="$PWS/bin:$PATH" PWI_DIR="$PWS" PLAYWRIGHT_BROWSERS_PATH="$PWS/browsers" \
    PLAYWRIGHT_INSTALL_TIMEOUT_SECS=0.2 PLAYWRIGHT_INSTALL_KILL_AFTER_SECS=0.2 \
    PLAYWRIGHT_INSTALL_ATTEMPTS=3 PLAYWRIGHT_INSTALL_RETRY_SLEEP_SECS=0 \
    timeout 60 bash "$PWI" "$@" 2>&1)"; rc=$?
  attempts="$(cat "$PWS/count" 2>/dev/null || echo 0)"
}
pwi_run ok 1.62.1
assert_rc  "pwi-ok-rc" 0 "$rc"
emi_expect "pwi-ok-one-attempt" "1" "$attempts"
emi_expect "pwi-ok-pinned-args" "-y playwright@1.62.1 install chromium" "$(cat "$PWS/args")"
pwi_run fail-then-ok 1.62.1
assert_rc  "pwi-retry-rc" 0 "$rc"
emi_expect "pwi-retry-two-attempts" "2" "$attempts"
assert_status "pwi-retry-logs-attempt-rc" "attempt 1/3 rc=3" "$out"
assert_rc "pwi-partial-wiped-between-attempts" 0 "$([[ ! -e "$PWS/stale" ]] && echo 0 || echo "1 ($(cat "$PWS/stale"))")"
pwi_run hang 1.62.1
assert_rc  "pwi-hang-fails" 1 "$rc"
emi_expect "pwi-hang-exactly-n-attempts" "3" "$attempts"
assert_status "pwi-hang-logs-timeout" "rc=124 (124=timeout" "$out"
assert_status "pwi-hang-names-step" "ERROR: devloop image step 'npx playwright@1.62.1 install chromium' failed after 3 attempt(s) (timeout 0.2s each, last rc=124)" "$out"
pwi_run hang-ignore-term 1.62.1
assert_rc  "pwi-term-ignored-still-bounded" 1 "$rc"
emi_expect "pwi-term-ignored-exactly-n-attempts" "3" "$attempts"
assert_status "pwi-term-ignored-killed" "last rc=137" "$out"
pwi_run fail 1.62.1
assert_rc  "pwi-fail-fails" 1 "$rc"
assert_rc "pwi-fail-wipes-final-partial" 0 "$([[ ! -e "$PWS/browsers" ]] && echo 0 || echo 1)"
pwi_run ok '1.62.1;rm -rf /'
assert_rc  "pwi-bad-version-refused" 1 "$rc"
emi_expect "pwi-bad-version-no-npx" "0" "$attempts"
assert_status "pwi-bad-version-says-why" "is not X.Y.Z" "$out"

# No cargo tool install in a workflow or an image may float or carry a literal version:
# `tool: cargo-x` must be `@${{ steps.pins.outputs.cargo-x }}`, and every `cargo install`
# must be --locked with a `--version "=${VAR}"` variable. Scans ALL workflow files and ALL
# Dockerfiles, so the next unpinned install is caught wherever it lands. sqlx-cli is pinned
# by Cargo.lock, not this file, and checked above (db-migrate's install spans two lines).
# Positive controls below.
cargo_tool_pin_violations() {  # $1 = root; prints one line per unpinned/literal install
  local root="$1" f
  for f in "${root}"/.github/workflows/*.yml "${root}/infra/devloop/Dockerfile" "${root}"/infra/docker/*/Dockerfile; do
    [[ -r "$f" ]] || continue
    grep -HnE '^[[:space:]]*tool:[[:space:]]*cargo-' "$f" \
      | grep -vE 'tool:[[:space:]]*(cargo-[a-z0-9-]+)@\$\{\{ steps\.pins\.outputs\.\1 \}\}[[:space:]]*$'
    grep -HnE 'cargo install [a-z]' "$f" \
      | grep -vE 'cargo install sqlx-cli ' \
      | grep -vE 'cargo install [a-z0-9-]+ (.* )?--locked( .*)? --version "=\$\{[A-Za-z_]+\}"' \
      | grep -vE '^[^:]+:[0-9]+:[[:space:]]*#'
  done
  return 0
}
cv="$(cargo_tool_pin_violations "$REPO_ROOT")"
assert_rc "cargo-tool-pins-no-violations" 0 "$([[ -z "$cv" ]] && echo 0 || echo "1 (${cv//$'\n'/ | })")"
# A command substitution inside echo's arguments does not trip `bash -e`, so a failing pin
# reader would write `x=` and pass an empty version on. Assign first, then echo.
ECHO_SUB_RE='^[[:space:]]*echo "[A-Za-z0-9_-]+=\$\('
ct_echo_sub="$(grep -HnE "$ECHO_SUB_RE" "${REPO_ROOT}"/.github/workflows/*.yml)"
assert_rc "workflow-outputs-never-echo-a-substitution" 0 "$([[ -z "$ct_echo_sub" ]] && echo 0 || echo "1 (${ct_echo_sub//$'\n'/ | })")"
assert_rc "workflow-echo-substitution-control-trips" 0 "$(printf '          echo "cargo-audit=$(cargo_tool_version f cargo-audit)" >> "$GITHUB_OUTPUT"\n' | grep -qE "$ECHO_SUB_RE" && echo 0 || echo 1)"
ct_pinned="$(cat "${REPO_ROOT}"/.github/workflows/*.yml | grep -cE 'tool:[[:space:]]*cargo-[a-z0-9-]+@\$\{\{ steps\.pins\.outputs\.')"
assert_rc "cargo-tool-pins-nonvacuous" 0 "$([[ "$ct_pinned" -ge 4 ]] && echo 0 || echo "1 (${ct_pinned} pinned tool: installs)")"
CTROOT="${WORK}/ctroot"; mkdir -p "$CTROOT/.github/workflows" "$CTROOT/infra/devloop"
printf '          tool: cargo-audit\n' > "$CTROOT/.github/workflows/w.yml"
assert_rc "cargo-tool-pins-unpinned-tool-trips" 0 "$([[ -n "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"
printf '          tool: cargo-audit@0.22.2\n' > "$CTROOT/.github/workflows/w.yml"
assert_rc "cargo-tool-pins-literal-tool-trips" 0 "$([[ -n "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"
printf '        run: cargo install cargo-fuzz\n' > "$CTROOT/.github/workflows/w.yml"
assert_rc "cargo-tool-pins-unpinned-install-trips" 0 "$([[ -n "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"
printf '        run: cargo install cargo-fuzz --locked --version =0.13.2\n' > "$CTROOT/.github/workflows/w.yml"
assert_rc "cargo-tool-pins-literal-install-trips" 0 "$([[ -n "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"
printf 'RUN cargo install cargo-nextest --version "=${V}"\n' > "$CTROOT/infra/devloop/Dockerfile"; : > "$CTROOT/.github/workflows/w.yml"
assert_rc "cargo-tool-pins-unlocked-image-install-trips" 0 "$([[ -n "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"
mkdir -p "$CTROOT/infra/docker/x"; printf 'RUN cargo install cargo-chef --locked --version "=0.1.78"\n' > "$CTROOT/infra/docker/x/Dockerfile"
assert_rc "cargo-tool-pins-service-image-literal-trips" 0 "$([[ -n "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"
printf 'RUN cargo install cargo-chef --locked --version "=${CARGO_CHEF_VERSION}"\nRUN cargo install sqlx-cli --locked \\\n    --version "=${SQLX_CLI_VERSION}"\n' > "$CTROOT/infra/docker/x/Dockerfile"
printf 'RUN cargo install cargo-nextest --locked --version "=${V}"\n' > "$CTROOT/infra/devloop/Dockerfile"
printf '          tool: cargo-audit@${{ steps.pins.outputs.cargo-audit }}\n' > "$CTROOT/.github/workflows/w.yml"
assert_rc "cargo-tool-pins-clean-root-passes" 0 "$([[ -z "$(cargo_tool_pin_violations "$CTROOT")" ]] && echo 0 || echo 1)"

# --- pnpm pin: the ONE packageManager reader (devloop image + dev-web.sh) ----------------------
PMLIB="${REPO_ROOT}/infra/lib/package-manager.sh"
pmspec() { src_run "$PMLIB" 'pnpm_package_manager_spec "${ARGS[@]}"' "$1" 2>&1; }
pmver()  { src_run "$PMLIB" 'pnpm_version "${ARGS[@]}"' "$1" 2>&1; }
PMS="${WORK}/pm"; mkdir -p "$PMS"
HEX128="$(printf 'a%.0s' {1..128})"
printf '{\n  "name": "x",\n  "packageManager": "pnpm@12.8.1+sha512.%s",\n  "private": true\n}\n' "$HEX128" > "$PMS/hashed.json"
printf '{ "packageManager": "pnpm@12.8.1+sha512.%s", "engines": {} }\n' "$HEX128" > "$PMS/oneline.json"
printf '{ "packageManager": "pnpm@12.8.1" }\n' > "$PMS/nohash.json"
printf '{ "name": "x" }\n' > "$PMS/missing.json"
printf '{ "packageManager": "yarn@4.1.0+sha512.%s" }\n' "$HEX128" > "$PMS/yarn.json"
printf '{ "packageManager": "pnpm@12.8+sha512.%s" }\n' "$HEX128" > "$PMS/malformed.json"
printf '{ "packageManager": "pnpm@12.8.1+sha512.%s",\n  "packageManager": "pnpm@12.8.1+sha512.%s" }\n' "$HEX128" "$HEX128" > "$PMS/dup.json"
out="$(pmspec "$PMS/hashed.json")"; rc=$?
assert_rc     "pm-spec-hashed-rc"            0 "$rc"
emi_expect    "pm-spec-hashed-verbatim"      "pnpm@12.8.1+sha512.${HEX128}" "$out"
out="$(pmver "$PMS/hashed.json")"; rc=$?
assert_rc     "pm-version-hashed-rc"         0 "$rc"
emi_expect    "pm-version-strips-hash"       "12.8.1" "$out"
out="$(pmver "$PMS/oneline.json")";  emi_expect "pm-version-single-line-json" "12.8.1" "$out"
out="$(pmspec "$PMS/nohash.json")"; rc=$?
assert_rc     "pm-missing-hash-fails"        1 "$rc"
assert_status "pm-missing-hash-says-so"      "expected pnpm@X.Y.Z+sha512" "$out"
out="$(pmspec "$PMS/missing.json")"; rc=$?
assert_rc     "pm-missing-field-fails"       1 "$rc"
assert_status "pm-missing-field-says-so"     "found 0" "$out"
out="$(pmspec "$PMS/yarn.json")"; rc=$?
assert_rc     "pm-not-pnpm-fails"            1 "$rc"
out="$(pmspec "$PMS/malformed.json")"; rc=$?
assert_rc     "pm-malformed-version-fails"   1 "$rc"
out="$(pmspec "$PMS/dup.json")"; rc=$?
assert_rc     "pm-duplicate-field-fails"     1 "$rc"
out="$(pmver "$PMS/absent.json")"; rc=$?
assert_rc     "pm-unreadable-fails"          1 "$rc"
out="$(pmver "${REPO_ROOT}/package.json")"; rc=$?
assert_rc     "pm-real-package-json" 0 "$([[ $rc -eq 0 && "$out" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] && echo 0 || echo "1 (${out})")"
# Every consumer takes the pin from that reader — never a hand-typed version or a second parser.
assert_status "devloop-sh-derives-pnpm-spec"     'pnpm_package_manager_spec "${script_dir}/../../package.json"' "$(cat "${REPO_ROOT}/infra/devloop/devloop.sh")"
assert_status "devloop-dockerfile-prepares-spec" 'corepack prepare "${PNPM_PACKAGE_MANAGER}" --activate' "$(cat "${REPO_ROOT}/infra/devloop/Dockerfile")"
assert_absent "devloop-dockerfile-no-pnpm-literal" 'ARG PNPM_VERSION=' "$(cat "${REPO_ROOT}/infra/devloop/Dockerfile")"
assert_status "dev-web-uses-pnpm-reader"         'pnpm_package_manager_spec package.json' "$(cat "${REPO_ROOT}/scripts/dev-web.sh")"
# pnpm/action-setup must carry NO `version:` key (it reads packageManager; it errors only on a
# MISMATCH, so an equal re-added pin would be the duplicate coming back silently). The window
# after `uses:` holds only `with:` / `run_install:`; `node-version-file:` cannot match the anchor.
pnpm_setup_version_keys() { grep -h -A4 'uses: pnpm/action-setup' "$@" | grep -E '^[[:space:]]+version:' || true; }
assert_rc "workflows-no-pnpm-version-input" 0 "$([[ -z "$(pnpm_setup_version_keys "${REPO_ROOT}"/.github/workflows/*.yml)" ]] && echo 0 || echo 1)"
assert_rc "workflows-pnpm-setup-sites-nonvacuous" 0 "$([[ "$(cat "${REPO_ROOT}"/.github/workflows/*.yml | grep -c 'uses: pnpm/action-setup')" -ge 5 ]] && echo 0 || echo 1)"
PMWF="${WORK}/pmwf.yml"
printf "      - uses: pnpm/action-setup@deadbeef # v6.1.0\n        with:\n          version: '12.8.1'\n          run_install: false\n" > "$PMWF"
assert_rc "workflows-pnpm-version-input-trips" 0 "$([[ -n "$(pnpm_setup_version_keys "$PMWF")" ]] && echo 0 || echo 1)"

# --- Builder and runtime on the SAME Debian release (glibc) ----------------------------------
# A binary links its builder's glibc; a builder newer than the runtime fails at exec with
# "GLIBC_2.xx not found" (ADR-0038 devloop 2 Gate 2: `rust:<v>-slim` had floated to trixie
# while the runtime was distroless cc-debian12). Every rust builder must name its release
# explicitly, and every distroless runtime stage must be that release.
declare -A DEBIAN_RELEASE=([bookworm]=12 [trixie]=13)
debian_release_mismatch() {  # prints a reason when Dockerfile $1 drifts; empty when in step
  local df="$1" codename want got
  codename="$(grep -oP '^FROM docker\.io/library/rust:\S*-slim-\K[a-z]+(?= AS )' "$df" | sort -u)"
  if [[ -z "$codename" || "$(grep -c . <<< "$codename")" -ne 1 ]]; then
    grep -q '^FROM docker\.io/library/rust:' "$df" && echo "rust builder does not pin exactly one Debian codename (-slim-<codename>): '${codename}'"
    return 0
  fi
  want="${DEBIAN_RELEASE[$codename]:-}"
  [[ -n "$want" ]] || { echo "unknown Debian codename '${codename}' (extend DEBIAN_RELEASE)"; return 0; }
  for got in $(grep -oP '^FROM gcr\.io/distroless/cc-debian\K[0-9]+' "$df" | sort -u); do
    [[ "$got" == "$want" ]] || echo "builder is ${codename} (debian${want}) but a runtime stage is distroless cc-debian${got}"
  done
}
rel_checked=0
for df in "${REPO_ROOT}"/infra/docker/*/Dockerfile; do
  grep -q '^FROM docker\.io/library/rust:' "$df" || continue
  rel_checked=$((rel_checked + 1))
  why="$(debian_release_mismatch "$df")"
  if [[ -z "$why" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[dockerfile-builder-runtime-same-release] ${df#"${REPO_ROOT}"/}: ${why}"); fi
done
assert_rc "dockerfile-builder-runtime-nonvacuous" 0 "$([[ $rel_checked -ge 5 ]] && echo 0 || echo "1 (${rel_checked})")"
# Negative controls: the Gate-2 shape (floating -slim) and a release mismatch both trip.
BADDF="${WORK}/bad.Dockerfile"
printf 'FROM docker.io/library/rust:${RUST_VERSION}-slim AS builder\nFROM gcr.io/distroless/cc-debian12:nonroot AS runtime\n' > "$BADDF"
assert_rc "dockerfile-floating-slim-trips" 0 "$([[ -n "$(debian_release_mismatch "$BADDF")" ]] && echo 0 || echo 1)"
printf 'FROM docker.io/library/rust:${RUST_VERSION}-slim-trixie AS builder\nFROM gcr.io/distroless/cc-debian12:nonroot AS runtime\n' > "$BADDF"
assert_rc "dockerfile-release-mismatch-trips" 0 "$([[ -n "$(debian_release_mismatch "$BADDF")" ]] && echo 0 || echo 1)"

# --- Repo derivation fails loudly (zero repos / a repo with no Dockerfile) --------------------
if [[ -n "$REAL_KUBECTL" ]]; then
  RCP="${WORK}/repocopy"; deploy_tree_copy "$RCP"
  mv "$RCP/infra/docker/gc-service" "$RCP/infra/docker/gc-service.moved"
  out="$(src_run "$RCP/infra/kind/scripts/deploy.sh" 'cache_renders; first_party_repos' 2>&1)"; rc=$?
  assert_rc "repo-without-dockerfile-fails" 1 "$rc"
  assert_status "repo-without-dockerfile-named" "localhost/gc-service has no infra/docker/gc-service/Dockerfile" "$out"
  mv "$RCP/infra/docker/gc-service.moved" "$RCP/infra/docker/gc-service"
  # Zero first-party images anywhere: every localhost/ ref rewritten away.
  grep -rl 'image: localhost/' "$RCP/infra/services" | xargs sed -i 's#image: localhost/#image: example.org/#'
  out="$(src_run "$RCP/infra/kind/scripts/deploy.sh" 'cache_renders; first_party_repos' 2>&1)"; rc=$?
  assert_rc "repos-derived-zero-fails" 1 "$rc"
  assert_status "repos-derived-zero-says-vacuous" "refusing a vacuous converge" "$out"
fi

# --- An empty --iidfile never becomes a guessed tag ------------------------------------------
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DEVLOOP_MIN_DISK_GB=0 STUB_IID="" \
  src_run "$DEPLOY" 'KIND_EXPERIMENTAL_PROVIDER=podman; build_content_tagged_image localhost/gc-service' 2>&1)"; rc=$?
assert_rc "iidfile-empty-fails" 1 "$rc"
assert_status "iidfile-empty-says-so" "wrote no image ID" "$out"
assert_absent "iidfile-empty-never-tags" "tag " "$(cat "${MARK}/podman.calls" 2>/dev/null | grep -v '^build')"

# --- The devloop container no longer migrates (test.sh is the one owner) ---------------------
entry_code="$(grep -vE '^[[:space:]]*#' "${REPO_ROOT}/infra/devloop/entrypoint.sh")"
assert_absent "entrypoint-has-no-sqlx-migrate" "sqlx migrate" "$entry_code"
assert_absent "entrypoint-has-no-masked-migration" "may already be applied" "$entry_code"

# === (G) a remedy names the command its caller can run ========================================
# provision.sh/deploy.sh always run on the host; who reads the message differs. The devloop
# helper sets DT_CALLER=devloop-helper (its reader is in a container: `dev-cluster ...`);
# anyone else is on the host (teardown.sh / setup.sh).
LIBC="${REPO_ROOT}/infra/kind/scripts/lib/common.sh"
RUN_REM='sp="$1"; set --; source "$sp" >/dev/null 2>&1; remedy CONTAINER-CMD HOST-CMD'
assert_status "remedy-helper-gets-container" "CONTAINER-CMD" "$(DT_CALLER=devloop-helper bash -c "$RUN_REM" _ "$LIBC")"
assert_status "remedy-setup-gets-host" "HOST-CMD" "$(DT_CALLER=setup.sh bash -c "$RUN_REM" _ "$LIBC")"
assert_status "remedy-person-gets-host" "HOST-CMD" "$(env -u DT_CALLER bash -c "$RUN_REM" _ "$LIBC")"
assert_absent "remedy-person-not-container" "CONTAINER-CMD" "$(env -u DT_CALLER bash -c "$RUN_REM" _ "$LIBC")"
# The helper's value and the scripts' value are one string in two languages: they must match.
rust_val="$(sed -n 's/^const DT_CALLER_DEVLOOP_HELPER: &str = "\([^"]*\)";$/\1/p' "${REPO_ROOT}/crates/devloop-helper/src/commands.rs")"
sh_val="$(sed -n 's/^DT_CALLER_DEVLOOP_HELPER="\([^"]*\)"$/\1/p' "$LIBC")"
assert_rc "dt-caller-value-in-sync" 0 "$([[ -n "$rust_val" && "$rust_val" == "$sh_val" ]] && echo 0 || echo "1 (rust='${rust_val}' sh='${sh_val}')")"

report_results "scripts/setup.test.sh"
