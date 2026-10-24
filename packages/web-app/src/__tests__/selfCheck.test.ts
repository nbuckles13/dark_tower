// File: packages/web-app/src/__tests__/selfCheck.test.ts
//
// The R-7 loopback self-check (story 3), in real Chromium with fake media
// devices. Four properties:
//
//   1. NOTHING IS SENT. The in-meeting panel is mounted exactly as InMeeting
//      mounts it — `localStatsOf` / `localCaptureTracksOf` closures over a
//      session-like object — whose every key read is recorded and must stay
//      inside an allow-list, and which ALSO carries `IWebTransport` and
//      `MediaTransport` RECORDING PROXIES (a genuine reference path). After
//      open, start, a tick, a stats sample and close, the transport log is
//      empty, AND the check demonstrably ran (the session's camera track shown,
//      the analyser built, the stats read), so "no calls" cannot pass on a
//      check that did nothing.
//   2. A REUSED SESSION TRACK IS NEVER STOPPED, CLONED, RE-CONSTRAINED OR
//      TOGGLED (the publish path owns the camera-light rule), and a track swap
//      (video mute, then unmute) re-binds.
//   3. AN OWN CAPTURE (pre-join) IS RELEASED on stop, on close and on destroy,
//      INCLUDING a preview that resolves after the check was closed; and it is
//      never opened before "Start self-check".
//   4. The meter's ONLY source is the local capture track, and nothing is
//      connected to the audio destination.

import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import {
  CaptureFailure,
  MediaCaptureError,
  localCaptureTracksOf,
  localStatsOf,
  type IWebTransport,
  type LocalCaptureTracks,
  type LocalPreview,
  type LocalReceiveSlotStats,
  type MediaTransport,
} from '@darktower/sdk-core';
import DebugPanel from '../views/DebugPanel.svelte';
import { DEBUG_PANEL_STORAGE_KEY, type ToggleStorage } from '../lib/debugToggle.js';
import { SelfCheck, rmsLevel, type MeterContext, type SelfCheckView } from '../lib/selfCheck.js';

const OPEN: ToggleStorage = {
  getItem: (k) => (k === DEBUG_PANEL_STORAGE_KEY ? '1' : null),
  setItem: () => undefined,
};

/** A proxy that records EVERY property access (get, set, has, apply...) by name. */
function recordingProxy<T extends object>(log: string[], name: string): T {
  const handler: ProxyHandler<object> = {
    get: (_t, prop) => {
      log.push(`${name}.get(${String(prop)})`);
      return recordingProxy(log, `${name}.${String(prop)}`);
    },
    set: (_t, prop) => {
      log.push(`${name}.set(${String(prop)})`);
      return true;
    },
    has: (_t, prop) => {
      log.push(`${name}.has(${String(prop)})`);
      return false;
    },
    apply: () => {
      log.push(`${name}()`);
      return undefined;
    },
  };
  return new Proxy(function () {}, handler) as unknown as T;
}

/** Real fake-device tracks, owned by the TEST (stopped in afterEach). */
const owned: MediaStreamTrack[] = [];
async function deviceTracks(): Promise<{ video: MediaStreamTrack; audio: MediaStreamTrack }> {
  const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: true });
  const video = stream.getVideoTracks()[0]!;
  const audio = stream.getAudioTracks()[0]!;
  owned.push(video, audio);
  return { video, audio };
}

afterEach(() => {
  for (const t of owned.splice(0)) t.stop();
  vi.restoreAllMocks();
});

/** A manual timer for the self-check tick. */
function manualTimer() {
  const timers = new Map<number, () => void>();
  let next = 1;
  return {
    setInterval: (fn: () => void) => {
      const id = next++;
      timers.set(id, fn);
      return id as unknown as ReturnType<typeof setInterval>;
    },
    clearInterval: (h: ReturnType<typeof setInterval>) => {
      timers.delete(h as unknown as number);
    },
    tick: () => {
      for (const fn of [...timers.values()]) fn();
    },
    get active() {
      return timers.size;
    },
  };
}

