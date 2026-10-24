// File: packages/web-app/src/__tests__/errorText.test.ts
//
// Story 3 R-2: the local "neither publishes nor receives" refusal and the
// not-yet-supported video publish reach the user as VISIBLE errors naming this
// app's knobs (and, since C-C1, the publish-set chooser). Story 2's over-cap refusal is retired (MC truncates; story 3 R-1).

import { describe, expect, it } from 'vitest';
import { SignalingError, SignalingErrorCode } from '@darktower/sdk-core';

import { errorText } from '../lib/errorText.js';

describe('errorText', () => {
  it('names BOTH per-kind slot variables for the nothing-to-publish-or-receive refusal', () => {
    const text = errorText(
      new SignalingError(SignalingErrorCode.NothingToPublishOrReceive, 'nothing to do'),
    );
    expect(text).toContain('SIGNALING:');
    expect(text).toContain('VITE_DT_AUDIO_RECEIVE_SLOTS');
    expect(text).toContain('VITE_DT_VIDEO_RECEIVE_SLOTS');
  });

  it('explains a malformed publish set (story 3 C-C1: video publishes now; the refusal is a duplicate)', () => {
    const text = errorText(
      new SignalingError(SignalingErrorCode.PublishKindNotSupported, 'duplicate publish kind'),
    );
    expect(text).toContain('duplicate publish kind');
    expect(text).toContain('at most once');
    expect(text).not.toContain('audio only');
  });

  it('the nothing-to-publish refusal names the chooser as well as the build knobs', () => {
    const text = errorText(
      new SignalingError(SignalingErrorCode.NothingToPublishOrReceive, 'nothing to do'),
    );
    expect(text).toContain('Choose the microphone or the camera');
  });

  it('never names the retired variable', () => {
    for (const code of [
      SignalingErrorCode.NothingToPublishOrReceive,
      SignalingErrorCode.PublishKindNotSupported,
    ]) {
      expect(errorText(new SignalingError(code, 'x'))).not.toContain('VITE_DT_RECEIVE_SLOTS');
    }
  });

  it('leaves other SDK errors in the generic form', () => {
    expect(errorText(new SignalingError(SignalingErrorCode.Timeout, 'join timed out'))).toBe(
      'SIGNALING: join timed out',
    );
  });
});
