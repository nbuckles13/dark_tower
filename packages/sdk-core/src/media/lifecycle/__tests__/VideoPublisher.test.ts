// File: packages/sdk-core/src/media/lifecycle/__tests__/VideoPublisher.test.ts
//
// Story 3 C-A2: the video publish lifecycle wired end to end over fakes — the
// camera seam, the encoder seam and `MockWebTransport` uni streams — with an
// injected clock and timers. No wall-clock sleeps.

import { describe, expect, it, vi } from 'vitest';
import { InMemoryMetricsSink, MockWebTransport } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG, type MediaConfig } from '../../../config/clientConfig.js';
import { HEADER_VERSION } from '../../frame/wireConstants.js';
import { decodeFrame } from '../../frame/frameCodec.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { CaptureFailure, MediaCaptureError } from '../../setup/captureFailure.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { MediaMetrics } from '../../setup/mediaMetrics.js';
import type {
  DirectedVideoEncoding,
  EncodedVideoFrame,
  VideoCaptureSource,
  VideoEncoderSeamOptions,
} from '../../setup/seams.js';
import type { MediaFault } from '../AudioPipeline.js';
import { MuteState, type MuteSnapshot } from '../muteState.js';
import { TransmitKeyManager } from '../transmitKeys.js';
import { VideoPublisher, type VideoSendDirective } from '../VideoPublisher.js';
import { uplinkRelayStreamId } from '../../pipeline/sendId.js';

const A = 'https://mh-0.example:4433';
const B = 'https://mh-1.example:4433';
const ENCODING: DirectedVideoEncoding = {
  codec: 'vp8',
  width: 320,
  height: 180,
  frameRateHz: 15,
  maxBitrateBps: 250_000,
  minKeyframeIntervalMs: 500,
};

async function settle(): Promise<void> {
  for (let i = 0; i < 30; i += 1) {
    await new Promise<void>((resolve) => {
      setTimeout(resolve, 0);
    });
  }
}

/** A captured frame stand-in: a timestamp and a close() the publisher must call. */
function frame(timestamp: number) {
  const f = { timestamp, closed: false, close: () => (f.closed = true) };
  return f;
}

class FakeCamera implements VideoCaptureSource {
  started = 0;
  stopped = 0;
  grantedDeviceId: string | undefined = 'cam-1';
  onFrame: ((f: VideoFrame) => void) | undefined;
  onEnded: (() => void) | undefined;
  async start(onFrame: (f: VideoFrame) => void, onEnded: () => void): Promise<void> {
    this.started += 1;
    this.onFrame = onFrame;
    this.onEnded = onEnded;
  }
  stop(): void {
    this.stopped += 1;
    this.onFrame = undefined;
  }
}

function manualTime() {
  let now = 0;
  let id = 1;
  const timers = new Map<number, { at: number; fn: () => void; every?: number }>();
  const add = (fn: () => void, ms: number, every?: number) => {
    const handle = id++;
    timers.set(handle, { at: now + ms, fn, ...(every !== undefined ? { every } : {}) });
    return handle as unknown as ReturnType<typeof setTimeout>;
  };
  return {
    now: () => now,
    setTimeout: (fn: () => void, ms: number) => add(fn, ms),
    clearTimeout: (h: ReturnType<typeof setTimeout>) => void timers.delete(h as unknown as number),
    setInterval: (fn: () => void, ms: number) => add(fn, ms, ms),
    clearInterval: (h: ReturnType<typeof setInterval>) =>
      void timers.delete(h as unknown as number),
    advance(ms: number) {
      const until = now + ms;
      for (;;) {
        const due = [...timers.entries()]
          .filter(([, t]) => t.at <= until)
          .sort((x, y) => x[1].at - y[1].at)[0];
        if (!due) break;
        const [handle, t] = due;
        now = t.at;
        if (t.every !== undefined) t.at += t.every;
        else timers.delete(handle);
        t.fn();
      }
      now = until;
    },
  };
}

