// File: packages/web-app/e2e/receiveEvidence.ts
//
// PER-ENTITY receive evidence for the story-2 multi-party scenarios (R-30, R-31;
// the evidence rule in `crates/env-tests/README.md`): every claim about what ONE
// participant receives from ONE sender comes from that receiver's own E2E bus —
// never from a Prometheus series, which is a fleet aggregate (client series carry
// the collector as `instance`, and no participant label).
//
//   * layer 1 (`keyed`) and layer 2 (`verified`) per (observed slot, sender), from
//     the bus's sampled `receiveLayers`;
//   * layer 3 (the decoded tone), via `fixtures.ts:expectHearsSender` and the
//     pure detector;
//   * the send counter, from the sender's own `mediaFrameCounts`.
//
// "Flat" is never asserted alone: `observeWindow` samples EVERY probe on one
// cadence over one window, requires a sample-count floor (a stalled sampler is a
// HARNESS failure, never a pass), and a flat probe is always paired, in the same
// window, with one that must advance.

import { expect } from 'playwright/test';
import { E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS } from '../src/lib/e2eBus.js';
import {
  expectHearsSender,
  FLAT_WINDOW_OBSERVE_MS,
  FLAT_WINDOW_SETTLE_MS,
  latestBusEventsOf,
  latestReceiveView,
  MIN_FLAT_WINDOW_SAMPLES,
} from './fixtures.js';
import type { InstanceCounters } from './instanceCounters.js';
import type { JoinedMember } from './cohortContexts.js';
import { laneCarriesTone, type BusSlotAssignment, type CohortTone } from './toneDetector.js';

/** One (observed slot, sender) row of layers 1 and 2, as the bus projects it. */
export interface LayerRow {
  readonly slot: number | 'undeclared' | 'unparsed';
  readonly senderId: string;
  readonly keyed: number;
  readonly verified: number;
}

/** Liveness bound for layers 1/2 to appear once a slot is active. Not a latency gate. */
const LAYERS_TIMEOUT_MS = 30_000;

/** The receiver's latest cumulative layer rows (empty before media starts). */
export async function latestLayers(receiver: JoinedMember): Promise<readonly LayerRow[]> {
  const { receiveLayers } = await latestBusEventsOf(receiver.page, ['receiveLayers']);
  const layers = receiveLayers?.['layers'];
  return Array.isArray(layers) ? (layers as LayerRow[]) : [];
}

/** Frames from `sender` that verified and decrypted at this receiver, on any slot. */
export function acceptedFromRows(rows: readonly LayerRow[], senderId: string): number {
  return rows.filter((r) => r.senderId === senderId).reduce((sum, r) => sum + r.verified, 0);
}

/** The receiver's latest slot assignments. */
export async function latestAssignments(
  receiver: JoinedMember,
): Promise<readonly BusSlotAssignment[]> {
  return (await latestReceiveView(receiver.page)).assignments;
}

/**
 * Layers 1 and 2 for (receiver, sender): the sender holds exactly one ACTIVE slot
 * S in the receiver's latest assignment; at S the key id names the sender
 * (`keyed` > 0) and frames verify and decrypt under the sender's roster key
 * (`verified` > 0); and NO other observed slot carries a frame keyed to this
 * sender — a count under the wrong key is misrouting, however well it decrypts.
 * Nothing here assumes which handler carries the pair.
 */
