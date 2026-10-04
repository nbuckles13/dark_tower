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
    # Hermetic git: no user/system config (core.hooksPath, rerere, rebase settings) and
    # no interactive editor, so a replay case can neither leak config nor hang.
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_EDITOR=true GIT_SEQUENCE_EDITOR=true
    git -c init.defaultBranch=main init -q
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
#     Confirms gate2_staged_trigger_slug returns 2 and the validator hard-blocks
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

# ---------------------------------------------------------------------------
# (p) REMEDIATION ORDERING — the drift message must instruct VALIDATE-then-STAGE
#     (`./scripts/layer-all.sh && git add -A`), never the reverse. This pins the
#     _gate2_binding.sh:727 reorder made this devloop (ADR-0037 §D7): emit signs the
#     WORKTREE, the hook compares the staged INDEX, and under D7 the fmt lane
#     reformats mid-run — so `git add -A && layer-all` stages pre-format content and
#     signs post-format, reproducing the mismatch and inducing `--no-verify`
#     (@operations flagged the reorder as the load-bearing anti-inducement). A revert
#     to the wrong order flips BOTH assertions. Reuses case_b's mismatch trigger.
# ---------------------------------------------------------------------------
case_p() {
  seed_commit
  mk_main_md "story-p" "complete"
  echo "original" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  echo "TAMPERED" > src.rs
  git add src.rs
  local out rc
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
  if [[ "$rc" -eq 0 ]]; then
    bad "(p) precondition: expected a signature-mismatch BLOCK to reach the remediation message (rc=0)"
    return
  fi
  if grep -qF './scripts/layer-all.sh && git add -A' <<<"$out"; then
    ok "(p) remediation instructs VALIDATE-then-stage (layer-all && git add)"
  else
    bad "(p) remediation missing the validate-then-stage form './scripts/layer-all.sh && git add -A'; output: $out"
  fi
  if grep -qF 'git add -A && ./scripts/layer-all.sh' <<<"$out"; then
    bad "(p) remediation instructs the WRONG stage-then-validate order (:727 reorder reverted) — under D7 this signs a tree the index doesn't hold and induces --no-verify"
  else
    ok "(p) remediation does NOT instruct the reverse stage-then-validate order"
  fi
}

# ===========================================================================
# REPLAY SKIP (devloop 2026-10-03-gate2-replay-skip; runbook §8.5 "Replays").
# Every case below uses REAL replay state (a conflicted cherry-pick / rebase / merge)
# unless the case is about forging that state, asserts the state exists before calling
# the validator (setup failures are their own message, so a conflict that never
# happened can't pass a case vacuously), and has NO verdict file — so a positive case
# fails if the replay check is removed, and a negative case asserts its own refusal
# token (rc=1 alone is the no-verdict baseline).
# ===========================================================================

__real_git="$(command -v git)"

# commit_msg <message> — commit the index with an exact message (verbatim).
commit_msg() {
  printf '%s\n' "$1" > "$DEVLOOP_TMP/msg"
  git commit -q --cleanup=verbatim -F "$DEVLOOP_TMP/msg"
}

# devloop_msg <slug> — the standard devloop commit message (devloop SKILL.md Step 8).
devloop_msg() {
  printf 'devloop work\n\nDevloop: %s\nSpecialist: infrastructure\nMode: full' "$1"
}

# replay_fixture <slug> <message> — base commit with src.rs; branch `devloop` adds the
# complete main.md + changes src.rs in ONE commit carrying <message>; `main` changes
# src.rs differently, so replaying `devloop` onto `main` conflicts on src.rs only.
replay_fixture() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md "$1" "complete"
  echo devloop > src.rs
  git add -A; commit_msg "$2"
  git checkout -q main
  echo target > src.rs; git commit -qam target
}

# conflicted_pick <rev> — start a cherry-pick that MUST stop on a conflict, then
# hand-resolve src.rs and stage it. Returns 1 (after `bad`) if the state is not real.
conflicted_pick() {
  if git cherry-pick "$1" >/dev/null 2>&1; then
    bad "setup: cherry-pick of $1 did not conflict"; return 1
  fi
  if ! git rev-parse -q --verify CHERRY_PICK_HEAD >/dev/null; then
    bad "setup: CHERRY_PICK_HEAD absent after a conflicted cherry-pick"; return 1
  fi
  echo hand-resolved > src.rs; git add src.rs
}

# run_validate — gate2_validate_commit with stderr captured into $out, rc into $rc.
run_validate() {
  out="$(gate2_validate_commit 2>&1)" && rc=0 || rc=$?
}

# no_skip_line — assert the skip did NOT fire.
no_skip_line() {
  if grep -q 'Gate-2: replay skip: ' <<<"$out"; then bad "$1: skip line printed but must not be: $out"; fi
}

# expect_refusal <case> <token> — block, the token is printed, no skip line.
expect_refusal() {
  if [[ "$rc" -ne 0 ]] && grep -qF "[$2]" <<<"$out"; then
    no_skip_line "$1"
    [[ "$__case_failed" -eq 0 ]] && ok "$1 → block, [$2]"
  else
    bad "$1 should BLOCK with [$2] (rc=$rc); output: $out"
  fi
}

