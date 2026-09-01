// File: packages/sdk-core/src/signaling/codecMap.ts
//
// ADR-0036 §5: the ONE oracle mapping the proto `Codec` (wire enum) → the
// stable SDK-side `SignalingCodec` union. Same pattern and same reasoning as
// `errorCodeMap.ts`, deliberately: the generated `*_pb.ts` are gitignored and
// not part of the published API, so `Codec` must not appear on the public
// surface (see the `events.ts` header).
//
// KEYED ON THE WIRE ENUM, not on the public union. That direction is what
// makes the guarantee real: adding a `Codec` member to the proto is a compile
// error here, because `Record<Codec, …>` must list every member. A
// union-keyed table would compile clean when a new *wire* codec appeared —
// failing open on precisely the side that changes outside the SDK's control.
//
// The send direction is DERIVED from this table rather than hand-written, so
// the two directions cannot silently disagree.

import { Codec } from '../proto/dark_tower/signaling/v1/signaling_pb.js';

/**
 * Stable, public codec vocabulary. Mirrors {@link ParticipantLeaveReason}'s
 * const-object-union shape. Carries only real codecs: there is deliberately no
 * member for `CODEC_UNSPECIFIED`, which is never valid in a declaration or a
 * send directive (ADR-0036 §5).
 */
export const SignalingCodec = {
  Opus: 'opus',
  Vp9: 'vp9',
  Av1: 'av1',
  H264: 'h264',
} as const;

export type SignalingCodec = (typeof SignalingCodec)[keyof typeof SignalingCodec];

/**
 * Exhaustive proto `Codec` → {@link SignalingCodec} table, and the single
 * source of truth for both directions.
 *
 * `null` means "no public representation": `CODEC_UNSPECIFIED` is the
 * fail-closed zero value, and the type system forces it to have an entry here
 * rather than being silently absent.
 */
const CODEC_TO_SIGNALING: Record<Codec, SignalingCodec | null> = {
  [Codec.UNSPECIFIED]: null,
  [Codec.OPUS]: SignalingCodec.Opus,
  [Codec.VP9]: SignalingCodec.Vp9,
  [Codec.AV1]: SignalingCodec.Av1,
  [Codec.H264]: SignalingCodec.H264,
};

/**
 * Send direction, derived from {@link CODEC_TO_SIGNALING} by inverting it —
 * never hand-written as a second table, which is how two vocabularies drift.
 */
const SIGNALING_TO_CODEC: ReadonlyMap<SignalingCodec, Codec> = new Map(
  Object.entries(CODEC_TO_SIGNALING)
    .filter((entry): entry is [string, SignalingCodec] => entry[1] !== null)
    .map(([wire, publicCodec]) => [publicCodec, Number(wire) as Codec]),
);

/**
 * Map a public {@link SignalingCodec} onto its wire value, for populating
 * `ParticipantCapabilities.supportedCodecs`.
 *
 * Returns `undefined` for a value outside the union (possible only if a caller
 * defeats the type system). Callers drop rather than substituting
 * `CODEC_UNSPECIFIED`, which is never valid on the wire.
 */
export function toWireCodec(codec: SignalingCodec): Codec | undefined {
  return SIGNALING_TO_CODEC.get(codec);
}

/**
 * Map a wire `Codec` onto the public union.
 *
 * Returns `null` for `CODEC_UNSPECIFIED` and for any out-of-range value — both
 * are protocol violations on a received message and MUST be rejected by the
 * caller, never defaulted to a codec. Mirrors `errorCodeMap.ts`'s collapse of
 * out-of-range values.
 */
export function fromWireCodec(codec: Codec): SignalingCodec | null {
  return CODEC_TO_SIGNALING[codec] ?? null;
}