export async function expectReceiveLayers(
  receiver: JoinedMember,
  sender: JoinedMember,
  timeoutMs = LAYERS_TIMEOUT_MS,
): Promise<void> {
  const who = `${receiver.label} receiving ${sender.label} (sender ${sender.tone.senderId})`;
  let last: string;
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const [assignments, rows] = await Promise.all([
      latestAssignments(receiver),
      latestLayers(receiver),
    ]);
    const slots = assignments.filter((a) => a.senderId === sender.tone.senderId);
    const mine = rows.filter((r) => r.senderId === sender.tone.senderId);
    if (slots.length === 1 && slots[0]!.slotState === 'active') {
      const slot = slots[0]!.slotId;
      const at = mine.find((r) => r.slot === slot);
      const elsewhere = mine.filter((r) => r.slot !== slot && r.keyed > 0);
      expect(
        elsewhere,
        `${who}: frames keyed to the sender arrived on a slot other than its assigned slot ${slot} ` +
          `— MISROUTING (layer 1): ${JSON.stringify(elsewhere)}`,
      ).toEqual([]);
      if (at !== undefined && at.keyed > 0 && at.verified > 0) return;
      last = `slot ${slot} active; layer row ${JSON.stringify(at ?? null)}`;
    } else {
      last = `assignment for the sender: ${JSON.stringify(slots)}`;
    }
    if (Date.now() > deadline) break;
    await receiver.page.waitForTimeout(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
  }
  throw new Error(`${who}: layers 1/2 not established within ${timeoutMs}ms — ${last}`);
}

/**
 * All three layers for (receiver, sender): layers 1/2 above, then the layer-3
 * two-sided tone check (with the S1 diagnostic on a missing sender).
 */
export async function expectHearsAtAllLayers(
  receiver: JoinedMember,
  sender: JoinedMember,
  cohort: readonly CohortTone[],
  admissionBaseline: InstanceCounters,
): Promise<void> {
  await expectHearsSender(receiver.page, receiver.tone, sender.tone, cohort, { admissionBaseline });
  await expectReceiveLayers(receiver, sender);
}

// ----------------------------------------------------------------------------
// Windows: flat / advance / zero, sampled together
// ----------------------------------------------------------------------------

/** What a probe must do across the window. */
export type ProbeExpectation = 'flat' | 'advance' | 'zero';

/** One sampled quantity. */
export interface Probe {
  readonly label: string;
  readonly expect: ProbeExpectation;
  read(): Promise<number>;
}

/** Frames from `sender` accepted at `receiver` (layer 2, summed over slots). */
export function acceptedProbe(
  receiver: JoinedMember,
  sender: JoinedMember,
  expectation: ProbeExpectation,
): Probe {
  return {
    label: `${receiver.label}.acceptedFrom(${sender.label})`,
    expect: expectation,
    read: async () => acceptedFromRows(await latestLayers(receiver), sender.tone.senderId),
  };
}

/** Frames keyed to `sender` that ARRIVED at `receiver` (layer 1, any slot, before verification). */
export function keyedProbe(
  receiver: JoinedMember,
  sender: JoinedMember,
  expectation: ProbeExpectation,
): Probe {
  return {
    label: `${receiver.label}.keyedFrom(${sender.label})`,
    expect: expectation,
    read: async () =>
      (await latestLayers(receiver))
        .filter((r) => r.senderId === sender.tone.senderId)
        .reduce((sum, r) => sum + r.keyed, 0),
  };
}

/** `member`'s own send counter (datagrams sent, all targets). */
export function sentProbe(member: JoinedMember, expectation: ProbeExpectation): Probe {
  return {
    label: `${member.label}.framesSent`,
    expect: expectation,
    read: async () => {
      const { mediaFrameCounts } = await latestBusEventsOf(member.page, ['mediaFrameCounts']);
      const sent = mediaFrameCounts?.['framesSent'];
      if (typeof sent !== 'number')
        throw new Error(`${member.label}: no mediaFrameCounts sample on the bus`);
      return sent;
    },
  };
}

// The window's settle, observation span and sample floor (the anti-vacuity
// control) have ONE home, shared with `expectCountersFlatOverWindow`:
// `FLAT_WINDOW_*` / `MIN_FLAT_WINDOW_SAMPLES` in `./fixtures`.

/**
 * Sample every probe on the bus cadence over ONE window (after `settleMs`) and
 * assert each probe's expectation. Requires at least one probe that must
 * `advance` — a window of only flat/zero probes proves nothing.
 */