/** Wrap a real AudioContext, recording every source stream and every connect target. */
function recordingAudio() {
  const sources: MediaStream[] = [];
  const connects: unknown[] = [];
  const contexts: AudioContext[] = [];
  const factory = (): MeterContext => {
    const ctx = new AudioContext();
    contexts.push(ctx);
    return {
      createMediaStreamSource: (stream) => {
        sources.push(stream);
        const node = ctx.createMediaStreamSource(stream);
        const connect = node.connect.bind(node) as (dest: AudioNode) => AudioNode;
        (node as unknown as { connect: (d: AudioNode) => AudioNode }).connect = (dest) => {
          connects.push(dest);
          return connect(dest);
        };
        return node;
      },
      createAnalyser: () => ctx.createAnalyser(),
      close: () => ctx.close(),
    };
  };
  return { factory, sources, connects, contexts };
}

test('in-meeting (REUSE), mounted exactly as InMeeting mounts it: only the allow-listed getters are read, zero transport access, and it RAN', async () => {
  const { video, audio } = await deviceTracks();
  const stops = [vi.spyOn(video, 'stop'), vi.spyOn(audio, 'stop')];
  const clones = [vi.spyOn(video, 'clone'), vi.spyOn(audio, 'clone')];
  const constrain = vi.spyOn(video, 'applyConstraints');
  const transportLog: string[] = [];
  const reads: string[] = [];
  /** Record every key read on `target` under `name` (a read-path allow-list check). */
  const watched = <T extends object>(target: T, name: string): T =>
    new Proxy(target, {
      get: (t, prop, recv) => {
        reads.push(`${name}.${String(prop)}`);
        return Reflect.get(t, prop, recv) as unknown;
      },
    });
  let localStatsReads = 0;
  const emptyDrops = {} as LocalReceiveSlotStats['drops'];
  // A session-like object of the LocalStatsSource / LocalTracksSource shape, that
  // ALSO carries recording transports: the panel has a genuine reference path
  // to them through the same object the real mount closes over.
  const sessionLike = watched(
    {
      transport: recordingProxy<IWebTransport>(transportLog, 'IWebTransport'),
      mediaTransport: recordingProxy<MediaTransport>(transportLog, 'MediaTransport'),
      media: watched(
        {
          captureTrack: audio as MediaStreamTrack | undefined,
          localStats: () => {
            localStatsReads += 1;
            return {
              receive: [
                {
                  kind: 'audio' as const,
                  slotId: 0,
                  packets: 1,
                  bytes: 10,
                  framesDecoded: 1,
                  queueDepth: 0,
                  drops: emptyDrops,
                  localDrops: undefined,
                  keyframesSeen: undefined,
                  unfulfilledMs: undefined,
                },
              ],
              send: {
                kind: 'audio' as const,
                packets: 1,
                bytes: 10,
                keyframes: undefined,
                groupsAbandoned: undefined,
              },
            };
          },
        },
        'media',
      ),
      video: watched(
        {
          captureTrack: video as MediaStreamTrack | undefined,
          localSendStats: () => ({
            kind: 'video' as const,
            packets: 0,
            bytes: 0,
            keyframes: { directive_start: 0, request: 0, abandonment: 0 },
            groupsAbandoned: 0,
          }),
        },
        'video',
      ),
      videoReceive: watched({ localStats: () => [] }, 'videoReceive'),
    },
    'session',
  );
  const timer = manualTimer();
  const audioRec = recordingAudio();
  const screen = await render(DebugPanel, {
    storage: OPEN,
    // EXACTLY the InMeeting mount's closures.
    stats: () => localStatsOf(sessionLike),
    tracks: () => localCaptureTracksOf(sessionLike),
    setInterval: timer.setInterval,
    clearInterval: timer.clearInterval,
    selfCheckDeps: {
      setInterval: timer.setInterval,
      clearInterval: timer.clearInterval,
      audioContext: audioRec.factory,
    },
  });
  await screen.getByTestId('self-check-start').click();
  await expect.element(screen.getByTestId('self-check-status')).toHaveTextContent('running');
  timer.tick(); // self-check tick AND a stats sample

  // It RAN: the preview shows the session's own camera track, the mic is metered,
  // and the statistics path was read.
  const el = screen.getByTestId('self-check-video').element() as HTMLVideoElement;
  await vi.waitFor(() =>
    expect((el.srcObject as MediaStream | null)?.getVideoTracks()[0]).toBe(video),
  );
  await expect.element(screen.getByTestId('self-check-level')).toBeInTheDocument();
  await expect.element(screen.getByTestId('debug-stats-slot-audio:0')).toBeInTheDocument();
  expect(localStatsReads).toBeGreaterThan(0);
  // The meter's only source is the LOCAL capture track; nothing reaches the destination.
  expect(audioRec.sources.map((s) => s.getAudioTracks()[0])).toEqual([audio]);
  expect(audioRec.connects.every((d) => d instanceof AnalyserNode)).toBe(true);
  expect(audioRec.connects).toHaveLength(1);

  await screen.getByTestId('self-check-stop').click();
  screen.unmount();

  // Only the allow-listed getters were read ...
  const ALLOWED = new Set([
    'session.media',
    'session.video',
    'session.videoReceive',
    'media.captureTrack',
    'media.localStats',
    'video.captureTrack',
    'video.localSendStats',
    'videoReceive.localStats',
  ]);
  expect(reads.length).toBeGreaterThan(0);
  expect(reads.filter((r) => !ALLOWED.has(r))).toEqual([]);
  // ... and NOTHING SENT: no access at all to either transport seam.
  expect(transportLog).toEqual([]);
  // The publish path's tracks: never stopped, cloned or re-constrained; still live.
  for (const s of [...stops, ...clones]) expect(s).not.toHaveBeenCalled();
  expect(constrain).not.toHaveBeenCalled();
  expect(video.readyState).toBe('live');
  expect(video.enabled).toBe(true);
  // The meter's own context is closed.
  await vi.waitFor(() => expect(audioRec.contexts.every((c) => c.state === 'closed')).toBe(true));
});

