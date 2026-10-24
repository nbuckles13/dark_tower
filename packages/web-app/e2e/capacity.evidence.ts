// File: packages/web-app/e2e/capacity.evidence.ts
//
// Story 3 R-28 CAPACITY EVIDENCE (task 21) — a RECORDED RUN, NOT A GATE.
//
// Run ONLY through `playwright.capacity.config.ts`
// (`pnpm --filter @darktower/web-app test:capacity-evidence`): the name is
// outside the default `*.spec.ts` match, so Layer 7's `test:e2e` never runs it.
//
// Two cohort shapes from the story's task 21, every member publishing both kinds:
//   * see-all:           P=4, N=3, M=3 -> 4 x (3 + 3) = 24 edges;
//   * over-subscription: P=4, N=2, M=2 -> 4 x (2 + 2) = 16 edges (3 candidate
//     senders per receiver for 2 slots of each kind).
// Through `VideoScenario`, so the test's OWN slot oracle predicts every slot.
//
// Per shape, over ONE scenario window (all members joined and converged ->
// the observation span), it RECORDS, as one `[capacity-evidence]` JSON line and
// a test attachment (gitignored `test-results/`, never committed):
//   * process-level CPU per browsing context (its renderer, mapped by
//     `processCpu.ts`'s fingerprint) and the browser-wide shared processes;
//   * per receiver and video slot: frames decoded and rendered (bus `videoSlots`);
//   * per publisher: keyframes produced by cause (bus `sendCounts`) and send
//     groups abandoned (R-35 overlay — the bus does not carry it);
//   * per receiver: each video slot's currently-unfulfilled reading at window end
//     (R-35 overlay; the Prometheus gauge is single-writer, so it cannot say
//     WHICH browser);
//   * the window's wall-clock bounds, for the MH reads from Prometheus.
//
// WHAT FAILS IT (the brief's "does not fit", stated as findings, never tuned):
// a receiver whose video slots lose liveness under the plan-7 detector
// (`VideoScenario.expectVideo`: three layers incl. the pattern liveness verdict,
// judged in rounds across the whole window), or a video slot still unfulfilled
// at window end. The evidence is printed FIRST, then the test fails. CPU is
// never judged: it is host-dependent and recorded only. The observation span is
// a minimum span to measure over, not a pass/fail bound.

import { execFileSync } from 'node:child_process';
import { expect, test, type Page } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import type { Member } from './cohortContexts.js';
import { latestBusEventsOf } from './fixtures.js';
import {
  assertMappingComplete,
  browserTree,
  deltaTicks,
  percentOfCore,
  pickFingerprintedPid,
  ticksByPid,
  type ChromiumProcess,
} from './processCpu.js';
import { BusShapeError, parseSendCounts, parseVideoSlots } from './videoBusShape.js';
import { videoSlotIds } from './videoPlan.js';
import { VideoScenario } from './videoScenario.js';

/** Minimum observation span the CPU and counter deltas are taken over (not a bound). */
const CAPACITY_WINDOW_MIN_MS = 30_000;
/** In-page busy span for the renderer fingerprint, and the share of it a match must show. */
const FINGERPRINT_BUSY_MS = 1_500;
const FINGERPRINT_MATCH_SHARE = 0.6;
/** A renderer burning at least this share of one core in the window must be mapped. */
const UNMAPPED_RENDERER_SHARE = 0.05;

const SHAPES = [
  { name: 'see-all', audioReceiveSlots: 3, videoReceiveSlots: 3 },
  { name: 'over-subscription', audioReceiveSlots: 2, videoReceiveSlots: 2 },
] as const;
const PARTICIPANTS = 4;

function ticksPerSecond(): number {
  const v = Number(execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }).trim());
  if (!Number.isInteger(v) || v <= 0) throw new Error(`HARNESS: getconf CLK_TCK gave '${v}'`);
  return v;
}

const renderers = (tree: readonly ChromiumProcess[]) =>
  ticksByPid(tree.filter((p) => p.type === 'renderer'));

