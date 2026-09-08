// File: packages/sdk-core/src/media/events.ts
//
// R-20/R-21: the PUBLIC, plain-typed surface of the media-transport layer. Like
// `signaling/events.ts`, these types are deliberately decoupled from the generated
// protobuf-es types (gitignored, not part of the published API). `MediaTransport`
// maps wire types onto these plain shapes at the boundary.
//
// The per-MH OUTPUT report shape (`MhConnectionStatusReport`) lives one layer down
// in `signaling/events.ts` (it is the input to `SignalingClient.sendMediaConnectionUpdate`),
// so the dependency edge runs media→signaling — see that file.

import type { MetricLabels, MetricsSink } from '../telemetry/MetricsSink.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';
import type { MediaConnectionError } from '../errors/MediaConnectionError.js';

/** Observable per-MH connection state (R-20: "per-MH state must be observable"). */
export type MediaConnectionState = 'connecting' | 'connected' | 'failed';

/** The typed events {@link MediaTransport} emits. */
export interface MediaTransportEventMap {
  /** Fired per MH whose handshake succeeded (payload: the MH URL). */
  connected: string;
  /** Fired per MH whose handshake failed (payload: the typed error). */
  failed: MediaConnectionError;
}

/**
 * Run a synchronous callback inside a specific OTel context (R-58). `MeetingSession`
 * passes `SignalingClient.runInJoinContext` so the MH connect-envelope inject runs
 * under the active `dt_client.join` span; the default is identity (no telemetry).
 */
export type RunInContext = <T>(fn: () => T) => T;

/** Construction options for {@link MediaTransport}. */
export interface MediaTransportOptions {
  /** Transport factory. Defaults to the production `connect`. Tests inject a mock factory. */
  readonly connect?: WebTransportConnectFn;
  /** Wall-clock source for `observedAt` (default `Date.now`). Injected for deterministic tests. */
  readonly clock?: () => number;
  /** Per-MH connect deadline in ms (default {@link DEFAULT_MH_CONNECT_TIMEOUT_MS}). */
  readonly connectTimeoutMs?: number;
  /** Metrics sink (default `getMetricsSink()`; no-op when telemetry unconfigured). */
  readonly metricsSink?: MetricsSink;
  /**
   * Join label set threaded from `MeetingSession` (`client_version` /
   * `meeting_id_hash` / `org_id`). Consumed by ONE grandfathered metric,
   * `dt_client_mh_connection_total` — see `MediaTransport.#emitMetric`.
   *
   * **NOT the media-path label set.** Media metrics are built by allow-list in
   * `setup/mediaMetrics.ts` from two named strings and carry no meeting
   * dimension (ADR-0036 §11; `docs/observability/label-taxonomy.md` R3, which
   * cites THIS FILE by path as the inertia source — an unqualified comment here
   * is what makes the violation arrive by default).
   */
  readonly metricLabels?: MetricLabels;
  /** Context runner for R-58 trace injection (default identity). */
  readonly runInContext?: RunInContext;
  /**
   * Datagram queue depths to set on each connected transport.
   *
   * Omitted for a connect-only transport (the join flow before task 19's media
   * pipeline starts); supplied by the media pipeline from `clientConfig.ts`.
   */
  readonly datagramQueue?: DatagramQueueSettings;
}

/**
 * The `WebTransportDatagramDuplexStream` queue depths this SDK CHOOSES.
 *
 * ADR-0036 §1 requires these be declared rather than inherited ("the default
 * being adequate is not the same as the default being chosen"), and §11 requires
 * the transport queue be kept shallow so the bounded application queue above it
 * makes — and counts — the drop decision. A drop inside the user agent's queue
 * is uncountable by this client AND structurally invisible to the media handler.
 *
 * Values come from `config/clientConfig.ts`; `validateMediaConfig` asserts that
 * the application bound exceeds `outgoingHighWaterMark` and that
 * `outgoingMaxAgeMs` sits clear of what both queues can legitimately hold.
 */
export interface DatagramQueueSettings {
  /** Outgoing depth, in datagrams (for audio: frames). */
  readonly outgoingHighWaterMark: number;
  /** Age after which the user agent silently discards a queued datagram, in ms. */
  readonly outgoingMaxAgeMs: number;
  /** Incoming depth, in datagrams. */
  readonly incomingHighWaterMark: number;
}

/**
 * A datagram send/receive channel over one connected media handler.
 *
 * `send` awaits the writer's `ready` before every write, so at most one write is
 * in flight in the transport and queue depth accumulates in the application
 * queue, where it is bounded and counted.
 */
export interface MediaDatagramChannel {
  /** Inbound datagrams. The ingress read loop owns this. */
  readonly readable: ReadableStream<Uint8Array>;
  /** False once the transport has closed, however it closed. */
  readonly isOpen: boolean;
  /**
   * The platform's maximum datagram size, when it reports one.
   *
   * `undefined` under a test double or a user agent that does not expose it, in
   * which case the sender skips the oversize check rather than inventing a bound
   * — a fabricated limit would produce drops with no basis.
   */
  readonly maxDatagramSize: number | undefined;
  /** Send one datagram. Rejects if the transport refuses it. */
  send(bytes: Uint8Array): Promise<void>;
}

/** Default per-MH connect deadline (ms). Connect-only, so shorter than the 15s MC join. */
export const DEFAULT_MH_CONNECT_TIMEOUT_MS = 10_000;
