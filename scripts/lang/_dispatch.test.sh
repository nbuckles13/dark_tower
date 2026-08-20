#!/usr/bin/env bash
# _dispatch.test.sh — dispatcher always-run + verb-dispatch + masking tests (test §E).
#
# Hermeticity (test §A + post-confirmation refinement): operates on a copy of
# `_dispatch.sh` and a synthetic lang/ tree in a tempdir. Never mutates the live
# scripts/lang/ tree — a flaky test can't leave fakeland/ behind.
#
# NB (2026-08-20, ADR-0033 §3): the per-lang `changed.sh` classifier was retired, so the
# synthetic lang trees here register ONLY the verb wrapper being exercised — the dispatcher
# always-runs it, with no changed.sh present or consulted.
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
# Test: dispatcher with a working lang emits expected STATUS shape (single-lang)
# -----------------------------------------------------------------------------

test_single_lang_no_double_emit() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/fakelang"
  cp "${__here}/_common.sh" "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  # Synthetic verb that emits its own STATUS (the dispatcher always-runs it — there is no
  # changed.sh gate any more).
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
# Test: stream-verbatim contract + cross-lang-masking CLOSED (task #52).
#
# Pairs no_verb_lang (a verb wrapper that should exist is missing → FAIL-MISSING-VERB,
# rank 5) with ok_lang (a working test.sh → OK, rank 2). Since the dispatcher always-runs
# every registered lang's verb (no changed.sh gate), ok_lang's verb runs and emits OK, and
# the masking assertion is that FAIL-MISSING-VERB beats OK in aggregate_worst_status — so
# the wiring fault wins the aggregate and the dispatcher exits 2, no sibling status can
# mask it.
#
# This test enforces:
#   (a) the dispatcher's aggregated STATUS line is FAIL-MISSING-VERB, and it exits 2,
#   (b) BOTH per-child STATUS lines survive VERBATIM in stdout — not silenced, not
#       aggregated-away (the loud-on-missing-verb streaming invariant),
#   (c) ok_lang RAN its verb (STATUS=OK) — the always-run behavior.
# -----------------------------------------------------------------------------

test_stream_verbatim_masking_closed() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/no_verb_lang" "${tmp}/lang/ok_lang"
  cp "${__here}/_common.sh" "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  # no_verb_lang: no test.sh → the dispatcher must emit FAIL-MISSING-VERB for this lang.

  # ok_lang: a working verb → the dispatcher always-runs it and it emits OK.
  cat > "${tmp}/lang/ok_lang/test.sh" <<'EOF'
#!/usr/bin/env bash
echo "STATUS=OK REASON=ok-lang-ran"
exit 0
EOF
  chmod +x "${tmp}/lang/ok_lang/test.sh"

  local out rc
  out=$(
    set +e
    DEVLOOP_LANG_ROOT="${tmp}/lang" bash -c "
      source '${tmp}/lang/_dispatch.sh'
      for_each_lang_with_verb 'test'
    " 2>&1
    echo "__rc=$?"
  )
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  # (b) per-child FAIL-MISSING-VERB line MUST be in the verbatim stream.
  if grep -q '^STATUS=FAIL-MISSING-VERB.*no_verb_lang' <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] per-child FAIL-MISSING-VERB for no_verb_lang missing from stdout
  output:
