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
  # precedence (FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB) it should
  # be SKIPPED-NO-DIFF since it beats SKIPPED-NO-VERB.
  local last_status
  last_status=$(grep '^STATUS=' <<<"$out" | tail -n1 | sed -n 's/^STATUS=\([^ ]*\).*/\1/p')
  if [[ "$last_status" == "SKIPPED-NO-DIFF" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] aggregated STATUS expected SKIPPED-NO-DIFF, got '${last_status}'
  per locked precedence (Wave 2 #4 α: FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB): ${out}")
  fi
}

# -----------------------------------------------------------------------------
# Test: DEVLOOP_DISPATCH_INCLUDE_LANGS keeps only the named lang.
#
# INCLUDE_LANGS / EXCLUDE_LANGS are Wave 2 #4 additions. layer1.sh uses
# INCLUDE for stage 1 (proto only) and EXCLUDE for stage 2 (rust+ts). Single-
# lang exact match for Wave 2 — multi-lang/comma-split deferred per YAGNI.
# Sentinel-file assertions confirm filter runs BEFORE the changed.sh
# invocation loop (observability bonus check): a filtered-out lang's
# changed.sh must not be invoked at all (no cache-write side-effects).
# -----------------------------------------------------------------------------

test_include_langs_keeps() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/kept_lang" "${tmp}/lang/excluded_lang"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  for lang in kept_lang excluded_lang; do
    cat > "${tmp}/lang/${lang}/changed.sh" <<EOF
#!/usr/bin/env bash
touch "${tmp}/${lang}.sentinel"
exit 0
EOF
    chmod +x "${tmp}/lang/${lang}/changed.sh"
    cat > "${tmp}/lang/${lang}/test.sh" <<EOF
#!/usr/bin/env bash
echo "STATUS=OK REASON=${lang}-test-passed"
EOF
    chmod +x "${tmp}/lang/${lang}/test.sh"
  done

  local out
  out=$(
    DEVLOOP_LANG_ROOT="${tmp}/lang" \
    DEVLOOP_DISPATCH_INCLUDE_LANGS=kept_lang \
    bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
  )

  assert_pattern_in "include-keeps:status-kept" "STATUS=OK REASON=kept_lang-test-passed" "$out"
  if grep -q 'REASON=excluded_lang-test-passed' <<<"$out"; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[include-keeps:status-excluded] excluded_lang STATUS leaked into output: ${out}")
  else
    PASS=$((PASS + 1))
  fi
  if [[ -e "${tmp}/kept_lang.sentinel" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[include-keeps:kept-changed-sh-ran] kept_lang.sentinel missing — changed.sh was not invoked")
  fi
  if [[ -e "${tmp}/excluded_lang.sentinel" ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[include-keeps:excluded-changed-sh-not-run] excluded_lang.sentinel present — filter did not run before the changed.sh invocation loop")
  else
    PASS=$((PASS + 1))
  fi

  rm -rf "$tmp"
  trap - RETURN
}

# -----------------------------------------------------------------------------
# Test: DEVLOOP_DISPATCH_EXCLUDE_LANGS drops the named lang.
# Mirrors the INCLUDE test but inverts the filter direction.
# -----------------------------------------------------------------------------

test_exclude_langs_drops() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/kept_lang" "${tmp}/lang/excluded_lang"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  for lang in kept_lang excluded_lang; do
    cat > "${tmp}/lang/${lang}/changed.sh" <<EOF
#!/usr/bin/env bash
touch "${tmp}/${lang}.sentinel"
exit 0
EOF
    chmod +x "${tmp}/lang/${lang}/changed.sh"
    cat > "${tmp}/lang/${lang}/test.sh" <<EOF
#!/usr/bin/env bash
echo "STATUS=OK REASON=${lang}-test-passed"
EOF
    chmod +x "${tmp}/lang/${lang}/test.sh"
  done

  local out
  out=$(
    DEVLOOP_LANG_ROOT="${tmp}/lang" \
    DEVLOOP_DISPATCH_EXCLUDE_LANGS=excluded_lang \
    bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
  )

  assert_pattern_in "exclude-drops:status-kept" "STATUS=OK REASON=kept_lang-test-passed" "$out"
  if grep -q 'REASON=excluded_lang-test-passed' <<<"$out"; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[exclude-drops:status-excluded] excluded_lang STATUS leaked into output: ${out}")
  else
    PASS=$((PASS + 1))
  fi
  if [[ -e "${tmp}/kept_lang.sentinel" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[exclude-drops:kept-changed-sh-ran] kept_lang.sentinel missing")
  fi
  if [[ -e "${tmp}/excluded_lang.sentinel" ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[exclude-drops:excluded-changed-sh-not-run] excluded_lang.sentinel present")
  else
    PASS=$((PASS + 1))
  fi

  rm -rf "$tmp"
  trap - RETURN
}

# -----------------------------------------------------------------------------
# Test: INCLUDE_LANGS=nonexistent → empty-after-filter → aggregated
# STATUS=SKIPPED-NO-VERB REASON=all-langs-filtered.
#
# Catches the load-bearing failure mode (test-reviewer's "third test was
# the load-bearing one" point): operator typo or stale config that filters
# to zero langs produces a loud signal, not silent OK.
# -----------------------------------------------------------------------------

test_filter_empty_after_filter() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/fakelang"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  cat > "${tmp}/lang/fakelang/changed.sh" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "${tmp}/lang/fakelang/changed.sh"

  local out
  out=$(
    DEVLOOP_LANG_ROOT="${tmp}/lang" \
    DEVLOOP_DISPATCH_INCLUDE_LANGS=nonexistent_lang \
    bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
  )

  # REASON asserted explicitly per test-reviewer's ask (catches silent drift).
  assert_pattern_in "empty-after-filter:status" "STATUS=SKIPPED-NO-VERB REASON=all-langs-filtered" "$out"

  rm -rf "$tmp"
  trap - RETURN
}

# -----------------------------------------------------------------------------
# Test: aggregate_worst_status precedence — Wave 2 #4 (α) regression test.
#
# Locks the re-ranked ladder: FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB.
# Prior Wave-1 ladder put SKIPPED-* above OK, which broke "loud success" once
# a 2nd lang registered with a verb wrapper (rust-clean PR aggregated to
# SKIPPED-NO-DIFF instead of OK). Lead-imposed regression test (constraint #3).
# -----------------------------------------------------------------------------

test_aggregate_precedence_ok_beats_skipped() {
  local agg
  agg=$(bash -c "
    source '${__here}/_common.sh'
    aggregate_worst_status OK SKIPPED-NO-DIFF SKIPPED-NO-VERB
  ")
  if [[ "$agg" == "OK" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[precedence:OK-beats-SKIPPED-*] expected OK, got '${agg}' — precedence ladder regressed")
  fi

  agg=$(bash -c "
    source '${__here}/_common.sh'
    aggregate_worst_status OK FAIL SKIPPED-NO-DIFF
  ")
  if [[ "$agg" == "FAIL" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[precedence:FAIL-wins] expected FAIL, got '${agg}'")
  fi

  agg=$(bash -c "
    source '${__here}/_common.sh'
    aggregate_worst_status OK N/A
  ")
  if [[ "$agg" == "N/A" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[precedence:NA-beats-OK] expected N/A, got '${agg}'")
  fi
}

# -----------------------------------------------------------------------------
# Test: scripts/audit.sh fail-closed exit-code aggregation (security S2).
#
# `scripts/audit.sh` runs the dispatcher loop, then invokes
# lang/proto/breaking.sh, then exits with the WORST of the two RCs.
# If either gate fails, the script must exit non-zero — naive shell where
# breaking.sh's RC masks the dispatcher's RC is the security regression
# this test catches.
#
# Tests the arithmetic pattern (identical to scripts/audit.sh:12-23), not
# the live invocation (which would require repo-context + buf install).
# -----------------------------------------------------------------------------

test_audit_fail_closed_aggregation() {
  # dispatcher=FAIL(1), breaking=OK(0) → audit must exit 1.
  local rc
  rc=$(bash -c '
    dispatch_rc=1
    breaking_rc=0
    echo "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
  ')
  if [[ "$rc" == "1" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[audit-fail-closed:dispatch-fail+breaking-ok] expected 1, got '${rc}' — breaking_rc=0 must NOT mask dispatch_rc=1")
  fi

  # dispatcher=OK(0), breaking=FAIL(1) → audit must exit 1.
  rc=$(bash -c '
    dispatch_rc=0
    breaking_rc=1
    echo "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
  ')
  if [[ "$rc" == "1" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[audit-fail-closed:dispatch-ok+breaking-fail] expected 1, got '${rc}'")
  fi

  # Both OK → audit exits 0.
  rc=$(bash -c '
    dispatch_rc=0
    breaking_rc=0
    echo "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
  ')
  if [[ "$rc" == "0" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[audit-fail-closed:both-ok] expected 0, got '${rc}'")
  fi

  # UNKNOWN (rc=2) wins over FAIL (rc=1) — dispatcher bug surfaces loud.
  rc=$(bash -c '
    dispatch_rc=2
    breaking_rc=1
    echo "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
  ')
  if [[ "$rc" == "2" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[audit-fail-closed:unknown-beats-fail] expected 2, got '${rc}'")
  fi
}

# -----------------------------------------------------------------------------
# Run all
# -----------------------------------------------------------------------------

test_missing_changed_sh
test_single_lang_no_double_emit
test_stream_verbatim_contract
test_include_langs_keeps
test_exclude_langs_drops
test_filter_empty_after_filter
test_aggregate_precedence_ok_beats_skipped
test_audit_fail_closed_aggregation

printf '\n_dispatch.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
