# Devloop Output: C-v3 — TypeScript frame codec for header v3 (Story 3 task 5)

**Date**: 2026-10-21
**Task**: TS v3 frame codec authored from ADR-0036 §2 table (R-9, R-14 codec half, R-22); transitional v2+v3 dual path
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present; --paired-with=protocol (security already on panel)
**Branch**: `feature/user-see-each-other`
**Duration**: ~2h

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `6bbe9b0f533b90d760c60639d45ef19f38447fa3` |
| Branch | `feature/user-see-each-other` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `semantic-guard` |
| Paired Protocol | `paired-protocol` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to complete and proceed to Step 8 (Commit)
     - Only mark complete after Gate 3 approval
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
     - Tier (ADR-0037 §D2): set the Tier row at setup from the manifest tier; under `light`, Step 5 SKIPS the Gate-1 plan round and writes a `### Gate 1 — SKIPPED (tier=light; reason: …)` marker instead of pending rows. A resumed devloop reads the tier from THIS Tier row (not the manifest, not the confirmations table); no Tier row ⇒ full (fail-safe).
-->

---

## Task Overview

### Objective
Implement the TypeScript header-v3 frame codec in `packages/sdk-core` from the ADR-0036 §2 normative table, independently of the Rust v3 work (R-22), with a transitional directive-driven v2+v3 dual path that P2 (task 6) deletes.

### Scope
- **Service(s)**: client SDK (`packages/sdk-core`); observability docs
- **Schema**: No
- **Cross-cutting**: No (client-only; nothing cluster-observable until P2)

### Debate Decision
NOT NEEDED - layout is fixed by ADR-0036 §2 (task 1/P1); no ADR clarification required.


---

## Cross-Boundary Classification

