#!/usr/bin/env bash
# selftest-gate2-verdict.sh — isolation self-test for the Gate-2 authority gate
# (task #51). Drives the producer (emit_gate2_verdict) and validator
# (gate2_validate_commit) from scripts/lang/_gate2_binding.sh against SYNTHETIC
# staged trees in throwaway temp git repos, covering the REQUIRED verification
# matrix (a–f) from
# docs/devloop-outputs/2026-06-09-gate2-authority-verdict-gate/main.md.
#
# WHY ISOLATION: this gate's own files (layer-all.sh, .githooks/pre-commit) gate
# the very commit that introduces them. A bug in the validator could wedge that
# commit. Proving the logic green here — against disposable repos, never touching
# the real index or the real hook — is the bootstrapping safety net the design
# mandates. Run this (and require it green) BEFORE relying on the new hook.
#
# Wired into the pipeline (Layer 3 guards via scripts/guards/run-guards.sh) per
# @test so it runs every devloop + in CI. Emits the ADR-0033 STATUS contract line.
#
# Exit: 0 + STATUS=OK on all-pass; 1 + STATUS=FAIL on any failure.

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"   # scripts/guards/simple
__repo_root="$(cd "${__here}/../../.." && pwd)"          # repo root (3 levels up)
__lib="${__repo_root}/scripts/lang/_gate2_binding.sh"

pass=0
fail=0

# ok/bad run INSIDE each case (which runs in a subshell). They print a result and
# set the subshell-local __case_failed marker; the case returns its pass/fail
# status as an exit code, which the parent counts (subshell vars don't propagate).
__case_failed=0
ok()    { printf '  ✅ %s\n' "$*"; }
bad()   { printf '  ❌ %s\n' "$*"; __case_failed=1; }

# Run one matrix case in a fully isolated temp git repo + temp DEVLOOP_TMP.
# The body of each case is a function name passed in; it runs in a SUBSHELL with
# CWD = the temp repo and DEVLOOP_TMP exported, so global state never leaks. The
# subshell's exit code (0 = all asserts in the case passed) is counted here.
with_temp_repo() {
  local case_fn="$1"
  local repo tmp_devloop rc
  repo="$(mktemp -d)"
  tmp_devloop="$(mktemp -d)"
  if (
    cd "$repo"
    export DEVLOOP_TMP="$tmp_devloop"
    git init -q
    git config user.email selftest@example.com
    git config user.name selftest
    git config commit.gpgsign false
    # Source the library AFTER cwd is the temp repo so all git plumbing targets it.
    # shellcheck source=/dev/null
    source "$__lib"
    __case_failed=0
    "$case_fn"
    exit "$__case_failed"
  ); then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
  fi
  rm -rf "$repo" "$tmp_devloop"
}