interface RigOptions {
  readonly captureFails?: () => Error | undefined;
  readonly config?: MediaConfig;
}

async function makeRig(opts: RigOptions = {}) {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink);
  const mocks = new Map([
    [A, new MockWebTransport()],
    [B, new MockWebTransport()],
  ]);
  const mute = new MuteState();
  const time = manualTime();
  const cameras: FakeCamera[] = [];
  const captureCalls: { deviceId: string | undefined; width: number }[] = [];
  const encodes: { ts: number; key: boolean }[] = [];
  /** The fake encoder's queue depth, settable by a test (`VideoEncoder.encodeQueueSize`). */
  const encodeQueue = { size: 0 };
  let encoderOptions: VideoEncoderSeamOptions | undefined;
  let encodersBuilt = 0;
  const reports: MuteSnapshot[] = [];
  const faults: MediaFault[] = [];
  const transmitKeys = new TransmitKeyManager(258);
  const publisher = new VideoPublisher({
    config: opts.config ?? DEFAULT_CLIENT_CONFIG.media,
    configSource: opts.config ? 'configured' : 'default',
    metrics: metrics.video,
    transmitKeys,
    kek: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(0x33), 0),
    identity: await MeetingIdentity.create(),
    mute,
    reportMute: (s) => reports.push(s),
    onMuteChanged: () => undefined,
    captureFactory: async (o) => {
      captureCalls.push({ deviceId: o.deviceId, width: o.width });
      const failure = opts.captureFails?.();
      if (failure) throw failure;
      const cam = new FakeCamera();
      cameras.push(cam);
      return cam;
    },
    encoderFactory: async (o) => {
      encoderOptions = o;
      encodersBuilt += 1;
      return {
        encode: (f, key) => encodes.push({ ts: f.timestamp, key }),
        get encodeQueueSize(): number {
          return encodeQueue.size;
        },
        close: () => undefined,
      };
    },
    openStream: (url) => mocks.get(url)?.createUnidirectionalStream(),
    fault: (f) => faults.push(f),
    clock: time.now,
    setTimeout: time.setTimeout,
    clearTimeout: time.clearTimeout,
    setInterval: time.setInterval,
    clearInterval: time.clearInterval,
    warn: () => undefined,
    info: () => undefined,
  });
  let ts = 0;
  const directive = (targets: string[], encoding = ENCODING): VideoSendDirective => ({
    streamNumber: 1,
    encoding,
    targets,
    headerVersion: HEADER_VERSION,
  });
  const camera = (): FakeCamera | undefined => cameras.at(-1);
  /** Capture one frame on the live camera. */
  const capture = (): ReturnType<typeof frame> => {
    ts += 66_666;
    const f = frame(ts);
    camera()?.onFrame?.(f as unknown as VideoFrame);
    return f;
  };
  /** Emit encoder output for every encode so far (key as requested unless overridden). */
  let flushed = 0;
  const flush = (override?: (e: { ts: number; key: boolean }) => boolean) => {
    for (; flushed < encodes.length; flushed += 1) {
      const e = encodes[flushed]!;
      const out: EncodedVideoFrame = {
        data: Uint8Array.of(1, 2, 3, 4),
        timestampUs: e.ts,
        key: override ? override(e) : e.key,
      };
      encoderOptions?.onOutput(out);
    }
  };
  const count = (name: string, labels: Record<string, string> = {}): number =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name && Object.entries(labels).every(([k, v]) => m.labels[k] === v))
      .reduce((n, m) => n + m.value, 0);
  const cameraLit = (): boolean => publisher.cameraTrackLive;
  const records = () => sink.getRecordedMetrics();
  return {
    records,
    encodeQueue,
    publisher,
    mocks,
    mute,
    time,
    cameras,
    captureCalls,
    encodes,
    encoderOptions: () => encoderOptions,
    encodersBuilt: () => encodersBuilt,
    reports,
    faults,
    transmitKeys,
    directive,
    capture,
    flush,
    count,
    cameraLit,
  };
}

