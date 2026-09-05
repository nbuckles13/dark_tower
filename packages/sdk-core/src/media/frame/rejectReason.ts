// File: packages/sdk-core/src/media/frame/rejectReason.ts
//
// The bounded reject-reason taxonomy for the v2 media frame receive path, and
// the typed errors that carry it.
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
//      all sixteen (`__tests__/rejectReason.test.ts`). This is the ONLY cover for
//      the three `has_vector: false` tokens — `no_transmit_key`,
//      `no_kek_for_generation`, `no_roster_entry` — which no row exercises and
//      where a typo would otherwise ship green.
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
 * Mirrors `reject_reasons[].token` in `proto/test-vectors/frame-v2.vectors.json`,
 * asserted for set equality in both directions by
 * `__tests__/rejectReason.test.ts`.
 */
export type RejectReason =
  // --- codec layer: the frame did not parse -------------------------------
  | 'unknown_version'
  | 'reserved_flag_bit_set'
  | 'payload_length_exceeds_max'
  | 'payload_length_exceeds_available'
  | 'truncated'
  | 'extensions_too_large'
  | 'extensions_malformed'
  | 'trailing_bytes'
  | 'no_transmit_key'
  // --- crypto layer: the frame parsed but did not open --------------------
  | 'signature_invalid'
  | 'decrypt_failed'
  | 'unwrap_failed'
  | 'replay_detected'
  | 'wrap_key_id_mismatch'
  // --- key layer: receiver state is not ready -----------------------------
  | 'no_kek_for_generation'
  | 'no_roster_entry';

/**
 * The tokens, as data, for the set-equality assertion.
 *
 * Hand-written, never derived from the vectors file — see the module header.
 */
export const ALL_REJECT_REASONS: readonly RejectReason[] = [
  'unknown_version',
  'reserved_flag_bit_set',
  'payload_length_exceeds_max',
  'payload_length_exceeds_available',
  'truncated',
  'extensions_too_large',
  'extensions_malformed',
  'trailing_bytes',
  'no_transmit_key',
  'signature_invalid',
  'decrypt_failed',
  'unwrap_failed',
  'replay_detected',
  'wrap_key_id_mismatch',
  'no_kek_for_generation',
  'no_roster_entry',
] as const;

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

/** Which parsing/crypto stage produced a reason. Mirrors `reject_reasons[].layer`. */
export type RejectLayer = 'codec' | 'crypto' | 'key';

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