# Helper (runs inside the temp repo): write a devloop main.md at a given Phase.
mk_main_md() {
  local slug="$1" phase="$2"
  mkdir -p "docs/devloop-outputs/${slug}"
  cat > "docs/devloop-outputs/${slug}/main.md" <<EOF
# Devloop Output: ${slug}

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | \`${phase}\` |
EOF
}

# Helper: an initial commit so HEAD exists and diffs are meaningful.
seed_commit() {
  echo "seed" > .seedfile
  git add .seedfile
  git commit -qm seed
}

# ---------------------------------------------------------------------------
# (a) clean run → PASS verdict over the staged tree → commit proceeds.
# ---------------------------------------------------------------------------
case_a() {
  seed_commit
  mk_main_md "story-a" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  # Producer emits over the worktree (== staged after add -A).
  declare -A ls=( [1]=OK [4]=OK ) ld=( [1]=1 [4]=2 )
  emit_gate2_verdict 0 ls ld
  if gate2_validate_commit; then ok "(a) clean run + matching verdict → allow"; else bad "(a) clean run should ALLOW"; fi
}

# ---------------------------------------------------------------------------
# (b) edit a validated source file AFTER the run → signature mismatch → block,
#     naming the drifted file.
# ---------------------------------------------------------------------------
case_b() {
  seed_commit
  mk_main_md "story-b" "complete"
  echo "original" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  # Now drift: change src.rs and re-stage it (staged blob != recorded blob).
  echo "TAMPERED" > src.rs
  git add src.rs
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "modified after validation: src.rs" <<<"$out"; then
    ok "(b) edited-after-run → block, names drifted file"
  else
    bad "(b) should BLOCK and name src.rs (rc=$rc); output: $out"
  fi
}

# ---------------------------------------------------------------------------
# (c) edit ONLY docs/devloop-outputs/<slug>/main.md after the run → still allow
#     (exclusion works; main.md is not in the binding).
# ---------------------------------------------------------------------------
case_c() {
  seed_commit
  mk_main_md "story-c" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  # Edit ONLY the main.md (an excluded path) and re-stage it.
  printf '\n<!-- post-run human edit -->\n' >> "docs/devloop-outputs/story-c/main.md"
  git add "docs/devloop-outputs/story-c/main.md"
  if gate2_validate_commit; then ok "(c) main.md-only edit (excluded) → allow"; else bad "(c) excluded-path edit should ALLOW"; fi
}

# ---------------------------------------------------------------------------
# (d) a layer fails → GATE2=FAIL written via the trap → block.
# ---------------------------------------------------------------------------
case_d() {
  seed_commit
  mk_main_md "story-d" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  # Simulate a failing pipeline: emit with a nonzero exit code.
  declare -A ls=( [4]=FAIL ) ld=( [4]=2 )
  emit_gate2_verdict 1 ls ld
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "not PASS" <<<"$out"; then
    ok "(d) GATE2=FAIL verdict → block"
  else
    bad "(d) should BLOCK on FAIL verdict (rc=$rc); output: $out"
  fi
}

# ---------------------------------------------------------------------------
# (e) verdict file absent → block.
# ---------------------------------------------------------------------------
case_e() {
  seed_commit
  mk_main_md "story-e" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  # Deliberately do NOT emit a verdict.
  rm -f "$GATE2_VERDICT_FILE"
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "no validation verdict found" <<<"$out"; then
    ok "(e) absent verdict → block"
  else
    bad "(e) should BLOCK when verdict absent (rc=$rc); output: $out"
  fi
}

# ---------------------------------------------------------------------------
# (f) a NON-devloop commit (no staged complete main.md) → hook no-ops.
#     Two sub-cases: no main.md at all, and a main.md NOT at Phase=complete.
# ---------------------------------------------------------------------------
case_f() {
  seed_commit
  # f1: ordinary source-only commit, no devloop main.md staged, no verdict.
  echo "fn main() {}" > src.rs
  git add -A
  rm -f "$GATE2_VERDICT_FILE"
  if gate2_validate_commit; then ok "(f1) non-devloop commit → no-op (allow)"; else bad "(f1) non-devloop commit should ALLOW"; fi

  # f2: a devloop main.md staged but at Phase=implementation (not complete) →
  #     trigger conjunct-1 false → no-op even with validated files + no verdict.
  mk_main_md "story-f2" "implementation"
  echo "more" > src2.rs
  git add -A
  rm -f "$GATE2_VERDICT_FILE"
  if gate2_validate_commit; then ok "(f2) main.md not-complete → no-op (allow)"; else bad "(f2) non-complete phase should ALLOW"; fi
}

# ---------------------------------------------------------------------------
# (g) EXTRA: stale verdict from a DIFFERENT devloop (slug mismatch) → block.
#     Guards the slug-as-diagnostic path.
# ---------------------------------------------------------------------------
case_g() {
  seed_commit
  mk_main_md "story-g" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  # Rewrite SLUG in the verdict to a different devloop, keeping the signature
  # valid (simulates a leftover verdict from another devloop in the same session).
  sed -i 's/^SLUG=.*/SLUG=some-other-devloop/' "$GATE2_VERDICT_FILE"
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "different devloop" <<<"$out"; then
    ok "(g) slug-mismatch (stale verdict) → block"
  else
    bad "(g) should BLOCK on slug mismatch (rc=$rc); output: $out"
  fi
}

# ---------------------------------------------------------------------------
# (h) EXTRA: a NEW staged file the verdict never saw → "staged but not in
#     verdict" drift → block. Guards the staged∖recorded bucket.
# ---------------------------------------------------------------------------
case_h() {
  seed_commit
  mk_main_md "story-h" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  # Stage an extra validated file the verdict didn't bind.
  echo "extra" > extra.rs
  git add extra.rs
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "staged but not in verdict: extra.rs" <<<"$out"; then
    ok "(h) new staged file not in verdict → block, names it"
  else
    bad "(h) should BLOCK and name extra.rs (rc=$rc); output: $out"
  fi
}

# ---------------------------------------------------------------------------
# (i) EXTRA: the producer stamps the CORRECT slug end-to-end (regression guard
#     for the bug where slug was derived from the POST-exclusion changeset —
#     which always drops the excluded docs/devloop-outputs/** main.md → empty
#     slug). Asserts SLUG == the staged devloop slug AND that validation passes
#     via the positive slug-match (recorded slug equals staged), not merely the
#     empty-slug signature-only fallback.
# ---------------------------------------------------------------------------
case_i() {
  seed_commit
  mk_main_md "story-i" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  local recorded_slug
  recorded_slug="$(gate2_verdict_get SLUG "$GATE2_VERDICT_FILE")"
  if [[ "$recorded_slug" == "story-i" ]] && gate2_validate_commit; then
    ok "(i) producer stamps correct slug (story-i) + validates"
  else
    bad "(i) expected SLUG=story-i and allow; got SLUG='$recorded_slug'"
  fi
}

# ---------------------------------------------------------------------------
# (j) EXCLUSION-PREDICATE FAITHFULNESS — the single highest-risk spot
#     (operations/security). Asserts gate2_is_excluded matches the Rust precedent
#     (cross_boundary_scope.rs::is_symmetric_exclusion) EXACTLY: the single-segment
#     entries must NOT over-exclude nested paths (the bash `case */` -matches-`/`
#     hazard). Pure-function test — no git needed, but kept in the isolated runner
#     for one place to assert the gate's correctness boundary.
#       MUST stay BOUND (not excluded): a validated file escaping here is a gate
#       correctness bug.
#       MUST be EXCLUDED: faithful narrowness still exempts the real bookkeeping.
# ---------------------------------------------------------------------------
case_j() {
  local p
  # MUST stay BOUND (gate2_is_excluded → false / nonzero). This direction catches a
  # predicate that excludes too MUCH — a validated source escaping the gate. Covers
  # the three smuggling paths + ordinary source.
  for p in \
    "docs/user-stories/sub/bar.md" \
    "docs/user-stories/notes.txt" \
    "docs/specialist-knowledge/a/b/INDEX.md" \
    "crates/foo/src/INDEX.md" \
    "crates/foo/src/lib.rs" \
    "docs/TODOxmd" \
    "crates/x/docs/TODO.md" \
    "scripts/layer-all.sh" \
    ".githooks/pre-commit" ; do
    if gate2_is_excluded "$p"; then bad "(j) MUST-BE-BOUND path was excluded: $p"; fi
  done
  # MUST be EXCLUDED (gate2_is_excluded → true / zero). This direction catches a
  # predicate that excludes too LITTLE (or nothing — a vacuously-open gate). Covers
  # each legit bookkeeping family + the by-design any-depth devloop-outputs/** widening.
  for p in \
    "docs/devloop-outputs/2026-06-09-x/main.md" \
    "docs/devloop-outputs/x/sub/attachment.md" \
    "docs/devloop-outputs/x/sub/attachment.bin" \
    "docs/TODO.md" \
    "docs/specialist-knowledge/x/INDEX.md" \
    "docs/user-stories/foo.md" ; do
    if ! gate2_is_excluded "$p"; then bad "(j) MUST-BE-EXCLUDED path was bound: $p"; fi
  done
  [[ "$__case_failed" -eq 0 ]] && ok "(j) exclusion predicate faithful to is_symmetric_exclusion (no case-glob over-exclusion)"
}

# ---------------------------------------------------------------------------
# (k) FAIL-CLOSED on AMBIGUITY: >1 staged devloop main.md at Phase=complete →
#     the hook BLOCKS (the Rust `bail!` analogue in the devloop-commit context).
#     Confirms gate2_staged_complete_slug returns 2 and the validator hard-blocks
#     rather than silently picking one. (The PRODUCER's pure slug-derive must NOT
#     abort on >1 — that asymmetry is tested implicitly by case_a/i never aborting.)
# ---------------------------------------------------------------------------
case_k() {
  seed_commit
  mk_main_md "story-k1" "complete"
  mk_main_md "story-k2" "complete"
  echo "fn main() {}" > src.rs
  git add -A
  # A verdict exists and is otherwise valid for the staged tree — proving the
  # block is due to AMBIGUITY, not a missing/failing verdict.
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "more than one staged devloop main.md" <<<"$out"; then
    ok "(k) >1 complete main.md → fail-closed block"
  else
    bad "(k) should BLOCK on ambiguous >1 complete main.md (rc=$rc); output: $out"
  fi
}

# ---------------------------------------------------------------------------
# (l) DELETION IS BOUND (security HIGH integrity-bypass regression guard).
#     A staged deletion of a validated file must change the signature → BLOCK.
#     Pre-fix: the deleted path's blob-resolution failure produced a matching
#     garbage/truncated record on both sides → false PASS. Post-fix: the deletion
#     is a first-class DELETED record, so deleting a bound file is detected.
# ---------------------------------------------------------------------------
case_l() {
  seed_commit
  echo "doomed">gone.rs                 # commit gone.rs so it is a TRACKED file at HEAD
  git add gone.rs; git commit -qm "add gone.rs"
  echo "keep">keep.rs; git add keep.rs   # a present, validated file
  git rm -q gone.rs                       # stage a deletion of a tracked file
  # ASSERTABLE INVARIANT (RED pre-fix, GREEN post-fix): a staged deletion must
  # produce a CLEAN, stable, identical-both-sides record "DELETED <path>" — NOT a
  # garbage record (pre-fix: `git rev-parse :gone.rs` echoed ":gone.rs", and
  # `git hash-object` on the missing worktree path emitted an EMPTY blob field, so
  # producer and hook recorded DIFFERENT garbage). A black-box pass/block check
  # can't distinguish (both garbage-mismatch and clean-DELETED happen to BLOCK
  # here); the RECORD CONTENT is the load-bearing difference, so assert it directly.
  local staged_recs
  staged_recs="$(gate2_records_staged | tr '\0' '\n')"
  if grep -qx "DELETED gone.rs" <<<"$staged_recs" \
     && ! grep -q ":gone.rs" <<<"$staged_recs" \
     && ! grep -qE '^ gone\.rs$' <<<"$staged_recs"; then
    ok "(l) staged deletion → clean 'DELETED gone.rs' record (no garbage; deletion bound)"
  else
    bad "(l) deletion must yield a clean DELETED record, got: $(tr '\n' '|' <<<"$staged_recs")"
  fi
}

# ---------------------------------------------------------------------------
# (m) DELETION + POST-EMIT TAMPER → BLOCK (the actual exploit security found).
#     A commit that deletes one file AND tampers another after validation must be
#     blocked. Pre-fix: the deletion truncated/corrupted the record stream so the
#     tampered file rode along → false PASS. Post-fix: the tamper is caught and named.
# ---------------------------------------------------------------------------
case_m() {
  seed_commit
  echo "doomed">gone.rs                 # commit gone.rs so `git rm` is a clean tracked-file delete
  git add gone.rs; git commit -qm "add gone.rs"
  mk_main_md "story-m" "complete"
  echo "original">keep.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  # Exploit attempt: delete gone.rs AND tamper keep.rs, both staged, after emit.
  git rm -q gone.rs
  echo "TAMPERED">keep.rs
  git add keep.rs
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "modified after validation: keep.rs" <<<"$out"; then
    ok "(m) deletion + post-emit tamper → block, names the tampered file"
  else
    bad "(m) deletion must NOT mask a post-emit tamper (rc=$rc); out: $out"
  fi
}

# ---------------------------------------------------------------------------
# (n) SOURCE-SAFETY containment is INTRINSIC, not call-site-incidental
#     (operations + code-reviewer). The strict fns open with `local -; set -euo
#     pipefail`; `local -` auto-restores shell options on return REGARDLESS of
#     whether the caller used a subshell. Prove it: from a `set +u` context, call a
#     strict fn DIRECTLY (not in $()), then assert -u is back OFF after it returns.
#     Pins the `local -` behavior so a future removal (reverting to a bare `set -u`)
#     regresses loudly instead of silently leaking options into the pre-commit hook.
# ---------------------------------------------------------------------------
case_n() {
  seed_commit
  echo "x">a.rs; git add a.rs
  set +u                       # caller context: nounset OFF
  # Direct (non-subshell) call of a strict fn. gate2_changeset_staged opens with
  # `local -; set -euo pipefail`; if `local -` works, -u reverts to OFF on return.
  gate2_changeset_staged >/dev/null
  case "$-" in
    *u*) bad "(n) local - leaked: -u still ON after a direct strict-fn call" ;;
    *)   ok  "(n) source-safety intrinsic: -u restored OFF after direct strict-fn call" ;;
  esac
  set -u                       # restore the harness's strict mode
}