describe('VideoPublisher — the camera follows the directive and mute (R-16, R-17)', () => {
  it('stays OFF until directed with targets; lights at the DIRECTED geometry; keys the first frame (directive_start)', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([]));
    await settle();
    expect(r.captureCalls).toEqual([]);
    r.publisher.setDirective(r.directive([A]));
    await settle();
    expect(r.captureCalls).toEqual([{ deviceId: undefined, width: 320 }]);
    expect(r.cameraLit()).toBe(true);
    r.capture();
    r.capture();
    expect(r.encodes.map((e) => e.key)).toEqual([true, false]);
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'directive_start' })).toBe(
      1,
    );
  });

  it.each([
    // A server mute arrives as the SAME empty video target set (MC withdraws
    // the directive), so this one row covers both.
    [
      'unwatched or server-muted (empty targets)',
      (r: Awaited<ReturnType<typeof makeRig>>) => r.publisher.setDirective(r.directive([])),
    ],
    ['client video mute', (r: Awaited<ReturnType<typeof makeRig>>) => r.publisher.setMuted(true)],
  ])(
    'the camera STOPS on %s and is re-acquired on restore with the SAME device',
    async (_n, off) => {
      const r = await makeRig();
      r.publisher.start();
      r.publisher.setDirective(r.directive([A]));
      await settle();
      off(r);
      expect(r.cameras[0]!.stopped).toBe(1);
      expect(r.cameraLit()).toBe(false);
      // Restore: a directive with targets, and (for the mute case) an unmute.
      r.publisher.setMuted(false);
      r.publisher.setDirective(r.directive([A]));
      await settle();
      expect(r.cameraLit()).toBe(true);
      expect(r.captureCalls.at(-1)?.deviceId).toBe('cam-1');
      expect(r.captureCalls).toHaveLength(2);
    },
  );

  it('client video mute never touches audio mute', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setMuted(true);
    expect(r.mute.audioMuted).toBe(false);
    expect(r.reports.at(-1)).toEqual({ audioMuted: false, videoMuted: true });
  });

  it('a re-sent directive with IDENTICAL video targets keys nothing and touches no camera (e.g. only audio changed)', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.time.advance(10_000);
    r.publisher.setDirective(r.directive([A]));
    r.publisher.setDirective(r.directive([A]));
    await settle();
    expect(r.captureCalls).toHaveLength(1);
    expect(r.cameras[0]!.stopped).toBe(0);
    expect(r.count('dt_client_media_keyframes_produced_total')).toBe(1);
  });

  it('a target REMOVED keys nothing; a target ADDED keys a new group on every target', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A, B]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    r.time.advance(1_000);
    r.publisher.setDirective(r.directive([A]));
    expect(r.count('dt_client_media_keyframes_produced_total')).toBe(1);
    r.publisher.setDirective(r.directive([A, B]));
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'directive_start' })).toBe(
      2,
    );
    r.capture();
    r.flush();
    await settle();
    expect(r.mocks.get(A)!.getOutboundUniStreams()).toHaveLength(2);
    expect(r.mocks.get(B)!.getOutboundUniStreams()).toHaveLength(2);
  });

  it('zero-initialises keyframes_produced (three sources) and groups_abandoned only when video is FIRST directed', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([]));
    expect(r.records().filter((m) => m.labels.media_kind === 'video')).toEqual([]);
    r.publisher.setDirective(r.directive([A]));
    const zeros = r
      .records()
      .filter((m) => m.value === 0)
      .map((m) => {
        const label = m.labels.source ?? m.labels.outcome;
        return `${m.name}${label ? `:${String(label)}` : ''}`;
      })
      .sort();
    expect(zeros).toEqual([
      'dt_client_media_encode_queue_dropped_total',
      'dt_client_media_keyframes_produced_total:abandonment',
      'dt_client_media_keyframes_produced_total:directive_start',
      'dt_client_media_keyframes_produced_total:request',
      'dt_client_media_send_groups_abandoned_total',
      'dt_client_media_send_groups_stopped_total:open_failed',
      'dt_client_media_send_groups_stopped_total:open_stalled',
      'dt_client_media_send_groups_stopped_total:peer_stopped',
    ]);
  });
});

