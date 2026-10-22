// File: packages/sdk-core/src/media/lifecycle/__tests__/cameraGate.test.ts
//
// Story 3 R-16/R-17/R-19 + security S1: the camera-light predicate and the
// failure latch, as a pure state machine — no fake tracks needed.

import { describe, expect, it } from 'vitest';

import {
  cameraWanted,
  initialCameraGate,
  stepCameraGate,
  type CameraGateEvent,
  type CameraGateState,
} from '../cameraGate.js';

/** Apply events, collecting the actions. */
function run(events: CameraGateEvent[], start: CameraGateState = initialCameraGate()) {
  let state = start;
  const acquires: number[] = [];
  const stops: number[] = [];
  const presented: boolean[] = [];
  events.forEach((event, i) => {
    const out = stepCameraGate(state, event);
    state = out.state;
    if (out.actions.acquire) acquires.push(i);
    if (out.actions.stop) stops.push(i);
    if (out.actions.presentMuted !== undefined) presented.push(out.actions.presentMuted);
  });
  return { state, acquires, stops, presented };
}

const directed: CameraGateEvent = { type: 'directive', nonEmpty: true };
const withdrawn: CameraGateEvent = { type: 'directive', nonEmpty: false };
const acquired: CameraGateEvent = { type: 'acquired' };

describe('camera light <=> directed with targets AND not client-muted AND healthy', () => {
  it('acquires on direction, stops on withdrawal (unwatched or server-muted), re-acquires on restore', () => {
    const r = run([directed, acquired, withdrawn, directed, acquired]);
    expect(r.acquires).toEqual([0, 3]);
    expect(r.stops).toEqual([2]);
    expect(cameraWanted(r.state)).toBe(true);
  });

  it('client video mute stops it; unmute re-acquires', () => {
    const r = run([
      directed,
      acquired,
      { type: 'userMute', muted: true },
      { type: 'userMute', muted: false },
    ]);
    expect(r.stops).toEqual([2]);
    expect(r.acquires).toEqual([0, 3]);
  });

  it('a re-sent identical directive (no edge) changes nothing', () => {
    const r = run([directed, acquired, directed, directed]);
    expect(r.acquires).toEqual([0]);
    expect(r.stops).toEqual([]);
  });

  it('never lights while muted, whatever MC directs', () => {
    const r = run([{ type: 'userMute', muted: true }, directed, withdrawn, directed]);
    expect(r.acquires).toEqual([]);
  });
});

describe('R-19: a camera failure latches; NO automatic retry', () => {
  it('a track ending presents the participant as video-muted and does not retry by itself', () => {
    const r = run([directed, acquired, { type: 'failed', outcome: 'ended' }, directed, directed]);
    expect(r.presented).toEqual([true]);
    expect(r.acquires).toEqual([0]);
  });

  it.each(['ended', 'device_unavailable', 'no_device', 'unsupported', 'unknown'] as const)(
    'after %s, the next directive RESTORE edge retries exactly once, acquiring BEFORE lifting the mute',
    (outcome) => {
      const r = run([
        directed,
        acquired,
        { type: 'failed', outcome },
        withdrawn,
        directed, // restore edge: retry
      ]);
      expect(r.acquires).toEqual([0, 4]);
      // Still presented as muted until the track is live.
      expect(r.state.muted).toBe(true);
      expect(r.presented).toEqual([true]);
      const after = stepCameraGate(r.state, acquired);
      expect(after.actions.presentMuted).toBe(false);
      expect(after.state.failure).toBeUndefined();
    },
  );

  it.each(['permission_denied', 'platform_unsupported'] as const)(
    'after %s, N restore edges trigger ZERO acquires (no prompt loop); only a user unmute retries',
    (outcome) => {
      const events: CameraGateEvent[] = [directed, { type: 'failed', outcome }];
      for (let i = 0; i < 5; i += 1) events.push(withdrawn, directed);
      const r = run(events);
      expect(r.acquires).toEqual([0]);
      const unmuted = stepCameraGate(r.state, { type: 'userMute', muted: false });
      expect(unmuted.actions.acquire).toBe(true);
    },
  );

  it('a failed restore retry latches again: still no loop', () => {
    const r = run([
      directed,
      { type: 'failed', outcome: 'ended' },
      withdrawn,
      directed,
      { type: 'failed', outcome: 'device_unavailable' },
      directed,
      directed,
    ]);
    expect(r.acquires).toEqual([0, 3]);
  });

  it('a USER mute clears the latch: a later restore does not unmute them', () => {
    const r = run([
      directed,
      { type: 'failed', outcome: 'ended' },
      { type: 'userMute', muted: true },
      withdrawn,
      directed,
    ]);
    expect(r.acquires).toEqual([0]);
    expect(r.state.failure).toBeUndefined();
    expect(r.state.muted).toBe(true);
  });

  it('a user unmute is always a retry edge, even after permission_denied', () => {
    const r = run([
      directed,
      { type: 'failed', outcome: 'permission_denied' },
      { type: 'userMute', muted: false },
    ]);
    expect(r.acquires).toEqual([0, 2]);
  });
});
