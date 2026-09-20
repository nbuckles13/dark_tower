#!/usr/bin/env bash
# fmt.test.sh — hermetic self-test for scripts/lang/proto/fmt.sh + _buf.sh (ADR-0037 §D7, COLLAPSE branch).
#
# HERMETIC: NO real toolchain. `pnpm` is a PATH STUB (no real pnpm/buf — satisfies layer3.sh's "no
# cluster, no network, no cargo/buf" invariant; the stub is not the toolchain). The stub RECORDS its argv
# so cases assert the exact command vector (`pnpm exec buf format --diff…` vs `… -w …`), and models
# `pnpm exec buf`'s rc/stdout per env. `jq`/`git` are the REAL binaries (preflight derives the expected
# version from the on-disk package.json — the SAME source of truth production uses, so the test cannot
# drift from the pin).
#
# FOREIGN-MODEL RESIDUAL (review-protocol §Assertion-Vacuity #4): the stub's rc/stdout mapping is a
# FROM-MEMORY model of `buf 1.72.0` (the version `pnpm exec` resolves): `buf format --diff --exit-code` →
# clean rc=0/empty · drift rc=100/unified-diff-on-stdout · parse-error rc=1/`Failure:`-on-stderr; `buf
# format -w` → rc=1 + NOTHING written on a parse error (batch fail-closed). This is the INVERSE of rust's
# rc map (rust drift=1) — a real defect this loop caught. A future buf change is not caught by this stubbed
# suite; it is only traceable via this comment and via the production post-condition re-check (which is
# tool-agnostic: it fails on any residual drift regardless of the rc model).
set -euo pipefail
IFS=$'\n\t'
__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${__here}/../_common.sh"          # DEVLOOP_TMP, init_devloop_tmp
source "${__here}/../_test_helpers.sh"    # PASS/FAIL, assert_status/_absent/_marker/report_results

# Scratch under DEVLOOP_TMP (700-perm), NEVER the repo tree — a self-test must be incapable of dirtying the
# tree a gate attests (security S-9). The stub never writes to proto/ either; it only models rc/stdout.
init_devloop_tmp
__work="$(mktemp -d "${DEVLOOP_TMP}/proto-fmt-selftest.XXXXXX")"
__stubbin="${__work}/bin"
mkdir -p "$__stubbin"
trap 'rm -rf "$__work"' EXIT

# The lockfile-declared pin — derived the SAME way _buf.sh derives `expected`, so the passing cases feed
# the stub the value preflight will accept, and the version-mismatch case feeds a deliberately-wrong one.
__root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
__pin="$(jq -er '.devDependencies["@bufbuild/buf"] // empty' "${__root}/package.json")"
[[ -n "$__pin" ]] || { echo "FATAL: could not derive @bufbuild/buf pin for the stub" >&2; exit 2; }

cat > "${__stubbin}/pnpm" <<'STUB'
#!/usr/bin/env bash
# stub pnpm — records argv, models `pnpm exec buf` per env (see the foreign-model residual in fmt.test.sh).
printf 'pnpm %s\n' "$*" >> "$STUB_LOG"
[[ "$1" == "exec" && "$2" == "buf" ]] || exit 0
shift 2                                                        # $@ is now buf's own args
case "$1" in
  --version)
    # STUB_BUF_VERSION empty -> `pnpm exec buf` fails to resolve (models @bufbuild/buf not installed).
    [[ -n "${STUB_BUF_VERSION:-}" ]] || exit 1
    printf '%s\n' "$STUB_BUF_VERSION"; exit 0 ;;
  format)
    if [[ "$*" == *"--diff"* ]]; then                         # CHECK, and both APPLY re-check passes
      # Stateful: 1st --diff uses STUB_FMT (pre-pass); every later --diff uses STUB_POST_FMT (post-check),
      # so "apply rewrote SOME but not all" is expressible (pre=drift, apply ok, post=drift).
      n="$(cat "$STUB_DIFF_COUNT" 2>/dev/null || echo 0)"; echo $((n + 1)) > "$STUB_DIFF_COUNT"
      if [[ "$n" -eq 0 ]]; then mode="$STUB_FMT"; else mode="${STUB_POST_FMT:-clean}"; fi
      case "$mode" in
        clean)    exit 0 ;;
        drift)    printf -- '--- proto/a.proto\n+++ proto/a.proto\t(formatted)\n@@ -1 +1 @@\n'; exit 100 ;;
        parseerr) echo "Failure: proto/a.proto:3:1: expected '}'" >&2; exit 1 ;;    # rc=1, NO diff
      esac
    elif [[ "$*" == *"-w"* ]]; then                           # APPLY write
      exit "${STUB_APPLY_RC:-0}"
    fi
    ;;
