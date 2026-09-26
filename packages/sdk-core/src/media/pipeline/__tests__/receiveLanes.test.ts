// File: packages/sdk-core/src/media/pipeline/__tests__/receiveLanes.test.ts
//
// The slot-edge gate's authority (MC's stated assignment) and the per-sender
// decode lanes it owns. Time is an injected clock; decoder creation resolves on
// the microtask queue and is awaited with `settle()`, never a wall-clock wait.

import { describe, expect, it } from 'vitest';
import { FakeAudioCodecs, InMemoryMetricsSink, RecordingPlaybackSink } from '@darktower/test-utils';

import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { ReceiveLanes, type SlotAssignment } from '../receiveLanes.js';

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i += 1) await Promise.resolve();
}

function frameOf(byte: number) {
  return { data: Uint8Array.of(byte), timestampUs: 0 };
}

function rig(opts: { declared?: number[]; pendingFrames?: number; maxLanes?: number } = {}) {
  let now = 0;
  const sink = new InMemoryMetricsSink();
  const codecs = new FakeAudioCodecs();
  const playback = new RecordingPlaybackSink();
  let faults = 0;
  const lanes = new ReceiveLanes({
    metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink),
    decoderFactory: codecs.decoderFactory as never,
    playback: playback as never,
    sampleRateHz: 48_000,
    channels: 1,
    declaredSlotIds: opts.declared ?? [0, 1, 2],
    maxLanes: opts.maxLanes ?? 8,
    restartBackoffMs: 1_000,
    pendingFrames: opts.pendingFrames ?? 3,
    clock: () => now,
    onDecoderFault: () => (faults += 1),
  });
  const count = (name: string) =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name)
      .reduce((sum, m) => sum + m.value, 0);
  return {
    lanes,
    codecs,
    playback,
    count,
    faults: () => faults,
    advance: (ms: number) => (now += ms),
  };
}

const assign = (...pairs: [number, number | undefined][]): SlotAssignment[] =>
  pairs.map(([slotId, senderId]) => ({ slotId, senderId }));

describe("the gate's authority is MC's stated assignment", () => {
  it('admits exactly the assigned senders', async () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11], [1, 22], [2, undefined]));
    expect(r.lanes.laneFor(11)).toBeDefined();
    expect(r.lanes.laneFor(22)).toBeDefined();
    expect(r.lanes.laneFor(33)).toBeUndefined();
    expect([...r.lanes.assignedSenders].sort()).toEqual([11, 22]);
  });

  it('IGNORES an assignment for a slot this client never declared', () => {
    // A misbehaving controller cannot inflate the decoder count by naming slots
    // this client did not ask for.
    const r = rig({ declared: [0] });
    r.lanes.setAssignments(assign([0, 11], [7, 22]));
    expect(r.lanes.laneFor(22)).toBeUndefined();
    expect(r.lanes.assignedSenders).toEqual([11]);
  });

  it('never treats an absent sender as sender 0', () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, undefined]));
    expect(r.lanes.laneFor(0)).toBeUndefined();
    expect(r.lanes.assignedSenders).toEqual([]);
  });

  it('REPLACES the view on every snapshot, never merges', () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11]));
    r.lanes.setAssignments(assign([1, 22]));
    expect(r.lanes.laneFor(11)).toBeUndefined();
    expect(r.lanes.laneFor(22)).toBeDefined();
  });

  it('opens ONE lane for a sender assigned to two slots', async () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11], [1, 11]));
    await settle();
    expect(r.codecs.decoders).toHaveLength(1);
    expect(r.playback.lanes).toHaveLength(1);
  });

  it('holds the independent lane bound even if more senders are assigned', () => {
    const r = rig({ declared: [0, 1, 2], maxLanes: 2 });
    r.lanes.setAssignments(assign([0, 11], [1, 22], [2, 33]));
    expect(r.lanes.assignedSenders).toHaveLength(2);
  });
});

describe('a re-map touches only the senders that changed', () => {
  it('closes ONLY the lane of a sender that left, leaving the others decoding', async () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11], [1, 22]));
    await settle();
    const [dec11, dec22] = r.codecs.decoders;
    r.lanes.setAssignments(assign([0, 11]));
    expect(dec22!.closed).toBe(true);
    expect(dec11!.closed).toBe(false);
    expect(r.playback.lanes[1]!.closed).toBe(true);
    expect(r.playback.lanes[0]!.closed).toBe(false);
  });

  it("keeps a sender's decoder when it only MOVES slot", async () => {
    // Slot-to-sender is a decoupled view: decoders are keyed by sender.
    const r = rig();
    r.lanes.setAssignments(assign([0, 11]));
    await settle();
    r.lanes.setAssignments(assign([2, 11]));
    await settle();
    expect(r.codecs.decoders).toHaveLength(1);
    expect(r.codecs.decoders[0]!.closed).toBe(false);
  });

  it('opens nothing and closes nothing for an unchanged snapshot', async () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11]));
    await settle();
    r.lanes.setAssignments(assign([0, 11]));
    await settle();
    expect(r.codecs.decoders).toHaveLength(1);
    expect(r.codecs.decoderClosed).toBe(0);
  });
});