# with_git_shim <match> [<cmd>] [<not>] -- run_validate with a `git` on PATH that fails
# (rc 97 with its own stderr) for any invocation having an argument matching glob
# <match> (and, if given, whose first argument is <cmd>, and which has no argument
# equal to <not>); everything else reaches real git.
with_git_shim() {
  local shim="$DEVLOOP_TMP/shim"
  mkdir -p "$shim"
  cat > "$shim/git" <<'EOF'
#!/usr/bin/env bash
if [[ -n "${SHIM_NOT:-}" ]]; then
  for a in "$@"; do [[ "$a" == "$SHIM_NOT" ]] && exec "$REAL_GIT" "$@"; done
fi
if [[ -z "${SHIM_CMD:-}" || "${1:-}" == "$SHIM_CMD" ]]; then
  for a in "$@"; do
    if [[ "$a" == $SHIM_MATCH ]]; then
      echo "shim: injected git failure for $a" >&2
      exit 97
    fi
  done
fi
exec "$REAL_GIT" "$@"
EOF
  chmod +x "$shim/git"
  out="$(PATH="$shim:$PATH" REAL_GIT="$__real_git" SHIM_MATCH="$1" SHIM_CMD="${2:-}" SHIM_NOT="${3:-}" gate2_validate_commit 2>&1)" && rc=0 || rc=$?
}

# ---------------------------------------------------------------------------
# (q1) cherry-pick, ALLOW DESPITE A HAND-EDITED RESOLUTION. Pins the user's
#      whole-commit decision: src.rs was resolved by hand (bytes nobody validated),
#      yet no verdict is required — and the skip line says so, with N/M counts derived
#      from the fixture itself.
# ---------------------------------------------------------------------------
case_q1() {
  replay_fixture story-q1 "$(devloop_msg story-q1)"
  conflicted_pick devloop || return
  local path="docs/devloop-outputs/story-q1/main.md" sha blob n m
  sha="$(git rev-parse devloop)"; blob="$(git rev-parse ":$path")"
  # Expected N/M from the fixture with plain git — NOT via gate2_changeset_staged, the
  # function the code under test counts with (a bug there would move both sides).
  local -a bound=()
  mapfile -t bound < <(git diff --cached --name-only HEAD | grep -v '^docs/devloop-outputs/')
  n="${#bound[@]}"
  m="$(git diff --cached --name-only "$sha" -- "${bound[@]}" | wc -l)"
  if [[ "$n" -ne 1 ]]; then bad "(q1) precondition: fixture should stage exactly one bound path (src.rs), got $n"; fi
  run_validate
  if [[ "$rc" -ne 0 ]]; then bad "(q1) cherry-pick replay should ALLOW (rc=$rc); output: $out"; return; fi
  if [[ "$m" -lt 1 ]]; then bad "(q1) precondition: the resolution must differ from the replayed commit (M=$m)"; fi
  if grep -qF "Gate-2: replay skip: $path blob $blob == CHERRY_PICK_HEAD $sha (slug story-q1); Gate-2 verdict NOT checked for this commit — $n bound path(s) not validated locally ($m differ from the replayed commit)" <<<"$out"; then
    [[ "$__case_failed" -eq 0 ]] && ok "(q1) conflicted cherry-pick, hand-edited resolution → allow; skip line names path/blob/ref/sha/slug, N=$n M=$m"
  else
    bad "(q1) skip line missing or wrong; output: $out"
  fi
}

# (q2) rebase stopped on the devloop commit, `git commit` at the stop → allow.
case_q2() {
  replay_fixture story-q2 "$(devloop_msg story-q2)"
  local sha; sha="$(git rev-parse devloop)"
  git checkout -q devloop
  if git rebase main >/dev/null 2>&1; then bad "setup: rebase did not conflict"; return; fi
  if [[ "$(git rev-parse -q --verify REBASE_HEAD)" != "$sha" ]]; then bad "setup: REBASE_HEAD is not the devloop commit"; return; fi
  echo hand-resolved > src.rs; git add src.rs
  run_validate
  if [[ "$rc" -eq 0 ]] && grep -qF "== REBASE_HEAD $sha (slug story-q2)" <<<"$out"; then
    ok "(q2) conflicted rebase stopped on the devloop commit → allow, skip line names REBASE_HEAD"
  else
    bad "(q2) rebase replay should ALLOW with a REBASE_HEAD skip line (rc=$rc); output: $out"
  fi
}

# (q3) linked worktree (`.git` is a FILE; per-worktree pseudorefs) → allow.
case_q3() {
  replay_fixture story-q3 "$(devloop_msg story-q3)"
  git worktree add -q -b wt "$DEVLOOP_TMP/wt" main
  cd "$DEVLOOP_TMP/wt"
  if [[ ! -f .git ]]; then bad "setup: linked worktree .git is not a file"; return; fi
  conflicted_pick devloop || return
  if [[ -e "$(git rev-parse --git-common-dir)/CHERRY_PICK_HEAD" ]]; then bad "setup: CHERRY_PICK_HEAD landed in the common dir, not per-worktree"; return; fi
  run_validate
  if [[ "$rc" -eq 0 ]] && grep -qF "== CHERRY_PICK_HEAD $(git rev-parse devloop) (slug story-q3)" <<<"$out"; then
    ok "(q3) cherry-pick in a linked worktree → allow (per-worktree pseudoref resolved)"
  else
    bad "(q3) linked-worktree replay should ALLOW (rc=$rc); output: $out"
  fi
}