/** Map `member`'s renderer by making its (idle) page busy for a fixed span. */
async function fingerprint(member: Member, hz: number, taken: Set<number>): Promise<number> {
  const before = renderers(browserTree());
  await member.page.evaluate((ms) => {
    const t = performance.now();
    while (performance.now() - t < ms) {
      /* busy: the fingerprint */
    }
  }, FINGERPRINT_BUSY_MS);
  const deltas = deltaTicks(before, renderers(browserTree()));
  const floor = Math.floor((FINGERPRINT_BUSY_MS / 1000) * hz * FINGERPRINT_MATCH_SHARE);
  const pid = pickFingerprintedPid(deltas, floor, member.label);
  if (taken.has(pid)) {
    throw new Error(`HARNESS: ${member.label} fingerprinted to renderer ${pid}, already mapped`);
  }
  taken.add(pid);
  return pid;
}

interface OverlayRead {
  readonly groupsAbandoned: string | null;
  readonly keyframes: string | null;
  /** Per video receive row: [slot label, decoded, unfulfilled cell text]. */
  readonly receive: ReadonlyArray<readonly [string, string, string]>;
}

/** Open the R-35 overlay, read the video rows, close it again (it samples only while open). */
async function readOverlay(page: Page): Promise<OverlayRead> {
  const toggle = page.getByTestId('debug-panel-toggle');
  await toggle.click();
  await expect(page.getByTestId('debug-stats-send-video')).toBeVisible();
  const read = await page.evaluate(() => {
    const cell = (row: Element | null, id: string) =>
      row?.querySelector(`[data-testid="${id}"]`)?.textContent?.trim() ?? null;
    const send = document.querySelector('[data-testid="debug-stats-send-video"]');
    const receive = [...document.querySelectorAll('[data-testid^="debug-stats-slot-video:"]')].map(
      (row) =>
        [
          row.querySelector('td')?.textContent?.trim() ?? '?',
          cell(row, 'decoded') ?? '?',
          cell(row, 'unfulfilled') ?? '?',
        ] as const,
    );
    return {
      groupsAbandoned: cell(send, 'groups-abandoned'),
      keyframes: cell(send, 'keyframes'),
      receive,
    };
  });
  await toggle.click();
  await expect(page.getByTestId('debug-panel')).toBeHidden();
  return read;
}

interface BusRead {
  readonly slots: ReturnType<typeof parseVideoSlots>['slots'];
  readonly keyframes: ReturnType<typeof parseSendCounts>['video']['keyframes'];
  readonly framesSent: number;
}

async function readBus(page: Page, label: string): Promise<BusRead> {
  const { videoSlots, sendCounts } = await latestBusEventsOf(page, ['videoSlots', 'sendCounts']);
  if (videoSlots === undefined || sendCounts === undefined) {
    throw new Error(`HARNESS: ${label}: no videoSlots/sendCounts on the bus`);
  }
  const counts = parseSendCounts(sendCounts);
  return {
    slots: parseVideoSlots(videoSlots).slots,
    keyframes: counts.video.keyframes,
    framesSent: counts.video.framesSent,
  };
}

/**
 * Anti-vacuity: the bus and the overlay must carry EXACTLY the declared video
 * slots, or a check over their rows could pass on rows that are not there (the
 * SDK's `localStats()` skips a slot with no tally, so a missing row is
 * reachable). A HARNESS error, never the story-blocking verdict.
 */
function assertSlotRows(label: string, where: string, got: number[], want: number[]): void {
  const g = [...got].sort((a, b) => a - b);
  if (JSON.stringify(g) !== JSON.stringify(want)) {
    throw new Error(
      `HARNESS: ${label} ${where}: video slot rows ${JSON.stringify(g)}, declared ${JSON.stringify(want)}`,
    );
  }
}

/** The overlay row label is "<kind> <slotId>" (DebugPanel.svelte); its slot id. */
function overlaySlotId(label: string, slot: string): number {
  const m = /^video (\d+)$/.exec(slot);
  if (m === null) throw new Error(`HARNESS: ${label}: overlay row label '${slot}'`);
  return Number(m[1]);
}

