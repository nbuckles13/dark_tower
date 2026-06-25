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
import { ErrorCode, LeaveReason } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';

describe('mapLeaveReason', () => {
  it('maps every proto LeaveReason to its bounded public reason', () => {
    expect(mapLeaveReason(LeaveReason.VOLUNTARY)).toBe(ParticipantLeaveReason.Voluntary);
    expect(mapLeaveReason(LeaveReason.KICKED)).toBe(ParticipantLeaveReason.Kicked);
    expect(mapLeaveReason(LeaveReason.CONNECTION_LOST)).toBe(ParticipantLeaveReason.ConnectionLost);
    expect(mapLeaveReason(LeaveReason.MEETING_ENDED)).toBe(ParticipantLeaveReason.MeetingEnded);
    expect(mapLeaveReason(LeaveReason.TIMEOUT)).toBe(ParticipantLeaveReason.Timeout);
  });

  it('collapses an unknown LeaveReason to Unknown', () => {
    expect(mapLeaveReason(999 as LeaveReason)).toBe(ParticipantLeaveReason.Unknown);
  });
});

describe('mapErrorCode / isAuthClass', () => {
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
