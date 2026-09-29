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

# =============================================================================================
# === (C) #1 name-length — CLUSTER_NAME cap 49 (setup.sh + teardown.sh), slug cap 41 (devloop.sh)
# =============================================================================================
# The cap is DERIVED from the 63-char DNS-label limit (see setup.sh's validate_cluster_name).
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

# --- setup.sh: source + call validate_cluster_name directly (the disk-guard pattern, :47).
#     Sourcing with a VALID DT_CLUSTER_NAME lets the top-level call pass; then we drive the
#     function with the test value. The function has NO cluster calls, so a reject cannot
#     create anything — "before creation" is structural.
SETUP_VALIDATE='sp="$1"; nm="$2"; set --; source "$sp" >/dev/null 2>&1; validate_cluster_name "$nm"'

c_rej_out="$(DT_CLUSTER_NAME=dark-tower bash -c "$SETUP_VALIDATE" _ "$SETUP" "$NAME50" 2>&1)"; c_rej_rc=$?
assert_rc     "namelen-setup-reject-exit1"  1 "$c_rej_rc"
assert_status "namelen-setup-reject-token"  "CLUSTER_NAME_TOO_LONG SUBJECT=cluster-name NAME=${NAME50} LEN=50 MAX=49" "$c_rej_out"

c_acc_out="$(DT_CLUSTER_NAME=dark-tower bash -c "$SETUP_VALIDATE" _ "$SETUP" "$NAME49" 2>&1)"; c_acc_rc=$?
assert_rc     "namelen-setup-accept-exit0"    0 "$c_acc_rc"
assert_absent "namelen-setup-accept-no-token" "CLUSTER_NAME_TOO_LONG" "$c_acc_out"   # not rejecting on some other axis

# --- teardown.sh DELIBERATELY has NO length cap (inverse precondition, @paired-operations):
#     it must be able to DELETE a pre-existing orphan cluster whose name is >49 chars. So a
#     50-char name must NOT be rejected — it must REACH `kind delete`. (Charset is still
#     enforced; the charset-in-sync check below covers that.) Run the REAL script (no
#     source-guard); the `get clusters` stub reports the orphan so teardown proceeds to delete.
cat > "${STUB_BIN}/kind" <<EOF
#!/usr/bin/env bash
case "\$1 \$2" in
  "get clusters")   echo "\${DT_CLUSTER_NAME}" ;;   # report the orphan cluster exists
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

# --- CHARSET regex in sync (@dry-reviewer): pin the CANONICAL literal absolutely (not just
#     compare the two files — a change applied to BOTH would pass a mutual compare). The charset
#     rule is the one thing setup.sh + teardown.sh must still agree on after the length-cap
#     divergence; a teardown charset STRICTER than setup's would strand a cluster setup could
#     create. `grep -c -F` the exact `=~`-anchored condition in EACH; require >= 1 in BOTH (zero
#     is drift — renamed fn / edited literal / deleted file — never a pass). NOTE a THIRD charset
#     site exists at infra/devloop/devloop.sh (TASK_SLUG) — deliberately NOT pinned here: it
#     validates the SLUG (different subject), and drift there surfaces loudly at setup.
# Needle is the `=~`-anchored PATTERN only (not the `[[ ! "${name}" … ]]` wrapper), so a
# reindent or a local-variable rename can't falsely red this (@dry-reviewer).
readonly CHARSET_NEEDLE='=~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$'
setup_n="$(grep -c -F -- "$CHARSET_NEEDLE" "$SETUP" || true)"
teardown_n="$(grep -c -F -- "$CHARSET_NEEDLE" "$TEARDOWN" || true)"
if (( setup_n >= 1 && teardown_n >= 1 )); then
  PASS=$((PASS + 1))
else
  FAIL=$((FAIL + 1))
  FAILURES+=("[charset-validators-in-sync] setup.sh(${setup_n})/teardown.sh(${teardown_n}) must EACH contain the canonical charset check — they must share the CHARSET rule (their LENGTH caps deliberately differ: teardown has none, by design; see its header)")
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

# =============================================================================================
# === (D) #2 kubeconfig-on-reuse — write_kubeconfig() runs on BOTH reuse return-paths ==========
# =============================================================================================
# The bug: create_cluster()'s two reuse `return 0` paths (AUTO_YES + interactive) skipped the
# host-kubeconfig export, so on reuse kubectl fell back to localhost:8080 and a healthy cluster
# read as broken. Drive the REUSE branch specifically (stub `kind get clusters` to report the
# cluster exists) — NOT the create path (which always exported) — and assert the export ran ON
# that path. MUTATION CHECK: removing write_kubeconfig from a reuse path drops ran.kind_export,
# reddening the assert_marker below. Both reuse branches are covered (the SSoT helper means one
# callsite could be present and the other dropped).
cat > "${STUB_BIN}/kind" <<EOF
#!/usr/bin/env bash
case "\$1 \$2" in
  "get clusters")      echo "reusecluster" ;;   # report the cluster already exists -> reuse path
  "export kubeconfig") touch "${MARK}/ran.kind_export" ;;
  "delete cluster")    touch "${MARK}/ran.kind_delete" ;;
  "create cluster")    touch "${MARK}/ran.kind_create" ;;
  *) echo "FAKE kind: unmodeled '\$*'" >&2; exit 3 ;;
