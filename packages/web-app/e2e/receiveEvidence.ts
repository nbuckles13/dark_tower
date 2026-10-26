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
// window, with one that must advance ON THE SAME PAGE — each read returns that
// page's latest bus snapshot, so a page whose in-page sampler stalled would repeat
// a stale value; its own mover is what fails in that case.

import { expect } from 'playwright/test';
import { E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS } from '../src/lib/e2eBus.js';
import {
  expectHearsSender,
  FLAT_WINDOW_MAX_OBSERVE_FACTOR,
  FLAT_WINDOW_OBSERVE_MS,
  FLAT_WINDOW_SETTLE_MS,
  latestBusEventsOf,
  latestReceiveView,
  MIN_FLAT_WINDOW_SAMPLES,
  pollBus,
  VIDEO_SLOT_STATE_LIVENESS_MS,
} from './fixtures.js';
import type { InstanceCounters } from './instanceCounters.js';
import type { JoinedMember, Member } from './cohortContexts.js';
import type { JoinedBusEvent, MuteKind, ROSTER_CLIENT_STATE_ATTRIBUTE } from './fixtures.js';

/** What the video helpers need of a member: its label and page. */
export type PageMember = Pick<Member, 'label' | 'page'>;
/** What the video helpers need of an occupant: its label and its OWN join binding. */
export type BoundMember = { readonly label: string; readonly joined: JoinedBusEvent };
import { laneCarriesTone, type BusSlotAssignment, type CohortTone } from './toneDetector.js';
import { sampleWindow, unpairedObservers } from './windowSampling.js';
import type { SlotExpectation } from './slotOracle.js';
import {
  parseSendTracks,
  parseVideoCaptureSource,
  parseReceiveLayers,
  parseSendCounts,
  parseVideoSlots,
  type VideoSlotsEvent,
} from './videoBusShape.js';
import {
  assertVideoSlots,
  judgePatternAnnouncement,
  judgeSendTracks,
  judgeVideoSlot,
  type SendSideExpectation,
  type LayerRow,
  type VideoWindow,
} from './videoEvidence.js';

/** One (observed slot, sender) row of layers 1 and 2 (one home: `./videoEvidence`). */
export type { LayerRow };

/** Liveness bound for layers 1/2 to appear once a slot is active. Not a latency gate. */
const LAYERS_TIMEOUT_MS = 30_000;

/** The receiver's latest cumulative layer rows (empty before media starts). */
export async function latestLayers(receiver: PageMember): Promise<readonly LayerRow[]> {
  const { receiveLayers } = await latestBusEventsOf(receiver.page, ['receiveLayers']);
  // No event yet (before media starts) is "no rows"; a PRESENT event is held to
  // the exact-key contract (`parseReceiveLayers`).
  return receiveLayers === undefined ? [] : parseReceiveLayers(receiveLayers['layers']);
}

/** Frames from `sender` that verified and decrypted at this receiver, on any slot. */
export function acceptedFromRows(rows: readonly LayerRow[], senderId: string): number {
  return rows.filter((r) => r.senderId === senderId).reduce((sum, r) => sum + r.verified, 0);
}

