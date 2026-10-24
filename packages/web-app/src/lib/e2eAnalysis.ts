// File: packages/web-app/src/lib/e2eAnalysis.ts
//
// Story 2 R-30: the E2E build's RECEIVE-SIDE INSTRUMENTATION — the recorder for
// layers 1/2 (+ drops) and the per-lane analyser for layer 3. Test builds only.
//
// ---------------------------------------------------------------------------
// EVERYTHING HERE IS BEHIND `if (__E2E_HOOKS__)`
// ---------------------------------------------------------------------------
//
// `createE2EInstrumentation` returns `undefined` in a production build, where the
// define is the literal `false`: the recorder class, the analyser construction
// and every string below are dead code and tree-shaken, which
// `tests/bundle-content.test.ts` proves (`createAnalyser`,
// `getFloatFrequencyData`, the bus event names). The AnalyserNode exists only
// when receive verification is on, because both are created here, together
// (@security).
//
// ---------------------------------------------------------------------------
// ANALYSED, NEVER RAW — AND NO DETECTOR
// ---------------------------------------------------------------------------
//
// Layer 3 is a per-lane (PRE-mix, post-decode) magnitude spectrum from a native
// `AnalyserNode`, band-clipped, in dB, sampled on the bus tick — never PCM,
// never phase, never time-domain samples, never per frame, and never into a
// metric, log, span or file. A signal LEVEL and a READY flag ride with it so
// "silent slot" and "wrong tone" are distinguishable and nothing is asserted
// over an unfilled window.
//
// This module does NO peak-picking and knows NOTHING about which tones exist:
// the tone detector is the test suite's (story 2 task 14, `e2e/`), implemented
// independently of the SDK's tone synthesis so a shared bug cannot cancel. The
// analysis band below is therefore its own quantity — wide enough to contain any
// tone the SDK documents — and deliberately not imported from the SDK.

import type { TileSurface } from '@darktower/sdk-svelte';
import {
  BoundedReceiveVerificationRecorder,
  createAudioContextPlaybackSink,
  type PlaybackLane,
  type PlaybackSink,
  type PlaybackSinkFactory,
  type ReceiveVerificationSnapshot,
} from '@darktower/sdk-core';

/** Analyser FFT size. At 48 kHz one bin is 48000 / 8192 ≈ 5.86 Hz. */
export const E2E_ANALYSER_FFT_SIZE = 8192;
/** Explicit, so the detector reads it instead of assuming it (@test). */
export const E2E_ANALYSER_SMOOTHING = 0;
/** `getFloatFrequencyData` is unclamped dB; these bound the Byte variant only. */
const E2E_ANALYSER_MIN_DB = -140;
const E2E_ANALYSER_MAX_DB = 0;
/**
 * The analysis band, Hz. Contains the SDK's documented tone band with margin
 * (asserted by `src/__tests__/e2eAnalysisBand.test.ts`, which is a TEST and so
 * may read the SDK's constants; this module does not). Sized to fit within
 * {@link E2E_ANALYSIS_MAX_BINS} at 44.1 kHz as well as 48 kHz, so the band is
 * delivered whole. At a rate where it would NOT fit, attaching the analyser
 * THROWS rather than clipping the band silently (@test): a tone pushed outside
 * the published spectrum would read as missing audio.
 */
export const E2E_ANALYSIS_BAND_START_HZ = 560;
export const E2E_ANALYSIS_BAND_END_HZ = 1200;
/** Hard bound on bins per record (@security: <= 120). */
export const E2E_ANALYSIS_MAX_BINS = 120;

/** One lane's analysed state, as sampled. Plain numbers only. */
export interface LaneAnalysis {
  /** The lane's verified key-id sender. */
  readonly senderId: number;
  /** RMS level over the analyser's current window, dBFS. `-Infinity` for silence. */
  readonly levelDbfs: number;
  /** True once the analyser has seen a full FFT window of this lane's audio. */
  readonly ready: boolean;
  readonly sampleRateHz: number;
  readonly fftSize: number;
  readonly smoothingTimeConstant: number;
  readonly binHz: number;
  /** Frequency of `spectrumDb[0]`. */
  readonly bandStartHz: number;
  /** Frequency of the LAST `spectrumDb` entry — the band actually delivered. */
  readonly bandEndHz: number;
  /** Magnitude, dB (`getFloatFrequencyData`, unclamped), rounded to 0.1 dB. */
  readonly spectrumDb: readonly number[];
}

/**
 * The render-surface luma grid (README §Render-surface luma projection): 6 x 4.
 *
 * DELIBERATELY NOT IMPORTED FROM THE GENERATOR (`sdk-core media/setup/
 * testPattern.ts` `TEST_PATTERN_COLUMNS/ROWS` / `testPatternRects()`), and its
 * block edges are re-derived here from the README text (DRY review, recorded
 * as a deliberate non-collapse). The sampler is the MEASURING side of layer 3:
 * if it took the generator's rects, a generator that painted its blocks in the
 * wrong place would be sampled in that same wrong place and still decode — a
 * self-consistent geometry error invisible to the detector. Reading the canvas
 * at the README's geometry is what makes a generator drift fail loudly.
 */