esac
exit 0
STUB
chmod +x "${__stubbin}/pnpm"

run_fmt() {  # $@ = env assignments for this invocation
  STUB_LOG="${__work}/argv.$$"; : > "$STUB_LOG"
  : > "${__work}/diffcount.$$"
  env -u DEVLOOP_FMT_CHECK_ONLY -u DEVLOOP_FMT_APPLY -u GITHUB_ACTIONS -u CI \
      -u STUB_BUF_VERSION -u STUB_FMT -u STUB_POST_FMT -u STUB_APPLY_RC \
      STUB_LOG="$STUB_LOG" STUB_DIFF_COUNT="${__work}/diffcount.$$" STUB_BUF_VERSION="$__pin" \
      PATH="${__stubbin}:$PATH" "$@" \
      bash "${__here}/fmt.sh" 2>&1
}
argv() { cat "${__work}/argv.$$" 2>/dev/null; }

# --- CHECK (default): --diff/--exit-code, NEVER -w; positive control that buf was reached ---
out="$(run_fmt STUB_FMT=clean)" || true
assert_status "check-default OK"         "STATUS=OK REASON=buf-format-passed"  "$out"
# obs F3: the lane decision MUST land in the log — assert both halves of the anchor (FMT_MODE + SOURCE), not
# just that buf ran. Without this, proto/fmt.sh could drop its fmt_mode_emit call and nothing reds.
assert_status "check emits FMT_MODE+SOURCE anchor" "FMT_MODE=check SOURCE=check-default" "$out"
assert_status "check reached buf (pos control)" "exec buf format --diff"       "$(argv)"
assert_absent "check NEVER writes (-w)"  "format -w"                           "$(argv)"

# --- CHECK drift fails LOUD (run_and_emit maps any nonzero -> failed; no silent skip) ---
out="$(run_fmt STUB_FMT=drift)" || true
assert_status "check drift -> failed"    "STATUS=FAIL REASON=buf-format-failed" "$out"
assert_absent "check drift never wrote"  "format -w"                            "$(argv)"

# --- APPLY drift -> apply + POST re-check clean: WARN anchor + FMT_APPLIED + applied token ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_FMT=drift STUB_APPLY_RC=0 STUB_POST_FMT=clean)" || true
assert_status "apply drift -> applied"     "STATUS=OK REASON=buf-format-applied" "$out"
assert_status "apply emits FMT_APPLIED"    "FMT_APPLIED=proto/a.proto"           "$out"
assert_status "apply emits GSA WARN anchor" "WARN PROTO_FMT_APPLIED FILES=proto/a.proto" "$out"
assert_status "apply actually wrote (-w)"  "format -w proto"                     "$(argv)"

# --- APPLY post-condition (@paired-protocol): pre drift, write ok, but re-check STILL drifts -> FAIL,
#     NEVER -applied, NEVER a WARN claiming success ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_FMT=drift STUB_APPLY_RC=0 STUB_POST_FMT=drift)" || true
assert_status "apply post-drift -> postcondition-failed" "STATUS=FAIL REASON=buf-format-postcondition-failed" "$out"
assert_absent "post-fail not mislabeled applied"         "buf-format-applied"                                 "$out"
assert_absent "post-fail emits no success WARN"          "WARN PROTO_FMT_APPLIED"                             "$out"

# --- APPLY parse-error: rc=1/EMPTY -> parse-error token, do NOT write, NO WARN, byte-identical proto
#     (the stub never writes; the control is that `-w` is NEVER invoked after a parse error) ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_FMT=parseerr)" || true
assert_status "apply parse-error -> parse-error token" "STATUS=FAIL REASON=buf-format-parse-error" "$out"
assert_absent "parse-error never wrote (-w)"           "format -w"                                 "$(argv)"
assert_absent "parse-error emits no WARN"              "WARN PROTO_FMT_APPLIED"                     "$out"

# --- CHECK parse-error also fails loud (run_and_emit) and never writes ---
out="$(run_fmt STUB_FMT=parseerr)" || true
assert_status "check parse-error -> failed"  "STATUS=FAIL REASON=buf-format-failed" "$out"

