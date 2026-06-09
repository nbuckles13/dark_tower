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

  # NOTE (task #50 cross-lang-masking residual — do NOT "fix" this expecting a red):
  # this fixture pairs touched_no_verb (UNEXPECTED verb-missing, enum SKIPPED-NO-VERB)
  # with untouched (SKIPPED-NO-DIFF, rank 1). The aggregate winner is SKIPPED-NO-DIFF,
  # so the UNEXPECTED child's enum != the winner and it is EXCLUDED from the worst-reason
  # pick → the dispatcher exits 0. That is the documented masking residual (a sibling
  # non-bug status masks a single unexpected verb-missing); the headline criterion-(b)
  # red is the SINGLE-lang case (test_unexpected_verb_missing_single_lang below). This
  # test only asserts STATUS lines (not the exit code), so it stays green by design.
  #
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

  local out rc
  out=$(
    set +e
    DEVLOOP_LANG_ROOT="${tmp}/lang" \
    DEVLOOP_DISPATCH_INCLUDE_LANGS=nonexistent_lang \
    bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
    echo "__rc=$?"
  )
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  # REASON asserted explicitly per test-reviewer's ask (catches silent drift).
  assert_pattern_in "empty-after-filter:status" "STATUS=SKIPPED-NO-VERB REASON=all-langs-filtered" "$out"
  # Criterion (c): all-langs-filtered is OPERATOR INTENT — must stay exit 0 even though
  # it shares the SKIPPED-NO-VERB enum with the now-exit-2 UNEXPECTED case (task #50).
  # Assert the rc explicitly so the reason-aware exit path can't silently flip it.
  if [[ "$rc" == "0" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[empty-after-filter:rc] expected 0 (operator-intent), got '${rc}': ${out}")
  fi

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
# Tests the arithmetic pattern (the `exit "$(( dispatch_rc > breaking_rc ? dispatch_rc
# : breaking_rc ))"` worst-rc fold at the tail of scripts/audit.sh), not the live
# invocation (which would require repo-context + buf install). NOTE: the audit-gate
# SECURITY guard added in task #50 sits BETWEEN the dispatch capture and this fold and is
# covered separately by test_audit_security_guard_reds_layer (which extracts that block
# live); this test stays focused on the breaking-rc-must-not-mask-dispatch-rc invariant.
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
# Test: scripts/audit.sh SECURITY fail-closed (security finding, Lead ruling, task #50).
#
# The cross-lang-masking residual would fail-OPEN an audit (security) gate: a deleted
# rust/audit.sh (UNEXPECTED) masked by ts/audit.sh OK → aggregate OK → the dep-vuln scan
# silently never ran. scripts/audit.sh closes the AUDIT slice WITHOUT touching the ladder:
# it scans the captured dispatch output for the UNEXPECTED marker and EMITS a STATUS=FAIL
# line (so the layer's tee_collect_statuses aggregates FAIL → layer exits 1) AND folds
# non-zero into its own rc (standalone path).
#
# CRITICAL coverage (Lead): the bare-rc fix alone is insufficient — layer6.sh keys the
# LAYER exit on the STATUS stream, not on audit.sh's rc. So this test drives the real
# scan-and-emit block (extracted live from scripts/audit.sh so it can't drift) through a
# LAYER lifecycle and asserts the LAYER exits non-zero, plus the proto-intentional-gap
# control stays exit 0.
# -----------------------------------------------------------------------------

# Run the SECURITY scan-and-emit block from the REAL scripts/audit.sh over a synthetic
# dispatch-output file, inside a real layer lifecycle. Echoes the emitted STATUS lines +
# a trailing __rc= (the LAYER exit code). $1 = newline-separated dispatch STATUS stream.
__run_audit_guard_through_layer() {
  local dispatch_stream="$1"
  local audit_sh="${__here}/../../scripts/audit.sh"
  local common="${__here}/_common.sh"
  # Extract the production scan-and-emit block (between the PIPESTATUS capture and the
  # breaking_rc line) so the test exercises the SHIPPED logic, not a copy.
  local block
  block=$(awk '/^dispatch_rc=\$\{PIPESTATUS\[0\]\}$/{f=1;next} /^breaking_rc=0$/{f=0} f' "$audit_sh")
  local tmp; tmp=$(mktemp -d)
  printf '%s\n' "$dispatch_stream" > "${tmp}/audit_out"
  local driver="${tmp}/drv.sh"
  {
    printf '#!/usr/bin/env bash\nset -uo pipefail\nIFS=$'"'"'\\n\\t'"'"'\n'
    printf 'source %q\n' "$common"
    printf 'layer_lifecycle_begin 6\n'
    # Sub-shell mirrors layer6: audit-side emits (dispatch stream replay + guard emits)
    # piped through tee_collect_statuses so the layer aggregates the full stream.
    printf '{\n'
    printf '  cat %q\n' "${tmp}/audit_out"
    printf '  __audit_out=%q\n' "${tmp}/audit_out"
    printf '  dispatch_rc=0\n'
    printf '%s\n' "$block"
    printf '} 2>&1 | tee_collect_statuses\n'
  } > "$driver"
  local out rc=0
  out=$(bash "$driver" 2>/dev/null) || rc=$?
  rm -rf "$tmp"
  printf '%s\n__rc=%s\n' "$out" "$rc"
}

# Run the SAME live scan-and-emit block STANDALONE (no layer) over a synthetic dispatch
# output, returning the folded dispatch_rc — to prove the standalone path agrees with the
# layer path on the exit code (Lead requirement #2).
__run_audit_guard_standalone_rc() {
  local dispatch_stream="$1"
  local audit_sh="${__here}/../../scripts/audit.sh"
  local common="${__here}/_common.sh"
  local block
  block=$(awk '/^dispatch_rc=\$\{PIPESTATUS\[0\]\}$/{f=1;next} /^breaking_rc=0$/{f=0} f' "$audit_sh")
  local tmp; tmp=$(mktemp -d)
  printf '%s\n' "$dispatch_stream" > "${tmp}/audit_out"
  local driver="${tmp}/drv.sh"
  {
    printf '#!/usr/bin/env bash\nset -uo pipefail\nIFS=$'"'"'\\n\\t'"'"'\n'
    printf 'source %q\n' "$common"
    printf '__audit_out=%q\n' "${tmp}/audit_out"
    printf 'dispatch_rc=0\n'
    printf '%s\n' "$block"
    printf 'exit "$dispatch_rc"\n'
  } > "$driver"
  local rc=0
  bash "$driver" >/dev/null 2>&1 || rc=$?
  rm -rf "$tmp"
  printf '%s\n' "$rc"
}

test_audit_security_guard_reds_layer() {
  local r out rc std_rc
  local masked='STATUS=SKIPPED-NO-VERB REASON=rust-audit-UNEXPECTED-verb-missing-or-not-executable
STATUS=OK REASON=ts-audit-passed
STATUS=OK REASON=audit-all-langs-ok'
  # Masking case: UNEXPECTED rust-audit + OK ts → LAYER must exit == 1 (FAIL aggregate,
  # Lead-ruled exit code for this guard) AND a STATUS=FAIL audit-gate-wrapper-missing-rust
  # line must be emitted so tee_collect_statuses reds the layer.
  r=$(__run_audit_guard_through_layer "$masked")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$r" | tail -n1 | cut -d= -f2)
  out=$(sed '/^__rc=[0-9]*$/d' <<<"$r")
  if [[ "$rc" == "1" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-guard:layer-reds] LAYER must exit 1 on masked UNEXPECTED audit, got '${rc}': ${out}"); fi
  assert_pattern_in "audit-guard:emits-fail" "STATUS=FAIL REASON=audit-gate-wrapper-missing-rust" "$out"

  # Both paths agree at exit 1 (Lead ruling, confirmed 2026-06-09 — FAIL/1 on both, NOT a
  # standalone=2/layer=1 divergence). The emitted STATUS=FAIL reds the LAYER at 1; the
  # STANDALONE path folds dispatch_rc=1 to match. Lock the standalone code so it can't
  # drift back to 2 (or up to a synthetic UNKNOWN/2).
  std_rc=$(__run_audit_guard_standalone_rc "$masked")
  if [[ "$std_rc" == "1" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-guard:standalone-rc] standalone audit.sh must exit 1 (FAIL, agrees with layer), got '${std_rc}'"); fi

  # Control: clean all-OK + proto INTENTIONAL gap (proto-audit-sh-missing-, NOT UNEXPECTED)
  # → LAYER stays exit 0, NO audit-gate-wrapper-missing emitted.
  r=$(__run_audit_guard_through_layer \
    'STATUS=OK REASON=cargo-audit-passed
STATUS=OK REASON=pnpm-audit-passed
STATUS=SKIPPED-NO-VERB REASON=proto-audit-sh-missing-or-not-executable
STATUS=OK REASON=audit-all-langs-ok')
  rc=$(grep -oE '__rc=[0-9]+' <<<"$r" | tail -n1 | cut -d= -f2)
  out=$(sed '/^__rc=[0-9]*$/d' <<<"$r")
  if [[ "$rc" == "0" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-guard:intentional-gap-stays-0] proto:audit intentional gap must not red the layer, got rc=${rc}: ${out}"); fi
  if grep -q 'audit-gate-wrapper-missing' <<<"$out"; then
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-guard:no-spurious-fail] intentional gap must NOT emit audit-gate-wrapper-missing: ${out}")
  else
    PASS=$((PASS + 1))
  fi
}

# -----------------------------------------------------------------------------
# Task #50 — reason-keyed verb-missing exit codes.
#
# Run the dispatcher against a synthetic lang tree and capture BOTH the combined
# output and the exit code, so we assert the exact code (== 2 for UNEXPECTED, == 0
# for intentional) not just "non-zero".
# -----------------------------------------------------------------------------

# Run for_each_lang_with_verb against $1=lang_root with extra env in $2 (string of
# VAR=val pairs), verb $3. Echoes output then a trailing __rc= line.
run_dispatch() {
  local lang_root="$1" env_pairs="$2" verb="$3"
  bash -c "
    set +e
    ${env_pairs} DEVLOOP_LANG_ROOT='${lang_root}' bash -c '
      source \"${lang_root}/_dispatch.sh\"
      for_each_lang_with_verb \"${verb}\"
    ' 2>&1
    echo \"__rc=\$?\"
  "
}

# Headline criterion-(b): a SINGLE touched lang, no verb, NOT allowlisted → the
# single-lang return path maps SKIPPED-NO-VERB + UNEXPECTED → exit 2.
test_unexpected_verb_missing_single_lang() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/fakeland"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' > "${tmp}/lang/fakeland/changed.sh"
  chmod +x "${tmp}/lang/fakeland/changed.sh"  # touched, but no test.sh

  local out rc
  out=$(run_dispatch "${tmp}/lang" "" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[unexpected-single:rc] expected 2, got '${rc}': ${out}"); fi
  assert_pattern_in "unexpected-single:reason" \
    "STATUS=SKIPPED-NO-VERB REASON=fakeland-test-UNEXPECTED-verb-missing-or-not-executable" "$out"

  rm -rf "$tmp"; trap - RETURN
}

# All-langs-missing (2 langs, both touched, both missing verb, neither allowlisted) →
# aggregate winner is the UNEXPECTED reason → exit 2.
test_unexpected_verb_missing_all_langs() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/alpha" "${tmp}/lang/beta"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  for l in alpha beta; do
    printf '#!/usr/bin/env bash\nexit 0\n' > "${tmp}/lang/${l}/changed.sh"
    chmod +x "${tmp}/lang/${l}/changed.sh"
  done

  local out rc
  out=$(run_dispatch "${tmp}/lang" "" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[unexpected-all:rc] expected 2, got '${rc}': ${out}"); fi

  rm -rf "$tmp"; trap - RETURN
}

# Intentional gap via the DEVLOOP_TEST-gated seam → exit 0, historical -sh-missing- token,
# and the token must NOT carry the UNEXPECTED anchor (matcher-shape guard).
test_intentional_gap_verb_missing_zero() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/fakeland"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' > "${tmp}/lang/fakeland/changed.sh"
  chmod +x "${tmp}/lang/fakeland/changed.sh"

  local out rc
  out=$(run_dispatch "${tmp}/lang" "DEVLOOP_TEST=1 DEVLOOP_INTENTIONAL_MISSING_VERBS=fakeland:test" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "0" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[intentional-gap:rc] expected 0, got '${rc}': ${out}"); fi
  assert_pattern_in "intentional-gap:reason" \
    "STATUS=SKIPPED-NO-VERB REASON=fakeland-test-sh-missing-or-not-executable" "$out"
  if grep -q 'UNEXPECTED' <<<"$out"; then
    FAIL=$((FAIL + 1)); FAILURES+=("[intentional-gap:no-unexpected] intentional token must not carry UNEXPECTED anchor: ${out}")
  else
    PASS=$((PASS + 1))
  fi

  rm -rf "$tmp"; trap - RETURN
}

# Security gate: the seam is IGNORED without DEVLOOP_TEST=1 → still exit 2 (an
# unconditional env surface would re-open the silent-skip class).
test_intentional_seam_ignored_without_test_sentinel() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/fakeland"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' > "${tmp}/lang/fakeland/changed.sh"
  chmod +x "${tmp}/lang/fakeland/changed.sh"

  local out rc
  out=$(run_dispatch "${tmp}/lang" "DEVLOOP_INTENTIONAL_MISSING_VERBS=fakeland:test" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[seam-gated:rc] expected 2 (seam ignored without DEVLOOP_TEST), got '${rc}': ${out}"); fi

  rm -rf "$tmp"; trap - RETURN
}

# MULTI-ENTRY allowlist split (test-reviewer finding): the allowlist is space-separated
# and __is_intentional_gap splits it with an explicit space-IFS `read -ra` (because
# _common.sh sets IFS=$'\n\t', so a bare split would NOT word-split on spaces). This
# test proves entry 2+ matches, not just the first — two touched langs both missing the
# verb, both in a 2-entry allowlist → BOTH get the intentional `-sh-missing-` token and
# the aggregate exits 0. (Regression guard for the IFS-split bug found during impl.)
test_intentional_gap_multi_entry_allowlist() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/alpha" "${tmp}/lang/beta"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  for l in alpha beta; do
    printf '#!/usr/bin/env bash\nexit 0\n' > "${tmp}/lang/${l}/changed.sh"
    chmod +x "${tmp}/lang/${l}/changed.sh"  # touched, no test.sh
  done

  local out rc
  out=$(run_dispatch "${tmp}/lang" "DEVLOOP_TEST=1 DEVLOOP_INTENTIONAL_MISSING_VERBS='alpha:test beta:test'" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  # BOTH langs must get the intentional token (proving entry-2 `beta:test` split worked).
  assert_pattern_in "multi-allow:alpha" "STATUS=SKIPPED-NO-VERB REASON=alpha-test-sh-missing-or-not-executable" "$out"
  assert_pattern_in "multi-allow:beta"  "STATUS=SKIPPED-NO-VERB REASON=beta-test-sh-missing-or-not-executable" "$out"
  if grep -q 'UNEXPECTED' <<<"$out"; then
    FAIL=$((FAIL + 1)); FAILURES+=("[multi-allow:no-unexpected] no lang should be UNEXPECTED when both are allowlisted: ${out}")
  else
    PASS=$((PASS + 1))
  fi
  if [[ "$rc" == "0" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[multi-allow:rc] expected 0 (both intentional), got '${rc}': ${out}"); fi

  rm -rf "$tmp"; trap - RETURN
}

# Direct __is_intentional_gap unit test against the PRODUCTION 2-entry constant
# `proto:test proto:audit` (test-reviewer: prove the SECOND token — different verb — is
# found, exercising the real prod shape, not just a same-verb pair). This is the most
# targeted guard for the IFS word-split: under a broken split the whole
# "proto:test proto:audit" string lands in allow[0] and matches NEITHER, so BOTH asserts
# below fail loud — the exact mutation the reviewer ran. Uses the production default (no
# DEVLOOP_TEST override), so it also pins the hardcoded constant.
test_is_intentional_gap_prod_constant_both_entries() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  cp "${__here}/_common.sh"   "${tmp}/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/_dispatch.sh"
  # FIRST entry (proto:test) must resolve intentional.
  if bash -c "source '${tmp}/_dispatch.sh'; __is_intentional_gap proto test"; then
    PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[is-gap-prod:proto-test] proto:test (entry 1) must be intentional"); fi
  # SECOND entry (proto:audit — different verb) must ALSO resolve intentional. This is the
  # load-bearing word-split assertion: a broken split fails HERE.
  if bash -c "source '${tmp}/_dispatch.sh'; __is_intentional_gap proto audit"; then
    PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[is-gap-prod:proto-audit] proto:audit (entry 2, different verb) must be intentional — broken IFS word-split would fail this"); fi
  # A non-allowlisted lang:verb must NOT be intentional (control).
  if bash -c "source '${tmp}/_dispatch.sh'; __is_intentional_gap rust audit"; then
    FAIL=$((FAIL + 1)); FAILURES+=("[is-gap-prod:rust-audit] rust:audit must NOT be intentional (not in the prod allowlist)")
  else
    PASS=$((PASS + 1))
  fi
  rm -rf "$tmp"; trap - RETURN
}

# MIXED: one lang in the 2-entry allowlist, one NOT → the allowlisted lang exits-0
# intentional, the other is UNEXPECTED → aggregate exit 2. Proves membership is
# per-entry, not "any entry present disables the check".
test_intentional_gap_multi_entry_mixed() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/alpha" "${tmp}/lang/gamma"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  for l in alpha gamma; do
    printf '#!/usr/bin/env bash\nexit 0\n' > "${tmp}/lang/${l}/changed.sh"
    chmod +x "${tmp}/lang/${l}/changed.sh"
  done

  local out rc
  # allowlist covers alpha:test + beta:test (beta absent); gamma is NOT listed.
  out=$(run_dispatch "${tmp}/lang" "DEVLOOP_TEST=1 DEVLOOP_INTENTIONAL_MISSING_VERBS='alpha:test beta:test'" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  assert_pattern_in "mixed:alpha-intentional" "STATUS=SKIPPED-NO-VERB REASON=alpha-test-sh-missing-or-not-executable" "$out"
  assert_pattern_in "mixed:gamma-unexpected"  "STATUS=SKIPPED-NO-VERB REASON=gamma-test-UNEXPECTED-verb-missing-or-not-executable" "$out"
  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[mixed:rc] expected 2 (gamma UNEXPECTED wins), got '${rc}': ${out}"); fi

  rm -rf "$tmp"; trap - RETURN
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
test_unexpected_verb_missing_single_lang
test_unexpected_verb_missing_all_langs
test_intentional_gap_verb_missing_zero
test_intentional_seam_ignored_without_test_sentinel
test_intentional_gap_multi_entry_allowlist
test_is_intentional_gap_prod_constant_both_entries
test_intentional_gap_multi_entry_mixed
test_audit_security_guard_reds_layer

printf '\n_dispatch.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