test('a meter that cannot be built is "microphone meter unavailable" (not "no microphone"), built ONCE across ticks; the session track is untouched', async () => {
  const { video, audio } = await deviceTracks();
  const stop = vi.spyOn(audio, 'stop');
  let built = 0;
  const timer = manualTimer();
  const screen = await render(DebugPanel, {
    storage: OPEN,
    tracks: () => ({ video, audio }),
    selfCheckDeps: {
      setInterval: timer.setInterval,
      clearInterval: timer.clearInterval,
      audioContext: () => {
        built += 1;
        throw new Error('AudioContext refused');
      },
    },
  });
  await screen.getByTestId('self-check-start').click();
  for (let i = 0; i < 10; i += 1) timer.tick();
  await expect
    .element(screen.getByTestId('self-check-meter-failed'))
    .toHaveTextContent('microphone meter unavailable');
  await expect.element(screen.getByTestId('self-check-no-microphone')).not.toBeInTheDocument();
  // The camera still works; the check did not fail as a whole.
  await expect.element(screen.getByTestId('self-check-status')).toHaveTextContent('running');
  expect(built).toBe(1);
  expect(stop).not.toHaveBeenCalled();
  screen.unmount();
});

test('a throwing tracks getter FAILS LOUDLY: status failed, timer cleared, never stuck on running', async () => {
  const timer = manualTimer();
  let calls = 0;
  const check = new SelfCheck(
    {
      tracks: () => {
        calls += 1;
        if (calls > 1) throw new Error('getter broke');
        return {};
      },
      setInterval: timer.setInterval,
      clearInterval: timer.clearInterval,
    },
    () => undefined,
  );
  await check.start();
  expect(check.view.status).toBe('running');
  timer.tick();
  expect(check.view.status).toBe('failed');
  expect(check.view.failure).toBe(CaptureFailure.Unknown);
  expect(timer.active).toBe(0);
});

test('in-meeting: a video mute (track dropped) shows "no local camera track"; an unmute re-binds the new track', async () => {
  const first = await deviceTracks();
  const second = await deviceTracks();
  let current: LocalCaptureTracks = { video: first.video };
  const timer = manualTimer();
  const screen = await render(DebugPanel, {
    storage: OPEN,
    tracks: () => current,
    selfCheckDeps: { setInterval: timer.setInterval, clearInterval: timer.clearInterval },
  });
  await screen.getByTestId('self-check-start').click();
  const el = () => screen.getByTestId('self-check-video').element() as HTMLVideoElement;
  await vi.waitFor(() =>
    expect((el().srcObject as MediaStream | null)?.getVideoTracks()[0]).toBe(first.video),
  );
  current = {};
  timer.tick();
  await expect.element(screen.getByTestId('self-check-no-camera')).toBeInTheDocument();
  await vi.waitFor(() => expect(el().srcObject).toBeNull());
  current = { video: second.video };
  timer.tick();
  await vi.waitFor(() =>
    expect((el().srcObject as MediaStream | null)?.getVideoTracks()[0]).toBe(second.video),
  );
  screen.unmount();
});

