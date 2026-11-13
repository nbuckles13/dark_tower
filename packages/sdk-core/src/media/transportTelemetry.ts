// File: packages/sdk-core/src/media/transportTelemetry.ts
//
// THE per-ref connection telemetry of the media transport (story 4 C2, R-42;
// names frozen in `docs/debates/2026-10-28-mh-replacement-and-migration/
// telemetry-contract.md` §Client). `MediaTransport` owns connection
// management; it only reports lifecycle HOOKS here (dial started, handshake,
// dial failed, session closed, ref removed, teardown) and this module decides
// every observation, so its invariants are unit-testable without a transport:
//
//   * EXACTLY ONE `dt_client_mh_connection_total` outcome per dial that reaches
//     one while the session lives. A handshake is PROVISIONAL for
//     `admissionWindowMs`: MH has no admission ack, and a refusal arrives as a
//     close right after the handshake. A close inside the window is the dial's
//     outcome (close-code token, `transport_failure` with no code); surviving
//     the window is `connected`, and the session is then ESTABLISHED. A refusal
//     arriving after the window is recorded as `connected` followed by
//     `disconnects{cause}` — misattributed between two series, never lost.
//   * `dt_client_mh_disconnects_total{cause}` once per end of an ESTABLISHED
//     session, never for a dial that did not establish.
//   * `dt_client_mh_reconnect_duration_ms` per ref: from the loss of an
//     established in-set handler (`isHandlerLoss(cause)`) to the next dial of
//     that ref finalising connected (valued to its handshake), the ref's
//     removal, or abandonment (session end or `reconnectAbandonMs`).
//   * `dt_client_time_to_first_mh_connected_ms` once per session, at the first
//     dial that FINALISES connected, valued to its handshake.
//   * LOCAL SESSION TEARDOWN records nothing on the connection or disconnect
//     series (no vocabulary value exists for it); an open reconnect episode is
//     `abandoned`.
//
// Holds NO credential: timers capture the per-ref state object, never a token.
// No ref, URL or slot ever reaches a label.

import type { TransportScheduler } from './events.js';
import { dialCloseOutcomeOf, disconnectCauseOf } from './closeCode.js';
import {
  dialDurationOutcome,
  isHandlerLoss,
  type MediaSessionMetrics,
  type MhDialOutcome,
  type MhDialPhase,
  type MhDisconnectCause,
  type MhReconnectOutcome,
} from './setup/mediaMetrics.js';

/** The metric methods this tracker uses (a narrow view of `MediaSessionMetrics`). */
export type MhConnectionMetrics = Pick<
  MediaSessionMetrics,
  'mhConnection' | 'mhDialDurationMs' | 'mhDisconnect' | 'mhReconnectDurationMs'
>;

/** Construction options for {@link TransportTelemetry}. */
export interface TransportTelemetryOptions {
  readonly metrics: MhConnectionMetrics | undefined;
  readonly clock: () => number;
  readonly scheduler: TransportScheduler;
  /** `media.handlers.admissionWindowMs`. */
  readonly admissionWindowMs: number;
  /** `media.handlers.reconnectAbandonMs`. */
  readonly reconnectAbandonMs: number;
  /**
   * Called ONCE per tracker, when the first dial FINALISES connected, with that
   * dial's handshake time (the session turns it into
   * `dt_client_time_to_first_mh_connected_ms`).
   */
  readonly onFirstEstablished?: (handshakeAtMs: number) => void;
}

/** A dial that has not reached its outcome yet. */
interface PendingDial {
  readonly phase: MhDialPhase;
  readonly startedAt: number;
  /** Set at the handshake: the dial is then provisional until the window ends. */
  handshakeAt: number | undefined;
  admissionTimer: unknown;
}

/**
 * One ref's telemetry state, held by the ref's record in `MediaTransport`. A
 * removed or torn-down ref's state is inert (`ended`).
 */
export interface RefTelemetry {
  dial: PendingDial | undefined;
  established: boolean;
  /** An open per-ref disruption episode (loss of an established in-set handler). */
  loss: { readonly startedAt: number; timer: unknown } | undefined;
  ended: boolean;
}

/** The per-ref connection telemetry tracker. One per `MediaTransport`. */
export class TransportTelemetry {
  readonly #options: TransportTelemetryOptions;
  #firstEstablished = false;

  constructor(options: TransportTelemetryOptions) {
    this.#options = options;
  }

  /** A fresh, inert state for a ref entering the set. */
  track(): RefTelemetry {
    return { dial: undefined, established: false, loss: undefined, ended: false };
  }

  /** A dial of the ref started (`phase` decided by the transport). */
  dialStarted(rt: RefTelemetry, phase: MhDialPhase): void {
    if (rt.ended) return;
    this.#clearAdmission(rt);
    rt.dial = {
      phase,
      startedAt: this.#options.clock(),
      handshakeAt: undefined,
      admissionTimer: undefined,
    };
  }

