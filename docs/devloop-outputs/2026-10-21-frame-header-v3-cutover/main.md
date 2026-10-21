# Devloop Output: Frame header v3 cutover (Story 3 P2)

**Date**: 2026-10-21
**Task**: Frame header v3 cutover — Rust codec v3-only + cut-through header-prefix parse/reader/rewrite + fuzz, v3 vectors from independent generator, SDK v3-only + audio GoP counter, v2 deleted atomically
**Specialist**: protocol
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/user-see-each-other`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `ff2fed55f12c8bf76c99bd80ab1393a34b68d785` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `protocol` |
| Tier | `full` |
| Iteration | `1` |
| Paired | `paired-client` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Media Handler (GSA co-sign, crates/media-protocol) | `media-handler` |

---

## Task Overview

### Objective
See task prompt (run-story task #6, story `docs/user-stories/2026-10-07-see-each-other.md`): the atomic v2→v3 frame header flip.

### Scope
- **Service(s)**: media-protocol, media-vector-gen, sdk-core, MC/MH test fixtures, guards, docs
- **Schema**: No
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED - ADR-0036 §2 already normative (task P1).

---

## Cross-Boundary Classification

| Path | Classification | Owner | Note |
|------|----------------|-------|------|
| `.github/workflows/fuzz-nightly.yml` | Mechanical | infrastructure | two new fuzz matrix entries (drift-checked by tests/fuzz_wiring.rs) |
| `Cargo.lock` | Minor-judgment | security | lockfile-only patch bumps forced by Layer 6 cargo-audit: RUSTSEC-2026-0381/0382/0383 (published 2026-10-19, pre-existing on the base commit, unrelated to v3): rustls-webpki 0.103.15->0.103.16, webpki-roots and webpki-root-certs 1.0.4->1.0.10; no manifest change |
| `crates/dt-guard/src/client_metrics_export.rs` | Minor-judgment | observability | VECTORS_JSON rename; six NOT_KEY_DELIVERY entries (security co-signed content) |
| `crates/dt-guard/tests/client_metrics_export_fixtures.rs` | Minor-judgment | observability | VECTORS_JSON rename; six NOT_KEY_DELIVERY entries (security co-signed content) |
| `crates/env-tests/Cargo.toml` | Mechanical | test | gop_counter in fixture parts; v3 wording |
| `crates/env-tests/src/fixtures/media.rs` | Mechanical | test | gop_counter in fixture parts; v3 wording |
| `crates/media-protocol/fuzz/Cargo.toml` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 fuzz targets incl. header_prefix, header_reader_split |
| `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 fuzz targets incl. header_prefix, header_reader_split |
| `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 fuzz targets incl. header_prefix, header_reader_split |
| `crates/media-protocol/fuzz/fuzz_targets/header_prefix.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 fuzz targets incl. header_prefix, header_reader_split |
| `crates/media-protocol/fuzz/fuzz_targets/header_reader_split.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 fuzz targets incl. header_prefix, header_reader_split |
| `crates/media-protocol/src/codec.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 layout, gop_counter, key_bearing_discardable, header-prefix parse / header-slice rewrite / chunk-fed HeaderReader |
| `crates/media-protocol/src/extensions.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 layout, gop_counter, key_bearing_discardable, header-prefix parse / header-slice rewrite / chunk-fed HeaderReader |
| `crates/media-protocol/src/frame.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 layout, gop_counter, key_bearing_discardable, header-prefix parse / header-slice rewrite / chunk-fed HeaderReader |
| `crates/media-protocol/src/lib.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 layout, gop_counter, key_bearing_discardable, header-prefix parse / header-slice rewrite / chunk-fed HeaderReader |
| `crates/media-protocol/src/reader.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 layout, gop_counter, key_bearing_discardable, header-prefix parse / header-slice rewrite / chunk-fed HeaderReader |
| `crates/media-protocol/tests/byte_coverage.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/common/mod.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/common/seeds.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/corpus_seeds.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/frame_properties.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/fuzz_wiring.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/header_prefix.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/no_alloc.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-protocol/tests/reject_reasons.rs` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. v3 tests: byte coverage, split invariance, counting allocator, fuzz wiring |
| `crates/media-vector-gen/src/fixtures.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/inventory.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/json.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/lib.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/main.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/mutate.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/rows.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/src/spec.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/tests/rust_codec_conformance.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/media-vector-gen/tests/vectors_are_current.rs` | Domain-judgment | protocol | independent v3 generator from ADR §2; new rows; reject_reasons entries |
| `crates/mh-service/src/config.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/src/media/forward.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/src/media/forwarder.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/src/observability/metrics.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/src/transport/mod.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/tests/common/media_frame.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/tests/common/mod.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `crates/mh-service/tests/media_metrics_integration.rs` | Mechanical | media-handler | v3 prose/figures, gop_counter fixture, vectors path, collision pin |
| `docs/FUZZ_THIS_CHECKLIST.md` | Mechanical | test | new fuzz targets listed; peek_frame_len removed |
| `docs/decisions/adr-0036-media-flow.md` | Minor-judgment | protocol | live-text rename; v3 current |
| `docs/observability/alerts.md` | Minor-judgment | observability | count-free wording, new tokens, rename |
| `docs/observability/label-taxonomy.md` | Minor-judgment | observability | count-free wording, new tokens, rename |
| `docs/observability/metrics/client.md` | Minor-judgment | observability | count-free wording, new tokens, rename |
| `docs/observability/metrics/mh-service.md` | Minor-judgment | observability | count-free wording, new tokens, rename |
| `docs/runbooks/client-dev-local.md` | Minor-judgment | operations | rename, current-state wording, re-measured guard runtime |
| `docs/runbooks/mh-deployment.md` | Minor-judgment | operations | buffer row figures 240 B / ~87 s (media-handler review F2) |
| `docs/runbooks/devloop-validation.md` | Minor-judgment | operations | rename, current-state wording, re-measured guard runtime |
| `infra/docker/prometheus/rules/mc-alerts.yaml` | Mechanical | observability | comment path rename only |
| `infra/services/otel-collector/collector.yaml` | Minor-judgment | observability | worst-case stream arithmetic recompute (comment) |
| `packages/sdk-core/scripts/gen-wire-constants.mjs` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/index.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/external-anchor.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/frameCodec.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/frameCodec.v3.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/frameVectors.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/receivePathIntegration.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/rejectReason.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/vectors.conformance.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/__tests__/wireConstants.drift.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/frameCodec.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/hex.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/rejectReason.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/sframe.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/frame/wireConstants.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/lifecycle/__tests__/transmitKeys.gopExhaustion.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/lifecycle/__tests__/transmitKeys.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/lifecycle/transmitKeys.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/pipeline/__tests__/egress.multiTarget.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/pipeline/__tests__/frameFixtures.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.attribution.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/pipeline/__tests__/ingress.rejectTokens.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/pipeline/egress.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/pipeline/ingress.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/media/setup/mediaMetrics.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/signaling/__tests__/mediaSignaling.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/src/signaling/events.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `packages/sdk-core/tests/bundle-content.test.ts` | Domain-judgment | client | edited by paired-client: v3-only SDK, regenerated wire constants, nextGopCounter, gop_counter_exhausted |
| `proto/dark_tower/signaling/v1/signaling.proto` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. comment-only vectors rename; no field/number change |
| `proto/test-vectors/README.md` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. rename + v3 current state |
| `proto/test-vectors/frame-v2.vectors.json` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. deleted (intentional frame-header wire break) |
| `proto/test-vectors/frame-v3.vectors.json` | Domain-judgment | protocol | GSA: media-handler co-sign, security panel. generated by media-vector-gen from ADR §2 |
| `scripts/guards/simple/validate-frame-vectors.sh` | Minor-judgment | protocol | retarget to v3 vectors; no new checks, no skip branch |
| `scripts/guards/validate-frame-vectors.test.sh` | Minor-judgment | protocol | retarget to v3 vectors; no new checks, no skip branch |
| `docs/TODO.md` | Mechanical | protocol | live path refs renamed, count-free; historical items untouched |

---

## Planning

**Restated as mechanism.** "The header gains a field" is the instance; the mechanism is *every consumer
of the frame-header layout and of the reject vocabulary must move in one change*: the one Rust parser and
its entry points, the independent generator's AAD arithmetic, the TS codec, the vectors file, every guard and
catalog that enumerates reject tokens (g5, G7 partition, MH `ALL_REJECT_REASONS` label set, label-taxonomy,
client.md, mh-service.md, runbooks), every fixture that builds `MediaFrameParts`, every doc that cites the
vectors path or a v2 byte figure. The plan below enumerates all of them; nothing is left partial.

**Split with @paired-client.** I (protocol) own `crates/**`, `proto/**`, `scripts/guards/**`, `.github/**`,
`infra/**` comment, docs. @paired-client owns `packages/**` (codec v3-only, v2 deleted, regenerated
`wireConstants.ts` via `gen-wire-constants.mjs`, `rejectReason.ts`, conformance/drift tests,
`TransmitKeyManager.nextGopCounter`, egress allocation, `gop_counter_exhausted` send-drop token). I do not
read `frameCodec.ts` while writing the generator.

### 1. Rust codec (`crates/media-protocol`), v3-only
- `frame.rs`: `GOP_COUNTER_FIELD_BYTES = 4`, `GOP_COUNTER_OFFSET`, `PUBLISHER_FIXED_PREFIX_SIZE` = 14 (sum
  incl. gop), `MAX_HEADER_BYTES` = 75 (derived), `PROTOCOL_VERSION = 3`; `Layout.gop_counter`;
  `MediaFrameView::gop_counter()`; layout table, Obligations section, hop-local relay semantics updated.
- `codec.rs`: new `RejectReason::KeyBearingDiscardable => "key_bearing_discardable"` (once, in the macro),
  checked in `parse_layout` after reserved-flag, before payload length (ADR §2 step 5);
  `EncodeError::KeyBearingDiscardable`; `MediaFrameParts.gop_counter: u32`.
- **New API, all delegating to the single `parse_layout`:**
  - `pub struct FrameHeader { flags, payload_length, stream_sequence, gop_counter, stream_id, hop_sequence,
    header_len, frame_len }` (Copy; no byte slice ever; `tail_len()` = payload_length + 64). **Hand-rolled
    Debug** printing flags and lengths only; `stream_sequence`, `gop_counter`, `stream_id`, `hop_sequence`
    redacted (gop_counter is stream identity, ADR §2/§11, R-30); unit test asserts the values do not appear.
    `HeaderPrefix`/`Feed` never carry a byte slice. `HeaderReader` Debug prints `filled`/state only. `pub enum HeaderPrefix { Incomplete { needed: usize }, Complete(FrameHeader) }`.
  - (2a) `parse_header_prefix(&[u8]) -> Result<HeaderPrefix, DecodeError>`: `parse_layout` -> Complete, or
    Incomplete with `needed = Shortfall.needed - Shortfall.available` (existing `require`/Shortfall
    machinery; region-local, so `needed <= MAX_HEADER_BYTES` and never derived from `payload_length`).
    Steps 1-6 are terminal before any Incomplete past the prefix (over-max length on a 14-byte buffer
    rejects). `needed` is region-relative: "at least this many more bytes", not "header ends after N".
    **`peek_frame_len` is deleted** (no non-test caller; superseded by `FrameHeader::frame_len`); its tests,
    fuzz use and doc mentions (FUZZ_THIS_CHECKLIST, INDEX files, mh transport doc) retargeted.
  - One builder, `FrameHeader::from_layout(data, layout)`, reads the relay fields at
    `Layout::relay_region_offset()`; `build_view`, `rewrite_relay_region` and `rewrite_relay_region_header`
    (via `write_relay`) take the relay offset from that same method. No recomputed offset anywhere.
  - (2b) `rewrite_relay_region_header(header: &mut [u8], stream_id, hop) -> Result<FrameHeader, DecodeError>`:
    re-runs `parse_layout` on the header bytes (no body needed), Incomplete -> `truncated`, writes ONLY the 6
    relay bytes. Shares one private `write_relay(buf, layout, ..)` kernel with `rewrite_relay_region`.
  - (2c) `pub struct HeaderReader { buf: [u8; MAX_HEADER_BYTES], filled, state }` with hand-rolled redacting
    Debug (the buffer holds the wrapped key). `feed(&mut self, chunk) -> Result<Feed, DecodeError>`,
    `Feed::NeedMore | Feed::Complete { header: FrameHeader, consumed: usize }`. Each step copies exactly
    `needed` bytes from the chunk (never past header end; remainder of chunk is tail, left with caller),
    then calls `parse_header_prefix` on its buffer -- it never reads a field itself. Errors are sticky until
    `reset()`; after Complete, `feed` without `reset()` returns the same Complete with `consumed: 0`
    (sticky; never starts a new header). `is_mid_header()` (filled > 0 and not complete) for MH's `partial_frame_discard`;
    `header_bytes() -> Option<&[u8]>` after Complete for the per-subscriber rewrite copy. No allocation.
  - Docs say "every public entry point", never a count.
- `MediaFrameView` Debug and `FrameHeader` Debug agree: both redact `stream_sequence` and `gop_counter`
  (and FrameHeader also the relay values); the absence unit test covers both types.
- `ProducibleBy` gains the new entry points in every arm; `Truncated` moves to a variant that also names
    `rewrite_relay_region_header`; `KeyBearingDiscardable` is `AllEntryPoints`. `operator_meaning` row added.
- Tests: `byte_coverage.rs` (gop bytes load-bearing; relay bytes outside signed ranges), `frame_properties.rs`,
  `reject_reasons.rs` (key_bearing_discardable per entry point incl. encode refusal; precedence vs
  unknown_version / reserved_flag_bit_set / payload_length_exceeds_max), new `header_prefix.rs`
  (split-invariance over every corpus seed x every 1-cut split + byte-at-a-time, full-outcome equality;
  entry-point coherence parse/peek/decode_stream_frame/reader; `needed` exactness; rewrite touches only relay
  bytes, refuses invalid header without writing, rewritten header + untouched tail decodes),
  new `no_alloc.rs` (own binary, counting `#[global_allocator]` with a **thread-local armed window**, so
  parallel libtest threads cannot pollute the count; positive control inside the same window; zero allocs for
  over-max/u32::MAX lengths and accept path), `corpus_seeds.rs` regenerated for v3.
- Reader tests also: `consumed` totals == header_len and leftover chunk bytes == tail prefix; sticky error
  until `reset()`, then a valid frame parses; `is_mid_header()` at 0, 1, header_len-1, Complete; every split
  test asserts >0 seeds with at least one accept and one reject seed.
- `tests/fuzz_wiring.rs`: every `[[bin]]` in `fuzz/Cargo.toml` appears in `fuzz-nightly.yml`'s matrix for
  `media-protocol` (and vice versa), so a later target cannot silently skip nightly.
- Fuzz: `fuzz_targets/header_prefix.rs`, `fuzz_targets/header_reader_split.rs` (reader vs one-shot vs
  decode_stream_frame, arbitrary splits incl 1-byte) + `fuzz/Cargo.toml` bins + `fuzz-nightly.yml` matrix.
  Gate evidence: `cargo +nightly fuzz build` and `-runs=N` over the seed corpus, recorded here.

### 2. Generator + vectors
- `media-vector-gen` from ADR §2 only: `spec.rs` draft gains `gop_counter`, writes it at §2's offset 10, and
  declares §2's literal 14-byte prefix, asserted against the codec constant. `VECTOR_FILE_PATH` ->
  `frame-v3.vectors.json`; tests import the constant instead of redefining the path.
- Schema (answers to @paired-client): `wire_constants` gains **`gop_counter_field_bytes`** (4) and
  **`max_header_bytes`** (75); `publisher_fixed_prefix_bytes` = 14. `decoded` gains **`gop_counter`**
  (number). One new kind: **`relay_rewrite`** (expected `{decodes:true, signature_valid:true, rewritten_from}`).
- Rows: every v2 row regenerated at v3; publisher rows carry relay `stream_id` = key-id `stream` = send id
  (audio rows 0, video-shaped rows 1); `gop_counter_zero_keyframe` (roundtrip), `gop_counter_u32_max`
  (roundtrip; comment pins the sender-side stop), `key_bearing_mid_group` (full_frame, flags 0x04, video,
  gop equal to its group's), `tamper_gop_counter` (verify_reject), `decode_reject_reserved_flag_bit_3`
  (0x08) beside the existing 0x80 row, `decode_reject_key_bearing_discardable`, `decode_reject_v2_labelled`
  (version byte 2 -> unknown_version), `relay_rewrite_still_verifies`, insider forgery regenerated.
- `reject_reasons`: `key_bearing_discardable` (codec, drops, has_vector true); `self_sender`,
  `sender_server_muted` (assignment); `decode_over_bound`, `stream_partial_reset`, `stream_slot_mismatch`
  (codec, receiver/stream-dependent); all five drops_frame true, has_vector false.
- `gated_by.typescript` / `cross_language_property_established` stay true in the generator; they are only
  truthful once @paired-client's conformance run is green against the new file -- that is a precondition of
  my "Ready for review".

### 3. Guard, catalog, alerts
- Token families (observability C1/C2, security correction): byte-determined = `key_bearing_discardable`,
  `stream_partial_reset`; receiver-state = `self_sender`, `sender_server_muted`, `decode_over_bound` (decoded
  output over the receiving slot's configured bounds, post-decrypt), `stream_slot_mismatch` (both halves:
  relay stream_id not among the receiver's declared slots, or differing between frames on one stream).
  Its NOT_KEY_DELIVERY string (security wording): "relay stream_id is not one of the receiver's declared
  slots, or differs between frames on one stream; relay/stream structure, not key delivery".
- Only `key_bearing_discardable` enters the Rust `RejectReason` enum; the five receiver tokens never do (MH
  would zero-init series it cannot emit). They join the literal non-codec list in
  `crates/mh-service/tests/media_metrics_integration.rs` (collision pin). Hardcoded "eight" counts
  (`mh-service/src/observability/metrics.rs`, `metrics/mh-service.md`, `media_metrics_integration.rs`)
  become count-free.
- `FrameHeader` doc states its sequence/gop/relay values are not telemetry (R-30). NOT_KEY_DELIVERY strings per
  observability/security sign-off.
- Client-media dashboard send_dropped panel is `sum by(reason)`, so `gop_counter_exhausted` shows without a
  panel edit.
- `validate-frame-vectors.sh` + self-test: path -> v3; g3/g8 etc unchanged in intent; no skip branch added.
- `dt-guard` `client_metrics_export.rs` `VECTORS_JSON` rename; six NOT_KEY_DELIVERY entries (reason strings
  sent to @observability/@security for content sign-off); MCMediaMissingKeyMaterial alternation unchanged.
- Docs: label-taxonomy, client.md, mh-service.md, alerts.md made count-free and include the new tokens;
  runbooks (devloop-validation re-measured guard time + row count, mh-deployment, client-dev-local); ADR live
  text; INDEX files; TODO live refs; mh-service config.rs prose/figures (236->240 B, etc).

### 4. Fixtures (MC/MH/env-tests)
- `gop_counter` added to `MediaFrameParts` at both fixture homes (`mh-service/tests/common/media_frame.rs`,
  `env-tests/src/fixtures/media.rs`), same way (0 / per-frame increment for audio). No hand-rolled offsets.
  MC: no source change (`header_version()` derives).

### 5. Audio GoP counter (@paired-client implements; I review)
- `TransmitKeyManager.nextGopCounter(stream)` -> 0..=u32::MAX then `null` forever (no wrap, no reset);
  allocated once in `egress.submit` beside `nextStreamSequence`, above fan-out; on null counts
  `dt_client_media_send_dropped_total{reason="gop_counter_exhausted"}` once per active lane, nothing sent.
  Never logged / labelled / on test bus.

## Expected Layer-State

Intentional wire break (frame header v2 -> v3), declared per `docs/protocol/CONVENTIONS.md`. Measured at layer-fast: buf-breaking passed, 0 findings, as predicted.
The break is in the **frame codec**, not in proto. `.proto` edits are comment-only (vectors path); no field,
number or `header_version` semantics change. **0 buf-breaking findings predicted.** Any Layer-6 finding is
unpredicted and is escalated.

### Rollback
Revert this whole commit + redeploy Kind + restart dev-web.sh; a partial revert (only SDK or only vectors)
recreates the mixed pair. See `docs/runbooks/mh-deployment.md` §Story 3 cutover "Rollback: three layers".

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Paired Client | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Media Handler | confirmed |

Classification-sanity guard: `STATUS=OK` (after Owner-field fix). Lead note: teammate INDEX/review-protocol injected by reference (each told to read its file in full) rather than inlined.

---

## Pre-Work

None

---

## Implementation Summary

**Rust codec (`crates/media-protocol`), v3-only.** `PROTOCOL_VERSION = 3`; `GOP_COUNTER_FIELD_BYTES`/`GOP_COUNTER_OFFSET` (10);
`PUBLISHER_FIXED_PREFIX_SIZE` = 14; `MAX_HEADER_BYTES` = 75; `MediaFrameView::gop_counter()`; `MediaFrameParts.gop_counter`.
`RejectReason::KeyBearingDiscardable` (once, in the macro) at §2 step 5; `EncodeError::KeyBearingDiscardable`. No v2 path existed in
Rust beyond the constants; none remains. Cut-through API, all on the single `parse_layout`:
- `parse_header_prefix()` -> `HeaderPrefix::{Incomplete{needed}, Complete(FrameHeader)}`; `needed` from the existing Shortfall
  machinery, region-relative, `<= MAX_HEADER_BYTES`, never from `payload_length`.
- `FrameHeader` (scalars only; `from_layout()` is the crate's one relay-region reader, at `Layout::relay_region_offset()`;
  hand-rolled Debug redacts `stream_sequence`, `gop_counter`, `stream_id`, `hop_sequence`). `MediaFrameView`/`MediaFrameParts`
  Debug redact `stream_sequence`/`gop_counter` too.
- `rewrite_relay_region_header()`; both rewrites share one `write_relay()` kernel.
- `reader::HeaderReader` (inline `[u8; 75]`, copies exactly `needed` per step, sticky Complete/Err until `reset()`,
  `is_mid_header()`, `header_bytes()`, redacting Debug printing fill + state only; `reset()` zeroes the buffer).
- `peek_frame_len` deleted (no non-test caller). `ProducibleBy` gains `CompleteHeaderRequired` (`truncated`); every arm names
  every entry point.

**Generator + vectors.** `frame-v2.vectors.json` deleted; `frame-v3.vectors.json` (32 rows) generated. `spec.rs` declares §2's
literal version 3 and 14-byte prefix, compile-time-asserted against the codec, and writes `gop_counter` at offset 10 in the AAD
derivation. Rows added per plan; publisher rows stamp relay `stream_id` = key-id stream (audio 0, video 1); the wrap-binding row's
key ids are now derived, not hardcoded. Six tokens added to `reject_reasons` (layers per plan).

**Cross-language result: ZERO disagreements.** @paired-client's TS v3 codec (written by C-v3 from §2) passed all 32 rows
(167/167 conformance tests) on first contact with the independently generated file; nothing was changed on either side to
make them agree. Mutation control: flipping bit 0 of the TS decoded `gop_counter` reds 23 conformance tests. `gated_by.typescript`
and `cross_language_property_established` stay `true` truthfully (g14 green).

**SDK (paired-client).** v3-only codec, v2 path deleted, wire constants re-rendered from the v3 file, `TransmitKeyManager.nextGopCounter`
(0..=u32::MAX then `null` forever) allocated once in `egress.submit` above the fan-out; exhaustion counts
`dt_client_media_send_dropped_total{reason="gop_counter_exhausted"}` per active lane and sends nothing.

**Guards / catalogs / docs.** frame-vectors guard + self-test retargeted (no new checks, no skip branch); dt-guard G7 path +
six NOT_KEY_DELIVERY entries (security-approved strings); MH collision pin gains the five receiver tokens; observability docs
count-free; runbooks current-state; ADR live text; INDEX files; collector worst-case arithmetic recomputed (58 / 102 / 146
streams per org; still above the 3-run trigger).

**Cargo.lock.** Layer 6 cargo-audit was red on the base commit's lockfile from three advisories published 2026-10-19
(RUSTSEC-2026-0381/0382/0383, rustls-webpki / webpki-roots / webpki-root-certs). Fixed with patch-level lockfile bumps
(`cargo update -p rustls-webpki -p webpki-roots -p webpki-root-certs`); no manifest change. @security: please confirm.

---

## Files Modified

Every changed path is a row in §Cross-Boundary Classification (exact paths; `validate-cross-boundary-scope` green).

---

## Devloop Verification Steps

- `./scripts/layer-fast.sh` (Layers 1-6): **TOTAL_RESULT=OK** (L1 31s, L2 4s, L3 144s, L4 234s, L5 15s, L6 5s).
- Layer 6 buf-breaking: **passed, 0 findings** — matches the declared Expected Layer-State.
- Fuzz (cargo-fuzz 0.13.2 from `infra/cargo-tools.versions`): `cd crates/media-protocol && cargo +nightly fuzz build` builds all
  four targets; seed corpus from `cargo test -p media-protocol --test corpus_seeds` (578 seeds per target), then
  `cargo +nightly fuzz run <t> fuzz/corpus/<t> -- -runs=200000` for `codec_decode`, `codec_roundtrip`, `header_prefix`,
  `header_reader_split`: each `Done 200000 runs`, no crash.
- Frame-vectors guard: 0.789 / 0.803 / 0.791 s at 32 rows; self-test 41/41 in 28.4 s.
- Sweep greps (all empty; `--untracked` so the new files are covered — re-run after review, test F4):
  - `git grep --untracked -n "frame-v2\.vectors" -- ':!docs/devloop-outputs' ':!docs/user-stories' ':!docs/runbooks/devloop-validation.md'`
    (devloop-validation.md:86 is a historical incident citation, kept by operations' ruling)
  - `git grep --untracked -nE "HEADER_VERSION_V2|PROTOCOL_VERSION: u8 = 2|parsePrefixV2|decodeFrameV2|SENDABLE_HEADER_VERSIONS|gopCounterFor|peek_frame_len" -- crates packages proto scripts infra`
  - `git grep --untracked -nE "version (===|==) 2|=== 2 &&|HEADER_VERSION_V|isV3|isV2" -- crates packages/sdk-core/src`
- **Gate 2 (Lead), `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`:** `TOTAL_RESULT=OK` (L1 5s, L2 5s, L3 144s, L4 140s, L5 1s,
  L6 4s, L7 1138s). Layer 7: `env-tests-passed`, `browser-e2e-passed` (17 passed, incl. story-2 audio specs
  `multi-party-hear`, `partial-connectivity`, `solo-participant` on v3). Images deployed by Layer 7's `deploy` from this tree
  (content-tagged): `ac-service:sha-ab514d19d940c988`, `gc-service:sha-299b63a2bf1b0ed9`, `mc-service:sha-e9b08e2133973347`,
  `mh-service:sha-c99aa6fae567fd12`.
- Staged index: only the deletion of `proto/test-vectors/frame-v2.vectors.json` is staged (`git rm --cached`), because
  `no-insecure-browser-flags` enumerates candidates from the index and reds on an unstaged deletion. Nothing committed.

---

## Code Review Results

Iteration 1 findings, all FIXED (none deferred); `layer-fast.sh` re-run green after the fixes.

| Reviewer | Finding | Resolution |
|---|---|---|
| observability F1 / code-reviewer F1 | `stream_id`/`hop_sequence` printed by `MediaFrameView`/`MediaFrameParts` Debug; `Layout` derived Debug | all three types redact sequence, GoP and relay values; `Layout` has no Debug; redaction test covers all three |
| observability F2/F3 | "ninth"/"eight tokens" counts; overlong line | count-free; rewrapped |
| media-handler F1 / paired-client F1 | env-tests audio fixture stamped uplink `stream_id` 0xFFFF | `AUDIO_SEND_ID = 0` (MC send id, named in prose); rewrite proof is the hop sentinel, noted at `assert_relayed_on_slot`; MH twin keeps 0xFFFF until MH-A1, with a note |
| media-handler F2 | runbook 236 B / 89 s | 240 B / ~87 s |
| code-reviewer F2 | "version-2 extension registry" | "header v3" |
| code-reviewer F3 | reader's defensive `truncated` contradicted the table | documented at `CompleteHeaderRequired` and the reader: reachable only on a parser defect |
| code-reviewer F4 | `into_datagram_error(0)` magic 0 | `Shortfall::into_truncated()` at all three header-shortfall sites |
| code-reviewer F5-F9 | comment alignment; `MAX_HEADER_BYTES` doc; unchecked `tail_len`; literal 6; no-op `allow(unsafe_code)` | fixed (`tail_len = payload_length + SIGNATURE_SIZE`; `RELAY_REGION_SIZE`; attribute removed) |
| code-reviewer F10, dry F1 | SDK doc wrap; TS vectors-path drift | fixed by paired-client (equality test ties the two TS homes; bundle test imports it) |
| dry F2 | 25+ path literals in the guard self-test | one exported `VEC` |
| dry F3 / test F3 | fuzz wiring media-protocol-only; target files unchecked | `fuzz_wiring.rs` now workspace-scoped: every `crates/*/fuzz` crate, files vs bins vs matrix, both directions |
| test F1 | "exactly `needed` advances" near-vacuous | `len + needed` asserted to be an artifact-derived region boundary |
| test F2 | no-alloc asserted only `is_err()` | asserts `payload_length_exceeds_max` on both paths and `buffered_len() == 14` |
| test F4 | sweep greps missed untracked files | re-run with `--untracked` (empty); evidence updated |

### Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Paired Client | RESOLVED-FIXED | 2 | 2 | 0 | TS v3 codec passes 32/32 rows, 0 cross-language disagreements; g14 truthful |
| Security | CLEAR | 0 | 0 | 0 | acceptance criteria met; Cargo.lock RUSTSEC bumps confirmed |
| Test | RESOLVED-FIXED | 4 | 4 | 0 | |
| Observability | RESOLVED-FIXED | 3 | 3 | 0 | |
| Code Quality | RESOLVED-FIXED | 10 | 10 | 0 | |
| DRY | RESOLVED-FIXED | 3 | 3 | 0 | no extraction TODOs |
| Operations | CLEAR | 0 | 0 | 0 | commit message must carry Expected Layer-State |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 | |
| Media Handler (GSA co-sign) | RESOLVED-FIXED | 2 | 2 | 0 | header-prefix API serves MH-A1/MH-A2 |

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

1. Start commit: `ff2fed55f12c8bf76c99bd80ab1393a34b68d785`
2. **Safe-revert unit**: this whole commit. Revert it entirely, redeploy Kind, restart `dev-web.sh`; a partial revert (SDK only, vectors only) recreates the mixed v2/v3 pair. See `docs/runbooks/mh-deployment.md` §Story 3 cutover "Rollback: three layers".

---

## Issues Encountered & Resolutions

- Generator/TS disagreements: **none** (see Implementation Summary).
- `no-insecure-browser-flags` red on the unstaged vectors deletion -> staged the deletion.
- `validate-cross-boundary-scope` red on brace/glob paths in the classification table -> table rewritten as exact paths.
- cargo-audit red from new upstream advisories against the unchanged base lockfile -> patch bumps (above).
- First draft of `frame.rs` updated the `PROTOCOL_VERSION` doc but not its value; the v2-labelled test caught it.

---

## Lessons Learned

1. Independent generator + TS codec written from the same §2 table agreed on the first meeting (32 rows); the mutation control is what makes that agreement evidence.
2. Owner cells in the classification table must be exactly one specialist; co-sign notes go in a Note column.
