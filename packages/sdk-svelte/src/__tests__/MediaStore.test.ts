// File: packages/sdk-svelte/src/__tests__/MediaStore.test.ts
//
// The media stores, and the two properties they exist to provide:
//
//   1. Every value is a PROJECTION of an SDK event, never a derivation. The
//      indicator a user reads must be the state that gated capture (ADR-0036
//      §5), so a test that drove the store by any other route would be testing
//      the wrong thing.
//   2. A mute toggle does not invalidate the roster projection ("minimizing
//      re-renders on mute and unmute"). Asserted by COUNTING recomputations,
//      with a positive control — a claim about re-renders that is not measured
//      is a comment.

import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { MeetingSessionState } from '@darktower/sdk-core';
import type { StreamAssignmentEvent, VideoReceiveView, VideoSink } from '@darktower/sdk-core';
import type { TileSurface } from '../components/VideoTile.svelte';
import { MediaStore } from '../stores/MediaStore.svelte.js';
import BoundHarness from './helpers/BoundHarness.svelte';
import GranularityHarness from './helpers/GranularityHarness.svelte';
import CellGranularityHarness from './helpers/CellGranularityHarness.svelte';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

function slot(overrides: Partial<StreamAssignmentEvent> = {}): StreamAssignmentEvent {
  return {
    slotId: 0,
    mediaKind: 'audio',
    senderId: 258,
    mediaHandlerUrl: 'https://mh-0:4433',
    slotState: 'active',
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// The container, driven directly
// ---------------------------------------------------------------------------

test('starts unmuted with no media, no slots and no fault', () => {
  const store = new MediaStore();
  // `false`, not "unknown": `MuteState` starts unmuted, so there is no third
  // state for a UI to mishandle.
  expect(store.audioMuted).toBe(false);
  expect(store.firstMediaFrameMs).toBeUndefined();
  expect(store.hasReceivedMedia).toBe(false);
  expect(store.slots).toEqual([]);
  expect(store.lastMediaFault).toBeUndefined();
});

test('mute state mirrors the SDK snapshot in both directions', () => {
  const store = new MediaStore();
  store.applyMuteChanged({ audioMuted: true, videoMuted: false });
  expect(store.audioMuted).toBe(true);
  store.applyMuteChanged({ audioMuted: false, videoMuted: false });
  expect(store.audioMuted).toBe(false);
});

test('first-media is recorded once and never moves', () => {
  // The SDK observes this at most once per session; a second write would turn a
  // startup measurement into something that drifts, and a UI showing a changing
  // "time to first audio" would be reporting a number that means nothing.
  const store = new MediaStore();
  store.applyFirstMediaFrame(87);
  store.applyFirstMediaFrame(9_999);
  expect(store.firstMediaFrameMs).toBe(87);
  expect(store.hasReceivedMedia).toBe(true);
});

test('slot assignments REPLACE rather than merge', () => {
  // The message is MC's complete current view of this subscriber's slots, so
  // merging would resurrect a slot MC has dropped — a grid cell for a
  // participant the controller has already removed.
  const store = new MediaStore();
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [slot({ slotId: 0 }), slot({ slotId: 1 })],
  });
  expect(store.slots).toHaveLength(2);

  store.applyStreamAssignments({ unreachableSenderIds: [], assignments: [slot({ slotId: 0 })] });
  expect(store.slots).toHaveLength(1);
  expect(store.slots[0]?.slotId).toBe(0);
});

test('the unreachable set REPLACES rather than merges — a flicker does not latch', () => {
  // ---------------------------------------------------------------------------
  // WHAT THIS PINS, AND FOR WHOM
  // ---------------------------------------------------------------------------
  //
  // `StreamAssignments.unreachable_sender_ids` (ADR-0036 §9) names the roster
  // participants THIS subscriber shares no connected handler with. Without a
  // settle rule on the controller side, a peer can enter and leave that set
  // during an ordinary multi-handler join, between its first and second handler
  // connection.
  //
  // So the property that matters operationally is not "unreachable renders" — it
  // is that leaving the set returns to normal WITHOUT needing a "now reachable"
  // signal of its own. A merged set would latch the transient into a permanent
  // "can't hear X" badge on a meeting whose audio is fine, which is
  // indistinguishable to the user from the real fault and sends oncall chasing a
  // connectivity break that does not exist.
  //
  // This is the seam task 15 renders from, so the no-latch property is pinned
  // here, where it is observable, rather than asserted of a DOM that does not
  // exist yet.
  const store = new MediaStore();
  expect(store.unreachableSenderIds).toEqual([]);

  // B and C are each unreachable to the other: the canonical three-participant
  // case, seen from B.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [259, 260],
  });
  expect(store.unreachableSenderIds).toEqual([259, 260]);

  // C's second handler connection lands. C is reachable now, and says so only by
  // being ABSENT from the new set.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [259],
  });
  expect(store.unreachableSenderIds).toEqual([259]);

  // Everyone is connected everywhere: the set empties. A merge would still be
  // holding 259 and 260 here, which is the bug.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [],
  });
  expect(store.unreachableSenderIds).toEqual([]);
});

