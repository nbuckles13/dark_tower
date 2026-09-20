#!/usr/bin/env bash
# fmt.test.sh — hermetic self-test for scripts/lang/rust/fmt.sh (ADR-0037 §D7).
#
# HERMETIC: NO real toolchain. `cargo` is a PATH STUB (no real cargo — satisfies layer3.sh's
# "no cluster, no network, no cargo" invariant; the stub is not the toolchain). The stub RECORDS its
# argv so cases assert the exact flag vector, and models cargo-fmt's rc/stdout per env.
#
# FOREIGN-MODEL RESIDUAL (review-protocol §Assertion-Vacuity #4): the stub's rc/stdout mapping is a
# FROM-MEMORY model of `rustfmt 1.9.0-stable` (verified on-box: `cargo fmt --all -- --check -l` →
# clean rc=0/empty, drift rc=1/paths, parse-error rc=1/EMPTY+stderr; bare `cargo fmt --all` on a parse
# error → rc=1, file unchanged). A future rustfmt change is not caught by this stubbed suite — it is
# only traceable via this comment. The apply path is ALSO independently fail-closed in production
# (`cargo fmt --all` exits non-zero on a parse error), so `cargo-fmt-unparseable` is a better token,
# not the sole control.
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"          # DEVLOOP_TMP, init_devloop_tmp
source "${__here}/../_test_helpers.sh"    # PASS/FAIL, assert_status/_absent/_marker/report_results

# Scratch under DEVLOOP_TMP (700-perm), NEVER the repo tree. The dirtying-safety (security S-9) comes from
# the STUBBED cargo performing NO writes — NOT from cwd isolation (mirrors proto/fmt.test.sh's phrasing).
# NOTE (do NOT "fix" this by cd-ing into $__work): run_fmt deliberately runs the wrapper from the test's
# cwd (the real repo root), so the wrapper's `git rev-parse --show-toplevel` resolves to the SAME path as
# STUB_ROOT — that identity is what makes the APPLY-drift relativization assertion pass. A `cd "$__work"`
# would make git fall back to pwd=$__work (no .git) while STUB_ROOT stays the real root, breaking both the
# `FMT_APPLIED=crates/foo/src/lib.rs` and `assert_absent FMT_APPLIED=/` assertions.
init_devloop_tmp
__work="$(mktemp -d "${DEVLOOP_TMP}/rust-fmt-selftest.XXXXXX")"
__stubbin="${__work}/bin"
mkdir -p "$__stubbin"
trap 'rm -rf "$__work"' EXIT

cat > "${__stubbin}/cargo" <<'STUB'
#!/usr/bin/env bash
# stub cargo — records argv, models `cargo fmt` per env (see the foreign-model residual in fmt.test.sh).
printf 'cargo %s\n' "$*" >> "$STUB_LOG"
args="$*"
if [[ "$args" == *"--check"* && "$args" == *"-l"* ]]; then           # APPLY pre-pass
  case "$STUB_PREPASS" in
    clean)    exit 0 ;;
    drift)    echo "${STUB_ROOT}/crates/foo/src/lib.rs"; exit 1 ;;   # ABSOLUTE under the repo root → tests relativization
    parseerr) echo "error: unclosed delimiter" >&2; exit 1 ;;       # rc1 + EMPTY stdout
  esac
elif [[ "$args" == *"--check"* ]]; then                              # plain CHECK
  exit "${STUB_CHECK_RC:-0}"
else                                                                 # bare apply
  exit "${STUB_APPLY_RC:-0}"
fi
STUB
chmod +x "${__stubbin}/cargo"

# Run the REAL wrapper with the stub first on PATH and a fresh argv log; capture stdout+stderr.
__root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"   # the root fmt.sh will relativize against
run_fmt() {  # $@ = env assignments for this invocation
  STUB_LOG="${__work}/argv.$$"; : > "$STUB_LOG"
  env -u DEVLOOP_FMT_CHECK_ONLY -u DEVLOOP_FMT_APPLY -u GITHUB_ACTIONS -u CI \
      STUB_LOG="$STUB_LOG" STUB_ROOT="$__root" PATH="${__stubbin}:$PATH" "$@" \
      bash "${__here}/fmt.sh" 2>&1
}
argv() { cat "${__work}/argv.$$" 2>/dev/null; }
# Count cargo invocations recorded this run — distinguishes "pre-pass only" (1) from "pre-pass + apply" (2),
# the only way to prove the apply form did / did not run (obs F4/F5): the apply line `cargo fmt --all` is a
# SUBSTRING of the pre-pass `cargo fmt --all -- --check -l`, so assert_absent cannot separate them.
assert_cargo_calls() {  # <label> <expected-count>
  local label="$1" want="$2" got
  got="$(argv | grep -c '^cargo' || true)"
  if [ "$got" -eq "$want" ]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[${label}] expected ${want} cargo invocation(s), got ${got}"); fi
}

