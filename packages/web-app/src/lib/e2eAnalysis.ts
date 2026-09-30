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

/** What the bus reads. */
export interface E2EInstrumentation {
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

    return {
      recorder,
      playbackFactory,
      verification: () => recorder.snapshot(),
      analysis,
    };
  }
  return undefined;
}