# __install_hook_shim — a minimal pre-commit hook that sources the library and calls
# the real entry point (the repo hook also runs cargo, irrelevant here).
__install_hook_shim() {
  mkdir -p "$DEVLOOP_TMP/hooks"
  printf '#!/usr/bin/env bash\nsource %q\ngate2_validate_commit\n' "$__lib" > "$DEVLOOP_TMP/hooks/pre-commit"
  chmod +x "$DEVLOOP_TMP/hooks/pre-commit"
  git config core.hooksPath "$DEVLOOP_TMP/hooks"
}

# (q4) END-TO-END: `GIT_EDITOR=true git cherry-pick --continue` (absorb-devloop.sh's
#      exact call) runs the hook with CHERRY_PICK_HEAD set, and the skip lets it land.
case_q4() {
  replay_fixture story-q4 "$(devloop_msg story-q4)"
  __install_hook_shim
  conflicted_pick devloop || return
  local o r=0
  o="$(GIT_EDITOR=true git cherry-pick --continue 2>&1)" || r=$?
  if [[ "$r" -eq 0 ]] && ! git rev-parse -q --verify CHERRY_PICK_HEAD >/dev/null \
     && [[ "$(git log -1 --format=%s)" == "devloop work" ]] && grep -q 'Gate-2: replay skip: ' <<<"$o"; then
    ok "(q4) e2e cherry-pick --continue: hook ran, skip fired, commit landed"
  else
    bad "(q4) e2e cherry-pick --continue should land via the skip (rc=$r); output: $o"
  fi
}

# (q5) END-TO-END negative twin: same, main.md edited → the hook refuses; the pick
#      stays paused and HEAD does not move.
case_q5() {
  replay_fixture story-q5 "$(devloop_msg story-q5)"
  __install_hook_shim
  conflicted_pick devloop || return
  printf '\nedited during resolution\n' >> docs/devloop-outputs/story-q5/main.md
  git add docs/devloop-outputs/story-q5/main.md
  local before o r=0; before="$(git rev-parse HEAD)"
  o="$(GIT_EDITOR=true git cherry-pick --continue 2>&1)" || r=$?
  if [[ "$r" -ne 0 ]] && git rev-parse -q --verify CHERRY_PICK_HEAD >/dev/null \
     && [[ "$(git rev-parse HEAD)" == "$before" ]] && grep -qF '[main-md-modified]' <<<"$o"; then
    ok "(q5) e2e cherry-pick --continue with edited main.md → refused, pick still paused, HEAD unchanged"
  else
    bad "(q5) e2e refusal expected (rc=$r); output: $o"
  fi
}

# (q7) the replayed commit did NOT itself change main.md (an earlier commit wrote it);
#      main.md hand-staged identical to its tree → [not-written-by-replayed-commit].
case_q7() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md story-q7 complete; git add -A; commit_msg "$(devloop_msg story-q7)"
  echo devloop > src.rs; git add -A; commit_msg "$(devloop_msg story-q7)"
  git checkout -q main
  echo target > src.rs; git commit -qam target
  conflicted_pick devloop || return
  git checkout devloop -- docs/devloop-outputs/story-q7/main.md
  run_validate
  # Restoring main.md can't fix this: the action is validate-or-bypass.
  if grep -q 'git checkout' <<<"$out" || ! grep -q 'Not fixable by restoring main.md' <<<"$out"; then
    bad "(q7) not-written must not advise git checkout, and must give the validate/--no-verify action; output: $out"
  fi
  expect_refusal "(q7) replayed commit didn't write main.md" not-written-by-replayed-commit
}

# (q8) absorbed commit replayed again: the commit being picked is itself a
#      `cherry-pick -x` copy (message ends in a "(cherry picked from …)" paragraph) →
#      allow; the message is irrelevant.
case_q8() {
  replay_fixture story-q8 "$(printf '%s\n\n(cherry picked from commit %s)' "$(devloop_msg story-q8)" 0123456789abcdef0123456789abcdef01234567)"
  conflicted_pick devloop || return
  run_validate
  if [[ "$rc" -eq 0 ]] && grep -qF '(slug story-q8)' <<<"$out"; then
    ok "(q8) absorbed (-x) commit replayed → allow"
  else
    bad "(q8) should ALLOW (rc=$rc); output: $out"
  fi
}

# (q9) the replayed commit is a ROOT commit (no parent) that adds main.md → refused:
#      no legitimate devloop commit is a root.
case_q9() {
  seed_commit
  echo target > src.rs; git add src.rs; git commit -qm target
  git checkout -q --orphan devloop; git rm -rq --cached . >/dev/null; rm -rf docs src.rs
  mk_main_md story-q9 complete; echo devloop > src.rs
  git add -A; commit_msg "$(devloop_msg story-q9)"
  git checkout -q main
  conflicted_pick devloop || return
  run_validate; expect_refusal "(q9) root replayed commit" root-commit-not-vouched
}