esac
exit 0
EOF
chmod +x "${STUB_BIN}/kind"

# AUTO_YES reuse branch.
reset_marks
PATH="${STUB_BIN}:${PATH}" DT_CLUSTER_NAME=reusecluster \
  bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; AUTO_YES=true; create_cluster' _ "$SETUP" >/dev/null 2>&1
assert_marker    "kubeconfig-reuse-autoyes-exported"      "$MARK" "ran.kind_export"
assert_no_marker "kubeconfig-reuse-autoyes-not-recreated" "$MARK" "ran.kind_delete"
assert_no_marker "kubeconfig-reuse-autoyes-not-created"   "$MARK" "ran.kind_create"

# Interactive reuse branch ("Delete and recreate? [y/N]" answered n -> "Using existing cluster").
reset_marks
echo n | PATH="${STUB_BIN}:${PATH}" DT_CLUSTER_NAME=reusecluster \
  bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; AUTO_YES=false; create_cluster' _ "$SETUP" >/dev/null 2>&1
assert_marker    "kubeconfig-reuse-interactive-exported"      "$MARK" "ran.kind_export"
assert_no_marker "kubeconfig-reuse-interactive-not-recreated" "$MARK" "ran.kind_delete"
# Symmetric with the AUTO_YES branch (@test): catches an `else`-branch that lost its
# `return 0` and fell through to `kind create cluster` — which would touch ran.kind_export
# via the create-path write AND leave ran.kind_delete absent, passing the two assertions
# above while wrongly RECREATING the cluster. This is the "reuse must not create" control.
assert_no_marker "kubeconfig-reuse-interactive-not-created"   "$MARK" "ran.kind_create"

# --- #2 LOUD-on-failure (@paired-operations): a FAILING `kind export kubeconfig` on a reuse
#     path must ABORT create_cluster non-zero with the diagnostic — this is the anti-masking
#     property #2 exists to guarantee (a future `|| true`/dropped `exit 1` would silently
#     re-introduce the kubectl→localhost:8080 bug with every success-path test still green).
cat > "${STUB_BIN}/kind" <<EOF
#!/usr/bin/env bash
case "\$1 \$2" in
  "get clusters")      echo "reusecluster" ;;
  "export kubeconfig") echo "kind: simulated export failure" >&2; exit 1 ;;
  *) exit 0 ;;
esac
EOF
chmod +x "${STUB_BIN}/kind"
reset_marks
wk_out="$(PATH="${STUB_BIN}:${PATH}" DT_CLUSTER_NAME=reusecluster \
  bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; AUTO_YES=true; create_cluster' _ "$SETUP" 2>&1)"; wk_rc=$?
assert_rc     "kubeconfig-export-failure-aborts" 1 "$wk_rc"
assert_status "kubeconfig-export-failure-loud"   "localhost:8080" "$wk_out"

