// File: packages/sdk-core/src/media/__tests__/transportTelemetry.test.ts
//
// R-42 connection telemetry (story 4 C2; telemetry-contract.md §Client), on the
// pure tracker (`transportTelemetry.ts`) with a hand-driven `FakeScheduler` —
// no transport, no sleeps — plus the ONE close-code projection
// (`closeCode.ts:closeCodeToken`). Every negative has its positive arm on the
// same fixture.

import { describe, expect, it } from 'vitest';
import { FakeScheduler, InMemoryMetricsSink } from '@darktower/test-utils';

import { closeCodeToken } from '../closeCode.js';
import { TransportTelemetry, type RefTelemetry } from '../transportTelemetry.js';
import {
  MH_CLOSE_CODE_TOKENS,
  MH_DIAL_OUTCOMES,
  MH_DIAL_PHASES,
  MH_DISCONNECT_CAUSES,
  MH_NON_MEMBER_SOURCES,
  MH_DIAL_DURATION_BUCKETS_MS,
  MH_DISRUPTION_BUCKETS_MS,
  MediaSessionMetrics,
  dialDurationOutcome,
  isHandlerLoss,
} from '../setup/mediaMetrics.js';
import { DEFAULT_MH_CONNECT_TIMEOUT_MS } from '../events.js';
import { HANDLER_SET_CONFIG_RANGES } from '../../config/clientConfig.js';
import { MhCloseCode } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';

const WINDOW = 2_000;
const ABANDON = 60_000;
const SESSION_KEYS = ['client_version', 'key_custody', 'org_id'];

function rig() {
  const sched = new FakeScheduler();
  const sink = new InMemoryMetricsSink();
  const firsts: number[] = [];
  const tt = new TransportTelemetry({
    metrics: new MediaSessionMetrics({ clientVersion: 'v', orgId: 'o' }, sink),
    clock: sched.clock,
    scheduler: sched,
    admissionWindowMs: WINDOW,
    reconnectAbandonMs: ABANDON,
    onFirstEstablished: (at) => firsts.push(at),
  });
  const count = (name: string, labels: Record<string, string> = {}): number =>
    sink
      .getRecordedMetrics()
      .filter(
        (m) =>
          m.name === name &&
          m.kind === 'counter' &&
          Object.entries(labels).every(([k, v]) => m.labels[k] === v),
      )
      .reduce((n, m) => n + m.value, 0);
  const observations = (name: string, labels: Record<string, string> = {}): number[] =>
    sink
      .getRecordedMetrics()
      .filter(
        (m) =>
          m.name === name &&
          m.kind === 'histogram' &&
          Object.entries(labels).every(([k, v]) => m.labels[k] === v),
      )
      .map((m) => m.value);
  return { sched, sink, tt, firsts, count, observations };
}

/** A dial that handshook and survived the window: established. */
function establish(
  r: ReturnType<typeof rig>,
  rt: RefTelemetry,
  phase: 'join' | 'added' | 'redial' = 'join',
): void {
  r.tt.dialStarted(rt, phase);
  r.sched.advance(100);
  r.tt.handshake(rt);
  r.sched.advance(WINDOW);
}

// ---------------------------------------------------------------------------
// The ONE close-code projection
// ---------------------------------------------------------------------------

/** Every generated `MhCloseCode` numeric value, read from the enum (never hand-listed). */
const GENERATED_CODES = Object.values(MhCloseCode).filter(
  (v): v is number => typeof v === 'number',
);