test('pre-join (OWN): nothing is acquired until "Start self-check"; every track is stopped on stop', async () => {
  const { video, audio } = await deviceTracks();
  const stopped: string[] = [];
  const openPreview = vi.fn(async (): Promise<LocalPreview> => ({
    stream: new MediaStream([video, audio]),
    video,
    audio,
    stop: () => stopped.push('all'),
  }));
  const timer = manualTimer();
  const screen = await render(DebugPanel, {
    storage: OPEN,
    selfCheckDeps: {
      openPreview,
      setInterval: timer.setInterval,
      clearInterval: timer.clearInterval,
    },
  });
  await expect.element(screen.getByTestId('debug-panel')).toBeInTheDocument();
  expect(openPreview).not.toHaveBeenCalled();
  await screen.getByTestId('self-check-start').click();
  expect(openPreview).toHaveBeenCalledTimes(1);
  expect(openPreview).toHaveBeenCalledWith({ video: true, audio: true });
  await expect.element(screen.getByTestId('self-check-status')).toHaveTextContent('running');
  await screen.getByTestId('self-check-stop').click();
  expect(stopped).toEqual(['all']);
  expect(timer.active).toBe(0);
  screen.unmount();
});

test('pre-join (OWN): toggling the panel off and unmounting each release the capture', async () => {
  const { video } = await deviceTracks();
  let stops = 0;
  const openPreview = async (): Promise<LocalPreview> => ({
    stream: new MediaStream([video]),
    video,
    audio: undefined,
    stop: () => {
      stops += 1;
    },
  });
  const screen = await render(DebugPanel, { storage: OPEN, selfCheckDeps: { openPreview } });
  await screen.getByTestId('self-check-start').click();
  await expect.element(screen.getByTestId('self-check-status')).toHaveTextContent('running');
  await screen.getByTestId('debug-panel-toggle').click(); // hide
  expect(stops).toBe(1);

  const again = await render(DebugPanel, { storage: OPEN, selfCheckDeps: { openPreview } });
  await again.getByTestId('self-check-start').click();
  await expect.element(again.getByTestId('self-check-status')).toHaveTextContent('running');
  again.unmount(); // destroy: synchronous release
  expect(stops).toBe(2);
  screen.unmount();
});

test('pre-join (OWN): a preview that resolves AFTER the check was closed is stopped at once', async () => {
  let resolve: ((p: LocalPreview) => void) | undefined;
  const openPreview = () =>
    new Promise<LocalPreview>((r) => {
      resolve = r;
    });
  const views: SelfCheckView[] = [];
  const check = new SelfCheck({ openPreview }, (v) => views.push(v));
  const started = check.start();
  expect(check.view.status).toBe('starting');
  check.close();
  let stopped = 0;
  resolve!({
    stream: {} as MediaStream,
    video: undefined,
    audio: undefined,
    stop: () => {
      stopped += 1;
    },
  });
  await started;
  expect(stopped).toBe(1);
  expect(check.view.status).toBe('idle');
});

test('a capture failure shows the bounded CaptureFailure token, never the platform message', async () => {
  const openPreview = async (): Promise<LocalPreview> => {
    throw new MediaCaptureError(CaptureFailure.PermissionDenied, 'camera access was denied');
  };
  const screen = await render(DebugPanel, { storage: OPEN, selfCheckDeps: { openPreview } });
  await screen.getByTestId('self-check-start').click();
  await expect
    .element(screen.getByTestId('self-check-failure'))
    .toHaveTextContent('Self-check failed: permission_denied');
  screen.unmount();

  const raw = new SelfCheck(
    {
      openPreview: async () => {
        throw new Error('secret platform detail /dev/video0');
      },
    },
    () => undefined,
  );
  await raw.start();
  expect(raw.view.failure).toBe('unknown');
});

test('rmsLevel: silence is 0, full-scale square wave is 1', () => {
  expect(rmsLevel(new Uint8Array([128, 128, 128]))).toBe(0);
  expect(rmsLevel(new Uint8Array([0, 0, 0, 0]))).toBe(1);
  expect(rmsLevel(new Uint8Array([]))).toBe(0);
});
