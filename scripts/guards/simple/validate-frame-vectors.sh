#!/usr/bin/env bash
#
# Frame-v2 cross-language vector drift guard (ADR-0036 §2, story task 8).
#
# Pins `proto/test-vectors/frame-v2.vectors.json` against the Rust codec's
# declarations and against the vendored external anchor. Pure jq + sed: it never
# invokes cargo or pnpm, because Layer 3 gives each guard 30s with a hard
# --kill-after and Layer 3 + Layer 6 share a 90s p95. All codec conformance is
# Layer 4, where the cost is already budgeted.
#
# Emits NO `STATUS=` line: run-guards.sh owns STATUS emission, tee_collect_statuses
# would swallow one from here, and an unrecognised enum maps to exit 2.
#
# Exit 0 = pass, 1 = violation. Every check has a named PRECONDITION bail and
# there is NO skip branch anywhere: a guard that greps, finds nothing, compares
# nothing and reports success is the exact bug it was written to prevent.
#
# ---------------------------------------------------------------------------
# THREE INVARIANTS IN g14 THAT FAIL SILENTLY IF "SIMPLIFIED". Read before editing.
# ---------------------------------------------------------------------------
# 1. THE CODEC LIST IS DECLARED, NEVER DISCOVERED. A runner that iterates over
#    the codecs it *finds* reports success when it finds none: the absent side
#    is indistinguishable from the passing side, because both produce zero
#    failures. See commit 8bba6da, whose generalised lesson is that "any
#    assertion whose cost scales with the artifact it guards weakens on
#    precisely the inputs that most need checking, and does so quietly, because
#    the failure mode is an empty result rather than a wrong one."
# 2. THE `WARN ` PREFIX IS LOAD-BEARING, NOT FORMATTING. run-guards.sh's exit-0
#    arm surfaces only `^WARN ` lines; without the prefix the banner is captured
#    into $OUTPUT and discarded, and g14 becomes a silent pass — the exact
#    failure class invariant 1 cites.
# 3. PASSING-WITH-A-BANNER IS DELIBERATE. Do NOT make it exit nonzero until the
#    TypeScript leg lands (story task 15). g14 visibly *knows* the cross-language
#    property is unestablished and passes anyway; redding every devloop until
#    then would make DELETING THE GUARD the fastest route to green, inverting
#    the non-suppressibility argument rather than serving it. There is
#    deliberately no env var, flag or config key that suppresses the banner.
#
# Runbook: docs/runbooks/devloop-validation.md §6.3.
set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Deliberately does NOT source ../common.sh. An earlier draft did, and used none
# of its helpers — a dead `source ... 2>/dev/null || true` whose disappearance
# would have gone unnoticed, while the plan claimed the helpers were being
# reused. This guard is self-contained by nature (pure jq + sed with its own
# `fail`/`precondition`), so the honest state is no dependency at all.

# DEVLOOP_TEST-gated root seam, so the self-test can drive every branch against
# synthetic trees without touching the repository.
ROOT="${FRAME_VECTORS_ROOT:-$(cd "${SCRIPT_DIR}/../../.." && pwd)}"

VECTORS="${ROOT}/proto/test-vectors/frame-v2.vectors.json"
EXTERNAL_DIR="${ROOT}/proto/test-vectors/external/sframe-wg"
FRAME_RS="${ROOT}/crates/media-protocol/src/frame.rs"
CODEC_RS="${ROOT}/crates/media-protocol/src/codec.rs"
GEN_CARGO="${ROOT}/crates/media-vector-gen/Cargo.toml"
WS_CARGO="${ROOT}/Cargo.toml"
TODO="${ROOT}/docs/TODO.md"

VIOLATIONS=0

fail() { echo "VIOLATION: $*"; VIOLATIONS=$((VIOLATIONS + 1)); }

# Every precondition exits NONZERO. There is no branch that prints a warning and
# returns success: a missing input means the guard compared nothing, which must
# never read as a pass.
precondition() {
  echo "ERROR: PRECONDITION [$1] $2" >&2
  exit 1
}

command -v jq >/dev/null 2>&1 || precondition "jq-missing" \
  "jq is required. It is installed at infra/devloop/Dockerfile:31 and present on ubuntu-latest, \
so absence is an image regression, not a diff defect. Without this bail it would surface as exit \
127 classified as a violation, pointing triage at the diff."
command -v sha256sum >/dev/null 2>&1 || precondition "sha256sum-missing" \
  "sha256sum is required to verify the vendored external anchor."

