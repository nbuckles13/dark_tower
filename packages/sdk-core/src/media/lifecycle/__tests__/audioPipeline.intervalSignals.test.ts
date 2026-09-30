// File: packages/sdk-core/src/media/lifecycle/__tests__/audioPipeline.intervalSignals.test.ts
//
// Story 2 R-7 / R-28: the two per-export-interval signals the pipeline emits on
// ONE timer — the capture-source presence gauge and the receive-source deficit
// counter. Driven by an injected timer; no wall-clock waits.

import { describe, expect, it } from 'vitest';
import {
  FakeAudioCodecs,
  FakeCaptureSource,
  InMemoryMetricsSink,
  RecordingPlaybackSink,
} from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../../setup/rosterKeys.js';
import { AudioPipeline, type ReceiveAssignment } from '../AudioPipeline.js';

const INTERVAL_MS = 7_000;
const MH_URL = 'https://mh.example:4433';

async function rig(opts: { declaredSlotIds?: number[] } = {}) {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink);
  const timers = new Map<number, { fn: () => void; ms: number }>();
  let nextHandle = 1;
  const pipeline = new AudioPipeline({
    config: DEFAULT_CLIENT_CONFIG.media,
    metrics,
    kekSource: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(1), 0),
    roster: new RosterIdentityKeys(8),
    senderId: 1,
    identity: await MeetingIdentity.create(),
    declaredSlotIds: opts.declaredSlotIds ?? [0, 1],
    captureSource: 'microphone',
    metricExportIntervalMs: INTERVAL_MS,
    senderFor: () => undefined,
    readableFor: () => undefined,
    reportMute: () => {},
    captureFactory: async () => new FakeCaptureSource() as never,
    encoderFactory: new FakeAudioCodecs().encoderFactory as never,
    decoderFactory: new FakeAudioCodecs().decoderFactory as never,
    playbackFactory: new RecordingPlaybackSink().factory as never,
    clock: () => 0,
    setInterval: (fn, ms) => {
      const handle = nextHandle++;
      timers.set(handle, { fn, ms });
      return handle as unknown as ReturnType<typeof setInterval>;
    },
    clearInterval: (h) => {
      timers.delete(h as unknown as number);
    },
  });
  await pipeline.start();
  const intervalTimer = () => [...timers.values()].find((t) => t.ms === INTERVAL_MS);
  return {
    pipeline,
    sink,
    tick: () => intervalTimer()?.fn(),
    intervalTimer,
    deficit: () =>
      sink
        .getRecordedMetrics()
        .filter((m) => m.name === 'dt_client_media_receive_source_deficit_total')
        .reduce((sum, m) => sum + m.value, 0),
    gaugeWrites: () =>
      sink.getRecordedMetrics().filter((m) => m.name === 'dt_client_media_capture_source'),
  };
}

const slot = (
  slotId: number,
  senderId: number | undefined,
  active: boolean,
): ReceiveAssignment => ({
  slotId,
  senderId,
  mediaHandlerUrl: senderId === undefined ? '' : MH_URL,
  active,
});

describe('AudioPipeline per-interval signals', () => {
  it('ticks at the configured export interval — one timer for both signals', async () => {
    const r = await rig();
    expect(r.intervalTimer()?.ms).toBe(INTERVAL_MS);
  });

  it('asserts the capture-source gauge at start and RE-SETS it every tick; stops at teardown', async () => {
    const r = await rig();
    expect(r.gaugeWrites()).toHaveLength(1);
    r.tick();
    r.tick();
    expect(r.gaugeWrites()).toHaveLength(3);
    expect(r.gaugeWrites().every((g) => g.value === 1 && g.labels.mode === 'microphone')).toBe(
      true,
    );
    await r.pipeline.stop();
    expect(r.intervalTimer()).toBeUndefined();
  });

  it('zero-initialises the deficit counter at start', async () => {
    const r = await rig();
    const writes = r.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_receive_source_deficit_total');
    expect(writes.map((w) => w.value)).toEqual([0]);
  });

  it('counts an ACTIVE assignment silent for a whole interval, after one interval of grace', async () => {
    const r = await rig();
    r.pipeline.setReceiveAssignments([slot(0, 7, true)]);
    r.tick(); // became active mid-interval: grace
    expect(r.deficit()).toBe(0);
    r.tick(); // a full interval, nothing decoded
    expect(r.deficit()).toBe(1);
    r.tick();
    expect(r.deficit()).toBe(2); // monotone
  });

  it('two silent active slots add 2 per interval', async () => {
    const r = await rig();
    r.pipeline.setReceiveAssignments([slot(0, 7, true), slot(1, 8, true)]);
    r.tick();
    r.tick();
    expect(r.deficit()).toBe(2);
  });

  it('excludes source-muted slots and declared-but-unassigned slots', async () => {
    const r = await rig({ declaredSlotIds: [0, 1, 2] });
    // Positive control on slot 2: counting works in this very run, so the
    // exclusions below are not passing because nothing is counted at all.
    r.pipeline.setReceiveAssignments([
      slot(0, 7, false),
      slot(1, undefined, false),
      slot(2, 9, true),
    ]);
    r.tick();
    r.tick();
    expect(r.deficit()).toBe(1);
  });

  it('excludes an assignment on a slot this client never declared', async () => {
    const r = await rig();
    // Positive control on declared slot 0 alongside the undeclared slot 5.
    r.pipeline.setReceiveAssignments([slot(5, 7, true), slot(0, 8, true)]);
    r.tick();
    r.tick();
    expect(r.deficit()).toBe(1);
  });
});
