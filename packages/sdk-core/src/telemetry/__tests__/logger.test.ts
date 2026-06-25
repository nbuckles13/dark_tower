// File: packages/sdk-core/src/telemetry/__tests__/logger.test.ts
//
// R-26: the bounded-event join logger. Verifies the allowlisted fields are
// emitted, optional fields are included only when present, and (the PII
// boundary) that an extra enumerable property on a structurally-wider argument
// is DROPPED — never logged.

import { afterEach, describe, expect, it, vi } from 'vitest';

import { JoinEvent, logJoinEvent, type JoinLogRecord } from '../logger.js';

const BASE: JoinLogRecord = {
  meeting_id_hash: 'abc123truncatedsha256',
  client_version: '0.0.0-test',
  trace_id: '0af7651916cd43dd8448eb211c80319c',
  event: JoinEvent.Started,
};

describe('logJoinEvent (R-26)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('emits the four required fields', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    logJoinEvent(BASE);
    expect(info).toHaveBeenCalledTimes(1);
    const payload = info.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(payload).toEqual({
      meeting_id_hash: 'abc123truncatedsha256',
      client_version: '0.0.0-test',
      trace_id: '0af7651916cd43dd8448eb211c80319c',
      event: 'join.started',
    });
  });

  it('includes duration_ms and failure_stage only when present', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    logJoinEvent({
      ...BASE,
      event: JoinEvent.Failed,
      duration_ms: 1234,
      failure_stage: 'mc_signaling_connect',
    });
    const payload = info.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(payload['duration_ms']).toBe(1234);
    expect(payload['failure_stage']).toBe('mc_signaling_connect');
  });

  it('drops an extra (PII-shaped) enumerable property — the allowlist boundary', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    // A caller passes a structurally-wider object carrying PII. The logger
    // projects only the allowlisted fields, so the PII never reaches the log.
    const wider = {
      ...BASE,
      email: 'attacker@example.test',
      jwt: 'eyJhbGciOi.secret.token',
      meeting_id: 'RAW-MEETING-CODE',
    } as unknown as JoinLogRecord;
    logJoinEvent(wider);
    const payload = info.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(payload).not.toHaveProperty('email');
    expect(payload).not.toHaveProperty('jwt');
    expect(payload).not.toHaveProperty('meeting_id');
    expect(Object.keys(payload).sort()).toEqual([
      'client_version',
      'event',
      'meeting_id_hash',
      'trace_id',
    ]);
  });

  it('exposes the full bounded event enum', () => {
    expect(Object.values(JoinEvent).sort()).toEqual(
      [
        'join.started',
        'join.signup_complete',
        'join.gc_token_received',
        'join.signaling_ready',
        'join.mh_connected',
        'join.failed',
        'join.completed',
      ].sort(),
    );
  });
});