# === (C) --only accepts every target its dispatcher has, including otel =====================
# deploy_services (formerly deploy_only_service) has had an `otel)` arm since the dev collector landed, but the argument
# parser rejected `otel`, so the documented collector-refresh step (`setup.sh --only otel`,
# needed whenever the collector ConfigMap changes on an existing cluster) could never run.
# Parse-level pin: the kind stub reports no clusters, so an ACCEPTED target reaches the
# dispatcher and stops at its cluster-exists check, while a REJECTED one never gets there.
reset_marks
only_out="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C bash "$SETUP" --only otel 2>&1)"
assert_status "only-otel-reaches-dispatcher" "does not exist" "$only_out"
assert_absent "only-otel-not-rejected" "Unknown service" "$only_out"
reset_marks
only_out="$(PATH="${STUB_BIN}:${PATH}" LC_ALL=C bash "$SETUP" --only bogus 2>&1)"; only_rc=$?
assert_rc "only-bogus-rejected-rc" 1 "$only_rc"
assert_status "only-bogus-lists-otel" "Valid: ac, gc, mc, mh, otel" "$only_out"
assert_no_marker "only-bogus-never-reaches-kind" "$MARK" 'ran.kind'

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
  # setup.sh applies postgres/redis/otel-collector ahead of the root for ordering only; if their
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
  RUN_RENDER='sp="$1"; out="$2"; set --; source "$sp" >/dev/null 2>&1; '"${SET_REFS}"'; render_env_overlay "$out"'
  WRAP="${RWORK}/wrap"; mkdir -p "$WRAP"
  DT_HOST_GATEWAY_IP=10.1.2.3 MC_0_WEBTRANSPORT_PORT=24433 MC_1_WEBTRANSPORT_PORT=24435 \
    MH_0_WEBTRANSPORT_PORT=24434 MH_1_WEBTRANSPORT_PORT=24436 bash -c "$RUN_RENDER" _ "$SETUP" "$WRAP" >/dev/null 2>&1
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
  inst_wrapper="$(bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; advertise_instances' _ "$SETUP" | tr '\n' ' ')"
  inst_root="$(cm_names "${RWORK}/root" | grep -E '^m[ch]-[0-9]+-config-' | sed -E 's/-config-[^-]+$//' | sort | tr '\n' ' ')"
  assert_rc "wrapper-instances-equal-root-generators" 0 \
    "$([[ -n "$inst_root" && "$inst_wrapper" == "$inst_root" ]] && echo 0 || echo "1 (wrapper='${inst_wrapper}' root='${inst_root}')")"
  # Invalid inputs fail the render (the caller then refuses to apply anything).
  bad="$(DT_HOST_GATEWAY_IP=10.1.2.3 MC_0_WEBTRANSPORT_PORT=24433 MC_1_WEBTRANSPORT_PORT=70000 \
    MH_0_WEBTRANSPORT_PORT=24434 MH_1_WEBTRANSPORT_PORT=24436 bash -c "$RUN_RENDER" _ "$SETUP" "$WRAP" 2>&1)"; bad_rc=$?
  assert_rc "wrapper-bad-port-rejected" 1 "$bad_rc"
  assert_status "wrapper-bad-port-named" "MC_1_WEBTRANSPORT_PORT" "$bad"
  DT_HOST_GATEWAY_IP=10.1.2.3 bash -c "$RUN_RENDER" _ "$SETUP" "$WRAP" >/dev/null 2>&1
  assert_rc "wrapper-missing-port-rejected" 1 $?

  # --- (D2b) Content-tagged images: the wrapper ALWAYS carries the tags (ADR-0038 §2) -------
  STATIC="${RWORK}/static"; mkdir -p "$STATIC"
  env -u DT_HOST_GATEWAY_IP bash -c "$RUN_RENDER" _ "$SETUP" "$STATIC" >/dev/null 2>&1
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
  repos_out="$(bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; first_party_repos' _ "$SETUP" 2>/dev/null | tr '\n' ' ')"
  assert_rc "repos-derived-exact-set" 0 "$([[ "$repos_out" == "localhost/ac-service localhost/db-migrate localhost/gc-service localhost/mc-service localhost/mh-service " ]] && echo 0 || echo "1 (${repos_out})")"
  # A render with any root repo lacking a resolved ref refuses (never a placeholder apply).
  NOREF="${RWORK}/noref"; mkdir -p "$NOREF"
  out="$(bash -c 'sp="$1"; out="$2"; set --; source "$sp" >/dev/null 2>&1; IMAGE_REFS[localhost/ac-service]="localhost/ac-service:sha-0123456789abcdef"; render_env_overlay "$out"' _ "$SETUP" "$NOREF" 2>&1)"
  assert_rc "wrapper-missing-ref-refused" 1 "$?"
  assert_status "wrapper-missing-ref-named" "no content-tagged ref resolved for localhost/gc-service" "$out"

  # --- (D2c) The migration Job render (ADR-0038 §2 step 3) ------------------------------------
  RUN_JOB='sp="$1"; out="$2"; ref="$3"; set --; source "$sp" >/dev/null 2>&1; render_migration_job "$out" "$ref"'
  J1="${RWORK}/job1"; J2="${RWORK}/job2"; J3="${RWORK}/job3"; mkdir -p "$J1" "$J2" "$J3"
  name1="$(bash -c "$RUN_JOB" _ "$SETUP" "$J1" "localhost/db-migrate:${TEST_TAG}" 2>/dev/null)"
  name2="$(bash -c "$RUN_JOB" _ "$SETUP" "$J2" "localhost/db-migrate:${TEST_TAG}" 2>/dev/null)"
  name3="$(bash -c "$RUN_JOB" _ "$SETUP" "$J3" "localhost/db-migrate:sha-fedcba9876543210" 2>/dev/null)"
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
  JCP="${RWORK}/jobcopy"; mkdir -p "$JCP"; cp -r "${REPO_ROOT}/infra" "$JCP/"; cp "${REPO_ROOT}/Cargo.lock" "$JCP/"
  sed -i 's/backoffLimit: 1/backoffLimit: 2/' "$JCP/infra/services/db-migrate/job.yaml"
  J4="${RWORK}/job4"; mkdir -p "$J4"
  name4="$(bash -c "$RUN_JOB" _ "$JCP/infra/kind/scripts/setup.sh" "$J4" "localhost/db-migrate:${TEST_TAG}" 2>/dev/null)"
  assert_rc "job-name-changes-with-spec" 0 "$([[ -n "$name4" && "$name1" != "$name4" ]] && echo 0 || echo "1 (${name1} vs ${name4})")"
  # A placeholder / :latest ref never renders a Job.
  bash -c "$RUN_JOB" _ "$SETUP" "${RWORK}/job5" "localhost/db-migrate:latest" >/dev/null 2>&1
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
#   STUB_POD_DELETE_FAIL=1   `delete pods` fails
D4_BIN="${WORK}/d4bin"; mkdir -p "$D4_BIN"
cat > "${D4_BIN}/kubectl" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kubectl.calls"
args=("\$@")
# Drop a leading --context <ctx>.
if [[ "\${args[0]:-}" == "--context" ]]; then args=("\${args[@]:2}"); fi
case "\${args[0]:-}" in
  kustomize) exec "${REAL_KUBECTL:-/nonexistent-kubectl}" "\${args[@]}" ;;
  apply)
    if [[ "\${args[1]:-}" == "-k" ]]; then
      d="\${args[2]}"; printf '%s\n' "\$d" >> "${MARK}/applied"
      if [[ -f "\$d/kustomization.yaml" ]]; then
        if grep -q 'render_migration_job' "\$d/kustomization.yaml"; then
          cp "\$d/kustomization.yaml" "${MARK}/applied.migration.yaml"
          printf '%s' "\${STUB_JOB_COND_AFTER_APPLY-Complete }" > "${MARK}/jobcond"
        elif grep -q 'render_env_overlay' "\$d/kustomization.yaml"; then
          cp "\$d/kustomization.yaml" "${MARK}/applied.kustomization.yaml"
        fi
      fi
    fi
    exit 0 ;;
  delete)
    printf '%s\n' "\${args[*]}" >> "${MARK}/deleted"
    [[ "\${args[1]:-}" == "pods" && -n "\${STUB_POD_DELETE_FAIL:-}" ]] && { echo "kubectl stub: delete pods failed" >&2; exit 1; }
    exit 0 ;;
  rollout)
    case "\${args[1]:-}" in
      restart) printf '%s\n' "\${args[2]}" >> "${MARK}/restarted"; exit 0 ;;
      status)  exit 0 ;;
    esac ;;
  wait|describe|create) exit 0 ;;
  logs) printf 'Applied 20260322000001/migrate add participant tracking (postgres://darktower:hunter2@postgres:5432/x)\n'; exit 0 ;;
  exec) : > "${MARK}/ran.kubectl-exec"; exit 0 ;;
  get)
    [[ -n "\${STUB_GET_FAIL:-}" && "\${args[1]:-}" != "events" ]] && { echo "kubectl stub: get failed" >&2; exit 1; }
    case "\${args[1]:-}" in
      jobs) printf '%b' "\${STUB_JOBS:-}"; exit 0 ;;
      job)
        if [[ -f "${MARK}/jobcond" ]]; then cat "${MARK}/jobcond"; else printf '%s' "\${STUB_JOB_COND_INITIAL:-}"; fi
        printf '\n'; exit 0 ;;
      events) exit 0 ;;
      pods) printf 'calico-node-x 1/1 Running\n'; exit 0 ;;
      */*)
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
esac
exit 0
STUB
done
cat > "${D4_BIN}/kind" <<STUB
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "${MARK}/kind.calls"
case "\$1 \$2" in
  "get clusters") echo "d4cluster" ;;
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
RUN_APPLY='sp="$1"; set --; source "$sp" >/dev/null 2>&1; '"${D4_REFS}"'; apply_env_root'

reset_marks
PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" env -u DT_HOST_GATEWAY_IP bash -c "$RUN_APPLY" _ "$SETUP" >/dev/null 2>&1
assert_rc "apply-root-plain-rc" 0 $?
assert_absent "apply-root-plain-never-the-bare-root" "$ROOT_DIR" "$(cat "${MARK}/applied" 2>/dev/null)"
assert_status "apply-root-plain-applied-the-tagged-wrapper" "newTag: sha-" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_absent "apply-root-plain-no-advertise-merge" "behavior: merge" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
# Retired resources are converged away without --prune (security: the retired
# grafana-sidecar RoleBinding is a live privilege until deleted).
assert_status "apply-root-deletes-retired-grafana-rbac" "rolebinding/grafana-sidecar role/grafana-sidecar" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_status "apply-root-retired-delete-is-idempotent" "--ignore-not-found" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_rc "apply-root-plain-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-env-root.*" >/dev/null && echo 0 || echo 1)"

reset_marks
PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_HOST_GATEWAY_IP=10.1.2.3 MC_0_WEBTRANSPORT_PORT=24433 \
  MC_1_WEBTRANSPORT_PORT=24435 MH_0_WEBTRANSPORT_PORT=24434 MH_1_WEBTRANSPORT_PORT=24436 \
  bash -c "$RUN_APPLY" _ "$SETUP" >/dev/null 2>&1
assert_rc "apply-root-gateway-rc" 0 $?
assert_absent "apply-root-gateway-not-the-plain-root" "$ROOT_DIR" "$(cat "${MARK}/applied" 2>/dev/null)"
assert_status "apply-root-gateway-applied-the-wrapper" "behavior: merge" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_status "apply-root-gateway-wrapper-tagged" "newTag: sha-" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_status "apply-root-gateway-deletes-retired-grafana-rbac" "rolebinding/grafana-sidecar" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_rc "apply-root-gateway-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-env-root.*" >/dev/null && echo 0 || echo 1)"

reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_HOST_GATEWAY_IP=10.1.2.3 \
  bash -c "$RUN_APPLY" _ "$SETUP" 2>&1)"; rc=$?
assert_rc "apply-root-render-failure-aborts" 1 "$rc"
assert_no_marker "apply-root-render-failure-applies-nothing" "$MARK" "applied"
assert_status "apply-root-render-failure-says-no-fallback" "NOT applying" "$out"
assert_rc "apply-root-failure-tempdir-removed" 1 "$(compgen -G "${RWORK}/dt-env-root.*" >/dev/null && echo 0 || echo 1)"

# --- (D5) content_tag: the ONE tag derivation (pure) -------------------------------------------
RUN_CT='sp="$1"; id="$2"; set --; source "$sp" >/dev/null 2>&1; content_tag "$id"'
ct() { bash -c "$RUN_CT" _ "$SETUP" "$1" 2>/dev/null; }
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

# --- (D6) Ref resolution: built > deployed > LOUD failure --------------------------------------
RUN_RES='sp="$1"; built="$2"; set --; source "$sp" >/dev/null 2>&1; [[ -n "$built" ]] && BUILT_REFS[localhost/gc-service]="$built"; resolve_image_refs || exit 1; for r in "${!IMAGE_REFS[@]}"; do echo "$r=${IMAGE_REFS[$r]}"; done | sort'
res() { PATH="${D4_BIN}:${PATH}" DT_CLUSTER_NAME=d4cluster bash -c "$RUN_RES" _ "$SETUP" "$1" 2>&1; }
JOBS_OK="1 localhost/db-migrate:${DEPLOYED_TAG}\n"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" res "localhost/gc-service:${BUILT_TAG}")"; rc=$?
assert_rc "resolve-rc" 0 "$rc"
assert_status "resolve-built-wins" "localhost/gc-service=localhost/gc-service:${BUILT_TAG}" "$out"
assert_status "resolve-deployed-used-when-not-built" "localhost/mc-service=localhost/mc-service:${DEPLOYED_TAG}" "$out"
assert_status "resolve-deployed-migration-job-image" "localhost/db-migrate=localhost/db-migrate:${DEPLOYED_TAG}" "$out"
reset_marks
out="$(STUB_JOBS="" res "")"; rc=$?
assert_rc "resolve-none-fails-rc" 1 "$rc"
assert_rc "resolve-none-fails-loud-names-fix" 0 "$(grep -Eq 'IMAGE_UNRESOLVED: no content-tagged image for localhost/db-migrate \(deployed: none\); build it with .dev-cluster rebuild-all.* REASON=image-unresolved' <<< "$out" && echo 0 || echo "1 (${out})")"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_DEPLOYED_TAG=latest res "")"; rc=$?
assert_rc "resolve-deployed-latest-rejected" 1 "$rc"
assert_status "resolve-deployed-latest-named" "(deployed: localhost/ac-service:latest" "$out"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_DEPLOYED_TAG=render-required res "")"; rc=$?
assert_rc "resolve-deployed-render-required-rejected" 1 "$rc"
assert_status "resolve-deployed-render-required-token" "REASON=image-unresolved" "$out"
reset_marks
out="$(STUB_JOBS="$JOBS_OK" STUB_DEPLOYED_OVERRIDE="deployment/mc-1=localhost/mc-service:sha-cccccccccccccccc" res "")"; rc=$?
assert_rc "resolve-two-deployed-refs-fails" 1 "$rc"
assert_status "resolve-two-deployed-refs-lists-both" "localhost/mc-service:sha-cccccccccccccccc" "$out"
reset_marks
out="$(STUB_GET_FAIL=1 res "")"; rc=$?
assert_rc "resolve-kubectl-read-failure-rc" 1 "$rc"
assert_status "resolve-kubectl-read-failure-distinct" "REASON=image-ref-read-failed" "$out"
assert_absent "resolve-kubectl-read-failure-not-unresolved" "image-unresolved" "$out"

# --- (D7) run_migration_job: fails LOUDLY; an unchanged set is a no-op -----------------------
RUN_MIG='sp="$1"; set --; source "$sp" >/dev/null 2>&1; IMAGE_REFS[localhost/db-migrate]="localhost/db-migrate:'"${DEPLOYED_TAG}"'"; run_migration_job'
mig() { PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DT_JOB_POLL_SECONDS=0 bash -c "$RUN_MIG" _ "${1:-$SETUP}" 2>&1; }
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
assert_status "migrate-interim-deletes-succeeded-only" "--field-selector=status.phase==Succeeded" "$(grep '^delete pods' "${MARK}/deleted" 2>/dev/null)"
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
assert_no_marker "migrate-failed-no-interim-delete" "$MARK" "deleted"

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
MCP="${RWORK}/migcopy"; rm -rf "$MCP"; mkdir -p "$MCP"; cp -r "${REPO_ROOT}/infra" "$MCP/"; cp "${REPO_ROOT}/Cargo.lock" "$MCP/"
sed -i 's/activeDeadlineSeconds: 300/activeDeadlineSeconds: 1/' "$MCP/infra/services/db-migrate/job.yaml"
reset_marks
out="$(STUB_JOB_COND_AFTER_APPLY="" DT_JOB_WAIT_MARGIN_SECONDS=0 mig "$MCP/infra/kind/scripts/setup.sh")"; rc=$?
assert_rc "migrate-timeout-rc" 1 "$rc"
assert_rc "migrate-timeout-distinct-token" 0 "$(grep -Eq '^MIGRATION_FAILURE: .* REASON=migration-timeout' <<< "$out" && echo 0 || echo "1 (${out})")"
assert_absent "migrate-timeout-not-the-failed-token" "REASON=migration-failed" "$out"
assert_status "migrate-deadline-derived" "within 1s (activeDeadlineSeconds 1 + 0s" "$out"
sed -i '/activeDeadlineSeconds/d' "$MCP/infra/services/db-migrate/job.yaml"
reset_marks
out="$(mig "$MCP/infra/kind/scripts/setup.sh")"; rc=$?
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
assert_status "migrate-existing-complete-still-deletes-succeeded-pod" "delete pods" "$(cat "${MARK}/deleted" 2>/dev/null)"
assert_status "migrate-existing-complete-still-prunes-older" "metadata.name!=db-migrate-" "$(cat "${MARK}/deleted" 2>/dev/null)"

reset_marks
out="$(STUB_POD_DELETE_FAIL=1 mig)"; rc=$?
assert_rc "migrate-interim-delete-failure-fails-setup" 1 "$rc"
assert_status "migrate-interim-delete-failure-own-message" "Could not delete the Succeeded pod" "$out"

# --- (D8) Entry points: --only / --skip-build / --rebuild-all / full setup --------------------
builds() { grep '^build ' "${MARK}/podman.calls" 2>/dev/null | grep -o 'infra/docker/[a-z-]*/Dockerfile' | sort | tr '\n' ' '; }
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 DT_JOB_POLL_SECONDS=0 \
  STUB_JOBS="$JOBS_OK" env -u DT_HOST_GATEWAY_IP bash "$SETUP" --only gc 2>&1)"; rc=$?