  /**
   * The dial failed BEFORE its handshake (`timeout`, or `transport_failure`
   * for a refused / failed connect).
   */
  dialFailed(rt: RefTelemetry, outcome: 'timeout' | 'transport_failure'): void {
    if (rt.ended || !rt.dial || rt.dial.handshakeAt !== undefined) return;
    this.#finalise(rt, outcome);
  }

  /** The handshake completed: the dial is PROVISIONAL for the admission window. */
  handshake(rt: RefTelemetry): void {
    const dial = rt.dial;
    if (rt.ended || !dial || dial.handshakeAt !== undefined) return;
    dial.handshakeAt = this.#options.clock();
    dial.admissionTimer = this.#options.scheduler.setTimeout(() => {
      dial.admissionTimer = undefined;
      if (rt.ended || rt.dial !== dial) return;
      this.#finalise(rt, 'connected');
    }, this.#options.admissionWindowMs);
  }

  /**
   * A session that completed its handshake CLOSED (`closeCode` undefined = no
   * clean close). Inside the admission window it is the dial's outcome; after
   * it, a disconnect of an established session.
   *
   * @returns the disconnect cause when an ESTABLISHED session ended, else
   *   `undefined` (a provisional session never counts as a disconnect).
   */
  closed(rt: RefTelemetry, closeCode: number | undefined): MhDisconnectCause | undefined {
    if (rt.ended) return undefined;
    if (rt.dial?.handshakeAt !== undefined) {
      // Inside the admission window: a failed dial, never a disconnect.
      this.#finalise(rt, dialCloseOutcomeOf(closeCode));
      return undefined;
    }
    if (!rt.established) return undefined;
    rt.established = false;
    const cause = disconnectCauseOf(closeCode);
    this.#options.metrics?.mhDisconnect(cause);
    if (isHandlerLoss(cause)) this.#startLoss(rt);
    return cause;
  }

  /**
   * The ref LEFT THE SET (a directed removal). A dial in flight or provisional
   * is `aborted_by_removal`; an established session is `directed_removal`; an
   * open disruption episode ends `removed`. The state is inert afterwards.
   */
  removed(rt: RefTelemetry): void {
    if (rt.ended) return;
    if (rt.dial) {
      this.#finalise(rt, 'aborted_by_removal');
    } else if (rt.established) {
      rt.established = false;
      this.#options.metrics?.mhDisconnect('directed_removal');
    }
    this.#endLoss(rt, 'removed');
    rt.ended = true;
  }

  /**
   * LOCAL SESSION TEARDOWN: records nothing on the connection or disconnect
   * series (a dial in flight reaches no outcome); an open disruption episode is
   * `abandoned`. The state is inert afterwards.
   */
  teardown(rt: RefTelemetry): void {
    if (rt.ended) return;
    this.#clearAdmission(rt);
    rt.dial = undefined;
    rt.established = false;
    this.#endLoss(rt, 'abandoned');
    rt.ended = true;
  }

  #finalise(rt: RefTelemetry, outcome: MhDialOutcome): void {
    const dial = rt.dial;
    if (!dial) return;
    this.#clearAdmission(rt);
    rt.dial = undefined;
    const metrics = this.#options.metrics;
    metrics?.mhConnection(dial.phase, outcome);
    const now = this.#options.clock();
    const durationOutcome = dialDurationOutcome(outcome);
    // `connected` is measured to the handshake; `failed` to the failure/close.
    const endAt = durationOutcome === 'connected' ? (dial.handshakeAt ?? now) : now;
    metrics?.mhDialDurationMs(dial.phase, durationOutcome, endAt - dial.startedAt);
    if (outcome !== 'connected') return;
    rt.established = true;
    const handshakeAt = dial.handshakeAt ?? now;
    if (rt.loss) this.#endLoss(rt, 'reconnected', handshakeAt);
    if (!this.#firstEstablished) {
      this.#firstEstablished = true;
      this.#options.onFirstEstablished?.(handshakeAt);
    }
  }

  #startLoss(rt: RefTelemetry): void {
    if (rt.loss) return;
    const loss: { readonly startedAt: number; timer: unknown } = {
      startedAt: this.#options.clock(),
      timer: undefined,
    };
    loss.timer = this.#options.scheduler.setTimeout(() => {
      loss.timer = undefined;
      if (rt.loss !== loss) return;
      this.#endLoss(rt, 'abandoned');
    }, this.#options.reconnectAbandonMs);
    rt.loss = loss;
  }

  #endLoss(rt: RefTelemetry, outcome: MhReconnectOutcome, at?: number): void {
    const loss = rt.loss;
    if (!loss) return;
    rt.loss = undefined;
    if (loss.timer !== undefined) this.#options.scheduler.clearTimeout(loss.timer);
    const end = at ?? this.#options.clock();
    this.#options.metrics?.mhReconnectDurationMs(outcome, Math.max(0, end - loss.startedAt));
  }

  #clearAdmission(rt: RefTelemetry): void {
    const dial = rt.dial;
    if (dial?.admissionTimer !== undefined) {
      this.#options.scheduler.clearTimeout(dial.admissionTimer);
      dial.admissionTimer = undefined;
    }
  }
}