/** Read one member's bus and overlay with the slot-row completeness check. */
async function readMember(
  m: Member,
  want: number[],
  where: string,
): Promise<{ bus: BusRead; overlay: OverlayRead }> {
  const bus = await readBus(m.page, m.label);
  assertSlotRows(
    m.label,
    `${where} bus`,
    bus.slots.map((x) => x.slotId),
    want,
  );
  const overlay = await readOverlay(m.page);
  assertSlotRows(
    m.label,
    `${where} overlay`,
    overlay.receive.map(([slot]) => overlaySlotId(m.label, slot)),
    want,
  );
  return { bus, overlay };
}

/** An error that is the HARNESS's (or the bus contract's), never a liveness verdict. */
function isHarnessError(e: unknown): boolean {
  return e instanceof BusShapeError || (e instanceof Error && e.message.startsWith('HARNESS'));
}

/**
 * One context's evidence record: CPU, per-slot frame deltas, publisher counters,
 * and the unfulfilled readings (pushed onto `unfulfilledAtEnd` when > 0).
 */
function contextEvidence(
  label: string,
  start: { bus: BusRead; overlay: OverlayRead },
  end: { bus: BusRead; overlay: OverlayRead },
  rendererTicks: number,
  elapsedMs: number,
  hz: number,
  unfulfilledAtEnd: string[],
) {
  const b0 = start.bus;
  const b1 = end.bus;
  for (const [slot, , unfulfilled] of end.overlay.receive) {
    // The SDK reports a video slot's time in the unfulfilled state so far; 0 is a
    // fulfilled slot. Anything that is not "<n> ms" is a harness error, not a pass.
    const ms = /^(\d+) ms$/.exec(unfulfilled);
    if (ms === null)
      throw new Error(`HARNESS: ${label} ${slot}: unfulfilled cell '${unfulfilled}'`);
    if (Number(ms[1]) > 0) unfulfilledAtEnd.push(`${label} ${slot}: ${unfulfilled}`);
  }
  return {
    context: label,
    rendererCpuPercentOfOneCore: percentOfCore(rendererTicks, elapsedMs, hz),
    videoSlots: b1.slots.map((s) => {
      const s0 = b0.slots.find((x) => x.slotId === s.slotId);
      return {
        slotId: s.slotId,
        mcState: s.mcState,
        rendered: s.rendered,
        framesDecoded: s.framesDecoded - (s0?.framesDecoded ?? 0),
        framesRendered: s.framesRendered - (s0?.framesRendered ?? 0),
      };
    }),
    publisher: {
      videoFramesSent: b1.framesSent - b0.framesSent,
      keyframesProducedByCause: Object.fromEntries(
        Object.entries(b1.keyframes).map(([c, n]) => [
          c,
          n - (b0.keyframes[c as keyof typeof b0.keyframes] ?? 0),
        ]),
      ),
      // Cumulative since this member's media start (joins included), for scale.
      keyframesProducedSinceStartAtWindowEnd: b1.keyframes,
      groupsAbandoned: { start: start.overlay.groupsAbandoned, end: end.overlay.groupsAbandoned },
    },
    unfulfilledAtWindowEnd: end.overlay.receive.map(([slot, , u]) => `${slot}=${u}`),
  };
}