export async function observeWindow(
  probes: readonly Probe[],
  why: string,
  { settleMs = FLAT_WINDOW_SETTLE_MS, observeMs = FLAT_WINDOW_OBSERVE_MS } = {},
): Promise<void> {
  if (!probes.some((p) => p.expect === 'advance')) {
    throw new Error(`observeWindow(${why}): no probe must advance — the window would be vacuous`);
  }
  if (settleMs > 0) await new Promise((r) => setTimeout(r, settleMs));
  const samples: number[][] = probes.map(() => []);
  const end = Date.now() + observeMs;
  for (;;) {
    const values = await Promise.all(probes.map((p) => p.read()));
    values.forEach((v, i) => samples[i]!.push(v));
    if (Date.now() >= end) break;
    await new Promise((r) => setTimeout(r, E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS));
  }
  const summary = probes
    .map((p, i) => `${p.label}[${p.expect}]=${JSON.stringify(samples[i])}`)
    .join('; ');
  probes.forEach((probe, i) => {
    const series = samples[i]!;
    expect(
      series.length,
      `HARNESS: ${series.length} sample(s) for ${probe.label} (need >= ${MIN_FLAT_WINDOW_SAMPLES}); ` +
        `flatness over too few samples is vacuous. ${summary}`,
    ).toBeGreaterThanOrEqual(MIN_FLAT_WINDOW_SAMPLES);
    const first = series[0]!;
    const lastValue = series[series.length - 1]!;
    if (probe.expect === 'flat') {
      expect(
        series.every((v) => v === first),
        `${why}: ${probe.label} must be FLAT. ${summary}`,
      ).toBe(true);
    } else if (probe.expect === 'zero') {
      expect(
        series.every((v) => v === 0),
        `${why}: ${probe.label} must stay ZERO. ${summary}`,
      ).toBe(true);
    } else {
      expect(lastValue > first, `${why}: ${probe.label} must ADVANCE. ${summary}`).toBe(true);
    }
  });
}

// ----------------------------------------------------------------------------
// Wire-token waits (slot and roster)
// ----------------------------------------------------------------------------

/** Poll until the receiver's slot carrying `sender` reads `state` (wire token). */
export async function waitForSenderSlotState(
  receiver: JoinedMember,
  sender: JoinedMember,
  state: string,
  timeoutMs = 20_000,
): Promise<void> {
  await expect
    .poll(
      async () =>
        (await latestAssignments(receiver)).find((a) => a.senderId === sender.tone.senderId)
          ?.slotState ?? '(no slot)',
      {
        message: `${receiver.label}: the slot carrying ${sender.label} must read '${state}'`,
        timeout: timeoutMs,
      },
    )
    .toBe(state);
}

/** Poll until `attribute` on the receiver's roster row for `subject` reads `value`. */
export async function waitForRosterToken(
  receiver: JoinedMember,
  subject: JoinedMember,
  attribute: 'data-reachability' | 'data-server-muted' | 'data-client-muted',
  value: string,
  timeoutMs = 20_000,
): Promise<void> {
  await expect(
    receiver.page.getByTestId(`participant-${subject.joined.participantId}`),
    `${receiver.label}: roster row for ${subject.label} must carry ${attribute}=${value}`,
  ).toHaveAttribute(attribute, value, { timeout: timeoutMs });
}

/**
 * Poll until `sender`'s tone is ABSENT from the receiver's lane for it: no lane,
 * a silent lane, or the tone not present above the floor. Distinct from the
 * slot-state check — this is the decoded audio itself.
 */
export async function expectToneAbsent(
  receiver: JoinedMember,
  sender: JoinedMember,
  timeoutMs = 20_000,
): Promise<void> {
  await expect
    .poll(
      async () => {
        const { lanes } = await latestReceiveView(receiver.page);
        const lane = lanes.find((l) => l.senderId === sender.tone.senderId);
        // No lane at all, or a lane that does not carry the tone by the ONE
        // presence rule the hearing verdict also uses (`laneCarriesTone`).
        return lane !== undefined && laneCarriesTone(lane, sender.tone.toneHz)
          ? 'present'
          : 'absent';
      },
      {
        message: `${receiver.label}: ${sender.label}'s tone must be ABSENT from its lane`,
        timeout: timeoutMs,
      },
    )
    .toBe('absent');
}
