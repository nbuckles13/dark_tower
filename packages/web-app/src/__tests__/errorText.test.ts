// File: packages/web-app/src/__tests__/errorText.test.ts
//
// Story 2 R-1: the over-cap refusal reaches the user as a VISIBLE error naming
// both numbers (from the SDK) and the knobs to change (this app's vocabulary).

import { describe, expect, it } from 'vitest';
import { SignalingError, SignalingErrorCode } from '@darktower/sdk-core';

import { errorText } from '../lib/errorText.js';

describe('errorText', () => {
  it('names the knobs for a receive-slot declaration over the server cap', () => {
    const text = errorText(
      new SignalingError(
        SignalingErrorCode.ReceiveSlotsOverCap,
        'this client is configured to receive 5 audio slots, but the meeting controller allows at most 4',
      ),
    );
    expect(text).toContain('5');
    expect(text).toContain('4');
    expect(text).toContain('VITE_DT_RECEIVE_SLOTS');
    expect(text).toContain('MC_MAX_RECEIVE_SLOTS');
  });

  it('leaves other SDK errors in the generic form', () => {
    expect(errorText(new SignalingError(SignalingErrorCode.Timeout, 'join timed out'))).toBe(
      'SIGNALING: join timed out',
    );
  });
});