${out}
  → ADR-0033 §6 'loud-on-missing-verb' invariant relies on verbatim streaming")
  fi

  # (c) ok_lang's verb RAN (always-run) — its child OK line must be in the verbatim stream.
  if grep -q '^STATUS=OK REASON=ok-lang-ran' <<<"$out"; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] ok_lang did NOT run its verb — the dispatcher should always-run every registered lang: ${out}")
  fi

  # (a) aggregated dispatcher STATUS line is the LAST STATUS= line; FAIL-MISSING-VERB
  # (rank 5) wins over OK (rank 2) — masking closed.
  local last_status
  last_status=$(grep '^STATUS=' <<<"$out" | tail -n1 | sed -n 's/^STATUS=\([^ ]*\).*/\1/p')
  if [[ "$last_status" == "FAIL-MISSING-VERB" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[stream-verbatim] aggregated STATUS expected FAIL-MISSING-VERB (rank 5 beats OK), got '${last_status}': ${out}")
  fi
  # And the dispatcher exits 2 — the wiring fault is no longer masked by the sibling.
  assert_nonzero_exit "stream-verbatim:rc" "$rc"
  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[stream-verbatim:rc] expected 2 (masking closed), got '${rc}': ${out}"); fi
}

# -----------------------------------------------------------------------------
# Test: DEVLOOP_DISPATCH_INCLUDE_LANGS keeps only the named lang.
#
# INCLUDE_LANGS / EXCLUDE_LANGS are Wave 2 #4 additions. layer1.sh uses
# INCLUDE for stage 1 (proto only) and EXCLUDE for stage 2 (rust+ts). Single-
# lang exact match for Wave 2 — multi-lang/comma-split deferred per YAGNI.
# Sentinel-file assertions confirm the filter runs BEFORE any per-lang work
# (observability bonus check): a filtered-out lang's verb must not be invoked
# at all. The sentinel is touched by the VERB (test.sh).
# -----------------------------------------------------------------------------

test_include_langs_keeps() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN

  mkdir -p "${tmp}/lang/kept_lang" "${tmp}/lang/excluded_lang"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"

  for lang in kept_lang excluded_lang; do
    # Sentinel touched by the VERB: its presence proves the verb RAN for the kept lang
    # and its absence proves the filtered-out lang was dropped BEFORE any per-lang work.
    cat > "${tmp}/lang/${lang}/test.sh" <<EOF
#!/usr/bin/env bash
touch "${tmp}/${lang}.sentinel"
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
    FAILURES+=("[include-keeps:kept-verb-ran] kept_lang.sentinel missing — the kept lang's verb was not invoked")
  fi
  if [[ -e "${tmp}/excluded_lang.sentinel" ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[include-keeps:excluded-verb-not-run] excluded_lang.sentinel present — filter did not run before the verb dispatch loop")
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
    # Sentinel touched by the VERB: its presence proves the verb RAN for the kept lang
    # and its absence proves the filtered-out lang was dropped BEFORE any per-lang work.
    cat > "${tmp}/lang/${lang}/test.sh" <<EOF
#!/usr/bin/env bash
touch "${tmp}/${lang}.sentinel"
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
    FAILURES+=("[exclude-drops:kept-verb-ran] kept_lang.sentinel missing")
  fi
  if [[ -e "${tmp}/excluded_lang.sentinel" ]]; then
    FAIL=$((FAIL + 1))
    FAILURES+=("[exclude-drops:excluded-verb-not-run] excluded_lang.sentinel present")
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
  # fakelang has no wrappers — irrelevant here: INCLUDE=nonexistent clears the lang set
  # before any dispatch, so the outcome is SKIPPED-NO-VERB all-langs-filtered.

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
  # all-langs-filtered is OPERATOR INTENT — must stay exit 0. SKIPPED-NO-VERB now has this
  # as its ONLY producer (a genuinely-missing verb is FAIL-MISSING-VERB, exit 2 — task #52);
  # assert the rc explicitly so a future change can't silently flip operator-intent to fail.
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
# Test: aggregate_worst_status precedence — OK-beats-SKIPPED regression test.
#
# Locks the lower portion of the current ladder (full ladder, task #52:
# UNKNOWN > FAIL-MISSING-VERB > FAIL > N/A > OK > SKIPPED-NO-DIFF > SKIPPED-NO-VERB).
# This test pins the OK > SKIPPED-* relationship specifically: a Wave-1 ladder put
# SKIPPED-* above OK, which broke "loud success" once a 2nd lang registered with a
# verb wrapper (rust-clean PR aggregated to SKIPPED-NO-DIFF instead of OK). The upper
# rungs (FAIL-MISSING-VERB, UNKNOWN) are locked in _common.test.sh.
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
# invocation (which would require repo-context + buf install). The end-to-end audit
# fail-closed path — a missing <lang>/audit.sh reds the layer at exit 2 — is covered by
# test_audit_missing_wrapper_reds_layer (drives the real scripts/audit.sh through a layer6
# lifecycle); this test stays focused on the breaking-rc-must-not-mask-dispatch-rc invariant.
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

  # dispatch_rc=2 (FAIL-MISSING-VERB or UNKNOWN — a wiring fault) wins over breaking=FAIL(1):
  # a missing audit wrapper must not be masked by breaking.sh's lower rc. Surfaces loud.
  rc=$(bash -c '
    dispatch_rc=2
    breaking_rc=1
    echo "$(( dispatch_rc > breaking_rc ? dispatch_rc : breaking_rc ))"
  ')
  if [[ "$rc" == "2" ]]; then
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("[audit-fail-closed:wiring-fault-beats-fail] expected 2, got '${rc}'")
  fi
}

# -----------------------------------------------------------------------------
# Test: audit gate fails closed on a missing audit wrapper (criterion f, task #52).
#
# A deleted/chmod-stripped rust|ts audit.sh masked by a sibling's OK would fail-OPEN an
# audit (security) gate — the dep-vuln scan silently never runs while the layer stays
# green. #50 patched the AUDIT slice with a post-processor in scripts/audit.sh; task #52
# closes it GENERALLY at the ladder: the missing wrapper → FAIL-MISSING-VERB (rank 5)
# beats the sibling's OK → the layer reds at exit 2. These tests drive the REAL audit
# dispatch through a real layer6 lifecycle and assert the layer reds with the
# FAIL-MISSING-VERB token, plus a placeholder-gap control that stays exit 0.
# -----------------------------------------------------------------------------

# Successor to #50's test_audit_security_guard_reds_layer (task #52). #50's audit-slice
# post-processor (grep the dispatch output, emit a synthetic STATUS=FAIL) is RETIRED —
# the ladder now closes the masking natively. This drives the REAL audit dispatch (the
# always-run `for_each_lang_with_verb "audit"` that scripts/audit.sh invokes — its
# breaking.sh tail needs buf, so it's covered by the fold-arithmetic test, not run here)
# through a real layer6 lifecycle against a synthetic lang root, and asserts the LAYER
# reds at exit 2 with the FAIL-MISSING-VERB token in the stream (the GENERAL mechanism,
# not an audit-specific patch).
#
# Echoes the layer's combined output + a trailing __rc= (the LAYER exit code).
# $1 = lang_root (synthetic tree).
__run_audit_dispatch_through_layer6() {
  local lang_root="$1"
  local common="${__here}/_common.sh"
  local tmp; tmp=$(mktemp -d)
  local layer="${tmp}/layer6.sh"
  {
    printf '#!/usr/bin/env bash\nset -uo pipefail\nIFS=$'"'"'\\n\\t'"'"'\n'
    printf 'source %q\n' "$common"
    printf 'source %q\n' "${lang_root}/_dispatch.sh"
    printf 'layer_lifecycle_begin 6\n'
    # Mirror layer6.sh: the audit dispatch (dispatcher is unconditionally always-run) piped
    # through tee_collect_statuses at top-level command position (lastpipe) so the layer
    # aggregates the full stream.
    printf 'DEVLOOP_LANG_ROOT=%q for_each_lang_with_verb "audit" 2>&1 | tee_collect_statuses\n' "$lang_root"
  } > "$layer"
  local out rc=0
  out=$(bash "$layer" 2>&1) || rc=$?
  rm -rf "$tmp"
  printf '%s\n__rc=%s\n' "$out" "$rc"
}

test_audit_missing_wrapper_reds_layer() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  cp "${__here}/_common.sh"   "${tmp}/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/_dispatch.sh"

  # Masking case (criterion f): `present` lang has a working audit.sh (OK); `gone` lang
  # is missing its audit.sh (the deleted/chmod-stripped dep-vuln gate). Under always-run,
  # `gone` → FAIL-MISSING-VERB (rank 5) beats present's OK → LAYER reds at exit 2. Dual
  # assert (rc AND token): FAIL-MISSING-VERB→2 and UNKNOWN→2 collide on the bare code, so
  # the token proves the layer redded for the RIGHT cause.
  mkdir -p "${tmp}/present" "${tmp}/gone"
  printf '#!/usr/bin/env bash\necho "STATUS=OK REASON=present-audit-passed"\n' > "${tmp}/present/audit.sh"
  chmod +x "${tmp}/present/audit.sh"
  # gone/ has no audit.sh — the missing (deleted/chmod-stripped) dep-vuln gate.

  local r out rc
  r=$(__run_audit_dispatch_through_layer6 "$tmp")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$r" | tail -n1 | cut -d= -f2)
  out=$(sed '/^__rc=[0-9]*$/d' <<<"$r")
  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-missing:layer-reds] LAYER must exit 2 on a missing audit wrapper masked by a sibling OK, got '${rc}': ${out}"); fi
  assert_pattern_in "audit-missing:result-token" "RESULT=FAIL-MISSING-VERB" "$out"
  assert_pattern_in "audit-missing:child-token"  "STATUS=FAIL-MISSING-VERB REASON=gone-audit-verb-missing-or-not-executable" "$out"

  rm -rf "$tmp"; trap - RETURN
}

# Control: a proto-style INTENTIONAL gap registered via a placeholder audit.sh (emits
# N/A) co-running with a real OK audit → LAYER stays exit 0, aggregate is N/A (the
# placeholder's N/A rank 3 beats OK rank 2), NO FAIL-MISSING-VERB anywhere. Proves the
# placeholder convention keeps intentional gaps green under always-run audit.
test_audit_placeholder_gap_stays_0() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  cp "${__here}/_common.sh"   "${tmp}/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/_dispatch.sh"

  mkdir -p "${tmp}/present" "${tmp}/protolike"
  printf '#!/usr/bin/env bash\necho "STATUS=OK REASON=present-audit-passed"\n' > "${tmp}/present/audit.sh"
  chmod +x "${tmp}/present/audit.sh"
  # Placeholder audit.sh emitting N/A — the canonical intentional-gap registration.
  printf '#!/usr/bin/env bash\necho "STATUS=N/A REASON=not-applicable-to-this-lang"\n' > "${tmp}/protolike/audit.sh"
  chmod +x "${tmp}/protolike/audit.sh"

  local r out rc
  r=$(__run_audit_dispatch_through_layer6 "$tmp")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$r" | tail -n1 | cut -d= -f2)
  out=$(sed '/^__rc=[0-9]*$/d' <<<"$r")
  if [[ "$rc" == "0" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-placeholder:stays-0] intentional placeholder gap must not red the layer, got rc=${rc}: ${out}"); fi
  if grep -q 'FAIL-MISSING-VERB' <<<"$out"; then
    FAIL=$((FAIL + 1)); FAILURES+=("[audit-placeholder:no-missing-verb] placeholder gap must NOT emit FAIL-MISSING-VERB: ${out}")
  else
    PASS=$((PASS + 1))
  fi

  rm -rf "$tmp"; trap - RETURN
}

# -----------------------------------------------------------------------------
# Task #52 — FAIL-MISSING-VERB: missing-verb exit codes + parametric masking-closed.
#
# Run the dispatcher against a synthetic lang tree and capture BOTH the combined
# output and the exit code, so we assert the exact code (== 2 for a missing verb,
# == 0 for an intentional placeholder gap) not just "non-zero".
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

# Headline criterion-(b): a SINGLE touched lang, no verb script → the single-lang return
# path maps FAIL-MISSING-VERB → exit 2.
test_missing_verb_single_lang() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/fakeland"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  # fakeland has no test.sh — the dispatcher always-runs the verb and finds it missing.

  local out rc
  out=$(run_dispatch "${tmp}/lang" "" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[missing-single:rc] expected 2, got '${rc}': ${out}"); fi
  assert_pattern_in "missing-single:reason" \
    "STATUS=FAIL-MISSING-VERB REASON=fakeland-test-verb-missing-or-not-executable" "$out"

  rm -rf "$tmp"; trap - RETURN
}

# All-langs-missing (2 langs, both missing verb) → aggregate winner is
# FAIL-MISSING-VERB → exit 2.
test_missing_verb_all_langs() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/alpha" "${tmp}/lang/beta"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  # Neither alpha nor beta ships test.sh → each FAIL-MISSING-VERB.

  local out rc
  out=$(run_dispatch "${tmp}/lang" "" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[missing-all:rc] expected 2, got '${rc}': ${out}"); fi
  assert_pattern_in "missing-all:agg" "STATUS=FAIL-MISSING-VERB" "$out"

  rm -rf "$tmp"; trap - RETURN
}

# Intentional gap via a PLACEHOLDER <verb>.sh emitting N/A → exit 0, no FAIL-MISSING-VERB.
# This is the canonical task-#52 intentional-gap registration (replaces the #50 allowlist).
test_placeholder_gap_verb_zero() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "${tmp}/lang/fakeland"
  cp "${__here}/_common.sh"   "${tmp}/lang/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/lang/_dispatch.sh"
  # Placeholder verb wrapper — the intentional-gap registration.
  printf '#!/usr/bin/env bash\necho "STATUS=N/A REASON=not-applicable-to-this-lang"\n' > "${tmp}/lang/fakeland/test.sh"
  chmod +x "${tmp}/lang/fakeland/test.sh"

  local out rc
  out=$(run_dispatch "${tmp}/lang" "" "test")
  rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

  if [[ "$rc" == "0" ]]; then PASS=$((PASS + 1)); else
    FAIL=$((FAIL + 1)); FAILURES+=("[placeholder-gap:rc] expected 0, got '${rc}': ${out}"); fi
  assert_pattern_in "placeholder-gap:reason" \
    "STATUS=N/A REASON=not-applicable-to-this-lang" "$out"
  if grep -q 'FAIL-MISSING-VERB' <<<"$out"; then
    FAIL=$((FAIL + 1)); FAILURES+=("[placeholder-gap:no-missing-verb] placeholder must NOT emit FAIL-MISSING-VERB: ${out}")
  else
    PASS=$((PASS + 1))
  fi

  rm -rf "$tmp"; trap - RETURN
}