assert_rc "only-gc-build-rc" 0 "$rc"
assert_rc "only-gc-builds-gc-and-db-migrate" 0 "$([[ "$(builds)" == "infra/docker/db-migrate/Dockerfile infra/docker/gc-service/Dockerfile " ]] && echo 0 || echo "1 ($(builds))")"
assert_status "only-gc-db-migrate-gets-sqlx-version" "SQLX_CLI_VERSION=" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_status "only-gc-tags-by-content" "tag sha256:bbbb" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_status "only-gc-tags-gc-built" "name: localhost/gc-service"$'\n'"    newTag: ${BUILT_TAG}" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_status "only-gc-others-keep-deployed" "name: localhost/mc-service"$'\n'"    newTag: ${DEPLOYED_TAG}" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_no_marker "only-gc-restarts-nothing" "$MARK" "restarted"
assert_status "only-gc-runs-the-migration-job" "render_migration_job" "$(cat "${MARK}/applied.migration.yaml" 2>/dev/null)"
assert_status "only-gc-waits-for-every-root-workload" "statefulset/redis" "$(cat "${MARK}/kubectl.calls" 2>/dev/null)"
assert_status "prune-superseded-host" "rmi localhost/gc-service:${DEPLOYED_TAG}" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_status "prune-superseded-node" "exec d4cluster-control-plane crictl rmi localhost/gc-service:${DEPLOYED_TAG}" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_absent "prune-never-the-unchanged" "rmi localhost/mc-service" "$(cat "${MARK}/podman.calls" 2>/dev/null)"
assert_no_marker "only-gc-host-sqlx-never-invoked" "$MARK" "ran.host-sqlx"

reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DT_JOB_POLL_SECONDS=0 \
  STUB_JOBS="$JOBS_OK" env -u DT_HOST_GATEWAY_IP bash "$SETUP" --skip-build --only gc 2>&1)"; rc=$?
assert_rc "skip-build-rc" 0 "$rc"
assert_rc "skip-build-builds-nothing" 0 "$([[ -z "$(builds)" ]] && echo 0 || echo "1 ($(builds))")"
assert_status "skip-build-uses-deployed" "name: localhost/gc-service"$'\n'"    newTag: ${DEPLOYED_TAG}" "$(cat "${MARK}/applied.kustomization.yaml" 2>/dev/null)"
assert_no_marker "skip-build-restarts-nothing" "$MARK" "restarted"

reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DT_JOB_POLL_SECONDS=0 \
  STUB_JOBS="" env -u DT_HOST_GATEWAY_IP bash "$SETUP" --skip-build --only gc 2>&1)"; rc=$?
assert_rc "skip-build-nothing-deployed-fails" 1 "$rc"
assert_status "skip-build-nothing-deployed-token" "REASON=image-unresolved" "$out"
assert_no_marker "skip-build-nothing-deployed-applies-nothing" "$MARK" "applied"

reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 DT_JOB_POLL_SECONDS=0 \
  env -u DT_HOST_GATEWAY_IP bash "$SETUP" --rebuild-all 2>&1)"; rc=$?
