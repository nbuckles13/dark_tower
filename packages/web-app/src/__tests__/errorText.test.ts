// File: packages/web-app/src/__tests__/errorText.test.ts
//
// Story 3 R-2: the local "neither publishes nor receives" refusal and the
// not-yet-supported video publish reach the user as VISIBLE errors naming this
// app's knobs. Story 2's over-cap refusal is retired (MC truncates; story 3 R-1).

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

  it('explains a not-yet-supported publish kind', () => {
    const text = errorText(
      new SignalingError(
        SignalingErrorCode.PublishKindNotSupported,
        'publishing video is not yet supported by this client',
      ),
    );
    expect(text).toContain('not yet supported');
    expect(text).toContain('audio only');
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