describe('VideoPublisher — nothing of video leaves the device under mute (security S5)', () => {
  it('encoder output still in flight when the mute lands is dropped: zero bytes on every lane', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A, B]));
    await settle();
    r.capture();
    r.capture();
    r.publisher.setMuted(true);
    r.flush();
    await settle();
    for (const url of [A, B]) {
      const written = r.mocks
        .get(url)!
        .getOutboundUniStreams()
        .flatMap((s) => s.chunks);
      expect(written).toEqual([]);
    }
  });

  it('a captured frame is always closed, even when suppressed', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    const sent = r.capture();
    r.publisher.setMuted(true);
    expect(sent.closed).toBe(true);
  });
});

describe('VideoPublisher — keyframes (R-10, R-11, amendment 4)', () => {
  it('routes a request to the gate and keys the next captured frame', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    r.time.advance(600);
    expect(r.publisher.keyframeRequest(A, 0)).toBe('honoured');
    r.capture();
    expect(r.encodes.at(-1)?.key).toBe(true);
  });

  it('an UNSOLICITED key chunk followed by a request inside the interval gives ONE group start in that interval (security S2)', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    r.time.advance(600);
    r.capture();
    r.flush(() => true); // the platform keyed on its own
    await settle();
    expect(r.mocks.get(A)!.getOutboundUniStreams()).toHaveLength(2);
    r.time.advance(100);
    expect(r.publisher.keyframeRequest(A, 1)).toBe('min_interval');
    r.capture();
    expect(r.encodes.at(-1)?.key).toBe(false);
    r.time.advance(400);
    r.capture();
    expect(r.encodes.at(-1)?.key).toBe(true);
  });

  it('a publisher-decided abandonment re-keys (held as the deferral inside the interval), counted as abandonment', async () => {
    const config: MediaConfig = {
      ...DEFAULT_CLIENT_CONFIG.media,
      egress: { ...DEFAULT_CLIENT_CONFIG.media.egress, videoMaxBacklogFrames: 2 },
    };
    const r = await makeRig({ config });
    r.publisher.start();
    r.publisher.setDirective(r.directive([A, B]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    r.mocks.get(A)!.pauseUniStreamWrites(0);
    for (let i = 0; i < 4; i += 1) r.capture();
    r.flush();
    await settle();
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(1);
    // Inside the interval: held, not dropped.
    r.capture();
    expect(r.encodes.at(-1)?.key).toBe(false);
    r.time.advance(500);
    r.capture();
    expect(r.encodes.at(-1)?.key).toBe(true);
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'abandonment' })).toBe(1);
  });

  it('an MH STOP_SENDING never re-keys: zero keyframes at once or at a later fire', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    r.mocks.get(A)!.simulateOutboundUniStopSending(0, 3);
    r.capture();
    r.flush();
    await settle();
    r.time.advance(10_000);
    for (let i = 0; i < 3; i += 1) r.capture();
    expect(r.encodes.filter((e) => e.key)).toHaveLength(1);
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'abandonment' })).toBe(0);
    expect(r.count('dt_client_media_send_groups_abandoned_total')).toBe(0);
  });
});