# (r1) main.md edited during resolution → block; the ⚠️ reason line directly precedes
#      the ❌ block (cause, then consequence).
case_r1() {
  replay_fixture story-r1 "$(devloop_msg story-r1)"
  conflicted_pick devloop || return
  printf '\nedited\n' >> docs/devloop-outputs/story-r1/main.md
  git add docs/devloop-outputs/story-r1/main.md
  run_validate
  local why blk
  why="$(grep -n '\[main-md-modified\]' <<<"$out" | head -1 | cut -d: -f1)"
  blk="$(grep -n '❌ Gate-2: no validation verdict found' <<<"$out" | head -1 | cut -d: -f1)"
  if [[ -z "$why" || -z "$blk" || "$why" -ge "$blk" ]]; then
    bad "(r1) the [main-md-modified] line must precede the ❌ block; output: $out"
  fi
  if ! grep -qF 'git checkout CHERRY_PICK_HEAD -- docs/devloop-outputs/story-r1/main.md' <<<"$out"; then
    bad "(r1) the restore action must name the real path; output: $out"
  fi
  expect_refusal "(r1) edited main.md" main-md-modified
}

# (r2) the replayed commit's MESSAGE names no devloop → still allow: the evidence is
#      that the commit wrote the record, not what its message says.
case_r2() {
  replay_fixture story-r2 "devloop work without a trailer"
  conflicted_pick devloop || return
  run_validate
  if [[ "$rc" -eq 0 ]] && grep -qF '(slug story-r2)' <<<"$out"; then
    ok "(r2) message without a Devloop: line, commit wrote main.md → allow"
  else
    bad "(r2) should ALLOW (rc=$rc); output: $out"
  fi
}

# (r5) no replay in progress → today's block, and NO replay line of either kind.
case_r5() {
  seed_commit
  mk_main_md story-r5 complete
  echo "fn main() {}" > src.rs
  git add -A
  run_validate
  if [[ "$rc" -ne 0 ]] && grep -q 'no validation verdict found' <<<"$out" && ! grep -q 'replay skip' <<<"$out"; then
    ok "(r5) no replay state → today's block, no replay line"
  else
    bad "(r5) expected a silent-replay block (rc=$rc); output: $out"
  fi
}

# (r6) conflicted MERGE that would otherwise qualify (main.md identical to MERGE_HEAD's
#      tree, MERGE_HEAD's tip wrote it) → refused by the merge rule.
case_r6() {
  replay_fixture story-r6 "$(devloop_msg story-r6)"
  if git merge devloop >/dev/null 2>&1; then bad "setup: merge did not conflict"; return; fi
  if ! git rev-parse -q --verify MERGE_HEAD >/dev/null; then bad "setup: MERGE_HEAD absent"; return; fi
  echo hand-resolved > src.rs; git add src.rs
  run_validate
  if ! grep -qF 'git merge --abort' <<<"$out"; then bad "(r6) merge refusal must give the git merge --abort recovery; output: $out"; fi
  expect_refusal "(r6) conflicted merge, otherwise qualifying" merge-not-vouched
}

# set_pseudoref <NAME> <rev> — write a replay pseudoref FILE directly (git >= 2.45
# refuses `update-ref` on pseudorefs like MERGE_HEAD), then require that it resolves:
# a setup that silently fails turns a refusal case into a plain pick that passes.
set_pseudoref() {
  git rev-parse "$2" > "$(git rev-parse --git-path "$1")" || { bad "setup: cannot write $1"; return 1; }
  if [[ "$(git rev-parse -q --verify "$1^{commit}")" != "$(git rev-parse "$2")" ]]; then
    bad "setup: $1 does not resolve to $2"; return 1
  fi
}

# (r6b) MERGE_HEAD alongside a pick (`rebase -r` stopped on a merge shape; the file is
#       written directly because reaching it through real git needs a rebase-merges script)
#       → the merge rule wins over an otherwise-qualifying pick.
case_r6b() {
  replay_fixture story-r6b "$(devloop_msg story-r6b)"
  conflicted_pick devloop || return
  set_pseudoref MERGE_HEAD devloop || return
  run_validate; expect_refusal "(r6b) MERGE_HEAD + CHERRY_PICK_HEAD" merge-not-vouched
}

# (r6c) CHERRY_PICK_HEAD and REBASE_HEAD both present (REBASE_HEAD written directly).
case_r6c() {
  replay_fixture story-r6c "$(devloop_msg story-r6c)"
  conflicted_pick devloop || return
  set_pseudoref REBASE_HEAD devloop || return
  run_validate; expect_refusal "(r6c) CHERRY_PICK_HEAD + REBASE_HEAD" multiple-replay-states
}

# (r7) a BRANCH named CHERRY_PICK_HEAD, nothing in progress → not replay state.
case_r7() {
  replay_fixture story-r7 "$(devloop_msg story-r7)"
  git branch CHERRY_PICK_HEAD devloop
  git checkout devloop -- docs/devloop-outputs/story-r7/main.md
  echo other > other.rs; git add other.rs
  run_validate
  if grep -q 'git checkout' <<<"$out"; then bad "(r7) must not advise checking out from the spoof branch; output: $out"; fi
  expect_refusal "(r7) branch named CHERRY_PICK_HEAD" ref-not-pseudoref
}