# --- g1: file exists, parses, carries the non-production banner --------------
[[ -f "$VECTORS" ]] || precondition "vectors-missing" "$VECTORS does not exist"
jq -e . "$VECTORS" >/dev/null 2>&1 || precondition "vectors-unparseable" "$VECTORS is not valid JSON"
banner="$(jq -r '._non_production // ""' "$VECTORS")"
[[ -n "$banner" ]] || precondition "banner-missing" \
  "_non_production is absent or empty. It is the emitted label for the NON-PRODUCTION generator; \
removing it must red rather than pass."
jq -e '.schema_version' "$VECTORS" >/dev/null 2>&1 || precondition "vectors-unparseable" \
  "schema_version is absent"

# --- Rust declarations, extracted with named bails ---------------------------
rust_const() {
  local name="$1" value
  value="$(sed -n "s/^pub const ${name}: [A-Za-z0-9_]* = \([0-9_]*\);.*/\1/p" "$FRAME_RS" | head -1 | tr -d '_')"
  [[ -n "$value" ]] || precondition "rust-const-not-found" \
    "could not extract \`${name}\` from ${FRAME_RS}. The declaration was renamed or reshaped; \
fix the extraction rather than deleting the check, or this guard compares nothing and reports success."
  echo "$value"
}

# g2: MAX_PAYLOAD_BYTES is the cross-language SSoT.
rust_max="$(rust_const MAX_PAYLOAD_BYTES)"
json_max="$(jq -r '.max_payload_bytes' "$VECTORS")"
[[ "$rust_max" == "$json_max" ]] || fail \
  "g2 max_payload_bytes: vectors say ${json_max}, crates/media-protocol/src/frame.rs says ${rust_max}"

# g3: header_version == PROTOCOL_VERSION (TODO item (a)'s drift-guard clause).
rust_ver="$(rust_const PROTOCOL_VERSION)"
json_ver="$(jq -r '.header_version' "$VECTORS")"
[[ "$rust_ver" == "$json_ver" ]] || fail \
  "g3 header_version: vectors say ${json_ver}, PROTOCOL_VERSION is ${rust_ver}"

