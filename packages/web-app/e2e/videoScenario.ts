// File: packages/web-app/e2e/videoScenario.ts
//
// Story 3 C-C1: the shared harness for the video browser specs — ONE meeting,
// its plan members, and the test's OWN slot oracle driven by the spec's own
// join / leave / mute calls IN THE ORDER THE SPEC DRIVES THE BROWSERS
// (paired-test F4/F5).
//
// ORACLE INDEPENDENCE: expectations come only from `SlotOracle`, fed by this
// harness's calls. MC's `slotAssignments` are never fed into it, and nothing
// here "corrects" it from MC output — a disagreement fails the spec, and that
// is a finding about MC or the oracle. The expected sender id of a live slot
// is the occupant's OWN join binding (`PlanMember.senderId`), never its
// self-announced pattern id.
//
// FAILURE DIAGNOSTICS: every assertion helper runs inside
// `failureDiagnostics.withVideoDiagnostics`, with the MH admission baseline the
// scenario took BEFORE its first join.

import type { Browser, BrowserContext } from 'playwright/test';
import { readFileSync } from 'node:fs';
import {
  closeMembers,
  joinPlanMember,
  leaveGracefully,
  openPlanMember,
  type ContextLevers,
  type Member,
  type PlanMember,
} from './cohortContexts.js';
import type { CohortMemberPlan } from './cohort.js';
import { readMcReceiveCaps, serviceConfigEnvPath, type McReceiveCaps } from './configEnv.js';
import { bootstrapMeeting, waitForVideoSlotState } from './fixtures.js';
import type { InstanceCounters } from './instanceCounters.js';
import { mhAdmissionRejectionsByInstance } from './mcMetrics.js';
import {
  expectVideoSlots,
  joinBoundSenderId,
  type VideoSlotExpectation,
} from './receiveEvidence.js';
import { renderedFor } from './videoEvidence.js';
import { SlotOracle, type Kind } from './slotOracle.js';
import { videoSlotPlans } from './videoPlan.js';
import { withVideoDiagnostics } from './failureDiagnostics.js';

/** MC's per-kind caps, read independently from MC's own ConfigMap source. */
export function mcCaps(): McReceiveCaps {
  const path = serviceConfigEnvPath('mc-service');
  return readMcReceiveCaps(readFileSync(path, 'utf8'), path);
}

/** One video scenario: a meeting, its members, the oracle and the diagnostics baseline. */
export class VideoScenario {
  readonly oracle: SlotOracle;
  readonly opened: Member[] = [];
  readonly members = new Map<string, PlanMember>();
  readonly caps: McReceiveCaps;
  /** Members opened in CAMERA mode (`forceCameraCapture`): layer 3 does not apply to them. */
  private readonly cameraMode = new Set<string>();
  private constructor(
    private readonly browser: Browser,
    readonly meetingCode: string,
    readonly admissionBaseline: InstanceCounters,
    host: Member,
  ) {
    this.caps = mcCaps();
    this.oracle = new SlotOracle({ audio: this.caps.audio, video: this.caps.video });
    this.opened.push(host);
  }

  /**
   * Open the HOST (cohort member 0) with its plan, create the meeting with the
   * host's own token, and take the admission baseline BEFORE any join.
   */
  static async create(
    browser: Browser,
    title: string,
    hostPlan: CohortMemberPlan,
    hostLevers: Omit<ContextLevers, 'audioReceiveSlots' | 'videoReceiveSlots' | 'publish'> = {},
  ): Promise<{ scenario: VideoScenario; host: Member }> {
    const host = await openPlanMember(browser, 0, hostPlan, hostLevers);
    const meetingCode = await bootstrapMeeting(host.token, title);
    const baseline = await mhAdmissionRejectionsByInstance();
    return { scenario: new VideoScenario(browser, meetingCode, baseline, host), host };
  }

  /** Open cohort member `index` with its plan's N / M levers (no join yet). */
  async open(
    index: number,
    plan: CohortMemberPlan,
    levers: Omit<ContextLevers, 'audioReceiveSlots' | 'videoReceiveSlots' | 'publish'> = {},
    beforeNavigate?: (context: BrowserContext) => Promise<void>,
  ): Promise<Member> {
    const member = await openPlanMember(this.browser, index, plan, levers, beforeNavigate);
    this.opened.push(member);
    if (levers.forceCameraCapture === true) this.cameraMode.add(member.label);
    return member;
  }

