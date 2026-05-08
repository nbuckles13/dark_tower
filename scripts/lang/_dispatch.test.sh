#!/usr/bin/env bash
# _dispatch.test.sh — dispatcher loud-fail-on-missing-changed.sh test (test §E).
#
# Hermeticity (test §A + post-confirmation refinement): operates on a copy of
# `_dispatch.sh` and a synthetic lang/ tree in a tempdir. Never mutates the live
# scripts/lang/ tree — a flaky test can't leave fakeland/ behind.
set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PASS=0
FAIL=0
FAILURES=()

assert_pattern_in() {
  local label="$1" pattern="$2" haystack="$3"
  if grep -q "$pattern" <<<"$haystack"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected pattern '${pattern}' in output, got: ${haystack}")
  fi
}

assert_nonzero_exit() {
  local label="$1" actual="$2"
  if [[ "$actual" -ne 0 ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[${label}] expected non-zero exit, got 0")
  fi
}

# -----------------------------------------------------------------------------
# Test: dispatcher fails loud when a lang dir lacks changed.sh
# -----------------------------------------------------------------------------

test_missing_changed_sh() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  # Copy helpers + dispatcher into the tempdir so we never mutate live tree.
  mkdir -p "${tmp}/lang/fakeland"
  cp "${__here}/_common.sh" "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  # fakeland/ is intentionally empty — no changed.sh.

  # Invoke the dispatcher with DEVLOOP_LANG_ROOT pointing at our synthetic tree.
  local rc=0 out
  out=$(
    set +e
    DEVLOOP_LANG_ROOT="${tmp}/lang" bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
    echo "__rc=$?"
  )
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)
  rc="${rc:-0}"

  assert_nonzero_exit "missing-changed-sh" "$rc"
  assert_pattern_in   "missing-changed-sh" "fakeland/changed.sh" "$out"

  # Cleanup explicit (RETURN trap covers the lazy path).
  rm -rf "$tmp"
  trap - RETURN
}

# -----------------------------------------------------------------------------
# Test: dispatcher with a working lang emits expected STATUS shape (single-lang)
# -----------------------------------------------------------------------------

test_single_lang_no_double_emit() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/fakelang"
  cp "${__here}/_common.sh" "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  # Synthetic changed.sh that always says "touched".
  cat > "${tmp}/lang/fakelang/changed.sh" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "${tmp}/lang/fakelang/changed.sh"

  # Synthetic verb that emits its own STATUS.
  cat > "${tmp}/lang/fakelang/test.sh" <<'EOF'
#!/usr/bin/env bash
echo "STATUS=OK REASON=fakelang-test-passed"
exit 0
EOF
  chmod +x "${tmp}/lang/fakelang/test.sh"

  local out
  out=$(
    DEVLOOP_LANG_ROOT="${tmp}/lang" bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
  )

  # 1-lang case: dispatcher should NOT emit a duplicate aggregated STATUS.
  local count
  count=$(grep -c '^STATUS=' <<<"$out" || true)
  if [[ "$count" -eq 1 ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[single-lang-no-double-emit] expected exactly 1 STATUS= line, got ${count}: ${out}")
  fi
  assert_pattern_in "single-lang-no-double-emit" "STATUS=OK REASON=fakelang-test-passed" "$out"

  rm -rf "$tmp"
  trap - RETURN
}

# -----------------------------------------------------------------------------
# Test: stream-verbatim contract (test-reviewer ask post-Gate-1)
#
# With the precedence reorder (NO-DIFF beats NO-VERB), the layer's aggregated
# STATUS will MASK a NO-VERB child by promoting NO-DIFF. The structural-error
# signal must remain visible somewhere — that "somewhere" is the per-child
# STATUS line surviving in the layer's stdout VERBATIM.
#
# This test enforces: when one child emits NO-DIFF and another emits NO-VERB,
#   (a) the dispatcher's aggregated STATUS line is NO-DIFF (per precedence),
#   (b) the per-child NO-VERB STATUS line is present VERBATIM in the dispatcher's
#       stdout — not silenced, not aggregated-away.
#
# If a future refactor accidentally drops verbatim streaming (e.g. swallows
# child stdout, only emits aggregated), this test catches it loud.
# -----------------------------------------------------------------------------

test_stream_verbatim_contract() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/touched_no_verb" "${tmp}/lang/untouched"
  cp "${__here}/_common.sh" "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  # touched_no_verb: changed.sh says "touched", but no test.sh exists.
  # Dispatcher should emit STATUS=SKIPPED-NO-VERB for this lang.
  cat > "${tmp}/lang/touched_no_verb/changed.sh" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "${tmp}/lang/touched_no_verb/changed.sh"
  # No test.sh — dispatcher must emit SKIPPED-NO-VERB for this lang.

  # untouched: changed.sh says "untouched" (exit 1).
  # Dispatcher should emit STATUS=SKIPPED-NO-DIFF for this lang.
  cat > "${tmp}/lang/untouched/changed.sh" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
  chmod +x "${tmp}/lang/untouched/changed.sh"

  local out
  out=$(
    DEVLOOP_LANG_ROOT="${tmp}/lang" bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
  )

  # (b) per-child NO-VERB line MUST be in the verbatim stream.
  if grep -q '^STATUS=SKIPPED-NO-VERB.*touched_no_verb' <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] per-child SKIPPED-NO-VERB for touched_no_verb missing from stdout
  output:
${out}
  → ADR-0033 §6 'loud-on-missing-verb' invariant relies on verbatim streaming")
  fi

  # Per-child NO-DIFF line MUST also be in the verbatim stream.
  if grep -q '^STATUS=SKIPPED-NO-DIFF.*untouched' <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] per-child SKIPPED-NO-DIFF for untouched missing from stdout: ${out}")
  fi

  # (a) aggregated dispatcher STATUS line is the LAST STATUS= line; per locked
  # precedence (FAIL > N/A > SKIPPED-NO-DIFF > SKIPPED-NO-VERB > OK) it should
  # be SKIPPED-NO-DIFF since it beats SKIPPED-NO-VERB.
  local last_status
  last_status=$(grep '^STATUS=' <<<"$out" | tail -n1 | sed -n 's/^STATUS=\([^ ]*\).*/\1/p')
  if [[ "$last_status" == "SKIPPED-NO-DIFF" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] aggregated STATUS expected SKIPPED-NO-DIFF, got '${last_status}'
  per locked precedence (NO-DIFF > NO-VERB > OK): ${out}")
  fi
}

# -----------------------------------------------------------------------------
# Run all
# -----------------------------------------------------------------------------

test_missing_changed_sh
test_single_lang_no_double_emit
test_stream_verbatim_contract

printf '\n_dispatch.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