test('the unreachable set is independent of the slot list', () => {
  // A message MAY carry zero assignments and a non-empty unreachable set — every
  // other participant is elsewhere — and that is well-formed, not malformed
  // (`signaling.proto`). An unreachable participant consumes NO slot, so the two
  // must not be derived from one another in either direction.
  const store = new MediaStore();
  store.applyStreamAssignments({ assignments: [], unreachableSenderIds: [259] });
  expect(store.slots).toEqual([]);
  expect(store.unreachableSenderIds).toEqual([259]);

  // And slots can refill while the set stays put.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [259],
  });
  expect(store.slots).toHaveLength(1);
  expect(store.unreachableSenderIds).toEqual([259]);
});

test('slot state is carried through verbatim, including the muted and unreachable cases', () => {
  // ADR-0036 §6: "withheld by congestion", "fewer sources" and "source
  // unreachable" are indistinguishable to a client — all present as no media —
  // and render completely differently. The store must not collapse them.
  const store = new MediaStore();
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, slotState: 'avatar_client_muted' }),
      slot({ slotId: 1, slotState: 'withheld_by_congestion' }),
      slot({ slotId: 2, slotState: 'source_unreachable' }),
    ],
  });
  expect(store.slots.map((s) => s.slotState)).toEqual([
    'avatar_client_muted',
    'withheld_by_congestion',
    'source_unreachable',
  ]);
});

test('media faults are held as the bounded SDK value', () => {
  const store = new MediaStore();
  store.applyMediaFault({ stage: 'decoder', message: 'the audio decoder failed', fatal: true });
  expect(store.lastMediaFault?.stage).toBe('decoder');
});

// ---------------------------------------------------------------------------
// Bound through a real component, via real SDK events
// ---------------------------------------------------------------------------

test('bindMeetingSession wires all four media events into store.media', async () => {
  const session = new MockMeetingSession();
  const screen = await render(BoundHarness, { session });

  await expect.element(screen.getByTestId('audio-muted')).toHaveTextContent('unmuted');

  session.fire('muteChanged', { audioMuted: true, videoMuted: false });
  await expect.element(screen.getByTestId('audio-muted')).toHaveTextContent('muted');

  session.fire('firstMediaFrame', 42);
  await expect.element(screen.getByTestId('first-media-ms')).toHaveTextContent('42');

  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [slot(), slot({ slotId: 1 })],
  });
  await expect.element(screen.getByTestId('slot-count')).toHaveTextContent('2');

  session.fire('mediaFault', { stage: 'playback', message: 'playback stopped', fatal: false });
  await expect.element(screen.getByTestId('media-fault')).toHaveTextContent('playback');
});

test('unmount tears down the media subscriptions too — no leak', async () => {
  // The four media listeners join the SAME aggregate unsubscribe as the roster
  // listeners. A separate binding helper would have been a second teardown to
  // forget, which is what this asserts did not happen.
  const session = new MockMeetingSession();
  const screen = await render(BoundHarness, { session });

  session.fire('muteChanged', { audioMuted: true, videoMuted: false });
  await expect.element(screen.getByTestId('audio-muted')).toHaveTextContent('muted');

  screen.unmount();

  expect(() => {
    session.fire('muteChanged', { audioMuted: false, videoMuted: false });
    session.fire('firstMediaFrame', 1);
    session.fire('streamAssignments', { unreachableSenderIds: [], assignments: [slot()] });
    session.fire('mediaFault', { stage: 'crypto', message: 'x', fatal: false });
  }).not.toThrow();
});

// ---------------------------------------------------------------------------
// Re-render granularity
// ---------------------------------------------------------------------------

