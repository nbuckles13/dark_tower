// File: packages/sdk-core/src/media/pipeline/__tests__/renderDecision.test.ts
//
// The pure render mapping (story 3 R-4): every wire state token has exactly one
// rule; only `active` is live; never-emitted states fail closed; the avatar
// reason IS the token.

import { describe, expect, it } from 'vitest';

import {
  RENDER_RULES,
  isAvatarState,
  mayRender,
  sameRenderState,
  staticRenderState,
} from '../renderDecision.js';

describe('render rules', () => {
  it('only `active` is live, and exactly the three avatar tokens are avatars', () => {
    const live = Object.entries(RENDER_RULES)
      .filter(([, r]) => r === 'live')
      .map(([t]) => t);
    const avatars = Object.entries(RENDER_RULES)
      .filter(([, r]) => r === 'avatar')
      .map(([t]) => t);
    expect(live).toEqual(['active']);
    expect(avatars.sort()).toEqual([
      'avatar_client_muted',
      'avatar_no_video',
      'avatar_server_muted',
    ]);
    for (const t of avatars) expect(isAvatarState(t as never)).toBe(true);
    expect(isAvatarState('active')).toBe(false);
  });

  it('maps non-live states and fails closed for the never-emitted ones and for no assignment', () => {
    expect(staticRenderState('active', undefined)).toBeUndefined();
    expect(staticRenderState('avatar_server_muted', 4)).toEqual({
      kind: 'avatar',
      participantSenderId: 4,
      reason: 'avatar_server_muted',
    });
    expect(staticRenderState('unavailable', undefined)).toEqual({ kind: 'unavailable' });
    expect(staticRenderState('fewer_sources_than_slots', undefined)).toEqual({
      kind: 'fewer_sources',
    });
    for (const t of [
      'unspecified',
      'switch_pending',
      'withheld_by_congestion',
      'source_unreachable',
    ] as const) {
      expect(staticRenderState(t, 3)).toEqual({ kind: 'none' });
    }
    expect(staticRenderState(undefined, undefined)).toEqual({ kind: 'none' });
  });

  it('mayRender refuses self, muted and unmapped senders', () => {
    const guards = (state: 'muted' | 'unmuted' | 'unmapped') => ({
      selfSenderId: 1,
      videoMuteState: () => state,
    });
    expect(mayRender(2, guards('unmuted'))).toBe(true);
    expect(mayRender(1, guards('unmuted'))).toBe(false);
    expect(mayRender(2, guards('muted'))).toBe(false);
    expect(mayRender(2, guards('unmapped'))).toBe(false);
  });

  it('sameRenderState compares kind, sender, reason and participant', () => {
    expect(sameRenderState({ kind: 'active', senderId: 1 }, { kind: 'active', senderId: 1 })).toBe(
      true,
    );
    expect(sameRenderState({ kind: 'active', senderId: 1 }, { kind: 'active', senderId: 2 })).toBe(
      false,
    );
    const avatar = { kind: 'avatar', participantSenderId: 1, reason: 'avatar_no_video' } as const;
    expect(sameRenderState(avatar, { ...avatar })).toBe(true);
    expect(sameRenderState(avatar, { ...avatar, reason: 'avatar_client_muted' })).toBe(false);
    expect(sameRenderState({ kind: 'none' }, { kind: 'unavailable' })).toBe(false);
    expect(sameRenderState({ kind: 'none' }, { kind: 'none' })).toBe(true);
  });
});

describe('classifySender — the ONE classifier, two site mappings', () => {
  it('self / muted / unmapped / ok, in that precedence', async () => {
    const { classifySender } = await import('../renderDecision.js');
    const guards = (state: 'muted' | 'unmuted' | 'unmapped') => ({
      selfSenderId: 1,
      videoMuteState: () => state,
    });
    expect(classifySender(1, guards('muted'))).toBe('self');
    expect(classifySender(2, guards('muted'))).toBe('muted');
    expect(classifySender(2, guards('unmapped'))).toBe('unmapped');
    expect(classifySender(2, guards('unmuted'))).toBe('ok');
  });
});