  /**
   * Join `member` (SERIALISED: resolves only after its own join and media
   * start), then tell the oracle, in that order.
   */
  async join(
    member: Member,
    plan: CohortMemberPlan,
    {
      expectAllHandlers = true,
      videoCap = 'admits',
      handlers,
    }: {
      expectAllHandlers?: boolean;
      videoCap?: 'admits' | 'truncates';
      /** The handlers this member ACTUALLY connects to (a `blockHandlers` lever narrows them). */
      handlers?: readonly string[];
    } = {},
  ): Promise<PlanMember> {
    const joined = await withVideoDiagnostics(
      `${member.label} joins`,
      this.admissionBaseline,
      () => this.pages(),
      () => joinPlanMember(member, this.meetingCode, plan, { expectAllHandlers, videoCap }),
    );
    this.members.set(member.label, joined);
    this.oracle.join(member.label, {
      publish: plan.publish,
      slots: { audio: plan.audioReceiveSlots, video: plan.videoReceiveSlots },
      handlers: handlers ?? joined.joined.mediaServers,
    });
    return joined;
  }

  /** Leave `label` gracefully and tell the oracle. */
  async leave(label: string): Promise<void> {
    const m = this.member(label);
    await leaveGracefully(m.page);
    this.members.delete(label);
    this.oracle.leave(label);
  }

  /** Record a mute change in the oracle (the spec drives the browser itself). */
  muted(label: string, kind: Kind, mute: { client?: boolean; server?: boolean }): void {
    this.oracle.setMute(label, kind, mute);
  }

  member(label: string): PlanMember {
    const m = this.members.get(label);
    if (m === undefined) throw new Error(`HARNESS: no joined member ${label}`);
    return m;
  }

  /** Every open page, for the client-fault section of the diagnostics. */
  pages(): Array<{ label: string; page: Member['page'] }> {
    return this.opened.map((m) => ({ label: m.label, page: m.page }));
  }

  /**
   * Run `fn` with the scenario's failure diagnostics (MH budget / pending /
   * client fault dumped on failure, original error rethrown).
   */
  diagnosed<T>(what: string, fn: () => Promise<T>): Promise<T> {
    return withVideoDiagnostics(what, this.admissionBaseline, () => this.pages(), fn);
  }

  /**
   * The oracle's VIDEO expectation for `label`, per declared slot id, with each
   * live occupant bound to its OWN join binding.
   */
  videoExpectations(label: string): VideoSlotExpectation[] {
    const receiver = this.member(label);
    return videoSlotPlans(receiver.plan, this.oracle.expected(label).video).map((p) => {
      const occupant =
        p.expectation.occupant === undefined ? undefined : this.member(p.expectation.occupant);
      return {
        slotId: p.slotId,
        expectation: p.expectation,
        ...(occupant !== undefined
          ? {
              occupant,
              occupantSource: this.cameraMode.has(occupant.label) ? 'camera' : 'test_pattern',
            }
          : {}),
      } satisfies VideoSlotExpectation;
    });
  }

  /**
   * Every video slot of `label`: first CONVERGE (each slot reaches the oracle's
   * state and the matching rendered kind — for a live slot, a rendered frame
   * keyed to the expected occupant; a liveness bound, paired-test F2/F6), then
   * judge all three layers + state + rendered over ONE sampled window.
   */
  async expectVideo(label: string, why: string): Promise<void> {
    const receiver = this.member(label);
    const expectations = this.videoExpectations(label);
    await this.diagnosed(`${label} video slots (${why})`, async () => {
      for (const e of expectations) {
        const want = renderedFor(e.expectation.state);
        const sender = e.occupant === undefined ? undefined : joinBoundSenderId(e.occupant);
        await waitForVideoSlotState(
          receiver.page,
          e.slotId,
          (slot) =>
            slot !== undefined &&
            slot.mcState === e.expectation.state &&
            slot.rendered === want &&
            (e.expectation.state !== 'active' || slot.renderedSenderId === sender),
          `${label} slot ${e.slotId} -> ${e.expectation.state}${
            e.expectation.occupant === undefined ? '' : ` (${e.expectation.occupant})`
          } [${why}]`,
        );
      }
      await expectVideoSlots(receiver, expectations, why);
    });
  }

  /** Leave and close every opened member (on failure too). */
  async close(): Promise<void> {
    await closeMembers(this.opened);
  }
}