# ---------------------------------------------------------------------------
# (o) ADVERSARIAL ORDERING — the EXACT shape of the security HIGH bypass
#     (signature-void via deleted-sorts-first). A devloop commit deletes a file
#     whose path sorts BEFORE a tampered file (delete `aaa_del.rs`, tamper
#     `zzz_keep.rs`). Pre-fix, the deleted path's blob-resolution failure
#     truncated/garbled the LC_ALL=C-sorted record stream at its HEAD, emptying
#     the post-deletion tail on BOTH sides to the same hash → producer and hook
#     matched → rc=0 ALLOW even though zzz_keep.rs was tampered post-validation.
#
#     WHY THIS IS NOT A DUPLICATE OF case_m (test-lens finding, addressed):
#     a black-box BLOCK+names check is NOT sensitive to the mechanism — the
#     tampered file's record differs producer-vs-hook on its OWN, so the gate
#     blocks for the tamper regardless of the deletion or its sort position
#     (case_m already covers that; reversing the order here blocks identically).
#     To actually pin the deleted-sorts-FIRST bypass we assert the RECORD STREAM
#     directly (the load-bearing artifact, like case_l): the deletion record
#     sorts to the HEAD and must NOT truncate the tail — the tampered file's
#     record must SURVIVE after it, carrying the *staged* (TAMPERED) blob. This
#     assertion goes RED if the DELETED-record / fail-closed-blob mechanism is
#     reverted; the black-box BLOCK alone does not.
# ---------------------------------------------------------------------------
case_o() {
  seed_commit
  echo "doomed">aaa_del.rs                 # tracked at HEAD so `git rm` is a clean delete
  git add aaa_del.rs; git commit -qm "add aaa_del.rs"
  mk_main_md "story-o" "complete"
  echo "original">zzz_keep.rs              # a present, validated file (sorts AFTER aaa_del.rs)
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  # Exploit attempt: delete aaa_del.rs (sorts FIRST) AND tamper zzz_keep.rs, both
  # staged after emit. The deleted-sorts-first order is the load-bearing detail —
  # it is what truncated the pre-fix record stream head, emptying the tail.
  git rm -q aaa_del.rs
  echo "TAMPERED">zzz_keep.rs
  git add zzz_keep.rs

  # --- MECHANISM ASSERTION (ordering-sensitive; RED if DELETED-record reverted) -
  # The staged record stream must contain EXACTLY two intact, well-formed records:
  # a clean "DELETED aaa_del.rs" (deletion bound as a first-class record) AND the
  # tampered zzz_keep.rs carrying its *staged* (TAMPERED) blob. If the head
  # deletion truncated/garbled the stream (the bypass shape), one of these would
  # be missing or malformed. Compare as a SET (sort the two expected lines the
  # same way) so the assertion pins record CONTENT/INTEGRITY, not the incidental
  # token-vs-token sort order of "DELETED" against a hex OID.
  local got_recs tampered_blob expected_recs
  got_recs="$(gate2_records_staged | tr '\0' '\n' | LC_ALL=C sort)"
  tampered_blob="$(git rev-parse ":zzz_keep.rs")"   # the staged (TAMPERED) blob OID
  expected_recs="$(printf 'DELETED aaa_del.rs\n%s zzz_keep.rs\n' "$tampered_blob" | LC_ALL=C sort)"
  if [[ "$got_recs" != "$expected_recs" ]]; then
    bad "(o) record stream truncated/garbled by head deletion; got: $(tr '\n' '|' <<<"$got_recs")"
  fi

  # --- BLACK-BOX ASSERTION (end-to-end: the gate actually blocks + names it) ----
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -ne 0 ]] && grep -q "modified after validation: zzz_keep.rs" <<<"$out"; then
    [[ "$__case_failed" -eq 0 ]] && ok "(o) deleted-sorts-first: tail survives deletion → block, names tampered file"
  else
    bad "(o) deleted-sorts-first must NOT void the signature for a post-emit tamper (rc=$rc); out: $out"
  fi
}

printf 'gate2 isolation self-test (matrix a–f + slug/extra-file/exclusion/ambiguity/deletion/adversarial-ordering/source-safety guards):\n'
for c in case_a case_b case_c case_d case_e case_f case_g case_h case_i case_j case_k case_l case_m case_n case_o; do
  with_temp_repo "$c" || true
done

printf '\n%d case(s) passed, %d failed\n' "$pass" "$fail"
if [[ "$fail" -eq 0 ]]; then
  printf 'STATUS=OK REASON=gate2-selftest-passed\n'
  exit 0
else
  printf 'STATUS=FAIL REASON=gate2-selftest-failed\n'
  exit 1
fi