assert_rc "rebuild-all-rc" 0 "$rc"
assert_rc "rebuild-all-builds-every-repo" 0 "$([[ "$(builds)" == "infra/docker/ac-service/Dockerfile infra/docker/db-migrate/Dockerfile infra/docker/gc-service/Dockerfile infra/docker/mc-service/Dockerfile infra/docker/mh-service/Dockerfile " ]] && echo 0 || echo "1 ($(builds))")"
assert_no_marker "rebuild-all-restarts-nothing" "$MARK" "restarted"
bash "$SETUP" --rebuild-all --skip-build >/dev/null 2>&1; assert_rc "rebuild-all-skip-build-rejected" 1 $?
bash "$SETUP" --rebuild-all --only gc >/dev/null 2>&1; assert_rc "rebuild-all-only-rejected" 1 $?

# The imperative Secrets/namespaces are created as `create --dry-run=client -o yaml | apply -f -`.
# That shape is load-bearing on the NORMAL path, not a half-built-cluster workaround:
# deploy_services re-runs create_ac_secrets / create_mc_tls_secret / create_mh_tls_secret on
# every `--only ac|mc|mh` against a live, successfully built cluster, and a bare `create` would
# fail there with AlreadyExists. (Reusing a HALF-built cluster is a different matter — ADR-0038
# step 3's recorded blueprint hash closes that; nothing here makes setup reuse one.)
for obj in "secret generic ac-service-secrets" "secret tls mc-service-tls" "secret tls mh-service-tls" "namespace dark-tower "; do
  line="$(grep -A5 "create ${obj}" "$SETUP" | tr '\n' ' ')"
  assert_rc "reuse-safe-create-${obj// /-}" 0 "$([[ "$line" == *"--dry-run=client -o yaml | \${KUBECTL} apply -f -"* ]] && echo 0 || echo 1)"
