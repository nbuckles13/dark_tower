// Story 3 R-18: the "capture resumed after server mute" fail-safe, as a pure
// state machine. Every case asserts an EXACT event count.

import { describe, expect, it } from 'vitest';

import { ServerMuteResumeTracker, type ResumeKind } from '../serverMuteResume.js';

function rig(clientMuted: Partial<Record<ResumeKind, boolean>> = {}) {
  const events: ResumeKind[] = [];
  const tracker = new ServerMuteResumeTracker(
    (kind) => clientMuted[kind] ?? false,
    (kind) => events.push(kind),
  );
  return {
    events,
    mute: (kind: ResumeKind, muted: boolean) =>
      tracker.apply({ type: 'selfServerMute', kind, muted }),
    targets: (kind: ResumeKind, nonEmpty: boolean) =>
      tracker.apply({ type: 'effectiveTargets', kind, nonEmpty }),
  };
}

describe('R-18: capture resumed after server mute', () => {
  it('fires once when the unmute update arrives BEFORE the resume', () => {
    const r = rig();
    r.targets('audio', true);
    r.mute('audio', true);
    r.targets('audio', false); // withdrawn
    r.mute('audio', false); // update first
    r.targets('audio', true); // then resume
    expect(r.events).toEqual(['audio']);
  });

  it('fires once when the resume arrives BEFORE the unmute update', () => {
    const r = rig();
    r.targets('audio', true);
    r.mute('audio', true);
    r.targets('audio', false);
    r.targets('audio', true); // resume first
    expect(r.events).toEqual(['audio']);
    r.mute('audio', false); // the later clearing update
    expect(r.events).toEqual(['audio']); // ...raises NO second event
  });

  it('fires once when the unmute update is LOST', () => {
    const r = rig();
    r.targets('video', true);
    r.mute('video', true);
    r.targets('video', false);
    r.targets('video', true);
    expect(r.events).toEqual(['video']);
  });

  it('a later clearing update after the event raises no second event, nor does a later resume', () => {
    const r = rig();
    r.mute('audio', true);
    r.targets('audio', true);
    expect(r.events).toEqual(['audio']);
    r.mute('audio', false);
    r.targets('audio', false);
    r.targets('audio', true);
    expect(r.events).toEqual(['audio']);
  });

  it('client-muted: no capture resumes, so NO event (and the latch is consumed)', () => {
    const r = rig({ audio: true });
    r.mute('audio', true);
    r.targets('audio', true);
    expect(r.events).toEqual([]);
  });

  it('RESIDUAL, BY DESIGN: with no server mute ever recorded, a resume raises no notice', () => {
    // The participant was never told they were muted, so a resume (e.g. someone
    // started watching) is not news to them.
    const r = rig();
    r.targets('audio', true);
    r.targets('audio', false);
    r.targets('audio', true);
    expect(r.events).toEqual([]);
  });

  it('false-positive side: an unwatched -> watched edge with no latch emits nothing', () => {
    const r = rig();
    r.targets('video', false);
    r.targets('video', true);
    expect(r.events).toEqual([]);
  });

  it('latch-clear branch: mute, then unmute while STILL sending, then a later resume edge -> 0 events', () => {
    // A controller that does not withdraw on server mute (pre-MC-A): capture
    // never stopped, so the unmute clears the latch and a later unwatched ->
    // watched edge is not a server-unmute resume.
    const r = rig();
    r.targets('audio', true);
    r.mute('audio', true);
    r.mute('audio', false); // targets still non-empty
    r.targets('audio', false);
    r.targets('audio', true);
    expect(r.events).toEqual([]);
  });

  it('kinds are independent: an audio latch never fires for video', () => {
    const r = rig();
    r.mute('audio', true);
    r.targets('video', true);
    expect(r.events).toEqual([]);
    r.targets('audio', true);
    expect(r.events).toEqual(['audio']);
  });

  it('a repeated non-empty directive is not an edge', () => {
    const r = rig();
    r.mute('audio', true);
    r.targets('audio', true);
    r.targets('audio', true);
    expect(r.events).toEqual(['audio']);
  });
});