export const E2E_LUMA_GRID_COLUMNS = 6;
export const E2E_LUMA_GRID_ROWS = 4;

/** One video tile's render-surface sample. Counts and 24 integer lumas only. */
export interface SurfaceSample {
  readonly framesRendered: number;
  /** Present only once at least one frame has been painted. */
  readonly lumas: readonly number[] | undefined;
}

/** What the bus reads. */
export interface E2EInstrumentation {
  /**
   * Story 3 C-C1: handed to each `VideoTile` as `onSurface`, so the bus can
   * sample WHAT WAS PAINTED (the canvas), not the decoder output — a frame
   * painted into the wrong slot reads as the wrong id.
   */
  readonly onSurface: (surface: TileSurface | undefined, slotId: number) => void;
  /**
   * The slot's render-surface sample, or `undefined` before its tile mounted.
   * `withLumas`: read the canvas only when the caller will publish lumas (the
   * slot renders video) — nothing is read to be discarded. Sample it.
   */
  surface(slotId: number, options: { readonly withLumas: boolean }): SurfaceSample | undefined;
  /** Injected as `MeetingSessionOptions.receiveVerification`. */
  readonly recorder: BoundedReceiveVerificationRecorder;
  /** Injected as `MeetingSessionOptions.playbackFactory`. */
  readonly playbackFactory: PlaybackSinkFactory;
  /** Layers 1/2 + drops, cumulative. Sample it; never call it per frame. */
  verification(): ReceiveVerificationSnapshot;
  /** One record per OPEN lane. Sample it; never call it per frame. */
  analysis(): LaneAnalysis[];
}

interface Tap {
  readonly analyser: AnalyserNode;
  readonly frequency: Float32Array<ArrayBuffer>;
  readonly time: Float32Array<ArrayBuffer>;
  /**
   * `context.currentTime` of the lane's FIRST decoded frame, or `undefined`
   * before one. Lanes open eagerly at assignment, so the lane-open time would
   * make `ready` true over silence.
   */
  firstAudioAt: number | undefined;
}

const round1 = (v: number): number => Math.round(v * 10) / 10;

/**
 * The 24 block-mean lumas of a painted canvas, row-major (README §Render-surface
 * luma projection): Y = 0.299R + 0.587G + 0.114B over each block's CENTRAL HALF
 * in each dimension, rounded to an integer. The tile sizes the backing store to
 * the frame, so the drawn picture is the whole canvas (no letterbox).
 * `undefined` when nothing has been painted.
 */
function sampleLumas(canvas: HTMLCanvasElement): number[] | undefined {
  const width = canvas.width;
  const height = canvas.height;
  if (width === 0 || height === 0) return undefined;
  const ctx = canvas.getContext('2d');
  if (ctx === null) throw new Error('E2E instrumentation: the tile canvas has no 2d context');
  const { data } = ctx.getImageData(0, 0, width, height);
  const out: number[] = [];
  for (let r = 0; r < E2E_LUMA_GRID_ROWS; r += 1) {
    const by = Math.floor((r * height) / E2E_LUMA_GRID_ROWS);
    const bh = Math.floor(((r + 1) * height) / E2E_LUMA_GRID_ROWS) - by;
    const y0 = Math.ceil(by + bh / 4);
    const y1 = Math.max(y0 + 1, Math.ceil(by + (3 * bh) / 4));
    for (let c = 0; c < E2E_LUMA_GRID_COLUMNS; c += 1) {
      const bx = Math.floor((c * width) / E2E_LUMA_GRID_COLUMNS);
      const bw = Math.floor(((c + 1) * width) / E2E_LUMA_GRID_COLUMNS) - bx;
      const x0 = Math.ceil(bx + bw / 4);
      const x1 = Math.max(x0 + 1, Math.ceil(bx + (3 * bw) / 4));
      let sum = 0;
      let n = 0;
      for (let y = y0; y < y1; y += 1) {
        for (let x = x0; x < x1; x += 1) {
          const i = (y * width + x) * 4;
          sum += 0.299 * data[i]! + 0.587 * data[i + 1]! + 0.114 * data[i + 2]!;
          n += 1;
        }
      }
      out.push(Math.min(255, Math.max(0, Math.round(sum / n))));
    }
  }
  return out;
}

/**
 * Build the instrumentation for ONE session, or `undefined` outside an E2E
 * build. Construct it before the `MeetingSession` it is injected into.
 */
