// File: packages/web-app/src/lib/selfCheck.ts
//
// The R-7 camera and microphone LOOPBACK self-check (story 3): the local camera
// shown in a `<video>` element and the local microphone METERED, straight from
// the local capture. OFF until the user presses "Start self-check".
//
// NOTHING IS SENT. This module touches no encoder, packetiser, SFrame stack,
// handler or transport — it never sees a session, only a capture-track getter
// or the SDK's local preview opener. `packages/web-app/tests/debugNeverExported.test.ts`
// denies it every transport, telemetry and test-bus import, and
// `src/__tests__/selfCheck.test.ts` proves zero transport calls by injection.
//
// TWO MODES, AND WHO OWNS THE CAPTURE IS THE WHOLE DIFFERENCE
//
//   * REUSE (in a meeting): reads the session's OWN tracks through the
//     injected `tracks()` getter (`localCaptureTracksOf(session)`). It holds
//     references only and NEVER stops, clones, re-constrains or toggles them —
//     the publish path owns the camera lifecycle and the camera-light rule
//     (camera live <=> directed AND not client-muted AND healthy). It re-reads
//     them on every tick, so a video mute (track stopped, reference dropped)
//     shows "no local camera track" and an unmute re-binds the new track. It
//     never opens a capture of its own.
//   * OWN (before join): opens ONE local preview through the SDK
//     (`openLocalPreview`, the same bounded failure vocabulary as every
//     capture), and STOPS every track on stop/close/destroy. A preview that
//     resolves after the check was closed is stopped at once.
//
// THE MICROPHONE IS METERED, NEVER PLAYED: its own `AudioContext`, a source
// node on the local track, an `AnalyserNode` read in the TIME domain for one
// scalar RMS level, and nothing connected to `destination` (audible playback
// in a meeting would be re-captured and sent). No frequency-domain read, no
// retained history. The context is closed on stop.

import {
  CaptureFailure,
  MediaCaptureError,
  openLocalPreview,
  type LocalCaptureTracks,
  type LocalPreview,
} from '@darktower/sdk-core';

/** How often the meter (and, reusing, the track binding) is re-read while running. */
export const SELF_CHECK_TICK_MS = 100;

/** The subset of `AudioContext` the meter uses. */
export interface MeterContext {
  createMediaStreamSource(stream: MediaStream): MediaStreamAudioSourceNode;
  createAnalyser(): AnalyserNode;
  close(): Promise<void>;
}

/** Injected seams. Every default is the browser's own. */
export interface SelfCheckDeps {
  /** REUSE mode: the session's own capture tracks. Absent => OWN mode. */
  readonly tracks?: () => LocalCaptureTracks;
  /** OWN mode: open a local preview (`openLocalPreview` by default). */
  readonly openPreview?: (kinds: { video: boolean; audio: boolean }) => Promise<LocalPreview>;
  readonly audioContext?: () => MeterContext;
  /** Wrap one track for a media element / source node (`new MediaStream([t])`). */
  readonly streamOf?: (track: MediaStreamTrack) => MediaStream;
  readonly setInterval?: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly clearInterval?: (handle: ReturnType<typeof setInterval>) => void;
}

/** What the panel renders. */
export interface SelfCheckView {
  readonly status: 'idle' | 'starting' | 'running' | 'failed';
  /** The camera track to show, or `undefined` (none / not lit). */
  readonly video: MediaStreamTrack | undefined;
  /** Whether a microphone track is being metered. */
  readonly metering: boolean;
  /**
   * A microphone track EXISTS but its meter could not be built (e.g. the
   * platform refused an AudioContext). Distinct from "no microphone track".
   */
  readonly meterFailed: boolean;
  /** RMS level 0..1, or `undefined` before the first reading. */
  readonly level: number | undefined;
  /** A bounded capture-failure token (never a platform message). */
  readonly failure: CaptureFailure | undefined;
}

const IDLE: SelfCheckView = {
  status: 'idle',
  video: undefined,
  metering: false,
  meterFailed: false,
  level: undefined,
  failure: undefined,
};

interface Meter {
  readonly track: MediaStreamTrack;
  readonly source: MediaStreamAudioSourceNode;
  readonly analyser: AnalyserNode;
  readonly buffer: Uint8Array<ArrayBuffer>;
}

/** THE default `streamOf` seam: wrap one track for a media element or a source node. */
export function streamOfTrack(track: MediaStreamTrack): MediaStream {
  return new MediaStream([track]);
}

/** RMS of 8-bit time-domain samples centred on 128, in 0..1. */
export function rmsLevel(samples: Uint8Array): number {
  if (samples.length === 0) return 0;
  let sum = 0;
  for (const s of samples) {
    const v = (s - 128) / 128;
    sum += v * v;
  }
  return Math.sqrt(sum / samples.length);
}

/** See the module header. One instance per panel; {@link close} it on destroy. */
export class SelfCheck {
  readonly #deps: SelfCheckDeps;
  readonly #onChange: (view: SelfCheckView) => void;
  #view: SelfCheckView = IDLE;
  #attempt = 0;
  #preview: LocalPreview | undefined;
  #meter: Meter | undefined;
  /**
   * ONE context per run, created on the first meter (inside the Start gesture)
   * and reused when the metered track changes, so a rebind from the timer never
   * builds a context outside a user gesture (which Chrome may leave suspended).
   * Closed on stop.
   */
  #context: MeterContext | undefined;
  /** The track whose meter failed: not retried until the track identity changes. */
  #meterFailedFor: MediaStreamTrack | undefined;
  #timer: ReturnType<typeof setInterval> | undefined;