test.describe('story 3 R-28: capacity evidence for the video cohort (recorded, not a gate)', () => {
  for (const shape of SHAPES) {
    test(`${shape.name}: P=${PARTICIPANTS}, N=${shape.audioReceiveSlots}, M=${shape.videoReceiveSlots}`, async ({
      browser,
    }, testInfo) => {
      test.setTimeout(420_000);
      const hz = ticksPerSecond();
      const plan: CohortMemberPlan = {
        publish: 'both',
        audioReceiveSlots: shape.audioReceiveSlots,
        videoReceiveSlots: shape.videoReceiveSlots,
      };
      const taken = new Set<number>();
      const rendererOf = new Map<string, number>();
      const { scenario, host } = await VideoScenario.create(
        browser,
        `E2E capacity ${shape.name}`,
        plan,
      );
      try {
        rendererOf.set(host.label, await fingerprint(host, hz, taken));
        await scenario.join(host, plan);
        for (let i = 1; i < PARTICIPANTS; i += 1) {
          const m = await scenario.open(i, plan);
          rendererOf.set(m.label, await fingerprint(m, hz, taken));
          await scenario.join(m, plan);
        }
        const members = [...scenario.members.values()];
        // Converge (and judge once) before the window opens.
        for (const r of members) await scenario.expectVideo(r.label, `${shape.name} converge`);

        // ---- window start ----
        const want = videoSlotIds(plan);
        const startRead = new Map<string, Awaited<ReturnType<typeof readMember>>>();
        for (const m of members) startRead.set(m.label, await readMember(m, want, 'window start'));
        const startTree = browserTree();
        const startAt = new Date();
        const t0 = performance.now();

        const livenessLost: string[] = [];
        let rounds = 0;
        while (rounds === 0 || performance.now() - t0 < CAPACITY_WINDOW_MIN_MS) {
          rounds += 1;
          for (const r of members) {
            try {
              await scenario.expectVideo(r.label, `${shape.name} window round ${rounds}`);
            } catch (e) {
              if (isHarnessError(e)) throw e; // a harness fault is never a liveness verdict
              livenessLost.push(
                `${r.label} round ${rounds}: ${e instanceof Error ? e.message.split('\n')[0] : String(e)}`,
              );
            }
          }
        }

        // ---- window end ----
        const endTree = browserTree();
        const elapsedMs = performance.now() - t0;
        const endAt = new Date();
        const endRead = new Map<string, Awaited<ReturnType<typeof readMember>>>();
        for (const m of members) endRead.set(m.label, await readMember(m, want, 'window end'));

        const delta = deltaTicks(ticksByPid(startTree), ticksByPid(endTree));
        const mapped = new Set(rendererOf.values());
        assertMappingComplete(
          deltaTicks(renderers(startTree), renderers(endTree)),
          mapped,
          Math.ceil((elapsedMs / 1000) * hz * UNMAPPED_RENDERER_SHARE),
        );
        const sharedByType = new Map<string, number>();
        for (const p of endTree) {
          if (mapped.has(p.pid)) continue;
          sharedByType.set(p.type, (sharedByType.get(p.type) ?? 0) + (delta.get(p.pid) ?? 0));
        }

        const unfulfilledAtEnd: string[] = [];
        const contexts = members.map((m) =>
          contextEvidence(
            m.label,
            startRead.get(m.label)!,
            endRead.get(m.label)!,
            delta.get(rendererOf.get(m.label)!) ?? 0,
            elapsedMs,
            hz,
            unfulfilledAtEnd,
          ),
        );

        const evidence = {
          shape: shape.name,
          participants: PARTICIPANTS,
          audioReceiveSlots: shape.audioReceiveSlots,
          videoReceiveSlots: shape.videoReceiveSlots,
          edges: PARTICIPANTS * (shape.audioReceiveSlots + shape.videoReceiveSlots),
          window: {
            start: startAt.toISOString(),
            end: endAt.toISOString(),
            elapsedMs: Math.round(elapsedMs),
            livenessRounds: rounds,
          },
          clockTicksPerSecond: hz,
          contexts,
          sharedCpuPercentOfOneCoreByType: Object.fromEntries(
            [...sharedByType].map(([t, ticks]) => [t, percentOfCore(ticks, elapsedMs, hz)]),
          ),
          livenessLost,
          unfulfilledAtEnd,
        };
        const line = JSON.stringify(evidence);
        console.log(`[capacity-evidence] ${line}`);
        await testInfo.attach(`capacity-evidence-${shape.name}.json`, {
          body: line,
          contentType: 'application/json',
        });

        expect(livenessLost, `${shape.name}: a slot lost liveness — story-blocking`).toEqual([]);
        expect(
          unfulfilledAtEnd,
          `${shape.name}: a video slot is still unfulfilled at window end — story-blocking`,
        ).toEqual([]);
      } finally {
        await scenario.close();
      }
    });
  }
});