/** The receiver's latest slot assignments. */
export async function latestAssignments(
  receiver: PageMember,
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
  receiver: PageMember,
  sender: Pick<JoinedMember, 'label' | 'tone'>,
  timeoutMs = LAYERS_TIMEOUT_MS,
): Promise<void> {
  const who = `${receiver.label} receiving ${sender.label} (sender ${sender.tone.senderId})`;
  // Through the one bus-cadence kernel; the misrouting `expect` fails at once.
  const { converged, last } = await pollBus(
    receiver.page,
    async (): Promise<{ readonly ok: boolean; readonly detail: string }> => {
      const [{ assignments, videoSlotIds }, rows] = await Promise.all([
        latestReceiveView(receiver.page),
        latestLayers(receiver),
      ]);
      const slots = assignments.filter((a) => a.senderId === sender.tone.senderId);
      const mine = rows.filter((r) => r.senderId === sender.tone.senderId);
      if (!(slots.length === 1 && slots[0]!.slotState === 'active')) {
        return { ok: false, detail: `assignment for the sender: ${JSON.stringify(slots)}` };
      }
      const slot = slots[0]!.slotId;
      const at = mine.find((r) => r.slot === slot);
      // The sender's VIDEO frames on this receiver's video slots are legitimate
      // (story 3); every other slot carrying its frames is misrouting.
      // RESIDUAL (story 3 C-C1, recorded): layer rows carry no kind, so an AUDIO
      // datagram MH stamped with a VIDEO slot id would pass this check. The
      // client cannot catch it either — on the datagram path the relay
      // `stream_id` is quarantined (it never decides drop-or-play,
      // `sdk-core pipeline/ingress.ts`), and the video stream path never sees a
      // datagram. It IS caught at MH: per-stream attribution keeps audio out of
      // video slots (R-20), proved by media-handler's Rust env-tests (MH-A1,
      // per-stream attribution / mixed meeting).
      const elsewhere = mine.filter(
        (r) =>
          r.slot !== slot &&
          r.keyed > 0 &&
          !(typeof r.slot === 'number' && videoSlotIds.has(r.slot)),
      );
      expect(
        elsewhere,
        `${who}: frames keyed to the sender arrived on a slot other than its assigned slot ${slot} ` +
          `— MISROUTING (layer 1): ${JSON.stringify(elsewhere)}`,
      ).toEqual([]);
      return {
        ok: at !== undefined && at.keyed > 0 && at.verified > 0,
        detail: `slot ${slot} active; layer row ${JSON.stringify(at ?? null)}`,
      };
    },
    (v) => v.ok,
    timeoutMs,
  );
  if (converged) return;
  throw new Error(`${who}: layers 1/2 not established within ${timeoutMs}ms — ${last.detail}`);
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
  /** Label of the member whose page this probe reads (the per-page mover rule). */
  readonly observer: string;
  readonly expect: ProbeExpectation;
  read(): Promise<number>;
}

/** Frames from `sender` accepted at `receiver` (layer 2, summed over slots). */
export function acceptedProbe(
  receiver: PageMember,
  sender: Pick<JoinedMember, 'label' | 'tone'>,
  expectation: ProbeExpectation,
): Probe {
  return {
    label: `${receiver.label}.acceptedFrom(${sender.label})`,
    observer: receiver.label,
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
    observer: receiver.label,
    expect: expectation,
    read: async () =>
      (await latestLayers(receiver))
        .filter((r) => r.senderId === sender.tone.senderId)
        .reduce((sum, r) => sum + r.keyed, 0),
  };
}

