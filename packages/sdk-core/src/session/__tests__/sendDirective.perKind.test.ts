// Story 3 R-16 + wire freeze: the per-kind send-directive listener.
//   * a published kind MISSING from a NON-EMPTY `streams` is a counted, typed
//     fault and fails closed (never the stale target set);
//   * an EMPTY `streams` is the pre-MC-A withdrawal (TRANSITIONAL shim c) — no
//     fault, no counter, still fails closed;
//   * a header version this client cannot send is refused and counted ONCE;
//   * the EFFECTIVE per-kind send state feeds R-18.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import {
  DIRECTIVE_KIND_MISSING_FAULT,
  kindMetrics,
  sendDirectiveListener,
} from '../mediaWiring.js';
import { MediaMetrics } from '../../media/setup/mediaMetrics.js';
import { HEADER_VERSION } from '../../media/frame/wireConstants.js';
import type { AudioSendDirective, MediaFault } from '../../media/lifecycle/AudioPipeline.js';
import type { SendDirectiveEvent, SendStreamDirective } from '../../signaling/events.js';
import { ServerMuteResumeTracker } from '../serverMuteResume.js';

const MH = 'https://mh-a';

function audio(targets: string[], streamNumber = 0): SendStreamDirective {
  return { streamNumber, mediaKind: 'audio', maxBitrateBps: undefined, targets };
}

function directive(
  streams: SendStreamDirective[],
  headerVersion = HEADER_VERSION,
): SendDirectiveEvent {
  return { streams, headerVersion };
}

function rig() {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 't', orgId: 'o' }, sink);
  const applied: AudioSendDirective[] = [];
  const faults: MediaFault[] = [];
  const effective: [string, boolean][] = [];
  const resumed: string[] = [];
  const tracker = new ServerMuteResumeTracker(
    () => false,
    (kind) => resumed.push(kind),
  );
  const listener = sendDirectiveListener({
    publish: ['audio'],
    pipeline: { setSendDirective: (d) => applied.push(d) },
    defaultBitrateBps: 32_000,
    metrics,
    fault: (f) => faults.push(f),
    effectiveTargets: (kind, nonEmpty) => {
      effective.push([kind, nonEmpty]);
      tracker.apply({ type: 'effectiveTargets', kind, nonEmpty });
    },
  });
  const count = (name: string): number =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name)
      .reduce((n, m) => n + m.value, 0);
  return { listener, applied, faults, effective, resumed, tracker, count };
}

const KIND_MISSING = 'dt_client_media_send_directive_kind_missing_total';
const HEADER_REFUSED = 'dt_client_media_send_directive_header_version_refused_total';

describe('per-kind send directive', () => {
  it('applies a listed audio stream with its targets', () => {
    const r = rig();
    r.listener(directive([audio([MH], 0)]));
    expect(r.applied.map((d) => [d.streamNumber, d.targets])).toEqual([[0, [MH]]]);
    expect(r.effective).toEqual([['audio', true]]);
    expect(r.faults).toEqual([]);
  });

  it('an EMPTY target set is a withdrawal: send nothing, mic kept open (no fault)', () => {
    const r = rig();
    r.listener(directive([audio([MH])]));
    r.listener(directive([audio([])]));
    expect(r.applied.map((d) => d.targets)).toEqual([[MH], []]);
    expect(r.faults).toEqual([]);
    expect(r.count(KIND_MISSING)).toBe(0);
  });

  it('a published kind MISSING from a NON-EMPTY streams list: counted, typed fault, FAILS CLOSED', () => {
    const r = rig();
    r.listener(directive([audio([MH])]));
    const video: SendStreamDirective = {
      streamNumber: 1,
      mediaKind: 'video',
      maxBitrateBps: undefined,
      targets: [MH],
    };
    r.listener(directive([video]));
    expect(r.count(KIND_MISSING)).toBe(1);
    expect(r.faults).toEqual([
      { stage: 'config', message: DIRECTIVE_KIND_MISSING_FAULT, fatal: false },
    ]);
    // Never the stale target set: withdrawn.
    expect(r.applied.at(-1)?.targets).toEqual([]);
    expect(r.effective.at(-1)).toEqual(['audio', false]);
  });

  it('the kind-missing fault is once per episode, re-armed by a complete directive; every one is counted', () => {
    const r = rig();
    const other: SendStreamDirective = {
      streamNumber: 9,
      mediaKind: 'unknown',
      maxBitrateBps: undefined,
      targets: [],
    };
    r.listener(directive([other]));
    r.listener(directive([other]));
    expect(r.faults).toHaveLength(1);
    expect(r.count(KIND_MISSING)).toBe(2);
    r.listener(directive([audio([MH])]));
    r.listener(directive([other]));
    expect(r.faults).toHaveLength(2);
  });

  it('TRANSITIONAL shim (c): an EMPTY streams list withdraws every published kind — no fault, no counter', () => {
    const r = rig();
    r.listener(directive([audio([MH])]));
    r.listener(directive([]));
    expect(r.count(KIND_MISSING)).toBe(0);
    expect(r.faults).toEqual([]);
    expect(r.applied.at(-1)?.targets).toEqual([]);
    expect(r.effective.at(-1)).toEqual(['audio', false]);
  });

  it('shim (c) with nothing ever directed invents no stream (no apply at all)', () => {
    const r = rig();
    r.listener(directive([]));
    expect(r.applied).toEqual([]);
  });

  it('a header version this client cannot send is refused and counted ONCE per directive, effective = empty', () => {
    const r = rig();
    r.listener(directive([audio([MH])], HEADER_VERSION + 1));
    expect(r.count(HEADER_REFUSED)).toBe(1);
    // The pipeline's own fail-closed path receives it verbatim and refuses it.
    expect(r.applied.at(-1)?.headerVersion).toBe(HEADER_VERSION + 1);
    expect(r.effective).toEqual([['audio', false]]);
    r.listener(directive([audio([MH])]));
    expect(r.count(HEADER_REFUSED)).toBe(1);
  });

  it('R-18 through the effective state: latched -> kind-missing fail-closed -> valid directive fires EXACTLY once', () => {
    const r = rig();
    r.listener(directive([audio([MH])]));
    r.tracker.apply({ type: 'selfServerMute', kind: 'audio', muted: true });
    const other: SendStreamDirective = {
      streamNumber: 9,
      mediaKind: 'unknown',
      maxBitrateBps: undefined,
      targets: [],
    };
    r.listener(directive([other])); // kind missing => effective empty
    r.listener(directive([audio([MH])])); // valid, non-empty
    r.listener(directive([audio([MH])]));
    expect(r.resumed).toEqual(['audio']);
  });

  it('a refused header version never counts as a resume edge', () => {
    const r = rig();
    r.tracker.apply({ type: 'selfServerMute', kind: 'audio', muted: true });
    r.listener(directive([audio([MH])], HEADER_VERSION + 1));
    expect(r.resumed).toEqual([]);
  });
});

describe('kindMetrics is exhaustive over the published kinds', () => {
  it('a published VIDEO kind with no handle is a loud invariant error, never a silent skip', () => {
    const metrics = new MediaMetrics({ clientVersion: 't', orgId: 'o' }, new InMemoryMetricsSink());
    expect(kindMetrics(metrics, 'audio')).toBe(metrics.audio);
    // C-A2 (task 13) adds MediaMetrics.video; until then publishing video is
    // refused upstream, and reaching here anyway must throw.
    expect(() => kindMetrics(metrics, 'video')).toThrow(/internal invariant/);
  });
});