<!-- List EVERY planned file change. For each, classify per ADR-0024 §6.2:
     - Mine — in the implementing specialist's domain (trivial, the common case)
     - Not mine, Mechanical — cross-boundary, sed-test clean, guard-pipeline covered
     - Not mine, Minor-judgment — cross-boundary, bounded impact; owner must review & confirm at Gate 1 + Gate 3
     - Not mine, Domain-judgment — needs owner-implements or --paired-with=<owner>

     For Guarded Shared Area paths (ADR-0024 §6.4), Mechanical is disallowed; Owner must be filled.
     Fill Owner (if not mine) for cross-boundary rows.

     Path column convention: backtick-quoted paths. Globs (`*`, `?`, `[]`,
     trailing `/`, `/**`) and parenthetical annotations like `foo.rs` (regen)
     are tolerated by the `validate-cross-boundary-scope` parser at
     scripts/guards/common.sh, and are recommended where they clarify intent
     — use `dir/**` (or `dir/`, which the parser canonicalizes to
     `dir/**`) to scope a whole tree, `*.svelte` for a filename glob,
     and `(regen)` / `(cleanup)` /
     `(skeleton-only)` suffixes for per-row context. Prefer the simplest
     form that is accurate: if a literal path conveys the same information,
     use that; reach for a glob when enumerating every file would be noise,
     and reach for a parenthetical when the row's nature (regen, cleanup,
     new-vs-modify) materially changes how a reviewer reads it. Longer-form
     file-shape context (rationale, scope qualifiers, "why this shape") still
     belongs in § Implementation Summary or § Files Modified — the table
     answers one question per row: whose domain is this, and how stringent
     is the involvement. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/src/media/frame/frameCodec.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/rejectReason.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/__tests__/frameCodec.test.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/__tests__/frameCodec.v3.test.ts` (new) | Mine | — |
| `packages/sdk-core/src/media/frame/__tests__/rejectReason.test.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/__tests__/receivePathIntegration.test.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/egress.ts` | Mine | — |
| `packages/sdk-core/src/media/pipeline/__tests__/*.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts` | Mine | — |
| `packages/sdk-core/src/media/lifecycle/__tests__/*.ts` | Mine | — |
| `packages/sdk-core/src/session/mediaWiring.ts` | Mine | — |
| `packages/sdk-core/src/session/__tests__/*.ts` | Mine | — |
| `packages/sdk-core/src/signaling/SignalingClient.ts` | Mine | — |
| `packages/sdk-core/src/signaling/events.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/*.ts` | Mine | — |
| `packages/sdk-core/src/media/frame/__tests__/frameVectors.ts` | Mine | — |
| `packages/sdk-core/src/media/setup/__tests__/mediaMetrics.test.ts` | Mine | — |
| `docs/observability/label-taxonomy.md` | Not mine, Minor-judgment | observability |
| `docs/observability/metrics/client.md` | Not mine, Minor-judgment | observability |
| `docs/specialist-knowledge/client/INDEX.md` | Mine | — |

---

## Planning

**Spec sources (R-22).** ADR-0036 §2 "Header v3 — the normative layout" (table, flags, cursor blocks, regions, frame extent, decode order steps 1–14, encode side, gop_counter rules, hop-local relay ids) and the story's Design §Wire freeze. I did not open `crates/media-protocol` or `crates/media-vector-gen`, and I did not open any draft v3 vectors. Prompt, ADR and Wire freeze agree: 14-byte prefix, gop_counter u32 at offset 10, wrapped key 50 at the cursor, ext_length, TLVs, relay(6), payload, sig(64). The ADR's "no dual decode, v3 only" is the post-P2 end state. It does not conflict with the transitional SDK dual path (paired-protocol confirmed).

**Mechanism restatement.** The codec's byte layout is a function of one version input. That input is the received `version` byte on decode, and MC's `SendDirective.header_version` on encode. Every version-dependent constant lives in one place. Wider class checked: other SDK sites that assume the v2 layout are `transmitKeys.ts` (comment only, "getUint32(6)": same offset in v3, still true), the test fixture builders (`pipeline/__tests__/frameFixtures.ts`, `ingress.hopSender.test.ts`, `receivePathIntegration.test.ts`) and egress. All are in scope here. Nothing else in `packages/` parses frame bytes.

### 1. Constants (one place, P2-replaced)
- These stay in `frameCodec.ts`: one hand-authored block marked "TRANSITIONAL, ADR-0036 §2 Header v3 table, authored by C-v3; P2 replaces with regenerated WIRE_CONSTANTS". It holds `HEADER_VERSION_V3 = 3` (the only literal 3), `GOP_COUNTER_FIELD_BYTES = 4`, and the v3 prefix, derived as v2 `publisher_fixed_prefix_bytes + GOP_COUNTER_FIELD_BYTES`. That derivation encodes the ADR's "adds ONLY gop_counter, at offset 10". A test pins 14 literally from the ADR table.
- Everything v3 does not change is reused from the generated `WIRE_CONSTANTS`: signature, relay, wrapped key, ext_length, MAX_EXT_BYTES, MAX_PAYLOAD_BYTES, legal mask. `wireConstants.ts` is not edited.
- `SUPPORTED_HEADER_VERSIONS = [HEADER_VERSION, HEADER_VERSION_V3]` and `isSupportedHeaderVersion()` are exported, so they are the one membership predicate.
- **Tripwire test:** it asserts `HEADER_VERSION === 2`, with the message "P2 flipped the vectors: delete the v2 path and the transitional v3 block". P2's regeneration turns it red, so dual-encode cannot outlive the story.

### 2. Decode (dispatch by received byte)
- Dispatch option (a), agreed with paired-protocol: every byte other than 3 keeps v2 ordering. The v3 "terminal on 1 byte" check (`[0x07]` → `unknown_version`) is asserted on the v3 routine directly (`decodeFrameV3`, exported for test). It becomes true end to end at P2.
- `decodeFrame(data)`: if `data.length >= 1 && data[0] === HEADER_VERSION_V3`, the v3 path runs. Otherwise the **unchanged v2 path** runs, marked `// v2 — deleted by P2`. It keeps its current order (prefix-10 check, then version). So an empty buffer gives `truncated`, and v1, v4 or any other version gives `unknown_version` exactly as today. All v2 vectors stay green.
- The v3 path follows ADR steps 1–14 in order: version present; `== 3` (terminal on 1 byte); 14 present; `0xF8` bits → `reserved_flag_bit_set`; key-bearing and discardable → `key_bearing_discardable`; payload max; seq; gop (no check); wrapped key; ext_length limit before availability; grammar; relay; step-13 arithmetic (`remaining<64` → truncated, else `exceeds_available`); trailing.
- Everything after the prefix (wrapped key, ext, relay, body, ranges) is ONE shared cursor routine for both versions. The two versions differ only in the prefix parse. That avoids a second copy of the tail.
- `DecodedFrame.gopCounter: number | null` (null on v2; P2 narrows it to `number`). `aeadAad = data.subarray(0, publisherRegionLen)`. `signedRange` is `concatBytes(subarray, subarray)`, as today.
- **Stream reader:** the v2 TS codec has no stream reader today. It is datagram-only, and `trailing_bytes` and shortfall are rejects. I keep that split. The "need more bytes" reader is C-B's (task 14).
- **Downgrade binding (security S3c), explicitly out of scope:** the receiver does not bind an accepted version to the directed version. During the transition it accepts v2 and v3 by byte. P2 deletes v2 decode in the same story, after which v3-only makes the binding moot.

### 3. Encode
- `PublisherRegionInput` / `EncodeFrameInput` gain **required** `headerVersion` and `gopCounter`. There is no default, so a caller must name the version. An unsupported version → `codecReject('unknown_version')`. The v2 path ignores `gopCounter` (comment: P2 deletes).
- In the v3 path: key-bearing and discardable → `codecReject('key_bearing_discardable')`. Payload over max, ext over max and the key-bearing/wrapped-key disagreement are refused as today.
- Range checks (`RangeError`, security S5) for non-integers and for values out of range: `streamSequence`, `gopCounter`, `hopSequence` (u32), `streamId` and `kekGeneration` (u16). They run on the **v3 encode path only** (operations A). v2 encode stays byte- and behaviour-identical, and P2 inherits the checks with v3. Every in-tree source is in range by construction anyway: `kekSource.install` refuses a generation above `MAX_KEK_GENERATION` (0xffff) at intake; the stream sequence wrap needs about 2.7 years at 50 fps (`transmitKeys.ts`); hop sequences are per-lane u32 counters; the stream id comes from declared slot ids. If a check does fire inside egress, it lands in `AudioPipeline`'s existing `submit().catch` → typed event-once `MediaFaultStage.Crypto` fault. The pipeline stays running and nothing escapes untyped.
- `key_bearing_discardable` is **v3-only**. v2 decode and encode stay byte- and behaviour-identical (operations 4), because rejecting it on v2 in the SDK alone would create an SDK/MH asymmetry in a live v2 cluster.

### 4. Version selection in the publish path
- `SendDirectiveEvent.headerVersion` → `sendDirectiveListener` → `AudioSendDirective.headerVersion` (required) → `AudioPipeline` → `EgressPipeline.setTargets(targets, headerVersion)` → `buildPublisherRegion`. MC directs 2 today, so the bytes are identical to today's. Proof: existing egress tests and v2 vector conformance, plus a test that directive 2 produces a frame byte-equal to the pre-change construction.
- Before any directive: egress is not constructed, so nothing is sent and there is no default version (operations 3).
- Recovery (operations B): a later valid directive clears the refusal fault's event-once latch, so the fault fires once per refusal EPISODE, not once per session. The valid directive is applied normally, and the empty→non-empty edge rotates the transmit key exactly as today. Test: bad version → good version → frames flow, and a second bad episode faults again.
- Directive producers (operations C): every producer outside MC is checked and given an explicit version. That covers `signaling/__tests__/helpers.ts` (already defaults to 2), the AudioPipeline/session test rigs (made required, so the type checker finds every site), `packages/test-utils` (none) and the web-app e2e (none: real MC).
- An unsupported directed version (0, 1, 4+): `AudioPipeline` refuses it fail-closed. The directive is applied as **send nothing** (empty targets), and a typed, event-once `fault` is emitted (`MediaFaultStage.Config`, "the controller directed an unsupported frame header version"). No series is minted: C-A1 owns the counted refusal and builds on this outcome. There is no fallback to v2 or to HEADER_VERSION.
- **Lead decision (option A):** the codec fully encodes and decodes v3, and the directive version is plumbed end to end. Until P2 adds the GoP counter owner, the publish path refuses a directed 3 fail-closed, by the same typed path as an unsupported version: it sends nothing and raises an event-once Config fault. The refusal site carries a "P2 enables" comment naming task 6.
- **P2 obligations handed off from this task:**
  - **in ONE edit:** replace `egress.ts:gopCounterFor` (inert 0 for v2, throws otherwise) with the per-(sender, stream) GoP counter owner on `TransmitKeyManager`, AND remove the explicit v3 exclusion from `SENDABLE_HEADER_VERSIONS`. The exclusion does not release itself: after the regeneration alone the sendable set is `[]`, so the join declares nothing and MC refuses it loudly. The tripwire message names both sites;
  - classify `key_bearing_discardable` as CODEC_SHAPE in dt-guard G7;
  - empty `PENDING_P2`;
  - delete the v2 path and the transitional v3 block (the `HEADER_VERSION === 2` tripwire goes red to force this);
  - fold the token into the counts in `label-taxonomy.md` / `metrics/client.md`.
  - the join's declared set follows the sendable set: `SignalingClient` defaults `supported_header_versions` to `SENDABLE_HEADER_VERSIONS`, so it becomes `[3]` at the flip with no edit. Verify that after the flip.
- **Lead completion item (join declaration):** when the caller gives no caps, the production join declares `SENDABLE_HEADER_VERSIONS` (`[2]` today), never the decode set and never a literal. An explicit caller list still wins. MC ignores the field until MC-A. Tested in `signaling-client.test.ts`.

### 5. Reject vocabulary
- Add `'key_bearing_discardable'` (exact ADR spelling) to the `RejectReason` union and `ALL_REJECT_REASONS`.
- `rejectReason.test.ts`: a hand-written `PENDING_P2 = ['key_bearing_discardable']`. Set equality is asserted against `vectors ∪ PENDING_P2`, plus an assertion that every pending token is ABSENT from the vectors. It goes red when P2 adds the token, so it retires itself.
- Not changed: dt-guard G7 (`client_metrics_export.rs` NOT_KEY_DELIVERY/CODEC_SHAPE) reads only the vectors. **P2 obligation:** classify `key_bearing_discardable` as CODEC_SHAPE. Recorded in the hand-off.
- Docs (observability, Minor-judgment): `label-taxonomy.md` §Frame reject reason and `metrics/client.md` `dt_client_media_frames_dropped_total` get a note. It says a nineteenth, byte-determined (structural family) token, `key_bearing_discardable`, is emitted on v3 decode only, and enters the vectors SSoT (and the counts) at P2. The counts stay true of the SSoT.

### 6. Header and comments
- The module header records that the codec was authored from the ADR-0036 §2 table by C-v3, and why a port of either side hollows out R-22.
- The layout diagram is redrawn for v3: gop at 10, wrapped key at 14. v2 is noted. The two-deviation comment is re-checked against v3 and is unchanged in substance.
- Every v2 branch carries `v2 — deleted by P2`. That is greppable for the runbook claim.

### 7. Tests (`__tests__/frameCodec.v3.test.ts`, hand-built from the ADR table)
- The file header says: self-consistency is not evidence of correctness, and P2's independent vectors are the gate this task makes meaningful.
- One hand-assembled v3 frame is built byte by byte from the table, without the encoder, and decoded. The encoder's output is asserted byte-equal to it. This is the closest a pre-vector test gets to independence.
- All 8 flag combinations, plus the reserved bits; key-bearing and discardable rejects on decode and encode; the 0xFE byte reports `reserved_flag_bit_set` first.
- Relay `stream_id` at 0 and 65535, and hop rewrite: `aeadAad` and `signedRange` are byte-identical and a real Ed25519 verify still passes.
- Flip each publisher byte (the 4 gop bytes included), and flip a payload byte: both ranges change and the verify fails (security S1).
- gop at 0 and u32::MAX; gop/seq/streamId/hop/kekGen at max+1, -1 and NaN → RangeError.
- Key-bearing and non-key-bearing, each with and without extensions.
- payload_length 0 and MAX (decodes); MAX+1 → `payload_length_exceeds_max` on a short buffer, proving the check runs before sizing.
- Truncation at every offset 0..len-1 → `FrameRejectedError` with the ADR-predicted reason per offset (`truncated` vs `payload_length_exceeds_available`, step 13), never an untyped throw. A 1-byte `[0x07]` → `unknown_version` on the v3 routine; `[0x03]` → `truncated`; empty → `truncated`. Trailing byte → `trailing_bytes`.
- Subarray aliasing: mutating an input byte changes the `aeadAad` view. The naive contiguous `[0, signatureOffset)` differs from `signedRange`.
- Encoded v2 is unchanged (existing suite, now passing `headerVersion: HEADER_VERSION`). Tripwire. Mixed decode: a v2 and a v3 frame decode in one ingress.
- Encode-side refusals stay on the egress fault path (observability): a test drives a refused build through `AudioPipeline` and asserts the existing event-once fault fires and that `frameDropped` and every other receive-side counter stay untouched. A send-side refusal must never enter `received = accepted + sum(drops)`.
- Test reviewer points, folded in:
  - T1: the truncation oracle is a hand-written table of offset ranges taken from the ADR steps, with a positive control on the number of iterations. Two sweeps: a key-bearing frame with extensions, and a plain frame.
  - T2: the signed range is checked positively, as `data[0,pubLen) ‖ data[payloadStart,sigOffset)` with hand-computed offsets. The AAD is checked as a view: same buffer, byteOffset 0, exact length.
  - T3: the header says the hand-assembled frame is the same author's reading and catches only encoder/decoder disagreement.
  - T4: v2 bytes are identical for gop 0 and 0xFFFFFFFF.
  - T5: the directive-2 byte-equality reference is a committed v2 vector row's `frame_hex`, never a re-run of the builder.
  - T6: fixed seeds and bytes, no timers.
  - T7: each reserved bit 0x08..0x80 is tested on its own, asserting the exact reason.
  - T8: the v3 encoder refuses MAX+1.
- `hotPathLayout.test.ts` stays green: no logging in frameCodec/egress. Reject messages carry lengths and offsets only, never gop, stream_id or bytes (S9 / obs 4).

### 8. Gate-1 implementation notes folded in
- **code-reviewer:**
  - (a) the prefix parse returns `{version, flags, payloadLength, streamSequence, gopCounter, prefixLen}`. The tail is version-blind. Encode mirrors this: a per-version prefix writer, then one layout.
  - (b) test seams such as `decodeFrameV3` stay out of `index.ts`; they are marked "test seam".
  - (c) one exact marker string: `v2 — deleted by P2`.
  - (d) egress takes `{ targets, headerVersion }` as one object, so the two cannot come apart.
  - (e) the derivation comment states it holds only while the vectors are v2.
  - (f) `isSendableHeaderVersion` sits beside `isSupportedHeaderVersion`, P2-marked, with the GoP-owner gap stated. A test pins it: directive 3 is refused today, and encoding v3 works.
- **dry-reviewer:**
  - N1: one `assertUintField(name, value, widthBytes)` helper, with each max derived from the field width. No new 0xffff/0xffffffff literals, and no `MAX_KEK_GENERATION` import.
  - N2: `HEADER_VERSION_V3` is the only production 3. Tests use `HEADER_VERSION` for "current".
- **semantic-guard:**
  - no input objects or bytes on `cause`/`detail`/fault payloads;
  - allow-listed fields only;
  - no module-level scratch buffer.

**Env-tests:** none. Nothing is cluster-observable until P2 flips the version.

---

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed (after T1-T8 folded in) |
| Observability | confirmed |
| Code Quality | confirmed (notes a-f) |
| DRY | confirmed (notes N1-N2) |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Paired Protocol | confirmed (layout conformance; no ADR edit) |

Lead rulings at Gate 1:
- gop_counter source pre-P2: option (A) — codec encodes/decodes v3 fully; egress refuses a directed 3 fail-closed until P2 lands the counter owner (supported by security, operations).
- Completion item added (from DRY observation): production join declares `supported_header_versions` = the sendable header-version set (derived, not literal); no story task owned the SDK-side declaration and MC-A (task 12) will refuse joins lacking 3.

## Pre-Work

None

---

## Implementation Summary

### Codec (`frameCodec.ts`)
| Item | Before | After |
|------|--------|-------|
| Decode | v2 only | dispatch on the received byte: 3 → `parsePrefixV3` (ADR steps 1–8); otherwise `parsePrefixV2` (unchanged order, `v2 — deleted by P2`). Both feed one version-blind `decodeAfterPrefix` (steps 9–14). |
| `DecodedFrame.gopCounter` | — | `number \| null` (null on v2; P2 narrows it) |
| Encode | v2 only | required `headerVersion` and `gopCounter` inputs. v3 adds the key-bearing+discardable refusal, the extension grammar refusal (via the decoder's own `parseExtensions`), and `assertUintField` range checks. v2 bytes unchanged. |
| Versions | `PROTOCOL_VERSION` | transitional block: `HEADER_VERSION_V3`, `GOP_COUNTER_FIELD_BYTES`, `SUPPORTED_`/`SENDABLE_HEADER_VERSIONS` + predicates |

### Publish path
`SendDirectiveEvent.headerVersion` → `sendDirectiveListener` → `AudioSendDirective.headerVersion` → `AudioPipeline.setSendDirective`. An unsendable version (incl. 3 until P2) is applied as send nothing and raises an event-once `config` fault, re-armed by a valid directive. Then `EgressPipeline.setTargets({targets, headerVersion})`, with a backstop `RangeError`.

### Join
`SignalingClient` defaults `supported_header_versions` to `SENDABLE_HEADER_VERSIONS`.

### Reject vocabulary
`key_bearing_discardable` added. `PENDING_P2_REJECT_TOKENS` (in `frameVectors.ts`) is self-retiring and shared by `rejectReason.test.ts` and `mediaMetrics.test.ts`.

### Tests
- New `frameCodec.v3.test.ts`.
- Pipeline header-version tests in `audioPipeline.test.ts`.
- A v3 end-to-end open in `receivePathIntegration.test.ts`.
- The join default in `signaling-client.test.ts`.
- Verbatim pass-through in `meeting-session.test.ts`.

---

## Files Modified

```
 docs/observability/label-taxonomy.md               |   6 +
 docs/observability/metrics/client.md               |   4 +-
 docs/specialist-knowledge/client/INDEX.md          |   2 +-
 .../src/media/frame/__tests__/frameCodec.test.ts   |  20 +-
 .../src/media/frame/__tests__/frameVectors.ts      |  12 +
 .../frame/__tests__/receivePathIntegration.test.ts |  27 +-
 .../src/media/frame/__tests__/rejectReason.test.ts |  17 +-
 packages/sdk-core/src/media/frame/frameCodec.ts    | 507 ++++++++++++++++++---
 packages/sdk-core/src/media/frame/rejectReason.ts  |  18 +-
 .../sdk-core/src/media/lifecycle/AudioPipeline.ts  |  57 ++-
 .../__tests__/audioPipeline.degradation.test.ts    |  17 +-
 .../audioPipeline.deviceAndCounts.test.ts          |   8 +-
 .../__tests__/audioPipeline.multiSender.test.ts    |   8 +-
 .../lifecycle/__tests__/audioPipeline.test.ts      | 106 ++++-
 .../pipeline/__tests__/egress.multiTarget.test.ts  |  37 +-
 .../src/media/pipeline/__tests__/frameFixtures.ts  |  23 +-
 .../pipeline/__tests__/ingress.attribution.test.ts |  33 ++
 .../pipeline/__tests__/ingress.hopSender.test.ts   |   3 +
 packages/sdk-core/src/media/pipeline/egress.ts     |  73 ++-
 .../src/media/setup/__tests__/mediaMetrics.test.ts |   9 +-
 .../src/session/__tests__/meeting-session.test.ts  |  16 +
 packages/sdk-core/src/session/mediaWiring.ts       |   3 +
 packages/sdk-core/src/signaling/SignalingClient.ts |  10 +-
 .../sdk-core/src/signaling/__tests__/helpers.ts    |   6 +-
 .../signaling/__tests__/signaling-client.test.ts   |  23 +
 packages/sdk-core/src/signaling/events.ts          |  10 +-
 26 files changed, 934 insertions(+), 121 deletions(-)
 (new) packages/sdk-core/src/media/frame/__tests__/frameCodec.v3.test.ts
 (new) docs/devloop-outputs/2026-10-21-c-v3-ts-frame-codec/main.md
```

### Key Changes by File
| File | Changes |
|------|---------|
| `packages/sdk-core/src/media/frame/frameCodec.ts` | v3 encode/decode from ADR-0036 §2; per-version prefix + shared tail; transitional v3 block; SUPPORTED/SENDABLE sets (v3 explicitly unsendable until P2) |
| `packages/sdk-core/src/media/frame/rejectReason.ts` | `key_bearing_discardable` token |
| `packages/sdk-core/src/media/pipeline/egress.ts` | headerVersion in send plan; `gopCounterFor` fail-closed for non-v2 |
| `packages/sdk-core/src/media/lifecycle/AudioPipeline.ts`, `session/mediaWiring.ts` | directive header_version plumbed; unsendable version refused fail-closed with per-episode Config fault |
| `packages/sdk-core/src/signaling/SignalingClient.ts`, `events.ts` | join declares sendable header-version set by default |
| `packages/sdk-core/src/media/frame/__tests__/frameCodec.v3.test.ts` | hand-built v3 tests from the ADR table |
| `docs/observability/label-taxonomy.md`, `metrics/client.md` | pending-P2 reject token noted (observability-confirmed) |

---

## Devloop Verification Steps

Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` — `TOTAL_RESULT=OK` (TOTAL_DURATION=1143s), first attempt after Gate 3.

| Layer | Result | Duration (s) |
|-------|--------|--------------|
| 1 Compile | OK | 4 |
| 2 Format | OK | 4 |
| 3 Guards | OK | 134 |
| 4 Test | OK | 145 |
| 5 Lint | OK | 1 |
| 6 Audit | OK (dep audits SKIPPED-NO-DIFF no-dep-changes; buf breaking passed) | 1 |
| 7 Env-tests | OK (env-tests-passed, browser-e2e-passed) | 854 |

Fast check (`layer-fast.sh`) green before review and after Gate-3 fixes. Log: `/tmp/layer-all-task5.log` (container).

---

## Code Review Results

### Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED | 1 | 1 | 0 | GoP placeholder would auto-release at P2 (shared with ops/protocol) |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | vacuous flip loop; missing mixed v2+v3 ingress test |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | refused-directive test pins all four counters; owner-confirmed doc edits |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | index.ts exports; silent egress return; scattered encode branching |
| DRY | CLEAR | 0 | 0 | 0 | TS/Rust parallel excluded per R-22 |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | GoP placeholder auto-release (shared) |
| Semantic Guard | CLEAR (native SAFE) | 0 | 0 | 0 | credential-leak 5-10 + client credential lifetime applied |
| Paired Protocol | RESOLVED-FIXED | 2 | 2 | 0 | v3 decode-side extension rejects untested; GoP placeholder (shared) |

Finding resolutions are detailed under §Issues Encountered & Resolutions → Gate-3 fixes. No ESCALATED verdicts; no Lead overrides.

---

## Accepted Deferrals

- (none surfaced in this devloop)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `6bbe9b0f533b90d760c60639d45ef19f38447fa3`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied
7. **Safe-revert unit** (answer explicitly, even if "the whole commit"): can any part of this diff be reverted or cherry-picked on its own, or does a partial revert reconstruct a state worse than either endpoint (e.g. a security fix split from the change that made it necessary, or a client/server/alert-rule set that must move together)? If partial reverts are unsafe, name the unit and the safe direction. **Answer**: `the whole commit` — the codec, the sendable-set exclusion, egress `gopCounterFor` fail-closed and the join declaration must move together (a partial revert keeping the derived join declaration without the v3 exclusion, or the codec without the egress guard, could re-open the self-releasing GoP placeholder at P2). Client-only; no server coupling pre-P2. (This converts silence into a visible unanswered slot; it cannot distinguish a checked answer from a reflexive one.)

---

## Issues Encountered & Resolutions

### Deviation from plan: range checks are v3-only
The plan (§3, security S5) first applied the u16/u32 encoder range checks to both versions. Following operations follow-up A, they run on v3 only, so v2 encode stays byte- and behaviour-identical. Security accepted this at Gate 3. v2 is deleted in this story, and `transmitKeys.ts` documents v2 `stream_sequence` exhaustion as unreachable.

### Gate-3 fixes
- **Security, operations and paired-protocol (one shared finding):** the GoP placeholder became reachable silently at P2. Fixed two ways:
  - `SENDABLE_HEADER_VERSIONS` excludes v3 explicitly, so the gate no longer releases itself.
  - Egress `gopCounterFor` throws for any version other than the pinned `HEADER_VERSION_V2`.
  - The tripwire message names both sites, and tests pin both.
- **code-reviewer:**
  - the transitional exports are dropped from `index.ts`;
  - egress now throws on an active target with no plan instead of returning;
  - the encoder is split into `assertV3EncodeInput` / `fixedPrefixBytes` / `writeFixedPrefix` with one `isV3` helper.
- **test:**
  - the flip loop asserts a hand-derived reject set `{0,2,3,4,5,64,65,66,67}` and 84 verify failures;
  - a v2 and a v3 frame are accepted through one ingress (`ingress.attribution.test.ts`).
- **paired-protocol:** v3 decode-side extension rejects are now tested. Too-large is checked before availability, at 64 and at 14; an unknown type and an out-of-set salience are malformed.
- **observability:** the refused-directive test pins send-dropped, received, accepted and dropped at 0.
- **security (optional, taken):** `GOP_COUNTER_OFFSET` is now `STREAM_SEQUENCE_OFFSET + 4`, the table's own relation.

---

## Lessons Learned

1. A transitional gate that "releases itself" on a regenerated constant is a masked placeholder: three reviewers independently caught that `SENDABLE = [HEADER_VERSION]` would ship gop=0 at P2. Transitional exclusions must require a deliberate edit at the owner's landing site.
2. R-22 independence held operationally by routing every ADR question to the paired protocol owner, who answered only from ADR/story text.
3. Flip/tamper loops need a hand-derived oracle for the reject set, otherwise "reject or verify-fail" passes vacuously.

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