# --- P-6 PRECEDENCE, BOTH conditions live: a stale writer (wrong version) AND queued drift in APPLY mode
#     must red as buf-version-mismatch and NEVER reach `buf format` (preflight precedes the mode branch).
#     If this ever fired buf-format-drift instead, the GSA acceptance's condition-3 is void. ---
out="$(run_fmt DEVLOOP_FMT_APPLY=1 STUB_BUF_VERSION=0.0.0-stale STUB_FMT=drift)" || true
assert_status "stale writer -> version-mismatch"      "STATUS=FAIL REASON=buf-version-mismatch" "$out"
assert_absent "version-mismatch never ran format"     "buf format"                              "$(argv)"

# --- FOUR-TOKEN taxonomy: buf-not-installed (pnpm present, `pnpm exec buf --version` fails to resolve) ---
out="$(run_fmt STUB_BUF_VERSION= STUB_FMT=clean)" || true   # empty version -> stub exits 1 on --version
assert_status "buf unresolved -> buf-not-installed"   "STATUS=FAIL REASON=buf-not-installed" "$out"

# --- FOUR-TOKEN discriminator: bare `buf` on PATH but NO pnpm -> pnpm-unavailable (NOT buf-not-installed).
#     This is the COLLAPSE decision under test: the lane does NOT fall back to a bare-PATH buf. Build an
#     isolated bin with the REAL coreutils (so _common.sh sources normally) but NO pnpm, plus a bare buf.
#     (This box ships a real pnpm on the default PATH, so we cannot just trim PATH to /usr/bin.) ---
__nopnpm="${__work}/nopnpm"; mkdir -p "$__nopnpm"
for __d in /usr/bin /bin; do for __f in "$__d"/*; do ln -sf "$__f" "$__nopnpm/" 2>/dev/null || true; done; done
rm -f "${__nopnpm}/pnpm"                                     # the discriminator: pnpm absent
printf '#!/usr/bin/env bash\nexit 0\n' > "${__nopnpm}/buf"; chmod +x "${__nopnpm}/buf"   # bare buf PRESENT
out="$(env -u DEVLOOP_FMT_CHECK_ONLY -u DEVLOOP_FMT_APPLY -u GITHUB_ACTIONS -u CI \
        PATH="${__nopnpm}" bash "${__here}/fmt.sh" 2>&1 || true)"
assert_status "no pnpm (bare buf present) -> pnpm-unavailable" "STATUS=FAIL REASON=pnpm-unavailable" "$out"

# --- INVALID knob: preflight passes (valid version) THEN mode is INVALID -> loud FAIL, format NEVER run ---
out="$(run_fmt DEVLOOP_FMT_APPLY=ture STUB_FMT=clean)" || true
assert_status "invalid knob -> fmt-mode-invalid-knob"  "STATUS=FAIL REASON=fmt-mode-invalid-knob" "$out"
assert_absent "invalid knob never ran buf format"      "buf format"                               "$(argv)"

# --- UNKNOWN verdict (obs F1 / defense-in-depth): fmt_mode returns only INVALID/CHECK/APPLY via knobs, so
#     to exercise the `*)` arm we OVERRIDE fmt_mode. Copy the real wrapper + _buf.sh beside a shim _common.sh
#     that sources the real one then redefines fmt_mode to a bogus token. Preflight runs FIRST (pnpm stub
#     with the correct version), THEN the bogus verdict hits `*)`, which must red with the accurate token —
#     NOT fall through to the EXIT trap's `wrapper-aborted-early-exit-0`. ---
__ghost="${__work}/ghost/lang/proto"; mkdir -p "$__ghost"
cp "${__here}/fmt.sh" "${__ghost}/fmt.sh"
cp "${__here}/_buf.sh" "${__ghost}/_buf.sh"
printf '#!/usr/bin/env bash\nsource %q\nfmt_mode() { printf "%%s\\n" "GHOST unreachable-verdict"; }\n' \
  "$(cd "${__here}/.." && pwd)/_common.sh" > "${__work}/ghost/lang/_common.sh"
out="$(env -u DEVLOOP_FMT_CHECK_ONLY -u DEVLOOP_FMT_APPLY -u GITHUB_ACTIONS -u CI \
       -u STUB_FMT -u STUB_POST_FMT -u STUB_APPLY_RC \
       STUB_LOG="${__work}/argv.ghost" STUB_DIFF_COUNT="${__work}/diffcount.ghost" STUB_BUF_VERSION="$__pin" \
       PATH="${__stubbin}:$PATH" bash "${__ghost}/fmt.sh" 2>&1 || true)"
assert_status "unknown verdict -> fmt-mode-unknown-verdict" "STATUS=FAIL REASON=fmt-mode-unknown-verdict" "$out"
assert_absent "unknown verdict NOT misdiagnosed as wrapper-abort" "wrapper-aborted-early-exit" "$out"

report_results "scripts/lang/proto/fmt.test.sh"
