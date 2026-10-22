// File: packages/sdk-core/src/media/frame/rejectReason.ts
//
// The bounded reject-reason taxonomy for the media frame receive path (header
// v3), and the typed errors that carry it.
//
// ---------------------------------------------------------------------------
// THE TOKENS ARE HAND-WRITTEN HERE, AND THAT IS THE POINT
// ---------------------------------------------------------------------------
//
// This module does NOT import the vector loader, and must not. The conformance
// harness asserts `codecEmittedToken === row.reject_reason`; if the token were
// read off the row and echoed back, the assertion would compare the file with
// itself and hold for any spelling. Independent emission on this side is what
// makes it a cross-check rather than a tautology.
//
// Three assertions guard this file, and none of them subsumes another:
//
//   1. Per-arm equality against the vectors' `reject_reason`, in the conformance
//      harness. Catches a token mapped to the WRONG condition.
//   2. Set equality against the SSoT's `reject_reasons[].token`, both directions,
//      over every token in `proto/test-vectors/frame-v3.vectors.json`
//      (`__tests__/rejectReason.test.ts`). This is the ONLY cover for the
//      `has_vector: false` tokens — the receiver-state-dependent ones no byte
//      string determines, so no row exercises them and a typo would otherwise
//      ship green.
//   3. `drops_frame` read FROM the file rather than hand-written here, so
//      `wrap_key_id_mismatch`'s non-dropping status cannot become a drop by
//      someone retyping a boolean.
//
// Exhaustiveness alone would be vacuous: both sides read the same file, so an
// exhaustive mapping with a wrong arm is green. Assertion 1 is what makes an arm
// falsifiable.
//
// ---------------------------------------------------------------------------
// NOTHING KEY-SHAPED MAY REACH AN ERROR MESSAGE, AND NO GUARD ENFORCES THAT
// ---------------------------------------------------------------------------
//
// `sdk-core` is a published SDK: `Error.message` propagates to an embedder's
// `console.error` or Sentry hook, which is a real log channel. Excluded from
// every message on this path, deliberately:
//
//   * transmit keys, wrapped-key bytes, the meeting KEK, Ed25519 seeds or private
//     keys, HKDF PRKs, derived keys, salts and nonces;
//   * raw frame, payload, plaintext or ciphertext bytes;
//   * the AAD span and the signed range, in any encoding;
//   * THE KEY ID AND ITS DECOMPOSED FIELDS. `sender_id` is a CATEGORY_B token in
//     `crates/dt-guard/src/common/pii_vocabulary.rs`, and `label-taxonomy.md` R2
//     bars stream identity on the media path — so `no transmit key for kid 0x…`
//     is the leak, not the diagnostic it looks like.
//
// WHAT IS PERMITTED, stated narrowly on purpose: FIXED-SIZE key-material lengths
// (safe under the `frame.rs::impl Debug for WrappedTransmitKey` ruling, because a
// compile-time constant is not a function of the secret), plus CLEAR-HEADER
// LENGTH FIELDS ON REJECT PATHS ONLY. That second clause is narrow deliberately.
// `payload_length` is NOT a fixed-size length: Opus is content-variable, so a
// per-frame payload length is a function of what was said, and the time-ordered
// sequence of sizes for one stream is the voice-activity trace R2 forbids. It is
// fine here — rejects are rare and aggregate, and the field is clear-header data
// MH already reads — and `declared 900000 exceeds available 512` is a good error.
// It would NOT be fine on a success path. The broader sentence "lengths are safe"
// is what would later license that, which is why it is not written anywhere.
//
// Enforcement reality: `crates/dt-guard/src/ts_pii.rs` scans `console.*` and
// `logger.*` call sites only — an `Error(...)` constructor is NOT scanned — and
// `wrapped_key` is still absent from CATEGORY_A. This rule is REVIEW-ENFORCED,
// named as the weaker form it is. The per-site exclusion comments are the control.
//
// This rule scopes to PRODUCTION error construction. It does NOT extend to the
// conformance harness's assertion messages, which must be able to say which key
// id and which sender mismatched or a red row is undebuggable — test-tier
// assertion output is neither a production log nor in `ts_pii.rs`'s scope.

/**
 * Every reject reason the receive path can produce, plus the one non-dropping
 * outcome.
 *
 * Mirrors `reject_reasons[].token` in `proto/test-vectors/frame-v3.vectors.json`,
 * asserted for set equality in both directions by
 * `__tests__/rejectReason.test.ts`. There is no second client-side union.
 */