test('a mute toggle does not recompute the roster projection', async () => {
  const counts = { roster: 0, mute: 0 };
  const session = new MockMeetingSession();
  const screen = await render(GranularityHarness, { session, counts });

  session.fire('stateChange', MeetingSessionState.Joined);
  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann');

  const rosterBaseline = counts.roster;
  const muteBaseline = counts.mute;

  session.fire('muteChanged', { audioMuted: true, videoMuted: false });
  await expect.element(screen.getByTestId('mute-text')).toHaveTextContent('muted');
  session.fire('muteChanged', { audioMuted: false, videoMuted: false });
  await expect.element(screen.getByTestId('mute-text')).toHaveTextContent('unmuted');

  // The property: two mute transitions, zero roster recomputations. This is
  // what the separate `$state` cell in `MediaStore` and the non-reactive
  // `MeetingStore.media` field buy; folding mute into the roster store, or into
  // one `$state` object holding both, breaks exactly this line.
  expect(counts.roster).toBe(rosterBaseline);

  // POSITIVE CONTROL — without it the assertion above passes just as well on a
  // harness whose derivations never recompute at all (an unrendered `$derived`,
  // a broken subscription, a store that stopped updating).
  expect(counts.mute).toBeGreaterThan(muteBaseline);

  const rosterBeforeJoin = counts.roster;
  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann,Bob');
  expect(counts.roster).toBeGreaterThan(rosterBeforeJoin);
});

test('a roster change does not recompute the mute projection', async () => {
  // The inverse direction, which matters during a call: a participant joining
  // must not invalidate the mute indicator every grid re-layout.
  const counts = { roster: 0, mute: 0 };
  const session = new MockMeetingSession();
  const screen = await render(GranularityHarness, { session, counts });

  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann');

  const muteBaseline = counts.mute;
  const rosterBaseline = counts.roster;

  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann,Bob');

  expect(counts.mute).toBe(muteBaseline);
  expect(counts.roster).toBeGreaterThan(rosterBaseline);
});

// ---------------------------------------------------------------------------
// Server mute + unmute requests (story 2 R-10/R-11, story 3 R-15) — wire-only,
// per participant, per kind
// ---------------------------------------------------------------------------

test('server mutes come ONLY from participantMuteChanged, per participant and per kind', () => {
  const store = new MediaStore();
  expect(store.participant('b').audioServerMuted).toBe(false);
  expect(store.participant('b').videoServerMuted).toBe(false);
  store.applyParticipantMute({
    participantId: 'b',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'h',
  });
  expect(store.participant('b').audioServerMuted).toBe(true);
  expect(store.participant('b').videoServerMuted).toBe(false);
  expect(store.participant('b').serverMutedBy).toBe('h');
  store.applyParticipantMute({
    participantId: 'c',
    audioServerMuted: false,
    videoServerMuted: true,
  });
  // Another participant's update leaves b exactly as it was.
  expect(store.participant('b').audioServerMuted).toBe(true);
  expect(store.participant('c').videoServerMuted).toBe(true);
  store.applyParticipantMute({
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: false,
    serverMutedBy: 'h',
  });
  expect(store.participant('b').audioServerMuted).toBe(false);
  // Who muted is meaningful only while muted.
  expect(store.participant('b').serverMutedBy).toBeUndefined();
});

test('an unmute request is held only while the requester IS server-muted, and clears on lift', () => {
  const store = new MediaStore();
  // A relay for someone not server-muted is not a pending request.
  store.applyUnmuteRequested({ participantId: 'b' });
  expect(store.participant('b').unmuteRequested).toBe(false);

  store.applyParticipantMute({
    participantId: 'b',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'h',
  });
  store.applyUnmuteRequested({ participantId: 'b' });
  expect(store.participant('b').unmuteRequested).toBe(true);
  // The request lifts NOTHING: b is still server-muted (R-10).
  expect(store.participant('b').audioServerMuted).toBe(true);

  // Lifting audio while VIDEO stays server-muted keeps the request pending.
  store.applyParticipantMute({
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: true,
  });
  expect(store.participant('b').unmuteRequested).toBe(true);
  store.applyParticipantMute({
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: false,
  });
  expect(store.participant('b').unmuteRequested).toBe(false);
});