# --- CHECK (default): plain --check, NO -l; positive control that cargo was reached ---
out="$(run_fmt STUB_CHECK_RC=0)" || true
assert_status "check-default OK"        "STATUS=OK REASON=cargo-fmt-passed" "$out"
# obs F3: the lane decision MUST land in the log — assert both halves of the anchor (FMT_MODE + SOURCE),
# not just that cargo ran. Without this, rust/fmt.sh could drop its fmt_mode_emit call and nothing reds.
assert_status "check emits FMT_MODE+SOURCE anchor" "FMT_MODE=check SOURCE=check-default" "$out"
assert_status "check uses --check"      "--check"                          "$(argv)"
assert_absent "check has NO -l"         " -l"                              "$(argv)"
assert_status "check reached cargo (positive control)" "cargo fmt"         "$(argv)"

# --- CHECK CI-fail non-vacuity (@test #1): REASON must be cargo-fmt-failed, NOT wrapper-aborted ---
out="$(run_fmt GITHUB_ACTIONS=1 STUB_CHECK_RC=1)" || true
assert_status "ci-fail is cargo-fmt-failed"  "REASON=cargo-fmt-failed"          "$out"
assert_absent "ci-fail not wrapper-aborted"  "wrapper-aborted-early-exit"       "$out"
assert_status "ci-fail actually ran --check" "--check"                          "$(argv)"

# --- APPLY local: pre-pass drift -> apply; applied token + relativized FMT_APPLIED; apply has NO --check ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_PREPASS=drift STUB_APPLY_RC=0)" || true
assert_status "apply drift -> applied"       "STATUS=OK REASON=cargo-fmt-applied" "$out"
assert_status "apply emits FMT_APPLIED"      "FMT_APPLIED=crates/foo/src/lib.rs"  "$out"   # relativized (no /abs prefix)
assert_absent "FMT_APPLIED has no leading /" "FMT_APPLIED=/"                       "$out"
assert_cargo_calls "apply drift ran BOTH pre-pass + apply" 2   # positive control for the F5 count below

# --- APPLY clean: pre-pass rc0 -> passed, no second (apply) cargo invocation (obs F5: the skip is real
#     behaviour — it's why the two-pass cost is paid only on runs that reformat — so assert it, don't just
#     claim it in a comment). ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_PREPASS=clean)" || true
assert_status "apply clean -> passed"        "STATUS=OK REASON=cargo-fmt-passed" "$out"
assert_cargo_calls "apply clean did NOT run a 2nd (apply) cargo" 1

# --- APPLY parse-error (@test): rc1 + EMPTY -> unparseable, do NOT apply (obs F4: assert the two ABSENCES
#     — no FMT_APPLIED line at all, and the apply form was never invoked — since "do NOT apply" is the
#     arm's whole purpose). ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_PREPASS=parseerr)" || true
assert_status "apply parse-error -> unparseable" "STATUS=FAIL REASON=cargo-fmt-unparseable" "$out"
assert_absent "parse-error emits NO FMT_APPLIED line" "FMT_APPLIED" "$out"
assert_cargo_calls "parse-error never invoked the apply form" 1

# --- INVALID knob: loud FAIL, cargo NEVER reached (positive control on the reject path) ---
out="$(run_fmt DEVLOOP_FMT_APPLY=ture)" || true
assert_status "invalid knob -> fmt-mode-invalid-knob" "STATUS=FAIL REASON=fmt-mode-invalid-knob" "$out"
assert_absent "invalid knob never ran cargo"          "cargo fmt"                                "$(argv)"

# --- UNKNOWN verdict (obs F1 / defense-in-depth): fmt_mode returns only INVALID/CHECK/APPLY via knobs, so
#     to exercise the `*)` arm we OVERRIDE fmt_mode. Copy the real wrapper beside a shim _common.sh that
#     sources the real one (for emit_status/run_and_emit/fmt_mode_emit/the trap) then redefines fmt_mode to
#     return a bogus token. The arm must red with the ACCURATE token, NOT fall through to the EXIT trap's
#     `wrapper-aborted-early-exit-0` (the confidently-wrong diagnosis obs flagged). ---
__ghost="${__work}/ghost/lang/rust"; mkdir -p "$__ghost"
cp "${__here}/fmt.sh" "${__ghost}/fmt.sh"
printf '#!/usr/bin/env bash\nsource %q\nfmt_mode() { printf "%%s\\n" "GHOST unreachable-verdict"; }\n' \
  "$(cd "${__here}/.." && pwd)/_common.sh" > "${__work}/ghost/lang/_common.sh"
out="$(env -u DEVLOOP_FMT_CHECK_ONLY -u DEVLOOP_FMT_APPLY -u GITHUB_ACTIONS -u CI \
       PATH="${__stubbin}:$PATH" bash "${__ghost}/fmt.sh" 2>&1 || true)"
assert_status "unknown verdict -> fmt-mode-unknown-verdict" "STATUS=FAIL REASON=fmt-mode-unknown-verdict" "$out"
assert_absent "unknown verdict NOT misdiagnosed as wrapper-abort" "wrapper-aborted-early-exit" "$out"

report_results "scripts/lang/rust/fmt.test.sh"
