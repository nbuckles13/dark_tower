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
  /** Join label set threaded from MeetingSession (`client_version`/`meeting_id_hash`/`org_id`). */
  readonly metricLabels?: MetricLabels;
  /** Context runner for R-58 trace injection (default identity). */
  readonly runInContext?: RunInContext;
}

/** Default per-MH connect deadline (ms). Connect-only, so shorter than the 15s MC join. */
export const DEFAULT_MH_CONNECT_TIMEOUT_MS = 10_000;