describe('VideoPublisher — R-19 camera failure', () => {
  it.each([true, false])(
    'is counted, presented as video client-muted (audio_muted carries its current state: %s), with NO retry loop',
    async (audioMuted) => {
      let fail = true;
      const r = await makeRig({
        captureFails: () =>
          fail ? new MediaCaptureError(CaptureFailure.PermissionDenied, 'denied') : undefined,
      });
      r.mute.setAudioMuted(audioMuted);
      r.publisher.start();
      r.publisher.setDirective(r.directive([A]));
      await settle();
      expect(
        r.count('dt_client_media_capture_failures_total', { outcome: 'permission_denied' }),
      ).toBe(1);
      expect(r.reports.at(-1)).toEqual({ audioMuted, videoMuted: true });
      expect(r.faults.map((f) => f.stage)).toEqual(['capture']);
      // Watcher churn: N restore edges, ZERO further getUserMedia calls.
      for (let i = 0; i < 5; i += 1) {
        r.publisher.setDirective(r.directive([]));
        r.publisher.setDirective(r.directive([A]));
      }
      r.time.advance(60_000);
      await settle();
      expect(r.captureCalls).toHaveLength(1);
      // Only a user unmute tries again.
      fail = false;
      r.publisher.setMuted(false);
      await settle();
      expect(r.captureCalls).toHaveLength(2);
      expect(r.cameraLit()).toBe(true);
    },
  );

  it('a track that ENDS mid-call: counted ended, muted, and retried exactly once on the next restore edge', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.cameras[0]!.onEnded?.();
    expect(r.count('dt_client_media_capture_failures_total', { outcome: 'ended' })).toBe(1);
    expect(r.mute.videoMuted).toBe(true);
    r.publisher.setDirective(r.directive([A])); // no edge
    await settle();
    expect(r.captureCalls).toHaveLength(1);
    r.publisher.setDirective(r.directive([]));
    r.publisher.setDirective(r.directive([A])); // restore edge
    await settle();
    expect(r.captureCalls).toHaveLength(2);
    // Lifted only once the track is live.
    expect(r.mute.videoMuted).toBe(false);
    expect(r.reports.at(-1)).toEqual({ audioMuted: false, videoMuted: false });
  });
});

describe('VideoPublisher — encoder and keys', () => {
  it('an encoder error is counted, a fatal video fault, and the camera stops', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.encoderOptions()?.onError(new Error('boom'));
    expect(r.count('dt_client_media_encoder_errors_total')).toBe(1);
    expect(r.faults.at(-1)).toMatchObject({ stage: 'encoder', fatal: true });
    expect(r.cameraLit()).toBe(false);
  });

  it('rotates the session generation every videoRotationPeriodMs, without forcing a keyframe', async () => {
    const r = await makeRig();
    r.publisher.start();
    const g0 = r.transmitKeys.generation;
    r.time.advance(DEFAULT_CLIENT_CONFIG.media.keys.videoRotationPeriodMs);
    expect(r.transmitKeys.generation).toBe(g0 + 1n);
    expect(r.count('dt_client_media_keyframes_produced_total')).toBe(0);
  });

  it('the GoP counter increments on the resume keyframe and the stream sequence keeps counting', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.capture();
    r.capture();
    r.flush();
    await settle();
    r.publisher.setDirective(r.directive([]));
    r.time.advance(1_000);
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    const frames = r.mocks
      .get(A)!
      .getOutboundUniStreams()
      .flatMap((s) => s.chunks.map(decodeFrame));
    expect(frames.map((f) => f.gopCounter)).toEqual([0, 0, 1]);
    expect(frames.map((f) => f.streamSequence)).toEqual([0, 1, 2]);
  });

  it('stop() releases the camera and every timer; it never clears the SESSION keys', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.publisher.stop();
    expect(r.cameras[0]!.stopped).toBe(1);
    const g = r.transmitKeys.generation;
    r.time.advance(10 * DEFAULT_CLIENT_CONFIG.media.keys.videoRotationPeriodMs);
    expect(r.transmitKeys.generation).toBe(g);
    expect(r.transmitKeys.currentGopCounter(1)).toBe(undefined);
  });
});