done

# Full setup: a FAILED migration Job stops bring-up BEFORE the seeds and the root apply, and
# the host sqlx is never invoked (the retired host path).
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DT_CLUSTER_NAME=d4cluster DEVLOOP_MIN_DISK_GB=0 DT_JOB_POLL_SECONDS=0 \
  STUB_JOB_COND_AFTER_APPLY="Failed BackoffLimitExceeded" env -u DT_HOST_GATEWAY_IP bash "$SETUP" --yes 2>&1)"; rc=$?
assert_rc "main-migrate-failure-rc" 1 "$rc"
assert_status "main-migrate-failure-token" "REASON=migration-failed" "$out"
assert_status "main-migrate-failure-reached-the-job" "render_migration_job" "$(cat "${MARK}/applied.migration.yaml" 2>/dev/null)"
assert_no_marker "main-migrate-failure-stops-before-seeds" "$MARK" "ran.kubectl-exec"
assert_no_marker "main-migrate-failure-stops-before-root" "$MARK" "applied.kustomization.yaml"
assert_no_marker "main-host-sqlx-never-invoked" "$MARK" "ran.host-sqlx"
setup_code="$(grep -vE '^[[:space:]]*#' "$SETUP")"
assert_absent "no-sqlx-cli-branch-left" "sqlx-cli not installed" "$setup_code"
assert_absent "no-host-sqlx-migrate-left" "sqlx migrate" "$setup_code"
assert_absent "no-port-forward-migration-left" "run_migrations()" "$setup_code"

