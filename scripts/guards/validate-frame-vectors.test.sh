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
  mkdir -p "$d/docs/user-stories"
  cp "$REPO"/docs/user-stories/*.md "$d/docs/user-stories/" 2>/dev/null || true
  mkdir -p "$d/docs/observability/metrics"
  cp "$REPO"/docs/observability/metrics/*.md "$d/docs/observability/metrics/" 2>/dev/null || true
  echo "$d"
}

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

expect "todo-tracking-entry-missing" 1 'PRECONDITION \[todo-tracking-entry-missing\]' \
  bash -c 'sed -i "s/frame-vectors-ts-leg/frame-vectors-ts-leg-MOVED/" docs/TODO.md'

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
expect "g12 inert while gated_by.typescript is false" 0 '^WARN frame-vectors' true
expect "g12 fires when the flag flips and a site is absent" 1 'PRECONDITION \[ts-site-not-found\]' \
  bash -c '
    mkdir -p packages/sdk-core/src/media/frame/__tests__
    touch packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts
    jq ".gated_by.typescript = true | .cross_language_property_established = true" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g12 catches a hardcoded literal in a declared TS site" 1 'VIOLATION.*g12.*1048576' \
  bash -c '
    mkdir -p packages/sdk-core/src/media/frame/__tests__
    touch packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts
    printf "export const MAX = 1048576;\n" > packages/sdk-core/src/media/frame/frameCodec.ts
    printf "export const SUITE = 5;\n" > packages/sdk-core/src/media/frame/sframe.ts
    jq ".gated_by.typescript = true | .cross_language_property_established = true" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
expect "g12 catches a TS site that never reads the vectors (the flag-mask half)" 1 'VIOLATION.*g12.*never references the vectors' \
  bash -c '
    mkdir -p packages/sdk-core/src/media/frame/__tests__
    touch packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts
    printf "export const LEGAL_FLAGS = 0b111;\n" > packages/sdk-core/src/media/frame/frameCodec.ts
    printf "export const X = 1;\n" > packages/sdk-core/src/media/frame/sframe.ts
    jq ".gated_by.typescript = true | .cross_language_property_established = true" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'

# Fully-gated state: exit 0, no banner, no violations. `expect` cannot assert the
# ABSENCE of output, so this one is inline — the same reason g14 arm 4 is.
tree="$(new_tree)"
mkdir -p "$tree/packages/sdk-core/src/media/frame/__tests__"
touch "$tree/packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts"
printf 'import v from "vectors";\nexport const MAX = v.max_payload_bytes;\n' \
  > "$tree/packages/sdk-core/src/media/frame/frameCodec.ts"
printf 'export const SUITE = v.cipher_suite_id;\n' \
  > "$tree/packages/sdk-core/src/media/frame/sframe.ts"
jq '.gated_by.typescript = true | .cross_language_property_established = true' \
  "$tree/proto/test-vectors/frame-v2.vectors.json" > "$tree/t" \
  && mv "$tree/t" "$tree/proto/test-vectors/frame-v2.vectors.json"
out="$(FRAME_VECTORS_ROOT="$tree" DEVLOOP_TEST=1 "$GUARD" 2>&1)"; rc=$?
rm -rf "$tree"
if [[ "$rc" == 0 ]] && ! grep -qE 'VIOLATION|^WARN ' <<< "$out"; then
  echo "  ok: g12 passes silently when TS sites read the vectors (task-15 end state)"
  PASS=$((PASS + 1))
else
  echo "  FAIL: g12 task-15 end state — exit $rc, output: $(head -1 <<< "$out")"
  FAIL=$((FAIL + 1))
fi

# --- g14: BOTH directions, plus BOTH pass states -----------------------------
# Arm 1: flag flipped to true without the conformance marker present.
expect "g14 arm 1: flag true, marker absent" 1 'VIOLATION.*g14 gated_by.typescript is true' \
  bash -c 'jq ".gated_by.typescript = true" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
# Arm 2: the top-level claim true while a codec is ungated.
expect "g14 arm 2: claim true, codec ungated" 1 'VIOLATION.*g14 cross_language_property_established claims true' \
  bash -c 'jq ".cross_language_property_established = true" proto/test-vectors/frame-v2.vectors.json > t && mv t proto/test-vectors/frame-v2.vectors.json'
# Arm 3: the task-8 state. Passes AND emits the banner. Pinned because a later
# reader who reasonably thinks a guard that KNOWS the property is unestablished
# should exit nonzero can "fix" it and break nothing visible — which inverts the
# non-suppressibility argument, since redding every devloop makes DELETING the
# guard the fastest route to green.
expect "g14 arm 3: task-8 state passes with the banner" 0 '^WARN frame-vectors: .*typescript.*task 15' true
# Arm 4: the task-15 state. Passes with NO banner. Pinned so the guard cannot
# red on the state it is supposed to end in — discovered at task 15, which is
# the worst possible moment and the one where whoever hits it is under the most
# pressure to make it green fast. `expect` cannot assert the ABSENCE of output,
# so this case is written inline below rather than through it.
# Arm 4 needs its own check for the ABSENCE of the banner, which `expect`
# cannot express. Done inline.
tree="$(new_tree)"
mkdir -p "$tree/packages/sdk-core/src/media/frame/__tests__"
touch "$tree/packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts"
# The task-15 end state must satisfy g12 as well as g14 — the two flip on the
# same flag, so a fixture that satisfies one and not the other is not that state.
printf 'export const MAX = v.max_payload_bytes;\n' > "$tree/packages/sdk-core/src/media/frame/frameCodec.ts"
printf 'export const SUITE = v.cipher_suite_id;\n' > "$tree/packages/sdk-core/src/media/frame/sframe.ts"
jq '.gated_by.typescript = true | .cross_language_property_established = true' \
  "$tree/proto/test-vectors/frame-v2.vectors.json" > "$tree/t" && mv "$tree/t" "$tree/proto/test-vectors/frame-v2.vectors.json"
out="$(FRAME_VECTORS_ROOT="$tree" DEVLOOP_TEST=1 "$GUARD" 2>&1)"; rc=$?
rm -rf "$tree"
if [[ "$rc" == 0 ]] && ! grep -q '^WARN frame-vectors' <<< "$out"; then
  echo "  ok: g14 arm 4: task-15 state passes with NO banner"
  PASS=$((PASS+1))
else
  echo "  FAIL: g14 arm 4 — exit $rc, banner present: $(grep -c '^WARN frame-vectors' <<< "$out")"
  FAIL=$((FAIL+1))
fi

# --- The banner's PREFIX, pinned here -----------------------------------------
# run-guards.test.sh pins that the RUNNER surfaces `^WARN ` — but its stub guard
# is synthetic, so nothing there pins that THIS guard actually emits the prefix.
# Without this case, editing the banner's wording and losing `WARN ` would leave
# every test green while the banner silently vanished: the original defect
# wearing a different hat, landing on whoever's devloop is next.
tree="$(new_tree)"
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
[[ "$FAIL" -eq 0 ]] || exit 1
exit 0