export type RejectReason =
  // --- codec layer: the frame did not parse -------------------------------
  | 'unknown_version'
  | 'reserved_flag_bit_set'
  /**
   * A v3 frame with BOTH the key-bearing and the discardable flag set (ADR-0036
   * §2 decode step 5; §4: a frame carrying a new key must never be droppable).
   * Byte-determined, structural family.
   */
  | 'key_bearing_discardable'
  | 'payload_length_exceeds_max'
  | 'payload_length_exceeds_available'
  | 'truncated'
  | 'extensions_too_large'
  | 'extensions_malformed'
  | 'trailing_bytes'
  | 'no_transmit_key'
  // --- codec layer, receiver/stream-dependent (`has_vector: false`) -------
  //
  // Emitted by the VIDEO receive path (C-B), not by `decodeFrame`: no single
  // byte string determines them, so no conformance row can. Declared here now
  // because the vectors file is the vocabulary SSoT and set equality is
  // both-directions. Every one DROPS the frame.
  /** Decoded output exceeded the slot's configured receive bounds. */
  | 'decode_over_bound'
  /**
   * A stream reset mid-frame; the partial frame is discarded. Counted ONCE per
   * aborted frame against the stream path's `received` counting point (C-B), so
   * `received = accepted + sum(drops)` holds.
   */
  | 'stream_partial_reset'
  /**
   * Frames on one stream carried differing relay `stream_id`s; the stream is
   * aborted. A two-frame property, hence no vector. Same counting-point rule as
   * `stream_partial_reset`.
   */
  | 'stream_slot_mismatch'
  // --- crypto layer: the frame parsed but did not open --------------------
  | 'signature_invalid'
  | 'decrypt_failed'
  | 'unwrap_failed'
  | 'replay_detected'
  | 'wrap_key_id_mismatch'
  // --- key layer: receiver state is not ready -----------------------------
  | 'no_kek_for_generation'
  | 'no_roster_entry'
  /**
   * The wrap announces a KEK generation OLDER than this receiver retains.
   *
   * NOT `kek_generation_not_held`, the accepted-frame `WrapOutcome` one word
   * away, and the two sit on opposite sides of
   * `received = accepted + sum(drops by reason)`. A wrap announcing a KEK
   * generation this receiver does not hold splits on ONE question: is a usable
   * transmit key for this frame's key id already cached? CACHED means the frame
   * is ACCEPTED and counted as the wrap outcome kek_generation_not_held
   * (reachable whether the unheld generation is newer or older; here the cache
   * decides, not the age). NOT CACHED means the frame is DROPPED
   * and counted as a reject reason, and only there does direction matter:
   * no_kek_for_generation when newer than the newest held, kek_generation_stale
   * when older than retention keeps.
   */
  | 'kek_generation_stale'
  // --- assignment layer: the sender is not one this receiver was assigned ---
  /**
   * A frame that verified against its sender's roster key, but whose key-id
   * sender is not in this receiver's MC-stated slot-assignment set. Dropped
   * BEFORE it is opened: no decrypt, no transmit-key caching, no replay advance.
   * Defence in depth against a misrouting or compromised handler; in the honest
   * data plane MH never forwards it.
   */
  | 'sender_not_assigned'
  /** The frame's verified key-id sender is this receiver itself (C-B). Never rendered. */
  | 'self_sender'
  /**
   * The verified sender is server-muted for this media kind per the latest
   * `ParticipantMuteUpdate` (C-B). The receiver-side signal that MH's ingress
   * mute enforcement failed; corroborating, not proof.
   */
  | 'sender_server_muted';

/**
 * The tokens, as data, for the set-equality assertion.
 *
 * Hand-written, never derived from the vectors file — see the module header.
 */
export const ALL_REJECT_REASONS: readonly RejectReason[] = [
  'unknown_version',
  'reserved_flag_bit_set',
  'key_bearing_discardable',
  'payload_length_exceeds_max',
  'payload_length_exceeds_available',
  'truncated',
  'extensions_too_large',
  'extensions_malformed',
  'trailing_bytes',
  'no_transmit_key',
  'decode_over_bound',
  'stream_partial_reset',
  'stream_slot_mismatch',
  'signature_invalid',
  'decrypt_failed',
  'unwrap_failed',
  'replay_detected',
  'wrap_key_id_mismatch',
  'no_kek_for_generation',
  'no_roster_entry',
  'kek_generation_stale',
  'sender_not_assigned',
  'self_sender',
  'sender_server_muted',
] as const;