# (r8) CHERRY_PICK_HEAD file forged with garbage, then with a TREE OID → `^{commit}`
#      doesn't resolve ⇒ no replay state ⇒ silence + today's block.
case_r8() {
  replay_fixture story-r8 "$(devloop_msg story-r8)"
  git checkout devloop -- docs/devloop-outputs/story-r8/main.md
  echo other > other.rs; git add other.rs
  local cph forged; cph="$(git rev-parse --git-path CHERRY_PICK_HEAD)"
  for forged in "not-a-sha" "$(git rev-parse 'devloop^{tree}')"; do
    printf '%s\n' "$forged" > "$cph"
    run_validate
    if [[ "$rc" -eq 0 ]] || grep -q 'replay skip' <<<"$out" || ! grep -q 'no validation verdict found' <<<"$out"; then
      bad "(r8) forged CHERRY_PICK_HEAD=$forged must block silently (rc=$rc); output: $out"
    fi
  done
  [[ "$__case_failed" -eq 0 ]] && ok "(r8) forged CHERRY_PICK_HEAD (garbage / tree OID) → today's no-verdict block, no replay line"
}

# (r9) CHERRY_PICK_HEAD forged to a REAL commit that wrote a different
#      main.md blob at that path → [main-md-modified].
case_r9() {
  replay_fixture story-r9 "$(devloop_msg story-r9)"
  git checkout -qb forged devloop
  printf '\nother content\n' >> docs/devloop-outputs/story-r9/main.md
  git add -A; commit_msg "$(devloop_msg story-r9)"
  git checkout -q main
  conflicted_pick devloop || return
  git rev-parse forged > "$(git rev-parse --git-path CHERRY_PICK_HEAD)"
  run_validate; expect_refusal "(r9) CHERRY_PICK_HEAD forged to a commit with a different blob" main-md-modified
}

# (r10) MIXED: the replayed main.md qualifies, a second complete main.md staged by hand
#       does not → a verdict is still required for the second.
case_r10() {
  replay_fixture story-r10 "$(devloop_msg story-r10)"
  conflicted_pick devloop || return
  mk_main_md story-r10-other complete; git add -A
  run_validate
  if [[ "$rc" -ne 0 ]] && grep -q 'Gate-2: replay vouched for docs/devloop-outputs/story-r10/main.md .*still requires a verdict' <<<"$out" \
     && ! grep -q 'Gate-2: replay skip: ' <<<"$out" \
     && grep -q 'replay skip not applied to docs/devloop-outputs/story-r10-other/main.md' <<<"$out" \
     && grep -q 'no validation verdict found' <<<"$out"; then
    ok "(r10) mixed commit → replayed main.md vouched (no 'verdict NOT checked' claim), verdict still required for the other"
  else
    bad "(r10) mixed commit should BLOCK on the non-qualifying main.md (rc=$rc); output: $out"
  fi
}

# (r10b) one replayed commit that wrote TWO complete main.md → refused for both: the
#        hook never lets a commit complete two devloops, so this one bypassed it.
case_r10b() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md story-a complete; mk_main_md story-b complete
  echo devloop > src.rs; git add -A; commit_msg "$(devloop_msg story-a)"
  git checkout -q main; echo target > src.rs; git commit -qam target
  conflicted_pick devloop || return
  run_validate
  if [[ "$rc" -ne 0 ]] && grep -q 'not applied to docs/devloop-outputs/story-a/main.md.*\[multiple-records-in-replayed-commit\]' <<<"$out" \
     && grep -q 'not applied to docs/devloop-outputs/story-b/main.md.*\[multiple-records-in-replayed-commit\]' <<<"$out" \
     && ! grep -q 'Gate-2: replay skip: ' <<<"$out"; then
    ok "(r10b) replayed commit wrote two complete main.md → both refused, block"
  else
    bad "(r10b) both must refuse [multiple-records-in-replayed-commit] (rc=$rc); output: $out"
  fi
}

# (r11) identical content at a DIFFERENT slug's path → never matches across paths.
case_r11() {
  replay_fixture story-r11 "$(devloop_msg story-r11)"
  conflicted_pick devloop || return
  mkdir -p docs/devloop-outputs/story-r11-copy
  cp docs/devloop-outputs/story-r11/main.md docs/devloop-outputs/story-r11-copy/main.md
  git add -A
  run_validate
  # The genuine story-r11 main.md still qualifies; the copy must not.
  if [[ "$rc" -ne 0 ]] && grep -q 'not applied to docs/devloop-outputs/story-r11-copy/main.md.*\[path-absent-in-replayed-commit\]' <<<"$out" \
     && ! grep -q 'Gate-2: replay skip: docs/devloop-outputs/story-r11-copy/' <<<"$out"; then
    ok "(r11) same blob at another slug path → block, [path-absent-in-replayed-commit]"
  else
    bad "(r11) the copy must refuse [path-absent-in-replayed-commit] (rc=$rc); output: $out"
  fi
}