describe('VideoPublisher — edges', () => {
  it('a directive received BEFORE start() is applied at start', async () => {
    const r = await makeRig();
    r.publisher.setDirective(r.directive([A]));
    expect(r.captureCalls).toEqual([]);
    r.publisher.start();
    r.publisher.start(); // idempotent
    await settle();
    expect(r.captureCalls).toHaveLength(1);
    // The directed start keys through the gate — not left to the platform.
    r.capture();
    r.capture();
    expect(r.encodes.map((e) => e.key)).toEqual([true, false]);
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'directive_start' })).toBe(
      1,
    );
  });

  it('a stale capture attempt resolving LATE never orphans the current camera (light always trackable)', async () => {
    const resolvers: ((c: FakeCamera) => void)[] = [];
    const cams: FakeCamera[] = [];
    const publisher = new VideoPublisher({
      config: DEFAULT_CLIENT_CONFIG.media,
      configSource: 'default',
      metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, undefined).video,
      transmitKeys: new TransmitKeyManager(258),
      kek: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(1), 0),
      identity: await MeetingIdentity.create(),
      mute: new MuteState(),
      reportMute: () => undefined,
      onMuteChanged: () => undefined,
      captureFactory: () =>
        new Promise<FakeCamera>((resolve) => {
          resolvers.push(resolve);
        }),
      encoderFactory: async () => ({
        encode: () => undefined,
        encodeQueueSize: 0,
        close: () => undefined,
      }),
      openStream: () => undefined,
      fault: () => undefined,
      warn: () => undefined,
      info: () => undefined,
    });
    const d = (targets: string[]) => ({
      streamNumber: 1,
      encoding: ENCODING,
      targets,
      headerVersion: HEADER_VERSION,
    });
    publisher.start();
    publisher.setDirective(d([A])); // attempt A
    await settle();
    publisher.setDirective(d([])); // stop
    publisher.setDirective(d([A])); // attempt B
    await settle();
    expect(resolvers).toHaveLength(2);
    const camA = new FakeCamera();
    const camB = new FakeCamera();
    cams.push(camA, camB);
    resolvers[1]!(camB); // B first
    await settle();
    resolvers[0]!(camA); // A late
    await settle();
    expect(camA.stopped).toBe(1);
    expect(publisher.cameraTrackLive).toBe(true);
    // The CURRENT camera (B) is still tracked: a withdrawal stops it.
    publisher.setDirective(d([]));
    expect(camB.stopped).toBe(1);
    expect(publisher.cameraTrackLive).toBe(false);
    publisher.stop();
  });

  it('a withdrawal with nothing ever directed is a no-op', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(undefined);
    await settle();
    expect(r.captureCalls).toEqual([]);
  });

  it('an unstampable stream number is a config fault and sends nothing', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective({ ...r.directive([A]), streamNumber: 4096 });
    await settle();
    expect(r.faults.map((f) => f.stage)).toEqual(['config']);
    expect(r.captureCalls).toEqual([]);
  });

  it('a CHANGED directed encoding rebuilds the encoder and restarts capture at the new geometry, keyed', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.time.advance(1_000);
    r.publisher.setDirective(r.directive([A], { ...ENCODING, width: 640, height: 360 }));
    await settle();
    expect(r.cameras[0]!.stopped).toBe(1);
    expect(r.captureCalls.at(-1)).toEqual({ deviceId: 'cam-1', width: 640 });
    expect(r.encodersBuilt()).toBe(2);
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'directive_start' })).toBe(
      2,
    );
  });

  it('an encoder that cannot be built is counted and stops video', async () => {
    const sink = new InMemoryMetricsSink();
    const metrics = new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink);
    const faults: MediaFault[] = [];
    const publisher = new VideoPublisher({
      config: DEFAULT_CLIENT_CONFIG.media,
      configSource: 'default',
      metrics: metrics.video,
      transmitKeys: new TransmitKeyManager(258),
      kek: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(1), 0),
      identity: await MeetingIdentity.create(),
      mute: new MuteState(),
      reportMute: () => undefined,
      onMuteChanged: () => undefined,
      captureFactory: async () => new FakeCamera(),
      encoderFactory: () => Promise.reject(new Error('unsupported')),
      openStream: () => undefined,
      fault: (f) => faults.push(f),
      warn: () => undefined,
      info: () => undefined,
    });
    publisher.start();
    publisher.setDirective({
      streamNumber: 1,
      encoding: ENCODING,
      targets: [A],
      headerVersion: HEADER_VERSION,
    });
    await settle();
    expect(faults).toEqual([expect.objectContaining({ stage: 'encoder', fatal: true })]);
    publisher.stop();
    publisher.stop();
    publisher.setDirective(undefined);
    publisher.setMuted(true);
  });

  it('a capture that resolves after a withdrawal is released at once (stale attempt)', async () => {
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const r = await makeRig({ captureFails: () => undefined });
    const cam = new FakeCamera();
    const slow = new VideoPublisher({
      config: DEFAULT_CLIENT_CONFIG.media,
      configSource: 'default',
      metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, undefined).video,
      transmitKeys: new TransmitKeyManager(258),
      kek: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(1), 0),
      identity: await MeetingIdentity.create(),
      mute: new MuteState(),
      reportMute: () => undefined,
      onMuteChanged: () => undefined,
      captureFactory: async () => {
        await gate;
        return cam;
      },
      encoderFactory: async () => ({
        encode: () => undefined,
        encodeQueueSize: 0,
        close: () => undefined,
      }),
      openStream: (url) => r.mocks.get(url)?.createUnidirectionalStream(),
      fault: () => undefined,
      warn: () => undefined,
      info: () => undefined,
    });
    slow.start();
    slow.setDirective({
      streamNumber: 1,
      encoding: ENCODING,
      targets: [A],
      headerVersion: HEADER_VERSION,
    });
    await settle();
    slow.setDirective({
      streamNumber: 1,
      encoding: ENCODING,
      targets: [],
      headerVersion: HEADER_VERSION,
    });
    release();
    await settle();
    expect(cam.stopped).toBe(1);
    expect(cam.started).toBe(0);
    expect(slow.cameraTrackLive).toBe(false);
    slow.stop();
  });

  it('a non-typed capture error is classified by name; an ended track from a replaced capture is ignored', async () => {
    const named = Object.assign(new Error('x'), { name: 'NotReadableError' });
    let fail = true;
    const r = await makeRig({ captureFails: () => (fail ? named : undefined) });
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    expect(
      r.count('dt_client_media_capture_failures_total', { outcome: 'device_unavailable' }),
    ).toBe(1);
    fail = false;
    r.publisher.setDirective(r.directive([]));
    r.publisher.setDirective(r.directive([A]));
    await settle();
    const first = r.cameras[0]!;
    r.publisher.setMuted(true);
    first.onEnded?.(); // the stopped capture's late ended event
    expect(r.count('dt_client_media_capture_failures_total', { outcome: 'ended' })).toBe(0);
  });

  it('encoder output for a frame it never saw is dropped; frames without a target are closed', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.encoderOptions()?.onOutput({ data: Uint8Array.of(1), timestampUs: 999_999_999, key: true });
    await settle();
    expect(r.mocks.get(A)!.getOutboundUniStreams()).toHaveLength(0);
    const late = r.cameras[0]!.onFrame!;
    r.publisher.setDirective(r.directive([]));
    // A frame the stopped track still delivers is suppressed AND closed.
    const f = frame(123);
    late(f as unknown as VideoFrame);
    expect(f.closed).toBe(true);
  });

  it('builds with default clock and timers (production wiring) and stops them', async () => {
    const publisher = new VideoPublisher({
      config: DEFAULT_CLIENT_CONFIG.media,
      configSource: 'default',
      metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, undefined).video,
      transmitKeys: new TransmitKeyManager(258),
      kek: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(1), 0),
      identity: await MeetingIdentity.create(),
      mute: new MuteState(),
      reportMute: () => undefined,
      onMuteChanged: () => undefined,
      captureFactory: async () => new FakeCamera(),
      encoderFactory: async () => ({
        encode: () => undefined,
        encodeQueueSize: 0,
        close: () => undefined,
      }),
      openStream: () => undefined,
      fault: () => undefined,
    });
    const info = vi.spyOn(console, 'info').mockImplementation(() => undefined);
    publisher.start();
    expect(info).toHaveBeenCalledWith(
      '[dt-media-video-config]',
      expect.stringContaining('(compiled)'),
    );
    publisher.stop();
    info.mockRestore();
  });
});