/**
 * The reject reasons that judge DECODER OUTPUT (or the decrypted bitstream the
 * decoder is about to see) rather than the frame on the wire — the ONE
 * exception to the parse-boundary accounting identity (story 3 C-B):
 *
 *     received = accepted + sum(drops{reason not in this set})
 *
 * A video frame is ACCEPTED once it verified, decrypted and reached its slot
 * pipeline; only then can its decoded geometry or rate be judged, so a
 * `decode_over_bound` frame is counted on BOTH sides and is subtracted here.
 * The token cannot move to a post-accept counter: it is a member of the vectors
 * file's closed vocabulary and the task names it a frame drop.
 *
 * A subset of {@link RejectReason}, not a second union.
 */
export const VIDEO_POST_DECODE_REJECT_REASONS = [
  'decode_over_bound',
] as const satisfies readonly RejectReason[];

/**
 * The ONE reason that does not drop the frame.
 *
 * ADR-0036 §4: the receiver ignores a wrap bound to another key id and otherwise
 * plays the frame. It is modelled as an outcome on the SUCCESS return channel
 * (see `receivePath.ts`), never as a thrown error, so it is structurally
 * impossible to `catch` into a drop counter — which would break R-25's
 * `received = played + sum(drops)` identity in aggregate, long after the label
 * set is frozen.
 */
export const NON_DROPPING_REASON = 'wrap_key_id_mismatch' satisfies RejectReason;

/**
 * Which processing stage produced a reason. Mirrors `reject_reasons[].layer`.
 *
 * A STAGE, not the owner of the remedy, and never an alert-selector input:
 * `no_transmit_key` is `codec` yet fires on key-store membership. Classify a
 * token by `docs/observability/label-taxonomy.md` §Frame reject reason, never
 * by this field.
 */
export type RejectLayer = 'codec' | 'crypto' | 'key' | 'assignment';

/**
 * Structured detail attached to a reject.
 *
 * Lengths and offsets ONLY — see the module header for what is excluded and why
 * no guard enforces it.
 */
export interface RejectDetail {
  /** Byte offset at which parsing stopped, where meaningful. */
  readonly at?: number;
  /** A declared length that failed a bound. Clear-header data; reject paths only. */
  readonly declared?: number;
  /** The bound it failed against. Always a compile-time constant. */
  readonly limit?: number;
  /** Bytes actually available. Never the bytes themselves. */
  readonly available?: number;
}

/**
 * A frame rejected on the receive path.
 *
 * Carries `rejectReason` as a stable, machine-readable discriminant — NOT only a
 * message string. Story task 19 does `counter.add(1, { reason: err.rejectReason })`
 * at the receive path; without this field it would have to string-match a message
 * or re-derive the taxonomy at the call site.
 */
export class FrameRejectedError extends Error {
  /** The bounded token. Safe as a metric label value. */
  readonly rejectReason: RejectReason;
  /** Which stage produced it. */
  readonly layer: RejectLayer;
  /** Lengths and offsets only. */
  readonly detail: RejectDetail;

  constructor(
    reason: RejectReason,
    layer: RejectLayer,
    message: string,
    detail: RejectDetail = {},
  ) {
    super(message);
    this.name = 'FrameRejectedError';
    this.rejectReason = reason;
    this.layer = layer;
    this.detail = detail;
  }
}

/** Construct a codec-layer reject. */
export function codecReject(
  reason: RejectReason,
  message: string,
  detail: RejectDetail = {},
): FrameRejectedError {
  return new FrameRejectedError(reason, 'codec', message, detail);
}

/** Construct a crypto-layer reject. */
export function cryptoReject(
  reason: RejectReason,
  message: string,
  detail: RejectDetail = {},
): FrameRejectedError {
  return new FrameRejectedError(reason, 'crypto', message, detail);
}

/** Construct a key-layer reject. */
export function keyReject(
  reason: RejectReason,
  message: string,
  detail: RejectDetail = {},
): FrameRejectedError {
  return new FrameRejectedError(reason, 'key', message, detail);
}

/** Construct an assignment-layer reject. */
export function assignmentReject(
  reason: RejectReason,
  message: string,
  detail: RejectDetail = {},
): FrameRejectedError {
  return new FrameRejectedError(reason, 'assignment', message, detail);
}
