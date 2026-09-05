#!/usr/bin/env bash
#
# SELF-TEST for scripts/guards/simple/validate-frame-vectors.sh.
#
# The guard passes in-tree on every run, so its PASS path is exercised
# constantly and its FAIL paths never are — and those branches are its entire
# value. A check that greps, finds nothing, compares nothing and reports success
# is the exact bug it was written to prevent.
#
# Deliberately NOT under guards/simple/: run-guards.sh's `find -name '*.sh'`
# matches `*.test.sh` too, so it would be auto-run as a guard as well as here.
# Wired explicitly in scripts/layer3.sh alongside the existing precedents.
#
# Hermetic: a synthetic tree per case via the DEVLOOP_TEST-gated
# FRAME_VECTORS_ROOT seam. No cluster, no network, no cargo.
set -uo pipefail
IFS=$'\n\t'

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"
GUARD="${HERE}/simple/validate-frame-vectors.sh"

PASS=0
FAIL=0

# Build a synthetic tree that is a faithful copy of the real one, so each case
# mutates exactly one thing. Copying rather than fabricating matters: a
# hand-built fixture drifts from the real shape and the self-test then exercises
# branches that no longer exist.
new_tree() {
  local d
  d="$(mktemp -d)"
  mkdir -p "$d/proto/test-vectors/external/sframe-wg" \
           "$d/crates/media-protocol/src" \
           "$d/crates/media-vector-gen/tests" \
           "$d/docs"
  cp "$REPO/proto/test-vectors/frame-v2.vectors.json" "$d/proto/test-vectors/"
  cp "$REPO"/proto/test-vectors/external/sframe-wg/* "$d/proto/test-vectors/external/sframe-wg/"
  cp "$REPO/crates/media-protocol/src/frame.rs" "$d/crates/media-protocol/src/"
  cp "$REPO/crates/media-protocol/src/codec.rs" "$d/crates/media-protocol/src/"
  cp "$REPO/crates/media-vector-gen/Cargo.toml" "$d/crates/media-vector-gen/"
  cp "$REPO/crates/media-vector-gen/tests/rust_codec_conformance.rs" "$d/crates/media-vector-gen/tests/"
  cp "$REPO/Cargo.toml" "$d/"
  cp "$REPO/docs/TODO.md" "$d/docs/"
  # The story file is a `spec_anchor` target for `wrap_key_id_mismatch`, so g16
  # reads it. Copying it was missed on the first draft and the self-test caught
  # it — which is the point: an incomplete fixture makes every downstream case
  # fail for the wrong reason.
  #
  # These two are deliberately NOT masked with `|| true`. They were, until story
  # task 15: a missing fixture silently changed what g16's spec_anchor check
  # exercised while every case still reported a tidy pass, which is the exact
  # shape this suite exists to catch.
  mkdir -p "$d/docs/user-stories"
  cp "$REPO"/docs/user-stories/*.md "$d/docs/user-stories/"
  mkdir -p "$d/docs/observability/metrics"
  cp "$REPO"/docs/observability/metrics/*.md "$d/docs/observability/metrics/"

  # The TypeScript codec sites g12 enumerates and the conformance marker g14
  # requires. COPIED from the real tree, never fabricated as stubs — for the same
  # reason as every other line of this function, and with a sharper consequence
  # here: a hand-written stub containing `max_payload_bytes` would satisfy g12's
  # positive arm in this suite FOREVER, including after the real `frameCodec.ts`
  # stopped reading the SSoT. The self-test would report the guard healthy while
  # testing a file that is not the one shipping.
  #
  # CONSEQUENCE WORTH KNOWING BEFORE YOU DEBUG A FAILURE HERE: every case in this
  # suite now transitively depends on the real `frameCodec.ts` and `sframe.ts`
  # satisfying g12. That is the correct coupling — it IS the production gate — but
  # it means a future client change that hardcodes `1048576` will red this suite
  # under case names like "g5 deleted codec token", pointing triage at the wrong
  # file. Check the g12 arm first.
  mkdir -p "$d/packages/sdk-core/src/media/frame/__tests__"
  cp "$REPO/packages/sdk-core/src/media/frame/frameCodec.ts" "$d/packages/sdk-core/src/media/frame/"
  cp "$REPO/packages/sdk-core/src/media/frame/sframe.ts" "$d/packages/sdk-core/src/media/frame/"
  cp "$REPO/packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts" \
     "$d/packages/sdk-core/src/media/frame/__tests__/"
  echo "$d"
}

# Return the fixture tree to the PRE-TASK-15 ungated state.
#
# `new_tree()` copies the REAL tree, and the real tree is now fully gated — so the
# states several of these cases must reach no longer exist upstream and have to be
# synthesized. Expressed once, here, rather than open-coded per mutator: the
# marker string below is what the guard `grep -qF`s, and five hand-copies of it
# would drift.
#
# ORDER MATTERS, and the helper invites getting it backwards: run this BEFORE any
# mutation that edits the marker (`todo-tracking-entry-missing` renames it). Run
# it after, and this re-appends a clean marker, the guard finds it, and the case
# exits 0 against a wanted 1.
#
# WHY THE MARKER HAS TO COME BACK AT ALL: `todo-tracking-entry-missing` lives in
# the `else` arm of g14's codec loop — the arm that only runs while a codec is
# ungated. `new_tree()` copies the real `docs/TODO.md`, from which story task 15
# deleted the marker. Without re-synthesizing it, every ungating case exits 1 on
# that precondition instead of reaching the outcome it asserts.
#
# AND THE STAKES ON THAT BRANCH ARE NOW HIGHER THAN THEY LOOK: post-task-15 both
# codecs are gated on every real run, so the `else` arm is DEAD IN TREE FOREVER.
# This suite is its only exerciser. It is also the least obviously valuable code
# here to whoever next tidies this file. Do not remove it.
#
# $1 = tree; $2 = value for cross_language_property_established (default false).
ungate_typescript() {
  local tree="$1" established="${2:-false}"
  # `|| return 1`, NOT a bare `&&` chain. Without it the function's exit status is
  # printf's (always 0), so a jq parse failure would leave a HALF-TRANSFORMED tree
  # — marker appended, flags un-flipped — and `expect`'s SETUP FAILED arm would
  # never fire because the mutator reported success. That is a silent pass hiding
  # inside the one construct whose whole job is to make an unreachable state
  # reachable, which is the exact failure this suite exists to prevent
  # (@infrastructure R1, demonstrated by mutation).
  jq ".gated_by.typescript = false | .cross_language_property_established = ${established}" \
    "$tree/proto/test-vectors/frame-v2.vectors.json" > "$tree/ug" \
    && mv "$tree/ug" "$tree/proto/test-vectors/frame-v2.vectors.json" \
    || return 1
  # The EXACT literal, inside a TODO-entry-shaped line. Load-bearing for the
  # `-MOVED` case, whose whole point is that `<!-- frame-vectors-ts-leg-MOVED -->`
  # still contains the substring but not the full marker.
  printf '\n- [ ] <!-- frame-vectors-ts-leg --> **synthetic fixture entry (self-test)**\n' \
    >> "$tree/docs/TODO.md"
}
# `expect` runs mutators via `bash -c`, which does not inherit shell functions.
export -f ungate_typescript

# expect <case> <want-exit> <want-pattern> <mutator...>
expect() {
  local name="$1" want_exit="$2" want_pat="$3"; shift 3
  local tree out rc
  tree="$(new_tree)"
  ( cd "$tree" && "$@" ) || { echo "  SETUP FAILED: $name"; rm -rf "$tree"; FAIL=$((FAIL+1)); return; }
  out="$(FRAME_VECTORS_ROOT="$tree" DEVLOOP_TEST=1 "$GUARD" 2>&1)"; rc=$?
  rm -rf "$tree"
  if [[ "$rc" != "$want_exit" ]]; then
    echo "  FAIL: $name — exit $rc, wanted $want_exit"
    echo "$out" | sed 's/^/      /' | head -4
    FAIL=$((FAIL+1)); return
  fi
  if ! grep -qE "$want_pat" <<< "$out"; then
    echo "  FAIL: $name — exit was right but output lacked /$want_pat/"
    echo "$out" | sed 's/^/      /' | head -4
    FAIL=$((FAIL+1)); return
  fi
  echo "  ok: $name"
  PASS=$((PASS+1))
}

echo "validate-frame-vectors self-test"

# --- vacuity bails: every missing input must be LOUD, never a skip -----------
expect "vectors-missing"      1 'PRECONDITION \[vectors-missing\]'      rm proto/test-vectors/frame-v2.vectors.json
expect "vectors-unparseable"  1 'PRECONDITION \[vectors-unparseable\]'  bash -c 'echo "{" > proto/test-vectors/frame-v2.vectors.json'
expect "banner-missing"       1 'PRECONDITION \[banner-missing\]'       bash -c 'jq "del(._non_production)" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "rust-const-not-found" 1 'PRECONDITION \[rust-const-not-found\]' bash -c 'sed -i "s/^pub const MAX_PAYLOAD_BYTES/pub const RENAMED_MAX_PAYLOAD/" crates/media-protocol/src/frame.rs'
expect "flag-consts-not-found" 1 'PRECONDITION \[rust-flag-consts-not-found\]' bash -c 'sed -i "s/^pub const FLAG_DISCARDABLE/pub const RENAMED_FLAG/" crates/media-protocol/src/frame.rs'
expect "reject-macro-not-found" 1 'PRECONDITION \[reject-macro-not-found\]' bash -c 'sed -i "s/ => \"/ =X \"/g" crates/media-protocol/src/codec.rs'
expect "external-vectors-missing" 1 'PRECONDITION \[external-vectors-missing\]' rm proto/test-vectors/external/sframe-wg/test-vectors.json
expect "manifest-missing"     1 'PRECONDITION \[manifest-missing\]'     rm proto/test-vectors/external/sframe-wg/manifest.json
expect "provenance-digest-not-found" 1 'PRECONDITION \[provenance-digest-not-found\]' bash -c 'sed -i "s/SHA-256 of vendored bytes/SHA-256 OF NOTHING/" proto/test-vectors/external/sframe-wg/PROVENANCE.md'
expect "suite-list-empty"     1 'PRECONDITION \[suite-list-empty\]'     bash -c 'jq ".cipher_suites = []" proto/test-vectors/external/sframe-wg/manifest.json > t && mv t proto/test-vectors/external/sframe-wg/manifest.json'
expect "generator-crate-missing (file)"   1 'PRECONDITION \[generator-crate-missing\]' rm crates/media-vector-gen/Cargo.toml
expect "generator-crate-missing (member)" 1 'PRECONDITION \[generator-crate-missing\]' bash -c 'sed -i "s|\"crates/media-vector-gen\",||" Cargo.toml'
expect "crypto-field-unclassified" 1 'PRECONDITION \[crypto-field-unclassified\]' bash -c 'jq "(.vectors[0].crypto.new_secret_hex) = \"aabb\"" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'

# g15 anti-vacuity, PER SUITE. The aggregate form ("at least one row overall")
# would pass this case, because 0x0004 survives — which is exactly the
# degradation the anchor exists to catch.
expect "suite-yields-zero-rows (per-suite, not aggregate)" 1 'PRECONDITION \[suite-yields-zero-rows\]' \
  bash -c 'jq "(.sframe) |= map(select(.cipher_suite != 5))" proto/test-vectors/external/sframe-wg/test-vectors.json > t && mv t proto/test-vectors/external/sframe-wg/test-vectors.json'

# Ungate FIRST, then rename: the precondition being tested only runs while a codec
# is ungated. The rename must land on the marker the helper just wrote — reverse
# the order and the helper re-adds a clean one, the guard finds it, and the case
# exits 0 against a wanted 1.
expect "todo-tracking-entry-missing" 1 'PRECONDITION \[todo-tracking-entry-missing\]' \
  bash -c 'ungate_typescript . && sed -i "s/frame-vectors-ts-leg/frame-vectors-ts-leg-MOVED/" docs/TODO.md'

# --- violations: wrong values, not missing inputs ----------------------------
expect "g2 max_payload drift"  1 'VIOLATION.*g2 max_payload_bytes'  bash -c 'jq ".max_payload_bytes = 999" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g3 header_version drift" 1 'VIOLATION.*g3 header_version'   bash -c 'jq ".header_version = 9" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g4 flag mask drift"    1 'VIOLATION.*g4 legal_flag_mask'    bash -c 'jq ".wire_constants.legal_flag_mask = 15" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
# g5 EQUALITY, not subset: deleting a token from the file must red. Subset would pass.
expect "g5 deleted codec token" 1 'VIOLATION.*g5 codec reject tokens' bash -c 'jq "(.reject_reasons) |= map(select(.token != \"truncated\"))" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g5b has_vector false with a row" 1 'VIOLATION.*g5b' bash -c 'jq "(.reject_reasons[] | select(.token==\"replay_detected\") | .has_vector) = false" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g6 malformed token"    1 'VIOLATION.*g6 token'             bash -c 'jq "(.reject_reasons[0].token) = \"Bad-Token\"" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g7 aad not a subset"   1 'VIOLATION.*g7'                   bash -c 'jq "(.vectors[0].derived.aead_aad_hex) = (.vectors[0].derived.signed_input_hex)" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g9 vacuous near-miss"  1 'VIOLATION.*g9'                   bash -c 'jq "(.vectors[0].derived.naive_contiguous_signed_input_hex) = (.vectors[0].derived.signed_input_hex)" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g10 digest mismatch"   1 'VIOLATION.*g10'                  bash -c 'printf "\n" >> proto/test-vectors/external/sframe-wg/test-vectors.json'
expect "g13 reclassified secret" 1 'VIOLATION.*g13'                bash -c 'jq ".key_material_fields -= [\"kek_hex\"] | .derived_public_fields += [\"kek_hex\"]" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g13 real-looking key material" 1 'VIOLATION.*g13'          bash -c 'jq "(.vectors[0].crypto.kek_hex) = \"9f3c1a77b20e45d8e1c9042fa6b7381d5c0e29b4a87f6612d3049ee1bb70c5a2\"" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g16 crypto token typo"  1 'VIOLATION.*g16'                 bash -c 'jq "(.reject_reasons[] | select(.token==\"signature_invalid\") | .token) = \"signature_invald\"" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'

# --- g7b: derived-vs-frame_hex correspondence, declared per row --------------
# Mutating a byte inside the publisher region of a NON-mutated row makes its
# derived block stop describing its own frame. Undeclared, that must red.
expect "g7b undeclared derived/frame_hex divergence" 1 'VIOLATION.*g7b' \
  bash -c 'jq "(.vectors[0].frame_hex) = (\"03\" + .vectors[0].frame_hex[2:])" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
# A stale declaration on a row whose spans DO match is also wrong: it would tell
# a consumer to skip a comparison that is in fact meaningful.
expect "g7b stale derived_describes on a matching row" 1 'VIOLATION.*g7b' \
  bash -c 'jq "(.vectors[0].expected) = {\"derived_describes\": \"base_frame_before_mutation\"}" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "derived-note-missing" 1 'PRECONDITION \[derived-note-missing\]' \
  bash -c 'jq "del(._comment_derived_on_reject_rows)" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'

# --- g12: inert BY DECLARATION, and it really starts running when flipped -----
# The point @dry-reviewer pressed at Gate 1: inert-by-declaration and
# inert-by-absence are indistinguishable from a green run, so both branches are
# driven here rather than at task 15.
# The fixture is DEGENERATE ON PURPOSE — do not "tidy" it back to the copied file.
#
# This case proves g12 is inert BY DECLARATION. Before story task 15 it got that
# power from the TS sites being ABSENT: had g12 run anyway it would have hit
# `ts-site-not-found` and exited 1, so exit 0 proved the block was skipped. Now
# that `new_tree()` copies the real, CORRECT sites, g12 running anyway would
# simply pass — and the case would still see exit 0 with the banner. Green, and
# asserting nothing that `g14 arm 3` does not already cover.
#
# So the site is overwritten with a hardcoded `1048576`, which g12's banned-literal
# arm would red on. Exit 0 now means the block genuinely did not run.
#
# Generalisable: making a fixture MORE realistic can remove the unrealism an
# assertion was relying on. The improvement and the regression are in different
# parts of this file and nothing reds in between.
expect "g12 inert while gated_by.typescript is false" 0 '^WARN frame-vectors' \
  bash -c 'ungate_typescript . && printf "export const MAX = 1048576;\n" > packages/sdk-core/src/media/frame/frameCodec.ts'
# The sites are present in the fixture now, so this case must REMOVE one to reach
# the precondition. The list is ENUMERATED, not globbed: a renamed file must red
# here rather than silently drop out of the check.
expect "g12 fires when the flag flips and a site is absent" 1 'PRECONDITION \[ts-site-not-found\]' \
  rm packages/sdk-core/src/media/frame/sframe.ts
expect "g12 catches a hardcoded literal in a declared TS site" 1 'VIOLATION.*g12.*1048576' \
  bash -c 'printf "export const MAX = 1048576;\n" > packages/sdk-core/src/media/frame/frameCodec.ts'
# `legal_flag_mask` is 7, so a hardcode of it is not expressible as a banned
# literal at all — this positive arm is the only enforcement it has.
expect "g12 catches a TS site that never reads the vectors (the flag-mask half)" 1 'VIOLATION.*g12.*never references the vectors' \
  bash -c 'printf "export const LEGAL_FLAGS = 0b111;\n" > packages/sdk-core/src/media/frame/frameCodec.ts'

# expect_silent_pass <case> <mutator...>
#
# Asserts exit 0 with NO violations and NO banner of any kind.
#
# `expect` cannot express the ABSENCE of output, which is why these two cases were
# open-coded. `scripts/lang/_test_helpers.sh::assert_absent` is the repo's SSoT for
# that and is used by eight other suites; this suite is not being re-platformed onto
# the shared helpers mid-task, so one local helper stands in. That earlier
# justification ("`expect` cannot assert the ABSENCE of output") was true of `expect`
# and stale about the repo, and is corrected here rather than repeated.
#
# TAKES THE STRICTER OF THE TWO PREDICATES IT REPLACES: `VIOLATION|^WARN `, not
# `^WARN frame-vectors` alone. A shared helper inherits the WEAKER predicate unless
# someone checks, and the weaker one would let a VIOLATION through the g12
# end-state case under cover of a DRY cleanup.
expect_silent_pass() {
  local name="$1"; shift
  local tree out rc
  tree="$(new_tree)"
  if [[ $# -gt 0 ]]; then
    ( cd "$tree" && "$@" ) || { echo "  SETUP FAILED: $name"; rm -rf "$tree"; FAIL=$((FAIL+1)); return; }
  fi
  out="$(FRAME_VECTORS_ROOT="$tree" DEVLOOP_TEST=1 "$GUARD" 2>&1)"; rc=$?
  rm -rf "$tree"
  if [[ "$rc" == 0 ]] && ! grep -qE 'VIOLATION|^WARN ' <<< "$out"; then
    echo "  ok: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name — exit $rc, output: $(head -1 <<< "$out")"
    FAIL=$((FAIL + 1))
  fi
}

# Both cases below are the SAME tree state post-task-15 — the unmutated fixture —
# but they are kept under their existing names because they assert different
# intents: one that g12 passes silently when the sites read the vectors, one that
# g14 emits no banner once every codec is gated. Merging them would lose the
# record of which property a future failure broke.
expect_silent_pass "g12 passes silently when TS sites read the vectors (task-15 end state)"

# --- g14: BOTH directions, plus BOTH pass states -----------------------------
# Arm 1: the flag claims gated, but the conformance marker is gone. Deliberately
# does NOT use `ungate_typescript` — this case must stay in the GATED state, which
# is what it is testing. Routing it through the helper for symmetry would flip the
# very state the case exists to exercise.
expect "g14 arm 1: flag true, marker absent" 1 'VIOLATION.*g14 gated_by.typescript is true' \
  rm packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts
# Arm 2: the top-level claim true while a codec is ungated.
# Arm 2: ungate the codec but leave the top-level claim TRUE — the contradiction
# this arm exists to catch. The marker must come back with the ungate, or
# `todo-tracking-entry-missing` exits before the `fail` this asserts is reached.
expect "g14 arm 2: claim true, codec ungated" 1 'VIOLATION.*g14 cross_language_property_established claims true' \
  bash -c 'ungate_typescript . true'
# Arm 3: the task-8 state. Passes AND emits the banner. Pinned because a later
# reader who reasonably thinks a guard that KNOWS the property is unestablished
# should exit nonzero can "fix" it and break nothing visible — which inverts the
# non-suppressibility argument, since redding every devloop makes DELETING the
# guard the fastest route to green.
# Arm 3: the pre-task-15 state, synthesized. Passes AND emits the banner.
#
# NOTE the coupling: the banner text hardcodes "story task 15" and this pattern
# matches it. Post-flip that branch is unreachable in tree, so the wording now
# names a completed task — left as-is deliberately, because the branch is the
# drift-back protection if the flag is ever flipped false again. If anyone
# "corrects" the banner wording, this regex breaks with it.
expect "g14 arm 3: task-8 state passes with the banner" 0 '^WARN frame-vectors: .*typescript.*task 15' \
  bash -c 'ungate_typescript .'
# Arm 4: the task-15 state. Passes with NO banner. Pinned so the guard cannot red
# on the state it is supposed to end in — which would otherwise be discovered at
# task 15, the worst possible moment and the one where whoever hits it is under
# the most pressure to make it green fast.
expect_silent_pass "g14 arm 4: task-15 state passes with NO banner"

# --- The banner's PREFIX, pinned here -----------------------------------------
# run-guards.test.sh pins that the RUNNER surfaces `^WARN ` — but its stub guard
# is synthetic, so nothing there pins that THIS guard actually emits the prefix.
# Without this case, editing the banner's wording and losing `WARN ` would leave
# every test green while the banner silently vanished: the original defect
# wearing a different hat, landing on whoever's devloop is next.
tree="$(new_tree)"
# THE SIXTH BREAKING CASE, and the one a grep for `^expect ` cannot see — this is
# an inline block, not an `expect` call, which is exactly how it was missed when
# this rewrite was first scoped. Post-task-15 the unmutated fixture emits no
# banner at all, so the ungated state has to be synthesized for the prefix to
# exist to assert.
ungate_typescript "$tree"
out="$(FRAME_VECTORS_ROOT="$tree" DEVLOOP_TEST=1 "$GUARD" 2>&1)"
rm -rf "$tree"
if grep -qE '^WARN ' <<< "$out"; then
  echo "  ok: banner is line-anchored ^WARN (run-guards.sh's exit-0 arm greps for exactly this)"
  PASS=$((PASS+1))
else
  echo "  FAIL: banner lost its ^WARN prefix — run-guards.sh's pass arm would discard it"
  FAIL=$((FAIL+1))
fi

# --- In-tree state must pass --------------------------------------------------
out="$(FRAME_VECTORS_ROOT="$REPO" DEVLOOP_TEST=1 "$GUARD" 2>&1)"; rc=$?
if [[ "$rc" == 0 ]]; then
  echo "  ok: in-tree state passes"
  PASS=$((PASS+1))
else
  echo "  FAIL: in-tree state does not pass (exit $rc)"
  echo "$out" | sed 's/^/      /' | head -6
  FAIL=$((FAIL+1))
fi

echo "validate-frame-vectors self-test: ${PASS} passed, ${FAIL} failed"

# The case count, pinned.
#
# Without this the harness has NO FLOOR: silently dropping a case prints
# "35 passed, 0 failed" and exits 0. That is the same empty-result-reads-as-a-pass
# shape the guard's own header argues against at length in three numbered
# invariants — present, until now, inside the suite that enforces them.
#
# This is the run-time half of the review-time rule that no case is ever deleted.
# When you legitimately add or remove one, change this number in the same commit;
# it is deliberately the ONLY home for it, which is why `scripts/layer3.sh` no
# longer carries a copy.
EXPECTED_CASES=41
if [[ $((PASS + FAIL)) -ne "$EXPECTED_CASES" ]]; then
  echo "FAIL: ran $((PASS + FAIL)) cases, expected ${EXPECTED_CASES}. A case was added or dropped;" \
       "update EXPECTED_CASES deliberately rather than letting the tally float."
  exit 1
fi

[[ "$FAIL" -eq 0 ]] || exit 1
exit 0