// ---------------------------------------------------------------------------
// Story 3 C-C1: per-slot cells, the pairing view, the R-18 latch, camera health
// ---------------------------------------------------------------------------

test('a slot cell changes only when THAT slot changes, and reads undefined once MC drops it', () => {
  const store = new MediaStore();
  expect(store.slot(3).assignment).toBeUndefined();
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 7 }),
      slot({ slotId: 3, mediaKind: 'video', senderId: undefined }),
    ],
  });
  const zero = store.slot(0).assignment;
  expect(store.slot(3).assignment?.slotState).toBe('active');
  // An equal re-send keeps the SAME object (change-only write).
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 7 }),
      slot({ slotId: 3, mediaKind: 'video', senderId: 9, slotState: 'avatar_client_muted' }),
    ],
  });
  expect(store.slot(0).assignment).toBe(zero);
  expect(store.slot(3).assignment?.slotState).toBe('avatar_client_muted');
  store.applyStreamAssignments({ unreachableSenderIds: [], assignments: [slot({ slotId: 0 })] });
  expect(store.slot(3).assignment).toBeUndefined();
});

test('pairing is BY SENDER: an audio slot by its named sender, a live video slot by its verified sender', () => {
  const store = new MediaStore();
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 7 }),
      slot({ slotId: 1, senderId: 8 }),
      // Live video names no sender (R-5); slot ids deliberately NOT parallel.
      slot({ slotId: 3, mediaKind: 'video', senderId: undefined }),
      slot({ slotId: 4, mediaKind: 'video', senderId: 7, slotState: 'avatar_client_muted' }),
    ],
  });
  expect(store.pairOf(7)).toMatchObject({ audioSlotId: 0, videoSlotId: 4 });
  expect(store.pairOf(8)).toMatchObject({ audioSlotId: 1, videoSlotId: undefined });
  // Slot 3 renders sender 8's verified video: 8 now pairs audio 1 with video 3.
  store.applyVideoRender({ slotId: 3, state: { kind: 'active', senderId: 8 } });
  expect(store.pairOf(8)).toMatchObject({ audioSlotId: 1, videoSlotId: 3 });
  // Live -> avatar for nobody: the live half is withdrawn at once.
  store.applyVideoRender({ slotId: 3, state: { kind: 'fewer_sources' } });
  expect(store.pairOf(8).videoSlotId).toBeUndefined();
});

test('the R-18 notice latches per kind and clears ONLY on acknowledge', () => {
  const store = new MediaStore();
  expect(store.resumeNoticePending('video')).toBe(false);
  store.applyCaptureResumed({ kind: 'video' });
  store.applyCaptureResumed({ kind: 'audio' });
  // A repeat of the same kind is still one pending notice.
  store.applyCaptureResumed({ kind: 'video' });
  // Nothing else clears it: mute changes, assignments, faults, camera health.
  store.applyMuteChanged({ audioMuted: true, videoMuted: true });
  store.applyStreamAssignments({ unreachableSenderIds: [], assignments: [] });
  store.applyMediaFault({ stage: 'capture', message: 'x', fatal: false });
  store.applyCameraHealth({ failure: 'ended' });
  expect(store.resumeNoticePending('video')).toBe(true);
  expect(store.resumeNoticePending('audio')).toBe(true);
  store.acknowledgeResume('video');
  expect(store.resumeNoticePending('video')).toBe(false);
  // The other kind's notice is independent.
  expect(store.resumeNoticePending('audio')).toBe(true);
});

test('camera health and video mute mirror the SDK events', () => {
  const store = new MediaStore();
  expect(store.cameraFailure).toBeUndefined();
  store.applyCameraHealth({ failure: 'permission_denied' });
  expect(store.cameraFailure).toBe('permission_denied');
  store.applyCameraHealth({ failure: undefined });
  expect(store.cameraFailure).toBeUndefined();
  store.applyMuteChanged({ audioMuted: false, videoMuted: true });
  expect(store.videoMuted).toBe(true);
  expect(store.audioMuted).toBe(false);
});

// ---------------------------------------------------------------------------
// Story 3 C-C1 re-render granularity: per tile, per participant, and frames
// ---------------------------------------------------------------------------