# (r12) git error FAIL-CLOSED: (a) `cat-file commit <replayed>` fails; (b) MID-CHECK —
#       `rev-parse <replayed>:<path>` fails after replay state resolved; (d) `diff-tree
#       <parent> <replayed>` fails (e.g. a shallow boundary: the parent is missing). Each blocks
#       with [git-error] and git's (here: the shim's) own stderr is not swallowed.
case_r12() {
  replay_fixture story-r12 "$(devloop_msg story-r12)"
  conflicted_pick devloop || return
  with_git_shim 'cat-file'
  if [[ "$rc" -eq 0 ]] || ! grep -qF '[git-error]' <<<"$out" || ! grep -qF 'cat-file commit <replayed> failed (rc=97)' <<<"$out" \
     || ! grep -q 'shim: injected git failure for cat-file' <<<"$out" || grep -q 'Gate-2: replay skip: ' <<<"$out"; then
    bad "(r12a) cat-file failure must block with [git-error] + git's stderr (rc=$rc); output: $out"
  fi
  with_git_shim 'diff-tree'
  if [[ "$rc" -eq 0 ]] || ! grep -qF 'diff-tree <parent> <replayed> failed (rc=97)' <<<"$out" || grep -q 'Gate-2: replay skip: ' <<<"$out"; then
    bad "(r12d) diff-tree failure must block with [git-error] (rc=$rc); output: $out"
  fi
  with_git_shim "$(git rev-parse devloop):docs/*"
  if [[ "$rc" -eq 0 ]] || ! grep -qF '[git-error]' <<<"$out" || ! grep -q 'rc=97' <<<"$out" || ! grep -q 'shim: injected git failure' <<<"$out" || grep -q 'Gate-2: replay skip: ' <<<"$out"; then
    bad "(r12b) mid-check rev-parse failure must block with [git-error] rc=97 + git's stderr (rc=$rc); output: $out"
  fi
  [[ "$__case_failed" -eq 0 ]] && ok "(r12) git errors (first + mid-check) → block, [git-error], git stderr kept"
}

# (r12c) git errors in the REPLAY-STATE stage (gate2_replay_source) are git-error
#        refusals, never "no replay": the `<REF>^{commit}` lookup failing with an rc
#        other than 1 (the absent case), `--git-path MERGE_HEAD` failing, and the
#        `--symbolic-full-name` spoof check failing.
case_r12c() {
  replay_fixture story-r12c "$(devloop_msg story-r12c)"
  conflicted_pick devloop || return
  local -a arms=('CHERRY_PICK_HEAD^{commit}|rev-parse' 'MERGE_HEAD|rev-parse' 'MERGE_HEAD^{commit}|rev-parse' '--symbolic-full-name|rev-parse')
  local arm
  for arm in "${arms[@]}"; do
    with_git_shim "${arm%|*}" "${arm#*|}"
    if [[ "$rc" -eq 0 ]] || ! grep -qF '[git-error]' <<<"$out" || ! grep -q 'rc=97' <<<"$out" \
       || ! grep -q 'shim: injected git failure' <<<"$out" || grep -q 'Gate-2: replay skip: ' <<<"$out"; then
      bad "(r12c) shimmed '${arm%|*}' failure must block with [git-error] rc=97 + git's stderr, no skip (rc=$rc); output: $out"
    fi
    # An unresolved replay sha is shown as such, never as an empty field.
    if [[ "${arm%|*}" == "CHERRY_PICK_HEAD^{commit}" ]] && ! grep -qF '(CHERRY_PICK_HEAD unresolved)' <<<"$out"; then
      bad "(r12c) unresolved sha must print as 'unresolved'; output: $out"
    fi
  done
  [[ "$__case_failed" -eq 0 ]] && ok "(r12c) replay-state git errors (<REF>^{commit} / --git-path / MERGE_HEAD^{commit} / --symbolic-full-name) → [git-error], never 'no replay'"
}

# (r19) FAIL-CLOSED ENUMERATION (security S1): a failing `git diff --cached` must never
#       read as an empty changeset. (a) the trigger enumeration fails → trigger-error
#       block; (b) only conjunct 2's enumeration fails (the trigger's --diff-filter=d call
#       still works) → enumeration-error block; (c) on a qualifying replay, the audit
#       count's enumeration fails → the skip is withdrawn ([git-error], no skip line).
case_r19() {
  seed_commit
  mk_main_md story-r19 complete
  echo "fn main() {}" > src.rs
  git add -A
  with_git_shim '--cached' diff
  if [[ "$rc" -eq 0 ]] || ! grep -q 'error evaluating the commit trigger (rc=3)' <<<"$out"; then
    bad "(r19a) failing trigger enumeration must block via the trigger error (rc=$rc); output: $out"
  fi
  with_git_shim '--cached' diff '--diff-filter=d'
  if [[ "$rc" -eq 0 ]] || ! grep -q 'error enumerating the staged changeset' <<<"$out"; then
    bad "(r19b) failing conjunct-2 enumeration must block (rc=$rc); output: $out"
  fi
  [[ "$__case_failed" -eq 0 ]] || return
  git reset -q --hard; git clean -qfd
  git checkout -q -b r19c
  # (c) a qualifying replay on a fresh fixture in the same repo.
  git checkout -q -b devloop
  mk_main_md story-r19c complete
  echo devloop > src.rs; git add -A; commit_msg "$(devloop_msg story-r19c)"
  git checkout -q r19c; echo target > src.rs; git add -A; git commit -qm target
  conflicted_pick devloop || return
  with_git_shim '--cached' diff '--diff-filter=d'
  if [[ "$rc" -eq 0 ]] || ! grep -qF '[git-error] enumerating the staged changeset failed (rc=' <<<"$out" || grep -q 'Gate-2: replay skip: ' <<<"$out"; then
    bad "(r19c) failing audit-count enumeration must withdraw the skip with [git-error] (rc=$rc); output: $out"
  fi
  [[ "$__case_failed" -eq 0 ]] && ok "(r19) failing staged-changeset enumeration (trigger / conjunct 2 / audit count) → block, never an empty set"
}

