// Story 3 R-15 / R-17: audio and video client mute are independent.

import { describe, expect, it } from 'vitest';

import { MuteState } from '../muteState.js';

describe('MuteState — per kind', () => {
  it('starts unmuted in both kinds', () => {
    expect(new MuteState().snapshot()).toEqual({ audioMuted: false, videoMuted: false });
  });

  it('muting audio never changes video, and vice versa', () => {
    const m = new MuteState();
    expect(m.setAudioMuted(true)).toBe(true);
    expect(m.snapshot()).toEqual({ audioMuted: true, videoMuted: false });
    expect(m.setVideoMuted(true)).toBe(true);
    expect(m.setAudioMuted(false)).toBe(true);
    expect(m.snapshot()).toEqual({ audioMuted: false, videoMuted: true });
    expect(m.isMuted('video')).toBe(true);
    expect(m.isMuted('audio')).toBe(false);
  });

  it('reports only real transitions', () => {
    const m = new MuteState();
    expect(m.setVideoMuted(false)).toBe(false);
    expect(m.setVideoMuted(true)).toBe(true);
    expect(m.setVideoMuted(true)).toBe(false);
  });
});
