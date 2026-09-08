// File: packages/sdk-core/src/media/setup/measurement.ts
//
// The MEASUREMENT SEAM for first-media-received (ADR-0036 §10).
//
// ---------------------------------------------------------------------------
// OBSERVED, NEVER GATED
// ---------------------------------------------------------------------------
//
// §10: "Latency is observed, never gated. Asserting a wall-clock end-to-end
// target on a local cluster produces a permanent flake; ADR-0028 forbids
// quarantining quality gates, so the test would be deleted and the headline
// objective would end with ZERO coverage."
//
// So this seam reports a duration and carries NO threshold, NO target, and no
// comparison. Nothing in the SDK or its test suite asserts a bound on the value
// it produces. If a threshold ever appears against this metric, that is the
// defect §10 describes, not a tightening.
//
// "Sampled from the start" is the other half: `start()` is called when the media
// pipeline starts, BEFORE the first datagram can arrive, so the measurement
// exists for every session rather than only for sessions that got far enough for
// someone to instrument them.
//
// ---------------------------------------------------------------------------
// THIS IS NOT `MeetingSession.#histogram`, AND THE RESEMBLANCE IS THE HAZARD
// ---------------------------------------------------------------------------
//
// `MeetingSession` has a private `#histogram(name)` helper that emits
// `clock() - joinStartMs` with the join label set. `dt_client_time_to_first_media_frame_ms`
// looks like a fourth caller of it and must not be one, for two independent
// reasons:
//
//   * **Different epoch.** First-media is measured from MEDIA START, not from
//     join start. Reusing that helper would silently fold the whole join
//     duration into a media measurement.
//   * **Wrong label set.** That helper spreads the join labels, which carry
//     `meeting_id_hash` — the one dimension ADR-0036 §11 bars from every media
//     emission.
//
// The resemblance is close enough that a future reader will try to collapse
// them, and the collapse reintroduces the forbidden label. That is why the
// boundary is written here, where the collapse would be attempted.

import type { MediaMetrics } from './mediaMetrics.js';

/**
 * First-media-received measurement.
 *
 * Records at most one observation per instance: the transition from "no media
 * yet" to "media arriving" happens once, and a repeated observation would turn a
 * startup measurement into a per-frame one.
 */
export class FirstMediaObserver {
  readonly #metrics: MediaMetrics;
  readonly #clock: () => number;
  readonly #onObserved: ((elapsedMs: number) => void) | undefined;

  #startedAtMs: number | undefined;
  #observed = false;

  /**
   * @param onObserved optional side channel for the SDK's public
   * `firstMediaFrame` event. Receives the same elapsed value the histogram gets,
   * so an embedder and the metric can never disagree about the number.
   */
  constructor(
    metrics: MediaMetrics,
    clock: () => number,
    onObserved?: (elapsedMs: number) => void,
  ) {
    this.#metrics = metrics;
    this.#clock = clock;
    this.#onObserved = onObserved;
  }

  /** Begin measuring. Called at pipeline start, before any datagram can arrive. */
  start(): void {
    this.#startedAtMs = this.#clock();
    this.#observed = false;
  }

  /**
   * Called for every arriving datagram; observes on the first one only.
   *
   * A flag check after the first frame, so this costs one boolean read on the
   * per-frame path and never a clock read.
   */
  onFrameReceived(): void {
    if (this.#observed) return;
    const startedAt = this.#startedAtMs;
    // No start() means no epoch, so there is nothing honest to report. Silently
    // skipping is correct here and only here: the alternative is inventing an
    // epoch, which would produce a plausible wrong number rather than none.
    if (startedAt === undefined) return;
    this.#observed = true;
    const elapsedMs = this.#clock() - startedAt;
    this.#metrics.timeToFirstMediaFrameMs(elapsedMs);
    this.#onObserved?.(elapsedMs);
  }

  /** Whether the first frame has been observed. For teardown bookkeeping only. */
  get hasObserved(): boolean {
    return this.#observed;
  }

  /** Reset. No key material here; a plain state reset for symmetry with teardown. */
  clear(): void {
    this.#startedAtMs = undefined;
    this.#observed = false;
  }
}