# g4: legal_flag_mask == the OR of the three flag constants.
flag_or=0
for f in FLAG_INDEPENDENTLY_DECODABLE FLAG_DISCARDABLE FLAG_KEY_BEARING; do
  bits="$(sed -n "s/^pub const ${f}: u8 = 0b\([01_]*\);.*/\1/p" "$FRAME_RS" | head -1 | tr -d '_')"
  [[ -n "$bits" ]] || precondition "rust-flag-consts-not-found" \
    "could not extract \`${f}\` from ${FRAME_RS}"
  flag_or=$(( flag_or | 2#${bits} ))
done
json_mask="$(jq -r '.wire_constants.legal_flag_mask' "$VECTORS")"
[[ "$flag_or" == "$json_mask" ]] || fail \
  "g4 legal_flag_mask: vectors say ${json_mask}, the three flag constants OR to ${flag_or}"

# --- g5: codec tokens EQUAL ALL_REJECT_REASONS ------------------------------
# Equality, not subset. Subset stays green when a variant is DELETED from the
# reject_reasons! macro; only equality makes an omission unrepresentable across
# the language boundary, which is what the macro achieves within Rust.
# Scoped to has_vector==true because no_transmit_key is codec-family but
# receiver-state-dependent, hence deliberately not a Rust codec variant.
macro_tokens="$(sed -n 's/^\s*[A-Za-z]* => "\([a-z_]*\)",.*/\1/p' "$CODEC_RS" | sort -u)"
[[ -n "$macro_tokens" ]] || precondition "reject-macro-not-found" \
  "could not extract any token from the reject_reasons! block in ${CODEC_RS}"
json_codec_tokens="$(jq -r '[.reject_reasons[]|select(.layer=="codec" and .has_vector)|.token]|sort|.[]' "$VECTORS")"
if [[ "$macro_tokens" != "$json_codec_tokens" ]]; then
  fail "g5 codec reject tokens differ from ALL_REJECT_REASONS (equality, not subset)"
  diff <(echo "$macro_tokens") <(echo "$json_codec_tokens") | sed 's/^/  /' || true
fi

# --- g5b / g6: token hygiene -------------------------------------------------
row_count="$(jq -r '.vectors|length' "$VECTORS")"
[[ "$row_count" -gt 0 ]] || precondition "zero-rows" "the vectors array is empty"

while IFS=" " read -r tok has_vec; do
  used="$(jq -r --arg t "$tok" '[.vectors[]|select(.reject_reason==$t)]|length' "$VECTORS")"
  if [[ "$has_vec" == "true" && "$used" -eq 0 ]]; then
    fail "g5b token '${tok}' claims has_vector but no row asserts it"
  fi
  if [[ "$has_vec" == "false" && "$used" -ne 0 ]]; then
    fail "g5b token '${tok}' claims has_vector:false but ${used} row(s) assert it"
  fi
  [[ "$tok" =~ ^[a-z][a-z0-9_]*$ ]] || fail "g6 token '${tok}' does not match ^[a-z][a-z0-9_]*$"
done < <(jq -r '.reject_reasons[]|"\(.token) \(.has_vector)"' "$VECTORS")

dupes="$(jq -r '[.reject_reasons[].token]|group_by(.)|map(select(length>1))|flatten|.[]' "$VECTORS")"
[[ -z "$dupes" ]] || fail "g6 duplicate reject tokens: ${dupes}"
name_dupes="$(jq -r '[.vectors[].name]|group_by(.)|map(select(length>1))|flatten|.[]' "$VECTORS")"
[[ -z "$name_dupes" ]] || fail "g6 duplicate row names: ${name_dupes}"

# --- g7/g8/g9: per-row span relations, in ONE jq program --------------------
# One jq invocation for all rows, not one spawn per field per row: process
# spawns dominate a bash+jq guard, and per-field spawning is the shape that
# walks into a 124 as the row count grows.
#
# Key/ciphertext-valued fields are never echoed — offsets and lengths only — so
# key-shaped material never reaches CI logs, which travel further than the repo
# because they get pasted into tickets.
span_problems="$(jq -r '
  .vectors[] as $r
  | [ if ($r.derived.signed_input_hex | startswith($r.derived.aead_aad_hex) | not)
      then "g7 \($r.name): aead_aad_hex is not a prefix of signed_input_hex" else empty end,
      if ($r.derived.aead_aad_hex | length) >= ($r.derived.signed_input_hex | length)
      then "g7 \($r.name): aead_aad_hex is not a STRICT subset of signed_input_hex (payload unsigned?)" else empty end,
      if (($r.derived.aead_aad_hex|length)/2) != $r.offsets.publisher_region_len
      then "g7 \($r.name): aead_aad_hex length \((($r.derived.aead_aad_hex|length)/2)) != publisher_region_len \($r.offsets.publisher_region_len)" else empty end,
      if $r.offsets.relay_region_offset != $r.offsets.publisher_region_len
      then "g7 \($r.name): relay region does not begin where the publisher region ends" else empty end,
      if $r.offsets.payload_offset <= $r.offsets.relay_region_offset
      then "g7 \($r.name): payload must follow the relay region" else empty end,
      if $r.offsets.signature_offset <= $r.offsets.payload_offset
      then "g7 \($r.name): signature must follow the payload" else empty end,
      if $r.derived.naive_contiguous_signed_input_hex == $r.derived.signed_input_hex
      then "g9 \($r.name): the pinned near-miss coincides with signed_input_hex and is vacuous" else empty end,
      ( $r | [paths(type=="string")] as $p | empty ),
      ( [$r.frame_hex, $r.derived.signed_input_hex, $r.derived.aead_aad_hex,
         $r.derived.sframe_nonce_hex, $r.derived.signature_hex, $r.crypto.kek_hex]
        | map(select((test("^[0-9a-f]*$") | not) or (length % 2 != 0)))
        | if length > 0 then "g8 \($r.name): a hex field is not lowercase unprefixed even-length" else empty end )
    ] | .[]' "$VECTORS")"
if [[ -n "$span_problems" ]]; then
  while IFS= read -r line; do fail "$line"; done <<< "$span_problems"
fi

# --- g7b: `derived` must describe the bytes in `frame_hex` ---------------------
# g7 above checks the derived block's internal consistency and its LENGTH against
# publisher_region_len. It never compared it to the frame. A row could therefore
# carry spans belonging to a different frame and pass everything — which is what
# happened: the six mutated rows whose frame no longer decodes keep the base
# frame's spans, differing by exactly the mutated byte.
#
# That is correct behaviour (a frame that fails to decode has no publisher region
# and no meaningful AAD) but it must be DECLARED, not inferred. So the exemption
# is per row via `expected.derived_describes`, never scoped by `kind`: a future
# roundtrip row that diverges reds instead of being covered by its neighbours'
# exemption. The declaration is **block-scoped** — it covers the whole `derived`
# block, not just the AAD span. An earlier span-scoped reading let
# `tamper_publisher_region` be honest about its AAD and silent about its nonce.
#
# The STALENESS arm below is `kind`-scoped, and that is a deliberate exception to
# the per-row principle rather than a lapse. Two decode_reject rows
# (`decode_reject_truncated`, `decode_reject_trailing_bytes`) declare the
# exemption while their spans happen to match, because their mutations fall
# outside the publisher region. Those declarations are vacuous but not wrong —
# the frames genuinely do not decode — so flagging them would demand removing a
# true statement. The `kind` scope buys exactly that exemption and nothing wider:
# on any row that DOES decode, a stale declaration still reds.
derived_problems="$(jq -r '
  .vectors[] as $r
  | ($r.frame_hex[0:($r.offsets.publisher_region_len * 2)]) as $from_wire
  | (($r.expected // {}) | .derived_describes? // "") as $declared
  | if $from_wire == $r.derived.aead_aad_hex then
      if $declared == "base_frame_before_mutation" and ($r.kind != "decode_reject") then
        "g7b \($r.name): declares derived_describes but its spans DO match frame_hex; the declaration is stale"
      else empty end
    else
      if $declared == "base_frame_before_mutation" then empty
      else
        "g7b \($r.name): aead_aad_hex does not match frame_hex[0..publisher_region_len] and the row does not declare expected.derived_describes = base_frame_before_mutation"
      end
    end' "$VECTORS")"
if [[ -n "$derived_problems" ]]; then
  while IFS= read -r line; do fail "$line"; done <<< "$derived_problems"
fi
jq -e '._comment_derived_on_reject_rows | length > 0' "$VECTORS" >/dev/null 2>&1 \
  || precondition "derived-note-missing" \
  "_comment_derived_on_reject_rows is absent. A consumer reading the most authoritative-looking \
field in the repo must be told that on non-decoding rows the derived block describes the base frame."

# g8: frame length must equal signature_offset + signature_bytes.
sig_bytes="$(jq -r '.wire_constants.signature_bytes' "$VECTORS")"
# Scoped to rows that are COMPLETE frames. A decode_reject row is mutated
# precisely so that it is not one — decode_reject_truncated is short by design
# and decode_reject_trailing_bytes is long by design, so asserting the identity
# over them would red on the rows working as intended.
len_problems="$(jq -r --argjson sig "$sig_bytes" '
  .vectors[] | select(.kind != "decode_reject")
  | select((.frame_hex|length)/2 != (.offsets.signature_offset + $sig))
  | "g8 \(.name): frame is \((.frame_hex|length)/2) bytes, expected signature_offset(\(.offsets.signature_offset)) + \($sig)"' "$VECTORS")"
if [[ -n "$len_problems" ]]; then
  while IFS= read -r line; do fail "$line"; done <<< "$len_problems"
fi

# --- g13: the crypto block is EXACTLY partitioned ---------------------------
# Not a name-suffix scan. The file is full of `_hex` fields that are derived
# crypto outputs which MUST look random (sframe_key_hex, signature_hex), so a
# suffix predicate would demand synthetic structure from them and defeat the
# external gate; a narrower suffix would silently miss a future secret.
jq -e '.vectors[0].crypto' "$VECTORS" >/dev/null 2>&1 || precondition "crypto-block-missing" \
  "no crypto block on the first row"
unclassified="$(jq -r '
  (.key_material_fields + .derived_public_fields) as $c
  | .vectors[0].crypto | keys[] | select(. as $k | ($c | index($k)) | not)' "$VECTORS")"
[[ -z "$unclassified" ]] || precondition "crypto-field-unclassified" \
  "crypto field(s) in neither key_material_fields nor derived_public_fields: ${unclassified}. \
Classify explicitly — silence must not be a route around the pattern check."
both="$(jq -r '.key_material_fields - (.key_material_fields - .derived_public_fields) | .[]' "$VECTORS")"
[[ -z "$both" ]] || fail "g13 crypto field(s) classified as BOTH secret and public: ${both}"

# g13(iii): name-segment vocabulary predicate. Generalises to fields that do not
# exist yet, which the explicit pin below cannot.
mis="$(jq -r '
  .derived_public_fields[]
  | select(. | split("_") | any(. == "key" or . == "secret" or . == "seed" or . == "kek"))' "$VECTORS")"
[[ -z "$mis" ]] || fail \
  "g13 field(s) named as key material but classified derived-public: ${mis}"

# g13(iv): explicit pin. Survives a rename, which the predicate cannot.
for f in kek_hex transmit_key_hex identity_private_seed_hex; do
  jq -e --arg f "$f" '.key_material_fields | index($f)' "$VECTORS" >/dev/null 2>&1 || fail \
    "g13 '${f}' must be in key_material_fields; reclassifying it would skip the synthetic-pattern check"
done

# g13(ii): the synthetic-pattern predicate itself. Each key material value must
# be a run of consecutive bytes — structurally, not an allowlist of today's
# values, so it survives adding a row and fires on exactly the change that
# matters: someone regenerating with real CSPRNG material.
# Consecutive-run check in bash: each key-material value must satisfy
# byte[i] == (byte[0] + i) mod 256. Values are synthetic fixtures, so printing
# a FAILURE mentions the field name and length only, never the bytes.
while IFS=" " read -r rowname field value; do
  first="$(( 16#${value:0:2} ))"
  n=$(( ${#value} / 2 ))
  ok=1
  for (( i=0; i<n; i++ )); do
    got="$(( 16#${value:$((i*2)):2} ))"
    want="$(( (first + i) % 256 ))"
    [[ "$got" -eq "$want" ]] || { ok=0; break; }
  done
  [[ "$ok" -eq 1 ]] || fail \
    "g13 ${rowname}.${field} (${n} bytes) is not the declared synthetic pattern — it looks like \
real key material. Values are NOT printed here on purpose."
done < <(jq -r '.key_material_fields as $kf | .vectors[] as $r | $kf[] as $f | "\($r.name) \($f) \($r.crypto[$f])"' "$VECTORS")

# --- g10 / g15: the vendored external anchor --------------------------------
[[ -d "$EXTERNAL_DIR" ]] || precondition "external-vectors-missing" "$EXTERNAL_DIR does not exist"
[[ -f "${EXTERNAL_DIR}/test-vectors.json" ]] || precondition "external-vectors-missing" \
  "${EXTERNAL_DIR}/test-vectors.json does not exist"
[[ -f "${EXTERNAL_DIR}/manifest.json" ]] || precondition "manifest-missing" \
  "${EXTERNAL_DIR}/manifest.json does not exist"

recorded="$(sed -n 's/.*| SHA-256 of vendored bytes | `\([0-9a-f]\{64\}\)` |.*/\1/p' \
  "${EXTERNAL_DIR}/PROVENANCE.md" | head -1)"
[[ -n "$recorded" ]] || precondition "provenance-digest-not-found" \
  "could not extract the recorded SHA-256 from ${EXTERNAL_DIR}/PROVENANCE.md"
actual="$(sha256sum "${EXTERNAL_DIR}/test-vectors.json" | cut -d' ' -f1)"
[[ "$recorded" == "$actual" ]] || fail \
  "g10 vendored external vectors digest mismatch: PROVENANCE.md records ${recorded}, file is ${actual}"

# g15: selector anti-vacuity. A valid digest must not imply a valid selection —
# a different failure surface, hence its own check rather than a clause in g10.
suites="$(jq -r '.cipher_suites[]?' "${EXTERNAL_DIR}/manifest.json")"
[[ -n "$suites" ]] || precondition "suite-list-empty" \
  "manifest.cipher_suites is empty or absent: both gates would iterate zero rows and pass"
for s in $suites; do
  n="$(jq -r --argjson s "$s" '[.sframe[]|select(.cipher_suite==$s)]|length' "${EXTERNAL_DIR}/test-vectors.json")"
  # Per-suite, NEVER "at least one row overall": the aggregate form passes when
  # 0x0005 vanishes upstream and 0x0004 survives, which is exactly the
  # degradation this anchor exists to catch.
  [[ "$n" -ge 1 ]] || precondition "suite-yields-zero-rows" \
    "manifest selects cipher_suite ${s} but the vendored file has no such row"
done

# --- g11: the generator crate must exist and be a workspace member ----------
# The mechanical protection against a later cleanup deleting it as dead Rust
# crypto. It is the ONLY independent check on the TypeScript crypto.
[[ -f "$GEN_CARGO" ]] || precondition "generator-crate-missing" \
  "crates/media-vector-gen/Cargo.toml is absent. It is deliberately non-production but must NOT \
be deleted: no production Rust component seals, signs, verifies or decrypts a media frame, so it \
is the only independent check on the TypeScript crypto."
grep -q '"crates/media-vector-gen"' "$WS_CARGO" || precondition "generator-crate-missing" \
  "crates/media-vector-gen is not in the workspace members list, so the pipeline does not compile it"

# --- g14: declared codec list, never discovered -----------------------------
declared_codecs=("rust" "typescript")
established="$(jq -r '.cross_language_property_established' "$VECTORS")"
all_gated=true
for codec in "${declared_codecs[@]}"; do
  # `has()`, never `//`: jq's alternative operator treats FALSE as absent, and
  # `gated_by.typescript` is legitimately false — `// "MISSING"` would report a
  # missing key on the one value the check exists to read.
  jq -e --arg c "$codec" '.gated_by | has($c)' "$VECTORS" >/dev/null 2>&1 || precondition "gated-by-key-missing" \
    "gated_by.${codec} is absent. The codec list is declared, not discovered, so a missing key \
is a wiring fault rather than a codec that happens not to exist yet."
  gated="$(jq -r --arg c "$codec" '.gated_by[$c]' "$VECTORS")"
  if [[ "$gated" == "true" ]]; then
    case "$codec" in
      rust)   marker="${ROOT}/crates/media-vector-gen/tests/rust_codec_conformance.rs" ;;
      typescript) marker="${ROOT}/packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts" ;;
      *) precondition "codec-list-undeclared" "no conformance marker declared for codec ${codec}" ;;
    esac
    [[ -f "$marker" ]] || fail \
      "g14 gated_by.${codec} is true but its conformance marker ${marker} does not exist"
  else
    all_gated=false
    # The FULL marker, not a substring: matching `frame-vectors-ts-leg` loosely
    # would still hit `frame-vectors-ts-leg-MOVED`, so a renamed anchor would
    # pass. The self-test drives exactly that case.
    grep -qF -- '<!-- frame-vectors-ts-leg -->' "$TODO" 2>/dev/null || precondition "todo-tracking-entry-missing" \
      "gated_by.${codec} is false but docs/TODO.md carries no <!-- frame-vectors-ts-leg --> marker. \
The tracking anchor moved or was removed; restore it or update this guard. Note the guard matches \
the MARKER, not the checkbox, so /close-story ticking the entry [x] satisfies it."
    echo "WARN frame-vectors: cross-language property NOT established — codec '${codec}' is ungated; closer is story task 15 (docs/TODO.md, marker frame-vectors-ts-leg)"
  fi
done
if [[ "$all_gated" == "true" && "$established" != "true" ]]; then
  fail "g14 every codec is gated but cross_language_property_established is ${established}"
fi
if [[ "$all_gated" == "false" && "$established" == "true" ]]; then
  fail "g14 cross_language_property_established claims true while a codec is ungated"
fi

# --- g12: the TypeScript codec must READ the vectors, not hardcode them ------
#
# The TS half of g2/g3/g4. Those three enforce that the vectors agree with the
# Rust constants; nothing else enforces the other direction, and
# `crates/media-protocol/src/frame.rs` promises it in terms: "the TypeScript
# codec derives its mask from that file rather than hardcoding one."
#
# Implemented NOW rather than deferred to task 15. It is inert **by declaration**
# — `gated_by.typescript` is false today, so the loop below does not run — which
# is a different thing from inert by absence: when task 15 flips the flag this
# check begins running, whereas an absent check would simply stay absent with
# nothing red to force its construction. The site list was co-agreed during this
# planning session and is not knowable later. Both branches are driven by the
# self-test through the FRAME_VECTORS_ROOT seam.
if [[ "$(jq -r '.gated_by.typescript' "$VECTORS")" == "true" ]]; then
  ts_sites=(
    "packages/sdk-core/src/media/frame/frameCodec.ts"
    "packages/sdk-core/src/media/frame/sframe.ts"
  )
  # TWO checks, because they are different claims and neither implies the other.
  #
  # (1) Negative: literals with a cross-language SSoT must not be hardcoded.
  # (2) Positive: the site must demonstrably READ the vectors.
  #
  # (2) exists because `legal_flag_mask` is **7**, and `grep -F -- "7"` against a
  # TypeScript file matches essentially every line — the flag mask is not
  # expressible as a banned literal at all. It is also the constant
  # `crates/media-protocol/src/frame.rs` names in terms ("the TypeScript codec
  # derives its mask from that file rather than hardcoding one"), so leaving it
  # to (1) would have meant the one promise made explicitly in Rust was the one
  # not enforced.
  #
  # (2) also strengthens the other two: absence-of-bad-literal and
  # presence-of-real-read are different claims, and a file can satisfy (1) while
  # hardcoding `1024 * 1024`.
  banned_literals=("1048576" "1_048_576" "0x0005")
  for site in "${ts_sites[@]}"; do
    [[ -f "${ROOT}/${site}" ]] || precondition "ts-site-not-found" \
      "gated_by.typescript is true but declared site ${site} does not exist. The list is \
ENUMERATED, not globbed: a renamed file must red here rather than silently drop out of the check."
    for lit in "${banned_literals[@]}"; do
      grep -qF -- "$lit" "${ROOT}/${site}" && fail \
        "g12 ${site} hardcodes '${lit}'; it must read the value from ${VECTORS#"${ROOT}/"}"
    done
    grep -qE 'frame-v2\.vectors|legal_flag_mask|max_payload_bytes|cipher_suite_id' \
      "${ROOT}/${site}" || fail \
      "g12 ${site} never references the vectors. The flag mask is 7 and cannot be checked as a \
banned literal, so the enforcement for it is that the site demonstrably reads the SSoT."
  done
fi

# --- g16: crypto-token spelling, both directions ----------------------------
# Typo coverage is 2 of 5 by construction and the file says so: the ABSENCE arm
# is satisfied more easily by a typo than by the correct spelling, so for
# fleet_spelling:null tokens this cannot fire on a misspelling. Those rest on
# Gate-3 human review until the task-22 catalog cross-reference.
#
# Absence is scoped to EMISSION SITES, not raw file occurrences: a naive
# `grep -r` would hit the story file's own task prompts and someone would
# "fix" it by weakening the check.
emission_paths=("${ROOT}/docs/observability/metrics" "${ROOT}/crates")
while IFS=" " read -r tok fleet anchor; do
  if [[ "$fleet" != "null" ]]; then
    found=0
    for p in "${emission_paths[@]}"; do
      [[ -d "$p" ]] || continue
      grep -rqw "$tok" "$p" --include='*.rs' --include='*.md' 2>/dev/null && { found=1; break; }
    done
    [[ "$found" -eq 1 ]] || fail \
      "g16 token '${tok}' declares fleet_spelling '${fleet}' but does not appear at any emission site — suspect a typo"
  fi
  if [[ "$anchor" != "null" ]]; then
    [[ -f "${ROOT}/${anchor}" ]] || precondition "spec-anchor-not-found" \
      "token '${tok}' declares spec_anchor '${anchor}' which does not exist"
    grep -qw "$tok" "${ROOT}/${anchor}" || fail \
      "g16 token '${tok}' does not appear verbatim in its declared spec_anchor ${anchor}"
  fi
done < <(jq -r '.reject_reasons[]|select(.layer!="codec")|"\(.token) \(.fleet_spelling // "null") \(.spec_anchor // "null")"' "$VECTORS")

if [[ "$VIOLATIONS" -gt 0 ]]; then
  echo ""
  echo "${VIOLATIONS} frame-vector violation(s). Regenerate with:"
  echo "    cargo run -p media-vector-gen --bin generate-frame-vectors"
  echo "Do NOT hand-edit proto/test-vectors/frame-v2.vectors.json: a vector that contradicts a"
  echo "recorded ruling is a defect to escalate, not to conform to (proto/test-vectors/README.md)."
  exit 1
fi
exit 0
