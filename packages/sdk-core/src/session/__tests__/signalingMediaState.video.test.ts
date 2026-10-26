// File: packages/sdk-core/src/session/__tests__/signalingMediaState.video.test.ts
//
// Story 3 C-B: the video server-mute state by VERIFIED sender, through the
// roster's ONE participant <-> sender index, and the mute -> unmute EDGE that
// drives the receiver's `server_mute_lifted` repair.

import { describe, expect, it } from 'vitest';

import { TypedEventEmitter } from '../../events/TypedEventEmitter.js';
import type { SignalingClient } from '../../signaling/SignalingClient.js';
import { SignalingMediaState } from '../signalingMediaState.js';

class FakeSignaling extends TypedEventEmitter<Record<string, unknown>> {
  readonly roster = new Map<string, number>([['p1', 11]]);
  senderIdOf(participantId: string): number | undefined {
    return this.roster.get(participantId);
  }
  participantIdOf(senderId: number): string | undefined {
    for (const [p, s] of this.roster) if (s === senderId) return p;
    return undefined;
  }
  fire(event: string, payload: unknown): void {
    this.emit(event, payload);
  }
}

function setup() {
  const signaling = new FakeSignaling();
  const state = new SignalingMediaState(() => {});
  state.observe(signaling as unknown as SignalingClient);
  const lifted: number[] = [];
  state.onVideoServerMuteLifted((s) => lifted.push(s));
  const mute = (video: boolean, audio = false) =>
    signaling.fire('participantMuteChanged', {
      participantId: 'p1',
      mediaStates: {
        audio: { clientState: 'live', serverMuted: audio },
        video: { clientState: 'live', serverMuted: video },
      },
    });
  return { signaling, state, lifted, mute };
}

describe('SignalingMediaState — video server mute by verified sender', () => {
  it('unmapped / unmuted / muted', () => {
    const h = setup();
    expect(h.state.videoMuteState(99)).toBe('unmapped');
    expect(h.state.videoMuteState(11)).toBe('unmuted');
    h.mute(true);
    expect(h.state.videoMuteState(11)).toBe('muted');
  });

  it('fires the lift ONLY on the video mute -> unmute edge, with the roster sender id', () => {
    const h = setup();
    h.mute(false);
    h.mute(true);
    h.mute(true, true);
    expect(h.lifted).toEqual([]);
    h.mute(false);
    expect(h.lifted).toEqual([11]);
    h.mute(false);
    expect(h.lifted).toEqual([11]);
  });

  it('an unsubscribed listener hears nothing', () => {
    const h = setup();
    h.state.onVideoServerMuteLifted(undefined);
    h.mute(true);
    h.mute(false);
    expect(h.lifted).toEqual([]);
  });
});