describe('closeCodeToken — total over MhCloseCode, 0/unknown = contract_fault', () => {
  it('reads a non-trivial enum (extraction control)', () => {
    expect(GENERATED_CODES.length).toBeGreaterThanOrEqual(6);
  });

  it.each(GENERATED_CODES.filter((c) => c !== MhCloseCode.UNSPECIFIED))(
    'code %i maps to its proto NAME, prefix stripped, lowercased',
    (code) => {
      // Independent derivation: the production switch is written out; this
      // reads the generated name.
      const name = (MhCloseCode as unknown as Record<number, string>)[code]!;
      expect(closeCodeToken(code)).toBe(name.toLowerCase());
    },
  );

  it('UNSPECIFIED (0) and unknown codes are contract_fault', () => {
    expect(closeCodeToken(MhCloseCode.UNSPECIFIED)).toBe('contract_fault');
    expect(closeCodeToken(0)).toBe('contract_fault');
    expect(closeCodeToken(999)).toBe('contract_fault');
    expect(closeCodeToken(-1)).toBe('contract_fault');
  });

  it('every projected token is in BOTH vocabularies (dial outcome and disconnect cause)', () => {
    for (const code of [...GENERATED_CODES, 999]) {
      const token = closeCodeToken(code);
      expect(MH_CLOSE_CODE_TOKENS).toContain(token);
      expect(MH_DIAL_OUTCOMES).toContain(token);
      expect(MH_DISCONNECT_CAUSES).toContain(token);
    }
  });

  it('the non-UNSPECIFIED codes map INJECTIVELY (no two codes collapse silently)', () => {
    const named = GENERATED_CODES.filter((c) => c !== MhCloseCode.UNSPECIFIED);
    expect(new Set(named.map(closeCodeToken)).size).toBe(named.length);
    expect(named.map(closeCodeToken)).not.toContain('contract_fault');
  });

  it('the lost-handler predicate is exactly transport_loss | handler_released', () => {
    expect(MH_DISCONNECT_CAUSES.filter(isHandlerLoss)).toEqual([
      'transport_loss',
      'handler_released',
    ]);
  });

  it('dial-duration projection: connected stays connected; every other outcome is failed', () => {
    for (const o of MH_DIAL_OUTCOMES) {
      expect(dialDurationOutcome(o)).toBe(o === 'connected' ? 'connected' : 'failed');
    }
  });
});

// ---------------------------------------------------------------------------
// Buckets
// ---------------------------------------------------------------------------

describe('bucket SSoTs reach past what they measure', () => {
  it("the disruption buckets top out at >= both abandonment caps' range MAX (read from the ranges)", () => {
    const top = MH_DISRUPTION_BUCKETS_MS.at(-1)!;
    expect(top).toBeGreaterThanOrEqual(HANDLER_SET_CONFIG_RANGES.reconnectAbandonMs[1]);
    expect(top).toBeGreaterThanOrEqual(HANDLER_SET_CONFIG_RANGES.resumeAbandonMs[1]);
  });

  it('the dial-duration buckets reach past connect deadline + admission window max (read, not restated)', () => {
    expect(MH_DIAL_DURATION_BUCKETS_MS.at(-1)!).toBeGreaterThanOrEqual(
      DEFAULT_MH_CONNECT_TIMEOUT_MS + HANDLER_SET_CONFIG_RANGES.admissionWindowMs[1],
    );
  });
});

// ---------------------------------------------------------------------------
// Zero-initialisation and ADR-0032 semantics
// ---------------------------------------------------------------------------