# (r20) BLOB-RESOLUTION FAIL-CLOSED inside the signature path (security S2): the record
#       stream is hashed from an `if !` condition, where errexit is inert, so a failing
#       `rev-parse :<bound path>` must abort the stream EXPLICITLY. Assert the stream
#       function's rc directly, and that the hook blocks via the signature-error path —
#       not a mismatch built from an empty-blob record.
case_r20() {
  seed_commit
  mk_main_md story-r20 complete
  echo "fn main() {}" > src.rs
  git add -A
  declare -A ls=( [4]=OK ) ld=( [4]=2 )
  emit_gate2_verdict 0 ls ld
  local shim="$DEVLOOP_TMP/shim"
  with_git_shim ':src.rs' rev-parse     # also installs the shim for the direct call below
  if [[ "$rc" -eq 0 ]] || ! grep -q 'error recomputing the staged-tree signature' <<<"$out" \
     || grep -q 'signature mismatch' <<<"$out"; then
    bad "(r20) blob failure must block via the signature-error path (rc=$rc); output: $out"
  fi
  if PATH="$shim:$PATH" REAL_GIT="$__real_git" SHIM_MATCH=':src.rs' SHIM_CMD=rev-parse SHIM_NOT='' \
       gate2_records_staged >/dev/null 2>&1; then
    bad "(r20) gate2_records_staged must return nonzero when a blob can't be resolved"
  fi
  [[ "$__case_failed" -eq 0 ]] && ok "(r20) blob-resolution failure → record stream aborts, hook blocks on the signature error (not a mismatch)"
}

# (r21) a git error during a replay with TWO complete main.md must not surface as the
#       misleading "more than one staged devloop main.md" ambiguity: the cause is the
#       git error, so the block goes through the trigger-error path instead.
case_r21() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md story-a complete; mk_main_md story-b complete
  echo devloop > src.rs; git add -A; commit_msg "$(devloop_msg story-a)"
  git checkout -q main; echo target > src.rs; git commit -qam target
  conflicted_pick devloop || return
  with_git_shim 'CHERRY_PICK_HEAD^{commit}' rev-parse
  if [[ "$rc" -ne 0 ]] && grep -q 'error evaluating the commit trigger (rc=3)' <<<"$out" \
     && ! grep -q 'more than one staged devloop main.md' <<<"$out" && grep -qF '[git-error]' <<<"$out"; then
    ok "(r21) git error with two complete main.md → trigger-error block, not the ambiguity message"
  else
    bad "(r21) expected the trigger-error block without the ambiguity message (rc=$rc); output: $out"
  fi
}

# (r22) replay state is reset per call: two DIRECT (non-subshell) calls of the trigger
#       fn in one shell each report exactly one skip line, never an inherited vouch.
case_r22() {
  replay_fixture story-r22 "$(devloop_msg story-r22)"
  conflicted_pick devloop || return
  local o1 o2
  gate2_staged_trigger_slug >/dev/null 2>"$DEVLOOP_TMP/e1"
  gate2_staged_trigger_slug >/dev/null 2>"$DEVLOOP_TMP/e2"
  o1="$(grep -c 'Gate-2: replay skip: ' "$DEVLOOP_TMP/e1" || true)"
  o2="$(grep -c 'Gate-2: replay skip: ' "$DEVLOOP_TMP/e2" || true)"
  if [[ "$o1" -eq 1 && "$o2" -eq 1 ]]; then
    ok "(r22) direct repeat calls → one skip line each (state reset at entry)"
  else
    bad "(r22) expected one skip line per direct call, got $o1 then $o2"
  fi
}

# (r13) a PRESENT staged main.md that cannot be read → the trigger-error block (rc 3),
#       not the old silent no-op.
case_r13() {
  seed_commit
  mk_main_md story-r13 complete
  echo "fn main() {}" > src.rs
  git add -A
  with_git_shim ':docs/devloop-outputs/*/main.md' show
  if [[ "$rc" -ne 0 ]] && grep -q 'error evaluating the commit trigger (rc=3)' <<<"$out"; then
    ok "(r13) unreadable staged main.md → trigger-error block (rc 3), not a silent no-op"
  else
    bad "(r13) should BLOCK via the trigger error (rc=$rc); output: $out"
  fi
}