export function createE2EInstrumentation(): E2EInstrumentation | undefined {
  if (__E2E_HOOKS__) {
    const recorder = new BoundedReceiveVerificationRecorder();
    const taps = new Map<number, Tap>();

    const attach = (senderId: number, output: AudioNode): void => {
      const binHz = output.context.sampleRate / E2E_ANALYSER_FFT_SIZE;
      const bins =
        Math.ceil(E2E_ANALYSIS_BAND_END_HZ / binHz) -
        Math.floor(E2E_ANALYSIS_BAND_START_HZ / binHz) +
        1;
      if (bins > E2E_ANALYSIS_MAX_BINS) {
        throw new Error(
          `E2E analysis band ${E2E_ANALYSIS_BAND_START_HZ}-${E2E_ANALYSIS_BAND_END_HZ} Hz needs ` +
            `${bins} bins at ${output.context.sampleRate} Hz, above the ${E2E_ANALYSIS_MAX_BINS}-bin bound`,
        );
      }
      const analyser = output.context.createAnalyser();
      analyser.fftSize = E2E_ANALYSER_FFT_SIZE;
      analyser.smoothingTimeConstant = E2E_ANALYSER_SMOOTHING;
      analyser.minDecibels = E2E_ANALYSER_MIN_DB;
      analyser.maxDecibels = E2E_ANALYSER_MAX_DB;
      // Observation only: the analyser is a sink, connected to nothing, so the
      // lane's audio is not duplicated into the mix.
      output.connect(analyser);
      taps.set(senderId, {
        analyser,
        frequency: new Float32Array(analyser.frequencyBinCount),
        time: new Float32Array(analyser.fftSize),
        firstAudioAt: undefined,
      });
    };

    const detach = (senderId: number, output: AudioNode): void => {
      const tap = taps.get(senderId);
      if (!tap) return;
      try {
        output.disconnect(tap.analyser);
      } catch {
        // Already disconnected by the lane's own close.
      }
      taps.delete(senderId);
    };

    const playbackFactory: PlaybackSinkFactory = async (options) => {
      const inner = await createAudioContextPlaybackSink(options);
      const sink: PlaybackSink = {
        openLane(senderId: number): PlaybackLane {
          const lane = inner.openLane(senderId);
          const output = lane.output;
          // A lane with no output node cannot be analysed: in THIS
          // instrumentation that is a broken harness, not an unanalysed sender,
          // and a silent skip would read to the detector as a receive failure.
          if (!output) {
            throw new Error('E2E instrumentation: the playback lane exposes no output node');
          }
          attach(senderId, output);
          return {
            enqueue: (data) => {
              const tap = taps.get(senderId);
              if (tap && tap.firstAudioAt === undefined) {
                tap.firstAudioAt = output.context.currentTime;
              }
              lane.enqueue(data);
            },
            close: () => {
              detach(senderId, output);
              lane.close();
            },
            output,
          };
        },
        close: () => {
          taps.clear();
          inner.close();
        },
      };
      return sink;
    };

    const analysis = (): LaneAnalysis[] => {
      const out: LaneAnalysis[] = [];
      for (const [senderId, tap] of taps) {
        const { analyser } = tap;
        const sampleRateHz = analyser.context.sampleRate;
        const binHz = sampleRateHz / analyser.fftSize;
        analyser.getFloatFrequencyData(tap.frequency);
        analyser.getFloatTimeDomainData(tap.time);
        let sumSquares = 0;
        for (const v of tap.time) sumSquares += v * v;
        const rms = Math.sqrt(sumSquares / tap.time.length);
        const first = Math.floor(E2E_ANALYSIS_BAND_START_HZ / binHz);
        // Fits by construction: `attach` refused any rate where it would not.
        const last = Math.min(
          Math.ceil(E2E_ANALYSIS_BAND_END_HZ / binHz),
          tap.frequency.length - 1,
        );
        const spectrumDb: number[] = [];
        for (let i = first; i <= last; i += 1)
          spectrumDb.push(round1(tap.frequency[i] ?? -Infinity));
        out.push({
          senderId,
          levelDbfs: rms > 0 ? round1(20 * Math.log10(rms)) : -Infinity,
          // A full FFT window of DECODED audio has been played into this lane.
          ready:
            tap.firstAudioAt !== undefined &&
            analyser.context.currentTime - tap.firstAudioAt >= analyser.fftSize / sampleRateHz,
          sampleRateHz,
          fftSize: analyser.fftSize,
          smoothingTimeConstant: analyser.smoothingTimeConstant,
          binHz,
          bandStartHz: first * binHz,
          bandEndHz: last * binHz,
          spectrumDb,
        });
      }
      return out;
    };

    // Story 3 C-C1: the painted render surfaces, by slot.
    const surfaces = new Map<number, TileSurface>();
    const onSurface = (surface: TileSurface | undefined, slotId: number): void => {
      if (surface === undefined) surfaces.delete(slotId);
      else surfaces.set(slotId, surface);
    };
    const surface = (
      slotId: number,
      { withLumas }: { readonly withLumas: boolean },
    ): SurfaceSample | undefined => {
      const s = surfaces.get(slotId);
      if (s === undefined) return undefined;
      const framesRendered = s.framesRendered();
      return {
        framesRendered,
        lumas: withLumas && framesRendered > 0 ? sampleLumas(s.canvas) : undefined,
      };
    };

    return {
      recorder,
      playbackFactory,
      onSurface,
      surface,
      verification: () => recorder.snapshot(),
      analysis,
    };
  }
  return undefined;
}