  constructor(deps: SelfCheckDeps, onChange: (view: SelfCheckView) => void) {
    this.#deps = deps;
    this.#onChange = onChange;
  }

  get view(): SelfCheckView {
    return this.#view;
  }

  /** Whether this check reuses the session's capture (never opens its own). */
  get reusing(): boolean {
    return this.#deps.tracks !== undefined;
  }

  /** Start on a user gesture. Idempotent while starting or running. */
  async start(): Promise<void> {
    if (this.#view.status === 'starting' || this.#view.status === 'running') return;
    const attempt = (this.#attempt += 1);
    if (this.#deps.tracks === undefined) {
      this.#set({ ...IDLE, status: 'starting' });
      let preview: LocalPreview;
      try {
        preview = await (this.#deps.openPreview ?? openLocalPreview)({ video: true, audio: true });
      } catch (err) {
        if (attempt !== this.#attempt) return;
        this.#set({ ...IDLE, status: 'failed', failure: failureToken(err) });
        return;
      }
      if (attempt !== this.#attempt) {
        // Closed (or restarted) while the acquisition was in flight: release now.
        preview.stop();
        return;
      }
      this.#preview = preview;
    }
    this.#set({ ...IDLE, status: 'running' });
    const setI = this.#deps.setInterval ?? ((fn, ms) => setInterval(fn, ms));
    this.#timer = setI(() => this.#tick(), SELF_CHECK_TICK_MS);
    this.#tick();
  }

  /**
   * Stop SYNCHRONOUSLY: the timer, the meter graph, and — in OWN mode only —
   * every preview track. A reused session track is never stopped. Idempotent.
   */
  stop(): void {
    this.#attempt += 1;
    if (this.#timer !== undefined) {
      (this.#deps.clearInterval ?? ((h) => clearInterval(h)))(this.#timer);
      this.#timer = undefined;
    }
    this.#closeMeter();
    this.#meterFailedFor = undefined;
    void this.#context?.close().catch(() => undefined);
    this.#context = undefined;
    this.#preview?.stop();
    this.#preview = undefined;
    if (this.#view !== IDLE) this.#set(IDLE);
  }

  /** The panel is going away. Same as {@link stop}. */
  close(): void {
    this.stop();
  }

  #currentTracks(): { video: MediaStreamTrack | undefined; audio: MediaStreamTrack | undefined } {
    if (this.#deps.tracks !== undefined) {
      const t = this.#deps.tracks();
      return { video: live(t.video), audio: live(t.audio) };
    }
    return { video: live(this.#preview?.video), audio: live(this.#preview?.audio) };
  }

  /**
   * One sample. FAILS LOUDLY: any throw (the tracks getter, the meter graph, the
   * analyser read) releases everything and shows `failed` with a bounded token,
   * never a check stuck on "running" or a swallowed error.
   */
  #tick(): void {
    if (this.#view.status !== 'running') return;
    try {
      const { video, audio } = this.#currentTracks();
      if (this.#meter?.track !== audio && this.#meterFailedFor !== audio) {
        this.#closeMeter();
        this.#meterFailedFor = undefined;
        if (audio !== undefined) {
          try {
            this.#openMeter(audio);
          } catch {
            // Reported as `meterFailed`, never as "no microphone"; retried only
            // when the track changes (no AudioContext churn on every tick).
            this.#closeMeter();
            this.#meterFailedFor = audio;
          }
        }
      }
      const meter = this.#meter;
      let level: number | undefined;
      if (meter !== undefined) {
        meter.analyser.getByteTimeDomainData(meter.buffer);
        level = rmsLevel(meter.buffer);
      }
      this.#set({
        ...this.#view,
        video,
        metering: meter !== undefined,
        meterFailed: audio !== undefined && this.#meterFailedFor === audio,
        level,
      });
    } catch (err) {
      this.stop();
      this.#set({ ...IDLE, status: 'failed', failure: failureToken(err) });
    }
  }

  /** Build the meter graph on `track`. Throws on failure (the tick reports it). */
  #openMeter(track: MediaStreamTrack): void {
    const context = (this.#context ??= (this.#deps.audioContext ?? (() => new AudioContext()))());
    const stream = (this.#deps.streamOf ?? streamOfTrack)(track);
    const source = context.createMediaStreamSource(stream);
    try {
      const analyser = context.createAnalyser();
      analyser.fftSize = 256;
      // Metered only: NEVER connected to `context.destination`.
      source.connect(analyser);
      this.#meter = { track, source, analyser, buffer: new Uint8Array(analyser.fftSize) };
    } catch (err) {
      source.disconnect();
      throw err;
    }
  }

  #closeMeter(): void {
    const meter = this.#meter;
    this.#meter = undefined;
    if (meter === undefined) return;
    try {
      meter.source.disconnect();
    } catch {
      // Already disconnected.
    }
  }

  #set(view: SelfCheckView): void {
    this.#view = view;
    this.#onChange(view);
  }
}

function live(track: MediaStreamTrack | undefined): MediaStreamTrack | undefined {
  return track !== undefined && track.readyState !== 'ended' ? track : undefined;
}

/** A bounded capture-failure token: the SDK's own classification, else `Unknown`. */
function failureToken(err: unknown): CaptureFailure {
  return err instanceof MediaCaptureError ? err.reason : CaptureFailure.Unknown;
}