describe('zero-initialisation (ADR-0032)', () => {
  it('zero-inits every counter series of the family, at 0, with exact label sets', () => {
    const sink = new InMemoryMetricsSink();
    new MediaSessionMetrics({ clientVersion: 'v', orgId: 'o' }, sink).zeroInitMhConnections();
    const recorded = sink.getRecordedMetrics();
    expect(recorded.every((m) => m.value === 0 && m.kind === 'counter')).toBe(true);
    const conn = recorded.filter((m) => m.name === 'dt_client_mh_connection_total');
    expect(conn).toHaveLength(MH_DIAL_PHASES.length * MH_DIAL_OUTCOMES.length);
    const disc = recorded.filter((m) => m.name === 'dt_client_mh_disconnects_total');
    expect(disc.map((m) => m.labels['cause']).sort()).toEqual([...MH_DISCONNECT_CAUSES].sort());
    const nonMember = recorded.filter((m) => m.name === 'dt_client_mh_non_member_refs_total');
    expect(nonMember.map((m) => m.labels['source']).sort()).toEqual(
      [...MH_NON_MEMBER_SOURCES].sort(),
    );
    for (const m of conn) {
      expect(Object.keys(m.labels).sort()).toEqual([...SESSION_KEYS, 'outcome', 'phase'].sort());
    }
    // Histograms are never zero-initialised: unobserved = absent (ADR-0032).
    expect(recorded.some((m) => m.kind === 'histogram')).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// One outcome per dial; the admission window
// ---------------------------------------------------------------------------

describe('exactly one connection_total outcome per dial', () => {
  it.each(MH_DIAL_PHASES)('phase=%s: a dial that survives the window is ONE connected', (phase) => {
    const r = rig();
    const rt = r.tt.track();
    r.tt.dialStarted(rt, phase);
    r.sched.advance(300);
    r.tt.handshake(rt);
    // Provisional: nothing yet (negative)...
    r.sched.advance(WINDOW - 1);
    expect(r.count('dt_client_mh_connection_total')).toBe(0);
    // ...at the window: ONE connected (positive).
    r.sched.advance(1);
    expect(r.count('dt_client_mh_connection_total', { phase, outcome: 'connected' })).toBe(1);
    expect(r.count('dt_client_mh_connection_total')).toBe(1);
    // Duration measured to the HANDSHAKE, not the window end.
    expect(
      r.observations('dt_client_mh_dial_duration_ms', { phase, outcome: 'connected' }),
    ).toEqual([300]);
  });

  it('timeout and transport_failure before the handshake are one failed outcome each', () => {
    const r = rig();
    const a = r.tt.track();
    r.tt.dialStarted(a, 'join');
    r.sched.advance(10_000);
    r.tt.dialFailed(a, 'timeout');
    const b = r.tt.track();
    r.tt.dialStarted(b, 'added');
    r.sched.advance(40);
    r.tt.dialFailed(b, 'transport_failure');
    expect(r.count('dt_client_mh_connection_total', { phase: 'join', outcome: 'timeout' })).toBe(1);
    expect(
      r.count('dt_client_mh_connection_total', { phase: 'added', outcome: 'transport_failure' }),
    ).toBe(1);
    expect(r.observations('dt_client_mh_dial_duration_ms', { outcome: 'failed' })).toEqual([
      10_000, 40,
    ]);
    // A failed dial is NEVER a disconnect.
    expect(r.count('dt_client_mh_disconnects_total')).toBe(0);
  });

  it('(a) an MH close at window-1 is the close-code outcome: 0 disconnects, no reconnect episode', () => {
    const r = rig();
    const rt = r.tt.track();
    r.tt.dialStarted(rt, 'redial');
    r.tt.handshake(rt);
    r.sched.advance(WINDOW - 1);
    expect(r.tt.closed(rt, MhCloseCode.WRONG_HANDLER)).toBeUndefined();
    expect(
      r.count('dt_client_mh_connection_total', { phase: 'redial', outcome: 'wrong_handler' }),
    ).toBe(1);
    expect(r.count('dt_client_mh_connection_total', { outcome: 'connected' })).toBe(0);
    expect(r.count('dt_client_mh_disconnects_total')).toBe(0);
    r.sched.advance(ABANDON * 2);
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toEqual([]);
    expect(r.observations('dt_client_mh_dial_duration_ms', { outcome: 'failed' })).toEqual([
      WINDOW - 1,
    ]);
  });

  it('(b) at the window it is connected; a later close is exactly 1 disconnect and NO second outcome', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    expect(r.tt.closed(rt, MhCloseCode.WRONG_HANDLER)).toBe('wrong_handler');
    expect(r.count('dt_client_mh_connection_total')).toBe(1);
    expect(r.count('dt_client_mh_disconnects_total', { cause: 'wrong_handler' })).toBe(1);
    expect(r.count('dt_client_mh_disconnects_total')).toBe(1);
  });

  it('(c) a no-code loss inside the window is transport_failure: 0 disconnects, no episode', () => {
    const r = rig();
    const rt = r.tt.track();
    r.tt.dialStarted(rt, 'join');
    r.tt.handshake(rt);
    r.tt.closed(rt, undefined);
    expect(r.count('dt_client_mh_connection_total', { outcome: 'transport_failure' })).toBe(1);
    expect(r.count('dt_client_mh_disconnects_total')).toBe(0);
    r.sched.advance(ABANDON * 2);
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toEqual([]);
  });

  it('a 0 / unknown code inside the window is contract_fault (and no disconnect)', () => {
    const r = rig();
    for (const code of [0, 999]) {
      const rt = r.tt.track();
      r.tt.dialStarted(rt, 'join');
      r.tt.handshake(rt);
      r.tt.closed(rt, code);
    }
    expect(r.count('dt_client_mh_connection_total', { outcome: 'contract_fault' })).toBe(2);
    expect(r.count('dt_client_mh_disconnects_total')).toBe(0);
  });

  it('(d) a directed removal inside the window (or mid-dial) is aborted_by_removal, no disconnect', () => {
    const r = rig();
    const provisional = r.tt.track();
    r.tt.dialStarted(provisional, 'added');
    r.tt.handshake(provisional);
    r.tt.removed(provisional);
    const midDial = r.tt.track();
    r.tt.dialStarted(midDial, 'added');
    r.tt.removed(midDial);
    expect(r.count('dt_client_mh_connection_total', { outcome: 'aborted_by_removal' })).toBe(2);
    expect(r.count('dt_client_mh_disconnects_total')).toBe(0);
    // Inert after removal: the window passing records nothing more.
    r.sched.advance(WINDOW * 2);
    expect(r.count('dt_client_mh_connection_total')).toBe(2);
  });

  it('(e) local teardown inside the window records NOTHING (unobserved), established or not', () => {
    const r = rig();
    const established = r.tt.track();
    establish(r, established);
    const provisional = r.tt.track();
    r.tt.dialStarted(provisional, 'added');
    r.tt.handshake(provisional);
    const before = r.sink.getRecordedMetrics().length;
    r.tt.teardown(provisional);
    r.tt.teardown(established);
    r.sched.advance(WINDOW * 5);
    // Positive arm: the established dial DID record its outcome before teardown.
    expect(r.count('dt_client_mh_connection_total', { outcome: 'connected' })).toBe(1);
    expect(r.sink.getRecordedMetrics()).toHaveLength(before);
  });

  it('(f) time_to_first fires at the FIRST finalised connect, valued to its handshake, exactly once', () => {
    const r = rig();
    // A refused first dial produces no observation.
    const refused = r.tt.track();
    r.tt.dialStarted(refused, 'join');
    r.sched.advance(50);
    r.tt.handshake(refused);
    r.tt.closed(refused, MhCloseCode.DECLINED);
    expect(r.firsts).toEqual([]);
    const ok = r.tt.track();
    r.tt.dialStarted(ok, 'join');
    r.sched.advance(70);
    const handshakeAt = r.sched.clock();
    r.tt.handshake(ok);
    r.sched.advance(WINDOW);
    expect(r.firsts).toEqual([handshakeAt]);
    const again = r.tt.track();
    establish(r, again, 'added');
    expect(r.firsts).toEqual([handshakeAt]);
  });
});

// ---------------------------------------------------------------------------
// Disconnects and the per-ref disruption timer
// ---------------------------------------------------------------------------

describe('disconnects_total and reconnect_duration_ms', () => {
  it('a directed removal of an ESTABLISHED session is directed_removal; a later server close adds nothing', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    r.tt.removed(rt);
    // The server's HANDLER_RELEASED after the removal is not a second event.
    expect(r.tt.closed(rt, MhCloseCode.HANDLER_RELEASED)).toBeUndefined();
    expect(r.count('dt_client_mh_disconnects_total', { cause: 'directed_removal' })).toBe(1);
    expect(r.count('dt_client_mh_disconnects_total')).toBe(1);
  });

  it('reconnected: loss → redial finalises connected; valued to the redial HANDSHAKE', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    expect(r.tt.closed(rt, undefined)).toBe('transport_loss');
    r.sched.advance(3_000);
    r.tt.dialStarted(rt, 'redial');
    r.sched.advance(400);
    r.tt.handshake(rt);
    // Not yet: the redial is provisional (a refused redial must not end it).
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toEqual([]);
    r.sched.advance(WINDOW);
    expect(
      r.observations('dt_client_mh_reconnect_duration_ms', { outcome: 'reconnected' }),
    ).toEqual([3_400]);
  });

  it('a redial REFUSED in the window does not end the episode; the next connected one does', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    r.tt.closed(rt, MhCloseCode.HANDLER_RELEASED);
    r.tt.dialStarted(rt, 'redial');
    r.tt.handshake(rt);
    r.tt.closed(rt, MhCloseCode.MC_UNAVAILABLE);
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toEqual([]);
    r.sched.advance(1_000);
    r.tt.dialStarted(rt, 'redial');
    r.tt.handshake(rt);
    r.sched.advance(WINDOW);
    expect(
      r.observations('dt_client_mh_reconnect_duration_ms', { outcome: 'reconnected' }),
    ).toEqual([1_000]);
  });

  it('removed: the lost ref leaves the set', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    r.tt.closed(rt, undefined);
    r.sched.advance(5_000);
    r.tt.removed(rt);
    expect(r.observations('dt_client_mh_reconnect_duration_ms', { outcome: 'removed' })).toEqual([
      5_000,
    ]);
  });

  it('abandoned at the cap: cap-1 open, cap observed (value = cap)', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    r.tt.closed(rt, undefined);
    r.sched.advance(ABANDON - 1);
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toEqual([]);
    r.sched.advance(1);
    expect(r.observations('dt_client_mh_reconnect_duration_ms', { outcome: 'abandoned' })).toEqual([
      ABANDON,
    ]);
    // Closed: a later reconnect observes nothing more.
    establish(r, rt, 'redial');
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toHaveLength(1);
  });

  it('abandoned at session end (teardown)', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    r.tt.closed(rt, MhCloseCode.HANDLER_RELEASED);
    r.sched.advance(700);
    r.tt.teardown(rt);
    expect(r.observations('dt_client_mh_reconnect_duration_ms', { outcome: 'abandoned' })).toEqual([
      700,
    ]);
  });

  it('a non-loss disconnect (retry code, declined, contract fault) starts NO episode; a loss does', () => {
    const r = rig();
    for (const code of [MhCloseCode.BINDING_CONFLICT, MhCloseCode.DECLINED, 0]) {
      const rt = r.tt.track();
      establish(r, rt);
      r.tt.closed(rt, code);
      r.tt.removed(rt);
    }
    expect(r.observations('dt_client_mh_reconnect_duration_ms')).toEqual([]);
    const lost = r.tt.track();
    establish(r, lost);
    r.tt.closed(lost, undefined);
    r.tt.removed(lost);
    expect(
      r.observations('dt_client_mh_reconnect_duration_ms', { outcome: 'removed' }),
    ).toHaveLength(1);
  });

  it('every disconnect cause and reconnect outcome carries exactly the session bag + its key', () => {
    const r = rig();
    const rt = r.tt.track();
    establish(r, rt);
    r.tt.closed(rt, undefined);
    r.tt.removed(rt);
    for (const m of r.sink.getRecordedMetrics()) {
      const extra =
        m.name === 'dt_client_mh_disconnects_total'
          ? ['cause']
          : m.name === 'dt_client_mh_reconnect_duration_ms'
            ? ['outcome']
            : ['outcome', 'phase'];
      expect(Object.keys(m.labels).sort()).toEqual([...SESSION_KEYS, ...extra].sort());
    }
  });
});
