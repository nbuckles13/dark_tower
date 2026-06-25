// File: packages/sdk-core/src/telemetry/logger.ts
//
// R-26: structured client-side logger for the join flow with a BOUNDED `event`
// enum. Console-only this story (the GC `/api/v1/telemetry` log signal is a
// later task). The bounded enum + a fixed record shape are the PII boundary:
// the type system only permits the allowlisted fields, and the runtime emitter
// only copies those fields, so an excluded value (email, password, raw meeting
// id, JWT, MLS key, user-agent above DEBUG) cannot reach the log channel even
// if a caller passes a wider object.
//
// PII discipline (R-23 / R-26):
//   - `meeting_id_hash` is a SHA-256-truncated digest, NEVER the raw meeting id
//     (hashing is the caller's job; this module type-forbids a `meeting_id`
//     field so the raw id has no slot to occupy).
//   - EXCLUDED entirely: email, password, raw meeting id, JWTs, MLS keys,
//     user-agent (above DEBUG). These are not fields on `JoinLogRecord`, and
//     the emitter builds the output object from a fixed allowlist — extra
//     enumerable properties on the input are dropped.

/**
 * Bounded set of join-flow log events (R-26). Adding a value here is a
 * deliberate, reviewable change — the enum is the cardinality + taxonomy
 * boundary for the log channel.
 */
export const JoinEvent = {
  Started: 'join.started',
  SignupComplete: 'join.signup_complete',
  GcTokenReceived: 'join.gc_token_received',
  SignalingReady: 'join.signaling_ready',
  MhConnected: 'join.mh_connected',
  Failed: 'join.failed',
  Completed: 'join.completed',
} as const;

export type JoinEvent = (typeof JoinEvent)[keyof typeof JoinEvent];

/**
 * The fixed, PII-safe shape of a join-flow log record (R-26). These are the
 * ONLY fields that may be logged. `duration_ms` and `failure_stage` are
 * optional because not every event carries them (`join.started` has no
 * duration; non-failure events have no `failure_stage`).
 *
 * EXCLUDED by construction (no field exists for them): email, password, raw
 * meeting id, JWTs, MLS keys, user-agent.
 */
export interface JoinLogRecord {
  /** SHA-256-truncated meeting id digest — NEVER the raw meeting id. */
  readonly meeting_id_hash: string;
  /** Build-time SDK version (`__SDK_VERSION__`). */
  readonly client_version: string;
  /** W3C trace id of the `dt_client.join` root span, for log↔trace correlation. */
  readonly trace_id: string;
  /** Bounded event discriminator. */
  readonly event: JoinEvent;
  /** Wall-clock ms since `join.started`, when applicable. */
  readonly duration_ms?: number;
  /**
   * Failure stage on `join.failed` (mirrors the R-25
   * `dt_client_join_attempts_total{failure_stage}` value set), when applicable.
   */
  readonly failure_stage?: string;
}

/**
 * Emit a structured join-flow log line. Console-only this story. Builds the
 * output object from the FIXED allowlist below — any extra enumerable property
 * on `record` (a wider object a caller might pass) is dropped, so PII cannot
 * leak through a structurally-wider argument.
 */
export function logJoinEvent(record: JoinLogRecord): void {
  // Fixed allowlist projection — the PII boundary. Optional fields are included
  // only when present so the log line stays compact and `exactOptionalProperty`
  // semantics hold.
  const safe: Record<string, string | number> = {
    meeting_id_hash: record.meeting_id_hash,
    client_version: record.client_version,
    trace_id: record.trace_id,
    event: record.event,
  };
  if (record.duration_ms !== undefined) {
    safe['duration_ms'] = record.duration_ms;
  }
  if (record.failure_stage !== undefined) {
    safe['failure_stage'] = record.failure_stage;
  }
  console.info('[dt-join]', safe);
}
