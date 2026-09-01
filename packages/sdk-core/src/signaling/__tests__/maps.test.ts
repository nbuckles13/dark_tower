// File: packages/sdk-core/src/signaling/__tests__/maps.test.ts
//
// Direct unit coverage for the pure mapping helpers (R-17/R-18): the proto
// `LeaveReason` → bounded public reason, and the proto `ErrorCode` →
// `SignalingErrorCode` table (incl. the defensive out-of-range fallback +
// auth-class classification).

import { describe, expect, it } from 'vitest';

import { isAuthClass, mapErrorCode, staticMessageFor } from '../errorCodeMap.js';
import { ParticipantLeaveReason, mapLeaveReason } from '../events.js';
import { SignalingErrorCode } from '../../errors/SignalingError.js';
import { Codec, ErrorCode, LeaveReason } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { SignalingCodec, fromWireCodec, toWireCodec } from '../codecMap.js';

describe('mapLeaveReason', () => {
  it('maps every proto LeaveReason to its bounded public reason', () => {
    expect(mapLeaveReason(LeaveReason.VOLUNTARY)).toBe(ParticipantLeaveReason.Voluntary);
    expect(mapLeaveReason(LeaveReason.KICKED)).toBe(ParticipantLeaveReason.Kicked);
    expect(mapLeaveReason(LeaveReason.CONNECTION_LOST)).toBe(ParticipantLeaveReason.ConnectionLost);
    expect(mapLeaveReason(LeaveReason.MEETING_ENDED)).toBe(ParticipantLeaveReason.MeetingEnded);
    expect(mapLeaveReason(LeaveReason.TIMEOUT)).toBe(ParticipantLeaveReason.Timeout);
  });

  it('collapses the UNSPECIFIED zero value to Unknown', () => {
    // ADR-0036 reshape: LeaveReason renumbered so 0 is UNSPECIFIED (the five
    // real reasons moved to 5..9, and 1..4 are reserved). Value 0 is the one
    // change proto3 forces to be a re-point rather than a reservation, so its
    // new meaning is asserted rather than assumed.
    expect(LeaveReason.UNSPECIFIED).toBe(0);
    expect(mapLeaveReason(LeaveReason.UNSPECIFIED)).toBe(ParticipantLeaveReason.Unknown);
  });

  it('collapses an unknown LeaveReason to Unknown', () => {
    expect(mapLeaveReason(999 as LeaveReason)).toBe(ParticipantLeaveReason.Unknown);
  });
});

describe('mapErrorCode / isAuthClass', () => {
  it('maps the UNSPECIFIED zero value to Unknown', () => {
    // ErrorCode is NOT renumbered: UNKNOWN -> ERROR_CODE_UNSPECIFIED is the
    // same number with the same meaning, so only the name moved.
    expect(ErrorCode.UNSPECIFIED).toBe(0);
    expect(mapErrorCode(ErrorCode.UNSPECIFIED)).toBe(SignalingErrorCode.Unknown);
  });

  it('maps each proto ErrorCode and flags only UNAUTHORIZED/FORBIDDEN as auth-class', () => {
    expect(mapErrorCode(ErrorCode.UNAUTHORIZED)).toBe(SignalingErrorCode.Unauthorized);
    expect(isAuthClass(SignalingErrorCode.Unauthorized)).toBe(true);
    expect(isAuthClass(SignalingErrorCode.Forbidden)).toBe(true);
    expect(isAuthClass(SignalingErrorCode.CapacityExceeded)).toBe(false);
    expect(isAuthClass(SignalingErrorCode.Framing)).toBe(false);
  });

  it('collapses an out-of-range ErrorCode to Unknown', () => {
    expect(mapErrorCode(42 as ErrorCode)).toBe(SignalingErrorCode.Unknown);
  });

  it('provides a static fallback message for every signaling code', () => {
    for (const code of Object.values(SignalingErrorCode)) {
      expect(staticMessageFor(code)).toBeTruthy();
    }
  });
});

describe('codecMap — the one wire-keyed codec oracle (ADR-0036 §5)', () => {
  it('maps every real wire codec onto the public union and back', () => {
    // Round trip through both directions. The send direction is DERIVED from
    // the same table, so this also proves the derivation did not drop or
    // mis-key an entry.
    for (const [wire, expected] of [
      [Codec.OPUS, SignalingCodec.Opus],
      [Codec.VP9, SignalingCodec.Vp9],
      [Codec.AV1, SignalingCodec.Av1],
      [Codec.H264, SignalingCodec.H264],
    ] as const) {
      expect(fromWireCodec(wire)).toBe(expected);
      expect(toWireCodec(expected)).toBe(wire);
    }
  });

  it('rejects CODEC_UNSPECIFIED rather than defaulting to a codec', () => {
    // Zero is not a silent Opus. On receipt it is a protocol violation, and
    // the oracle must surface that as null rather than picking a codec.
    expect(fromWireCodec(Codec.UNSPECIFIED)).toBeNull();
  });

  it('collapses an out-of-range wire codec to null', () => {
    expect(fromWireCodec(42 as Codec)).toBeNull();
  });
});
