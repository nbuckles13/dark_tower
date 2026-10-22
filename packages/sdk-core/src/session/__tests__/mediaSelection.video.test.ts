// File: packages/sdk-core/src/session/__tests__/mediaSelection.video.test.ts
//
// Story 3 R-27: THE one video capture-source substitution point. The sibling of
// meeting-session.test.ts's "DEFAULT capture is the microphone" test.

import { readFileSync } from 'node:fs';

import { describe, expect, it } from 'vitest';

import { selectVideoCaptureSource } from '../mediaSelection.js';
import { createCameraCapture } from '../../media/setup/videoCapture.js';

describe('selectVideoCaptureSource', () => {
  it('the DEFAULT video capture is the camera when __DT_TEST_PATTERN__ is off', () => {
    expect(__DT_TEST_PATTERN__).toBe(false);
    const chosen = selectVideoCaptureSource(undefined, 258);
    expect(chosen.info).toEqual({ mode: 'camera' });
    expect(chosen.factory).toBe(createCameraCapture);
  });

  it('an injected factory is respected and reported as the camera', () => {
    const injected = async () => ({
      start: async () => undefined,
      stop: () => undefined,
      grantedDeviceId: undefined,
    });
    const chosen = selectVideoCaptureSource(injected, 258);
    expect(chosen.factory).toBe(injected);
    expect(chosen.info).toEqual({ mode: 'camera' });
  });

  it('STRUCTURAL: the pattern source, its factory and the mode token sit only inside the gate', () => {
    const source = readFileSync(new URL('../mediaSelection.ts', import.meta.url), 'utf8');
    const gate = source.indexOf('if (__DT_TEST_PATTERN__) {');
    expect(gate).toBeGreaterThan(0);
    for (const token of ['new TestPatternSource(', 'createTestPatternCapture(', "'test_pattern'"]) {
      const at = source.indexOf(token);
      expect(at, token).toBeGreaterThan(gate);
      expect(source.lastIndexOf(token), `${token} appears outside the gate`).toBe(at);
    }
  });
});