# (r23) `cherry-pick -m 1 <merge>`: the replayed commit is a MERGE that brought in a
#       devloop branch → refused (a merge carries a whole branch; never vouched).
case_r23() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md story-r23 complete; echo devloop > src.rs; git add -A; commit_msg "$(devloop_msg story-r23)"
  git checkout -qb integ main; git merge -q --no-ff -m "merge devloop" devloop
  git checkout -q main; echo target > src.rs; git commit -qam target
  if git cherry-pick -m 1 integ >/dev/null 2>&1; then bad "setup: cherry-pick -m did not conflict"; return; fi
  echo hand-resolved > src.rs; git add src.rs
  run_validate; expect_refusal "(r23) replayed merge commit (cherry-pick -m)" replayed-merge-not-vouched
}

# (r24) a graft (`git replace --graft <sha>`, no parents) cannot make the replayed
#       commit look like a root, nor hide its parent: objects are read raw → allow.
case_r24() {
  replay_fixture story-r24 "$(devloop_msg story-r24)"
  git replace --graft devloop
  conflicted_pick devloop || return
  run_validate
  if [[ "$rc" -eq 0 ]] && grep -qF '(slug story-r24)' <<<"$out"; then
    ok "(r24) graft making the commit parentless is ignored → allow on the raw parent"
  else
    bad "(r24) should ALLOW on the raw object's parent (rc=$rc); output: $out"
  fi
}

# (r15) a staged DELETION of a complete main.md is not a trigger (and must not hit the
#       rc-3 read-failure block) — today's behaviour.
case_r15() {
  seed_commit
  mk_main_md story-r15 complete
  echo one > src.rs
  git add -A; git commit -qm "complete story-r15"
  git rm -q docs/devloop-outputs/story-r15/main.md
  echo two > src.rs; git add src.rs
  run_validate
  if [[ "$rc" -eq 0 ]]; then
    ok "(r15) staged deletion of a complete main.md → no trigger (allow)"
  else
    bad "(r15) a staged main.md deletion must not block (rc=$rc); output: $out"
  fi
}

# (r16) REGRESSION GUARD against an ancestor walk: A adds main.md; B on top touches
#       src.rs only. Cherry-pick B with a
#       conflict and hand-stage main.md identical to B's tree (else it isn't in the
#       changeset and the case is vacuous). An ancestor-walk rule would allow this.
case_r16() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md story-r16 complete; git add -A; commit_msg "$(devloop_msg story-r16)"
  echo devloop > src.rs; git add -A; commit_msg "follow-up without a trailer"
  git checkout -q main; echo target > src.rs; git commit -qam target
  conflicted_pick devloop || return
  git checkout devloop -- docs/devloop-outputs/story-r16/main.md
  run_validate; expect_refusal "(r16) only an ANCESTOR wrote main.md" not-written-by-replayed-commit
}

# (r17) rebase stopped on a LATER commit that itself rewrote main.md (REBASE_HEAD =
#       that commit) → allow: that commit wrote the blob, so it passed the hook (or
#       was bypassed) when it was made, like any other writer.
case_r17() {
  seed_commit
  echo base > src.rs; git add src.rs; git commit -qm base-src
  git checkout -qb devloop
  mk_main_md story-r17 complete; git add -A; commit_msg "$(devloop_msg story-r17)"
  printf '\nfollow-up note\n' >> docs/devloop-outputs/story-r17/main.md
  echo devloop > src.rs; git add -A; commit_msg "follow-up without a trailer"
  git checkout -q main; echo target > src.rs; git commit -qam target
  git checkout -q devloop
  if git rebase main >/dev/null 2>&1; then bad "setup: rebase did not conflict"; return; fi
  if [[ "$(git rev-parse -q --verify REBASE_HEAD)" != "$(git rev-parse devloop)" ]]; then bad "setup: rebase did not stop on the later commit"; return; fi
  echo hand-resolved > src.rs; git add src.rs
  run_validate
  if [[ "$rc" -eq 0 ]] && grep -qF '== REBASE_HEAD' <<<"$out"; then
    ok "(r17) rebase stopped on a later commit that rewrote main.md → allow"
  else
    bad "(r17) should ALLOW (rc=$rc); output: $out"
  fi
}

printf 'gate2 isolation self-test (matrix a–f + slug/extra-file/exclusion/ambiguity/deletion/adversarial-ordering/remediation-ordering/source-safety guards + replay skip q1–q9/r1–r24):\n'
for c in case_a case_b case_c case_d case_e case_f case_g case_h case_i case_j case_k case_l case_m case_n case_o case_p \
         case_q1 case_q2 case_q3 case_q4 case_q5 case_q7 case_q8 case_q9 \
         case_r1 case_r2 case_r5 case_r6 case_r6b case_r6c case_r7 case_r8 case_r9 \
         case_r10 case_r10b case_r11 case_r12 case_r12c case_r13 case_r19 case_r20 case_r21 case_r22 case_r15 case_r16 case_r17 case_r23 case_r24; do
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
