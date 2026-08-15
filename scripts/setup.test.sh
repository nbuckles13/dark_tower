#!/usr/bin/env bash
# setup.test.sh — self-test for infra/kind/scripts/setup.sh.
#
# TWO independent groups:
#   (A) the disk precondition guard (below) — the original scope of this file;
#   (B) `--provision-org`, the one mode invoked from INSIDE the devloop container
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
# shellcheck source=lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

# The guard deliberately `exit 2`s on the trip lane; run each case in its own `bash -c`
# subshell (fresh _DT_DISK_CHECKED sentinel + isolated exit) and capture rc/output, so the
# harness's set -e must not abort. report_results sets the final code.
set +e

# Source setup.sh (BASH_SOURCE guard suppresses main), then call the guard. $1=setup path,
# $2=runtime arg. `set --` clears the bash -c positional params BEFORE sourcing — otherwise
# the sourced setup.sh inherits them as its own args and its arg-parser `exit 1`s on the
# unknown "option" (the setup path). Source noise is discarded; the guard's stderr banner is
# what we capture.
RUN_GUARD='sp="$1"; rt="$2"; set --; source "$sp" >/dev/null 2>&1; check_build_disk_space "$rt"'

# === (1) forced TRIP: min-disk floor absurdly high → comparison trips on the real fs ========
# Real df on the real graphroot (or its PROJECT_ROOT fallback — always df-able, so avail_gb
# is always populated and the huge floor always trips, even on a host without podman).
trip_out="$(DEVLOOP_MIN_DISK_GB=999999999 bash -c "$RUN_GUARD" _ "$SETUP" podman 2>&1)"
trip_rc=$?
assert_rc "trip-exit2" 2 "$trip_rc"
# The operator contract: line-anchored, greppable by the §4 one-pass scan.
grep -Eq '^PRECONDITION_FAILURE:.*REASON=insufficient-disk' <<<"$trip_out"
assert_rc "trip-banner-anchored" 0 $?
assert_status "trip-remediation-hint" "podman image prune -f" "$trip_out"

# === (2) PASS path: a 0 GB floor cannot trip (avail_gb >= 0) → exit 0, no banner ============
# Guards against a regression where the guard trips unconditionally (which would still pass
# case 1). Proves the comparison is real, not always-fail.
pass_out="$(DEVLOOP_MIN_DISK_GB=0 bash -c "$RUN_GUARD" _ "$SETUP" podman 2>&1)"
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
#   argv   — a LEADING-HYPHEN value, which --only's `-z "${2:-}"` check at setup.sh:119 does
#            NOT catch. `--provision-org --skip-build` silently consumes the next flag as the
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
reject_case "leading-hyphen-known" "--skip-build"
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

# === (B6) --provision-org rejects flag combinations that can only express confusion ==========
reset_marks
PROV_OUT="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C DT_CLUSTER_NAME=devloop-fixture \
  bash "$SETUP" --provision-org "e2e-0123456789abcdef" --skip-build 2>&1)"; PROV_RC=$?
if [[ "$PROV_RC" -ne 0 ]]; then PASS=$((PASS+1)); else
  FAIL=$((FAIL+1))
  FAILURES+=("[provision-rejects-skip-build] --provision-org was silently combined with --skip-build instead of failing")
fi
assert_no_marker "provision-rejects-skip-build-no-psql" "$MARK" 'ran.psql'

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

report_results "scripts/setup.test.sh"