/** `member`'s own send counter (datagrams sent, all targets). */
export function sentProbe(member: PageMember, expectation: ProbeExpectation): Probe {
  return {
    label: `${member.label}.framesSent`,
    observer: member.label,
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

// The window's settle, observation span, sample floor (the anti-vacuity
// control) and hard bound have ONE home, shared with
// `expectCountersFlatOverWindow`: `FLAT_WINDOW_*` / `MIN_FLAT_WINDOW_SAMPLES` in
// `./fixtures`. The stop rule (span AND floor, else keep sampling until the hard
// bound, which throws) has one home too: `./windowSampling`.

/**
 * Sample every probe on the bus cadence over ONE window (after `settleMs`) and
 * assert each probe's expectation. Requires, for EVERY page carrying a flat or
 * zero probe, a probe on that same page that must `advance` — a flat reading on
 * a page with no mover could be a stalled sampler repeating a stale snapshot.
 */
export async function observeWindow(
  probes: readonly Probe[],
  why: string,
  { settleMs = FLAT_WINDOW_SETTLE_MS, observeMs = FLAT_WINDOW_OBSERVE_MS } = {},
): Promise<void> {
  const unpaired = unpairedObservers(probes);
  if (unpaired === null) {
    throw new Error(`observeWindow(${why}): no probe must advance — the window would be vacuous`);
  }
  if (unpaired.length > 0) {
    throw new Error(
      `observeWindow(${why}): no probe must advance on ${unpaired.join(', ')} — a flat/zero ` +
        `reading there could be a stalled sampler, so the window would be vacuous for that page`,
    );
  }
  if (settleMs > 0) await new Promise((r) => setTimeout(r, settleMs));
  const rounds = await sampleWindow({
    sample: () => Promise.all(probes.map((p) => p.read())),
    windowMs: observeMs,
    minSamples: MIN_FLAT_WINDOW_SAMPLES,
    // Scales with the span actually observed, so an `observeMs` override keeps
    // the same factor rather than a bound derived from the default span.
    maxMs: FLAT_WINDOW_MAX_OBSERVE_FACTOR * observeMs,
    intervalMs: E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS,
    why,
  });
  const samples: number[][] = probes.map((_, i) => rounds.map((round) => round[i]!));
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
  receiver: PageMember,
  subject: BoundMember,
  attribute:
    'data-reachability' | 'data-server-muted' | (typeof ROSTER_CLIENT_STATE_ATTRIBUTE)[MuteKind],
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

// ----------------------------------------------------------------------------
// Story 3 R-32: video slots, three layers + state + rendered (judges: ./videoEvidence)
// ----------------------------------------------------------------------------

/** One receiver video slot and what the oracle expects there. */
export interface VideoSlotExpectation {
  readonly slotId: number;
  readonly expectation: SlotExpectation;
  /** The expected occupant, for its JOIN-BOUND sender id (never its own announcement). */
  readonly occupant?: BoundMember;
  /** The occupant captures from the real camera: layer 3 does not apply (`videoEvidence.ts`). */
  readonly occupantSource?: 'test_pattern' | 'camera';
}

/**
 * `member`'s meeting-scoped sender id from its OWN `joined` bus event — the join
 * binding, never a self-announcement and never its audio tone (a video-only
 * publisher has no tone). Absent is a HARNESS error, never a default.
 */
export function joinBoundSenderId(member: BoundMember): string {
  const senderId = member.joined.senderId;
  if (senderId === undefined || senderId === '') {
    throw new Error(
      `HARNESS: ${member.label}'s joined bus event carries no senderId — the video ` +
        'evidence helpers cannot name its join-bound sender',
    );
  }
  return senderId;
}

/**
 * Sample the receiver's `videoSlots` and `receiveLayers` together over ONE
 * window on the shared stop rule (`./windowSampling`; samples counted by order,
 * no bus timestamp), then judge every listed slot and throw once, naming each
 * failing slot.
 */
export async function expectVideoSlots(
  receiver: PageMember,
  slots: readonly VideoSlotExpectation[],
  why: string,
  { observeMs = FLAT_WINDOW_OBSERVE_MS } = {},
): Promise<void> {
  assertVideoWindow(receiver, slots, await sampleVideoWindow(receiver, why, { observeMs }), why);
}

/**
 * Sample the receiver's `videoSlots` and `receiveLayers` together over ONE
 * window on the shared stop rule (`./windowSampling`; samples counted by order).
 */
export async function sampleVideoWindow(
  receiver: PageMember,
  why: string,
  { observeMs = FLAT_WINDOW_OBSERVE_MS } = {},
): Promise<VideoWindow> {
  const rounds = await sampleWindow({
    sample: async (): Promise<[VideoSlotsEvent, readonly LayerRow[]]> => {
      const { videoSlots, receiveLayers } = await latestBusEventsOf(receiver.page, [
        'videoSlots',
        'receiveLayers',
      ]);
      if (videoSlots === undefined) {
        throw new Error(`${receiver.label}: no videoSlots sample on the bus (${why})`);
      }
      return [
        parseVideoSlots(videoSlots),
        receiveLayers === undefined ? [] : parseReceiveLayers(receiveLayers['layers']),
      ];
    },
    windowMs: observeMs,
    minSamples: MIN_FLAT_WINDOW_SAMPLES,
    maxMs: FLAT_WINDOW_MAX_OBSERVE_FACTOR * observeMs,
    intervalMs: E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS,
    why,
  });
  return { slots: rounds.map((r) => r[0]), layers: rounds.map((r) => r[1]) };
}

/** Judge every listed slot over one sampled window; throw once, naming each failing slot. */
export function assertVideoWindow(
  receiver: PageMember,
  slots: readonly VideoSlotExpectation[],
  window: VideoWindow,
  why: string,
): void {
  assertVideoSlots(
    `${receiver.label} (${why})`,
    slots.map((s) =>
      judgeVideoSlot(
        {
          slotId: s.slotId,
          expectation: s.expectation,
          ...(s.occupant !== undefined ? { expectedSenderId: joinBoundSenderId(s.occupant) } : {}),
          ...(s.occupantSource !== undefined ? { occupantSource: s.occupantSource } : {}),
        },
        window,
      ),
    ),
  );
}

/** Assert `member`'s send tracks against the scenario's directive/mute/health facts. */
export async function expectSendTracks(
  member: PageMember,
  want: SendSideExpectation,
  timeoutMs = VIDEO_SLOT_STATE_LIVENESS_MS,
): Promise<void> {
  await expect
    .poll(
      async () => {
        const { sendTracks } = await latestBusEventsOf(member.page, ['sendTracks']);
        if (sendTracks === undefined) return ['no sendTracks sample on the bus'];
        return judgeSendTracks(want, parseSendTracks(sendTracks));
      },
      { message: `${member.label}: send tracks`, timeout: timeoutMs },
    )
    .toEqual([]);
}

/** Assert `member`'s announced test pattern names its own join-bound sender id. */
export async function expectPatternAnnounced(member: PageMember & BoundMember): Promise<void> {
  const { videoCaptureSource } = await latestBusEventsOf(member.page, ['videoCaptureSource']);
  if (videoCaptureSource === undefined) {
    throw new Error(`${member.label}: no videoCaptureSource on the bus (capture never started?)`);
  }
  expect(
    judgePatternAnnouncement(
      parseVideoCaptureSource(videoCaptureSource),
      joinBoundSenderId(member),
    ),
    `${member.label}: announced pattern source`,
  ).toEqual([]);
}

/** `member`'s VIDEO send counter (`sendCounts.video.framesSent`), the camera-light corroboration. */
export function videoSentProbe(member: PageMember, expectation: ProbeExpectation): Probe {
  return {
    label: `${member.label}.videoFramesSent`,
    observer: member.label,
    expect: expectation,
    read: async () => {
      const { sendCounts } = await latestBusEventsOf(member.page, ['sendCounts']);
      if (sendCounts === undefined)
        throw new Error(`${member.label}: no sendCounts sample on the bus`);
      return parseSendCounts(sendCounts).video.framesSent;
    },
  };
}

/** `member`'s total keyframes PRODUCED (all causes), from its own bus. */
export async function keyframesProduced(member: PageMember): Promise<number> {
  const { sendCounts } = await latestBusEventsOf(member.page, ['sendCounts']);
  if (sendCounts === undefined) throw new Error(`${member.label}: no sendCounts sample on the bus`);
  const { keyframes } = parseSendCounts(sendCounts).video;
  return Object.values(keyframes).reduce((a, b) => a + b, 0);
}

/** Frames verified-and-decrypted at `receiver` from `senderId` on video slot `slotId` (layer 2). */
export function videoVerifiedProbe(
  receiver: PageMember,
  slotId: number,
  senderId: string,
  expectation: ProbeExpectation,
): Probe {
  return {
    label: `${receiver.label}.verified(slot ${slotId}, sender ${senderId})`,
    observer: receiver.label,
    expect: expectation,
    read: async () =>
      (await latestLayers(receiver))
        .filter((r) => r.slot === slotId && r.senderId === senderId)
        .reduce((sum, r) => sum + r.verified, 0),
  };
}