describe('the pending queue while a decoder is being created', () => {
  it('delivers queued frames IN ORDER once the decoder exists', async () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11]));
    const lane = r.lanes.laneFor(11)!;
    lane.decode(frameOf(1));
    lane.decode(frameOf(2));
    expect(r.lanes.pendingFor(11)).toBe(2);
    await settle();
    lane.decode(frameOf(3));
    expect(r.codecs.decoders[0]!.decoded.map((f) => f.data[0])).toEqual([1, 2, 3]);
    expect(r.lanes.pendingFor(11)).toBe(0);
  });

  it('evicts the OLDEST on overflow and counts ONE per evicted frame', async () => {
    const r = rig({ pendingFrames: 3 });
    r.lanes.setAssignments(assign([0, 11]));
    const lane = r.lanes.laneFor(11)!;
    for (let b = 1; b <= 6; b += 1) lane.decode(frameOf(b));
    // Six in, room for three: three evicted, each counted — +3, not merely > 0.
    expect(r.count('dt_client_media_decode_queue_dropped_total')).toBe(3);
    await settle();
    expect(r.codecs.decoders[0]!.decoded.map((f) => f.data[0])).toEqual([4, 5, 6]);
    // Second-segment identity, exact in a no-fault run:
    // decoded + evicted + still pending == handed to the lane.
    expect(
      r.codecs.decoders[0]!.decoded.length +
        r.count('dt_client_media_decode_queue_dropped_total') +
        r.lanes.pendingFor(11),
    ).toBe(6);
  });

  it('never counts an eviction as a decoder error', async () => {
    const r = rig({ pendingFrames: 1 });
    r.lanes.setAssignments(assign([0, 11]));
    const lane = r.lanes.laneFor(11)!;
    lane.decode(frameOf(1));
    lane.decode(frameOf(2));
    await settle();
    expect(r.count('dt_client_media_decode_queue_dropped_total')).toBe(1);
    expect(r.count('dt_client_media_decoder_errors_total')).toBe(0);
  });
});

describe("a decoder fault replaces ONE sender's decoder, rate-bounded", () => {
  async function faulted() {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11], [1, 22]));
    await settle();
    const [dec11, dec22] = r.codecs.decoders;
    dec11!.fail();
    return { r, dec11: dec11!, dec22: dec22! };
  }

  it("closes only the faulted sender's decoder, counts once, reports once", async () => {
    const { r, dec11, dec22 } = await faulted();
    expect(dec11.closed).toBe(true);
    expect(dec22.closed).toBe(false);
    expect(r.count('dt_client_media_decoder_errors_total')).toBe(1);
    expect(r.faults()).toBe(1);
    // The other sender keeps decoding and playing.
    r.lanes.laneFor(22)!.decode(frameOf(9));
    expect(dec22.decoded.map((f) => f.data[0])).toEqual([9]);
    expect(r.playback.lanes[1]!.played).toHaveLength(1);
  });

  it('creates NO replacement inside the backoff, however many frames arrive', async () => {
    const { r } = await faulted();
    for (let b = 0; b < 10; b += 1) r.lanes.laneFor(11)!.decode(frameOf(b));
    await settle();
    expect(r.codecs.decoders).toHaveLength(2);
  });

  it('replaces the decoder on the first frame AFTER the backoff, and delivers what queued', async () => {
    const { r } = await faulted();
    r.lanes.laneFor(11)!.decode(frameOf(1));
    r.advance(1_000);
    r.lanes.laneFor(11)!.decode(frameOf(2));
    await settle();
    expect(r.codecs.decoders).toHaveLength(3);
    const replacement = r.codecs.decoders[2]!;
    expect(replacement.decoded.map((f) => f.data[0])).toEqual([1, 2]);
  });

  it('ignores a late error from a decoder it has already discarded', async () => {
    const { r, dec11 } = await faulted();
    dec11.fail();
    expect(r.count('dt_client_media_decoder_errors_total')).toBe(1);
    expect(r.faults()).toBe(1);
  });

  it('treats a decoder that cannot be CREATED as a fault on that sender only', async () => {
    const r = rig();
    r.codecs.failNextDecoderCreation = new Error('unsupported');
    r.lanes.setAssignments(assign([0, 11]));
    await settle();
    r.lanes.setAssignments(assign([0, 11], [1, 22]));
    await settle();
    expect(r.count('dt_client_media_decoder_errors_total')).toBe(1);
    expect(r.codecs.decoders).toHaveLength(1);
    r.lanes.laneFor(22)!.decode(frameOf(5));
    expect(r.codecs.decoders[0]!.decoded).toHaveLength(1);
  });
});

describe('close()', () => {
  it('closes every lane and admits no one afterwards', async () => {
    const r = rig();
    r.lanes.setAssignments(assign([0, 11], [1, 22]));
    await settle();
    r.lanes.close();
    expect(r.codecs.decoders.every((d) => d.closed)).toBe(true);
    expect(r.lanes.laneFor(11)).toBeUndefined();
    r.lanes.setAssignments(assign([0, 33]));
    expect(r.lanes.laneFor(33)).toBeUndefined();
  });
});