test('a slot state change recomputes ONLY that tile; a mute change ONLY that participant', async () => {
  const counts = { tile3: 0, tile4: 0, rowX: 0, rowY: 0 };
  const session = new MockMeetingSession();
  const screen = await render(CellGranularityHarness, { session, counts });

  // A never-written cell is READ first (inside a $derived), then written: the
  // factory constructs only, so no state_unsafe_mutation.
  await expect.element(screen.getByTestId('tile-3')).toHaveTextContent('none');
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 3, mediaKind: 'video', senderId: 7, slotState: 'avatar_no_video' }),
      slot({ slotId: 4, mediaKind: 'video', senderId: 8, slotState: 'avatar_no_video' }),
    ],
  });
  await expect.element(screen.getByTestId('tile-4')).toHaveTextContent('avatar_no_video');

  const base = { ...counts };
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 3, mediaKind: 'video', senderId: 7, slotState: 'avatar_client_muted' }),
      slot({ slotId: 4, mediaKind: 'video', senderId: 8, slotState: 'avatar_no_video' }),
    ],
  });
  await expect.element(screen.getByTestId('tile-3')).toHaveTextContent('avatar_client_muted');
  expect(counts.tile4).toBe(base.tile4);
  expect(counts.tile3).toBeGreaterThan(base.tile3); // positive control

  // The roster creates participant cells (MeetingStore -> registerParticipant),
  // so a FIRST server mute is not a creation edge for anyone else's row.
  session.fire('participantJoined', { participant: { participantId: 'x', name: 'X' } });
  session.fire('participantJoined', { participant: { participantId: 'y', name: 'Y' } });
  await expect.element(screen.getByTestId('row-y')).toHaveTextContent('audio-ok');
  const muteBase = { ...counts };
  session.fire('participantMuteChanged', {
    participantId: 'x',
    audioServerMuted: false,
    videoServerMuted: true,
  });
  await expect.element(screen.getByTestId('row-x')).toHaveTextContent('video-server-muted');
  expect(counts.rowY).toBe(muteBase.rowY);
  expect(counts.tile3).toBe(muteBase.tile3);
  expect(counts.tile4).toBe(muteBase.tile4);
  expect(counts.rowX).toBeGreaterThan(muteBase.rowX); // positive control

  // A render transition is a slot change too: only that tile.
  const renderBase = { ...counts };
  session.fire('videoSlotRenderChanged', { slotId: 4, state: { kind: 'unavailable' } });
  await expect.element(screen.getByTestId('tile-4')).toHaveTextContent('unavailable');
  expect(counts.tile3).toBe(renderBase.tile3);
  expect(counts.rowX).toBe(renderBase.rowX);
});

test('decoded frames NEVER touch the store: a paint recomputes nothing and is counted at the surface', async () => {
  const counts = { tile3: 0, tile4: 0, rowX: 0, rowY: 0 };
  const session = new MockMeetingSession();
  const sinks = new Map<number, VideoSink | undefined>();
  const view: VideoReceiveView = {
    setVideoSink: (slotId, sink) => sinks.set(slotId, sink),
    evidence: () => [],
    unfulfilledSnapshot: () => [],
    localStats: () => [],
  };
  let surface: TileSurface | undefined;
  const screen = await render(CellGranularityHarness, {
    session,
    counts,
    view,
    onSurface: (s: TileSurface | undefined) => {
      surface = s;
    },
  });
  session.fire('videoSlotRenderChanged', { slotId: 3, state: { kind: 'active', senderId: 7 } });
  await expect.element(screen.getByTestId('tile-3')).toHaveTextContent('active');
  await expect.poll(() => sinks.get(3)).toBeDefined();
  const base = { ...counts };
  const canvas = new OffscreenCanvas(64, 36);
  const g = canvas.getContext('2d')!;
  g.fillStyle = '#fff';
  g.fillRect(0, 0, 64, 36);
  for (let i = 0; i < 5; i += 1) {
    const frame = new VideoFrame(canvas, { timestamp: i });
    sinks.get(3)!.drawFrame(frame);
    frame.close();
  }
  // Let any (wrongly) scheduled reactive work flush before counting.
  await new Promise((r) => requestAnimationFrame(() => r(undefined)));
  expect(counts).toEqual(base);
  expect(surface?.framesRendered()).toBe(5);
  // The backing store is sized to the frame: no letterbox.
  expect(surface?.canvas.width).toBe(64);
  expect(surface?.canvas.height).toBe(36);

  screen.unmount();
  expect(sinks.get(3)).toBeUndefined();
  expect(surface).toBeUndefined();
});