describe('VideoPublisher — Gate-3 additions', () => {
  it('an MH STOP_SENDING target never re-keys at a LATER FIRE either (held self-key, the abandoned target removed)', async () => {
    const config: MediaConfig = {
      ...DEFAULT_CLIENT_CONFIG.media,
      egress: { ...DEFAULT_CLIENT_CONFIG.media.egress, videoMaxBacklogFrames: 2 },
    };
    const r = await makeRig({ config });
    r.publisher.start();
    r.publisher.setDirective(r.directive([A, B]));
    await settle();
    r.capture();
    r.flush();
    await settle();
    r.mocks.get(A)!.simulateOutboundUniStopSending(0, 9);
    r.mocks.get(B)!.pauseUniStreamWrites(0);
    for (let i = 0; i < 4; i += 1) r.capture();
    r.flush();
    await settle();
    // B's backlog cap tripped inside the interval: a held self-key (abandonment).
    expect(r.publisher.keyframeGate.hasDeferral).toBe(true);
    const keysBefore = r.encodes.filter((e) => e.key).length;
    // B leaves before the fire; only the mh_stopped A remains.
    r.publisher.setDirective(r.directive([A]));
    r.time.advance(10_000);
    r.capture();
    expect(r.encodes.filter((e) => e.key).length).toBe(keysBefore);
    expect(r.count('dt_client_media_keyframes_produced_total', { source: 'abandonment' })).toBe(0);
  });

  it('the uplink stamp and the keyed stream come from the DIRECTIVE (stream 3, not a hardcoded 1)', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective({ ...r.directive([A]), streamNumber: 3 });
    await settle();
    r.capture();
    r.flush();
    await settle();
    const frame = decodeFrame(r.mocks.get(A)!.getOutboundUniStreams()[0]!.chunks[0]!);
    expect(frame.streamId).toBe(uplinkRelayStreamId({ streamNumber: 3 }));
    r.time.advance(1_000);
    // The gate keys stream 3's group counter; a request is honoured for it.
    expect(r.publisher.keyframeRequest(A, 0)).toBe('honoured');
  });

  it('a full encoder queue drops the CAPTURED frame (counted) and the pending keyframe rides the next encoded frame', async () => {
    const r = await makeRig();
    r.publisher.start();
    r.publisher.setDirective(r.directive([A]));
    await settle();
    r.encodeQueue.size = DEFAULT_CLIENT_CONFIG.media.egress.videoMaxEncodeQueue + 1;
    const dropped = r.capture();
    expect(dropped.closed).toBe(true);
    expect(r.encodes).toEqual([]);
    expect(r.count('dt_client_media_encode_queue_dropped_total')).toBe(1);
    r.encodeQueue.size = 0;
    r.capture();
    expect(r.encodes.map((e) => e.key)).toEqual([true]);
  });

  it('logs the config line with its PROVENANCE', async () => {
    const lines: string[] = [];
    const publisher = new VideoPublisher({
      config: DEFAULT_CLIENT_CONFIG.media,
      configSource: 'configured',
      metrics: new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, undefined).video,
      transmitKeys: new TransmitKeyManager(258),
      kek: kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(1), 0),
      identity: await MeetingIdentity.create(),
      mute: new MuteState(),
      reportMute: () => undefined,
      onMuteChanged: () => undefined,
      captureFactory: async () => new FakeCamera(),
      encoderFactory: async () => ({
        encode: () => undefined,
        encodeQueueSize: 0,
        close: () => undefined,
      }),
      openStream: () => undefined,
      fault: () => undefined,
      warn: () => undefined,
      info: (m) => lines.push(m),
    });
    publisher.start();
    expect(lines).toEqual([expect.stringMatching(/^media config \(configured\): /)]);
    void publisher.stop();
  });
});
