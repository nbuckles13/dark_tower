// File: packages/sdk-core/src/telemetry/closeReason.ts
//
// Bounded `close_reason` label for `dt_client_signaling_connection_total`
// (R-25). Per @observability Gate-1: a raw WebTransport / server close reason
// string is FREE-FORM → unbounded cardinality + a PII vector (servers can put
// arbitrary text there). So we NEVER label by the raw string. Instead we
// normalize a close code (and/or raw reason) into this small, fixed enum, and
// THAT is the only thing that becomes a label value.
//
// The enum is the cardinality contract (documented in
// docs/observability/metrics/client.md). The emission call sites land in tasks
// #13/#14; this story fixes the bounded value set + the normalization seam so
// the wire contract cannot drift later.

/**
 * Bounded `close_reason` label values. Adding a value is a deliberate,
 * reviewable cardinality change.
 */
export const CloseReason = {
  /** Clean close, no error (e.g. local hang-up, normal stream end). */
  Normal: 'normal',
  /** Server is going away / draining (e.g. MC migration, shutdown). */
  GoingAway: 'going_away',
  /** Connection rejected/closed for auth reasons (bad/expired join token). */
  AuthFailed: 'auth_failed',
  /** Closed because an operation/handshake timed out. */
  Timeout: 'timeout',
  /** Server-side error close (internal error class). */
  ServerError: 'server_error',
  /** Anything not recognized — the safe catch-all (never the raw string). */
  Unknown: 'unknown',
} as const;

export type CloseReason = (typeof CloseReason)[keyof typeof CloseReason];

/**
 * Normalize a transport/application close code into a bounded {@link CloseReason}.
 *
 * The argument is a NUMERIC close code (WebTransport / application close code),
 * NOT a free-form reason string — the raw string is deliberately never accepted
 * as a label source. Unknown / unmapped codes collapse to `'unknown'`.
 *
 * NOTE: the concrete code→reason mapping is intentionally minimal here; the
 * emission call sites (tasks #13/#14) own the full close-code taxonomy as the
 * close paths are wired. This helper fixes the BOUNDED OUTPUT so cardinality is
 * guaranteed regardless of how many raw codes exist.
 */
export function normalizeCloseReason(code: number | undefined): CloseReason {
  switch (code) {
    case 0:
      return CloseReason.Normal;
    case 1001:
      return CloseReason.GoingAway;
    case 3401:
      return CloseReason.AuthFailed;
    case 3408:
      return CloseReason.Timeout;
    case 1011:
      return CloseReason.ServerError;
    default:
      return CloseReason.Unknown;
  }
}