# PARAMETRIC cross-lang-masking CLOSED across EVERY verb (criterion a). For each of
# compile/fmt/lint/test/audit: an `ok` lang with a working <verb>.sh (emits OK) co-running
# with a `missing` lang that lacks <verb>.sh → aggregate FAIL-MISSING-VERB (rank 5 beats
# OK rank 2) → exit 2. Every verb — including `audit` (the security-critical (f) path) —
# runs under the dispatcher's unconditional always-run behavior (no env knob). Dual assert
# per verb (aggregate token AND rc): FAIL-MISSING-VERB→2 and UNKNOWN→2 collide on the bare
# code, so the token proves it redded for the RIGHT cause.
test_parametric_masking_closed_all_verbs() {
  local tmp; tmp=$(mktemp -d)
  trap "rm -rf '$tmp'" RETURN
  cp "${__here}/_common.sh"   "${tmp}/_common.sh"
  cp "${__here}/_dispatch.sh" "${tmp}/_dispatch.sh"

  # ok lang: a working wrapper for EVERY verb (emits OK).
  # missing lang: NO verb wrappers at all → every verb FAIL-MISSING-VERB.
  mkdir -p "${tmp}/ok" "${tmp}/missing"
  local verb
  for verb in compile fmt lint test audit; do
    printf '#!/usr/bin/env bash\necho "STATUS=OK REASON=ok-%s-passed"\n' "$verb" > "${tmp}/ok/${verb}.sh"
    chmod +x "${tmp}/ok/${verb}.sh"
  done

  local out rc
  for verb in compile fmt lint test audit; do
    # All verbs run always-run (the dispatcher is unconditionally always-run); both langs
    # participate regardless of changed.sh.
    out=$(run_dispatch "${tmp}" "" "${verb}")
    rc=$(grep -oE '__rc=[0-9]+' <<<"$out" | tail -n1 | cut -d= -f2)

    # aggregate token (the LAST STATUS= line) must be FAIL-MISSING-VERB.
    local last_status
    last_status=$(grep '^STATUS=' <<<"$out" | tail -n1 | sed -n 's/^STATUS=\([^ ]*\).*/\1/p')
    if [[ "$last_status" == "FAIL-MISSING-VERB" ]]; then PASS=$((PASS + 1)); else
      FAIL=$((FAIL + 1)); FAILURES+=("[parametric:${verb}:agg] aggregate expected FAIL-MISSING-VERB, got '${last_status}': ${out}"); fi
    # per-child token names the offending lang/verb.
    assert_pattern_in "parametric:${verb}:child" \
      "STATUS=FAIL-MISSING-VERB REASON=missing-${verb}-verb-missing-or-not-executable" "$out"
    # exit 2.
    if [[ "$rc" == "2" ]]; then PASS=$((PASS + 1)); else
      FAIL=$((FAIL + 1)); FAILURES+=("[parametric:${verb}:rc] expected 2, got '${rc}': ${out}"); fi
  done

  rm -rf "$tmp"; trap - RETURN
}

# -----------------------------------------------------------------------------
# Run all
# -----------------------------------------------------------------------------

test_single_lang_no_double_emit
test_stream_verbatim_masking_closed
test_include_langs_keeps
test_exclude_langs_drops
test_filter_empty_after_filter
test_aggregate_precedence_ok_beats_skipped
test_audit_fail_closed_aggregation
test_audit_missing_wrapper_reds_layer
test_audit_placeholder_gap_stays_0
test_missing_verb_single_lang
test_missing_verb_all_langs
test_placeholder_gap_verb_zero
test_parametric_masking_closed_all_verbs

printf '\n_dispatch.test.sh: %d passed, %d failed\n' "$PASS" "$FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf 'Failures:\n'
  for f in "${FAILURES[@]}"; do
    printf '  - %s\n' "$f"
  done
  exit 1
fi
exit 0