# Static: the imperative ConfigMap patch and apply --prune (which could delete the
# imperatively-created Secrets) must not come back.
setup_code="$(grep -vE '^[[:space:]]*#' "$SETUP")"
assert_absent "setup-has-no-configmap-patch" "patch configmap" "$setup_code"
assert_absent "setup-has-no-prune" "--prune" "$setup_code"

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
RUN_EMI='sp="$1"; in="$2"; set --; source "$sp" >/dev/null 2>&1; printf "%s" "$in" | extract_manifest_images'
emi() { bash -c "$RUN_EMI" _ "$SETUP" "$1"; }
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
clv() { bash -c 'source "$1"; cargo_lock_version "$2" "$3"' _ "$LIB" "$1" "$2" 2>&1; }
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

# --- RUST_VERSION: one checked value across every image Dockerfile ---------------------------
rv="$(grep -h '^ARG RUST_VERSION=' "${REPO_ROOT}"/infra/docker/*/Dockerfile | sort -u)"
rv_n="$(grep -c . <<< "$rv")"
assert_rc "dockerfile-rust-version-defaults-agree" 0 "$([[ "$rv_n" -eq 1 ]] && echo 0 || echo "1 (${rv//$'\n'/ | })")"
assert_rc "dockerfile-rust-version-nonvacuous" 0 "$([[ "$(grep -l '^ARG RUST_VERSION=' "${REPO_ROOT}"/infra/docker/*/Dockerfile | wc -l)" -ge 5 ]] && echo 0 || echo 1)"

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
  RCP="${WORK}/repocopy"; mkdir -p "$RCP"; cp -r "${REPO_ROOT}/infra" "$RCP/"; cp "${REPO_ROOT}/Cargo.lock" "$RCP/"
  mv "$RCP/infra/docker/gc-service" "$RCP/infra/docker/gc-service.moved"
  out="$(bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; first_party_repos' _ "$RCP/infra/kind/scripts/setup.sh" 2>&1)"; rc=$?
  assert_rc "repo-without-dockerfile-fails" 1 "$rc"
  assert_status "repo-without-dockerfile-named" "localhost/gc-service has no infra/docker/gc-service/Dockerfile" "$out"
  mv "$RCP/infra/docker/gc-service.moved" "$RCP/infra/docker/gc-service"
  # Zero first-party images anywhere: every localhost/ ref rewritten away.
  grep -rl 'image: localhost/' "$RCP/infra/services" | xargs sed -i 's#image: localhost/#image: example.org/#'
  out="$(bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; first_party_repos' _ "$RCP/infra/kind/scripts/setup.sh" 2>&1)"; rc=$?
  assert_rc "repos-derived-zero-fails" 1 "$rc"
  assert_status "repos-derived-zero-says-vacuous" "refusing a vacuous converge" "$out"
fi

# --- An empty --iidfile never becomes a guessed tag ------------------------------------------
reset_marks
out="$(PATH="${D4_BIN}:${PATH}" TMPDIR="$RWORK" DEVLOOP_MIN_DISK_GB=0 STUB_IID="" \
  bash -c 'sp="$1"; set --; source "$sp" >/dev/null 2>&1; KIND_EXPERIMENTAL_PROVIDER=podman; build_content_tagged_image localhost/gc-service' _ "$SETUP" 2>&1)"; rc=$?
assert_rc "iidfile-empty-fails" 1 "$rc"
assert_status "iidfile-empty-says-so" "wrote no image ID" "$out"
assert_absent "iidfile-empty-never-tags" "tag " "$(cat "${MARK}/podman.calls" 2>/dev/null | grep -v '^build')"

# --- The devloop container no longer migrates (test.sh is the one owner) ---------------------
entry_code="$(grep -vE '^[[:space:]]*#' "${REPO_ROOT}/infra/devloop/entrypoint.sh")"
assert_absent "entrypoint-has-no-sqlx-migrate" "sqlx migrate" "$entry_code"
assert_absent "entrypoint-has-no-masked-migration" "may already be applied" "$entry_code"

report_results "scripts/setup.test.sh"
