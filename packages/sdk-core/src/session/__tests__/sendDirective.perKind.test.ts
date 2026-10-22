// Story 3 R-16 + wire freeze: the per-kind send-directive listener.
//   * a published kind MISSING from a NON-EMPTY `streams` is a counted, typed
//     fault and fails closed (never the stale target set);
//   * an EMPTY `streams` is that same fault for every published kind (shim c
//     deleted: MC lists every published kind), and no fault for a receive-only
//     participant;
//   * a VIDEO stream missing any encoding field is refused, counted and fails
//     closed; a complete one reaches the video publisher;
//   * a header version this client cannot send is refused and counted ONCE;
//   * the EFFECTIVE per-kind send state feeds R-18.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import {
  DIRECTIVE_KIND_MISSING_FAULT,
  VIDEO_ENCODING_REFUSED_FAULT,
  kindMetrics,
  sendDirectiveListener,
  validateVideoEncoding,
} from '../mediaWiring.js';
import type { VideoSendDirective } from '../../media/lifecycle/VideoPublisher.js';
import type { SendStreamEncoding } from '../../signaling/events.js';
import { MediaMetrics } from '../../media/setup/mediaMetrics.js';
import { HEADER_VERSION } from '../../media/frame/wireConstants.js';
import type { AudioSendDirective, MediaFault } from '../../media/lifecycle/AudioPipeline.js';
import type { SendDirectiveEvent, SendStreamDirective } from '../../signaling/events.js';
import { ServerMuteResumeTracker } from '../serverMuteResume.js';

const MH = 'https://mh-a';

function audio(targets: string[], streamNumber = 0): SendStreamDirective {
  return {
    streamNumber,
    mediaKind: 'audio',
    maxBitrateBps: undefined,
    encoding: undefined,
    targets,
  };
}

function directive(
  streams: SendStreamDirective[],
  headerVersion = HEADER_VERSION,
): SendDirectiveEvent {
  return { streams, headerVersion };
}

const VP8_ENCODING: SendStreamEncoding = {
  codec: 'vp8',
  width: 640,
  height: 360,
  frameRate: 30,
  maxBitrateBps: 800_000,
  minKeyframeIntervalMs: 500,
};

function video(
  targets: string[],
  encoding: SendStreamEncoding | undefined = VP8_ENCODING,
): SendStreamDirective {
  return { streamNumber: 1, mediaKind: 'video', maxBitrateBps: 800_000, encoding, targets };
}

function rig(publish: readonly ('audio' | 'video')[] = ['audio']) {
  const sink = new InMemoryMetricsSink();
  const videoApplied: (VideoSendDirective | undefined)[] = [];
  const warnings: string[] = [];
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
    publish,
    pipeline: { setSendDirective: (d) => applied.push(d) },
    video: { setDirective: (d) => videoApplied.push(d) },
    warn: (m) => warnings.push(m),
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
  return {
    listener,
    applied,
    videoApplied,
    warnings,
    faults,
    effective,
    resumed,
    tracker,
    count,
  };
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
    const videoOnly: SendStreamDirective = {
      streamNumber: 1,
      mediaKind: 'video',
      maxBitrateBps: undefined,
      encoding: undefined,
      targets: [MH],
    };
    r.listener(directive([videoOnly]));
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
      encoding: undefined,
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

  it('an EMPTY streams list is the kind-missing fault for every published kind (shim c deleted), and fails closed', () => {
    const r = rig(['audio', 'video']);
    r.listener(directive([audio([MH]), video([MH])]));
    r.listener(directive([]));
    expect(r.count(KIND_MISSING)).toBe(2);
    expect(r.faults).toEqual([
      { stage: 'config', message: DIRECTIVE_KIND_MISSING_FAULT, fatal: false },
    ]);
    expect(r.applied.at(-1)?.targets).toEqual([]);
    expect(r.videoApplied.at(-1)).toBeUndefined();
    expect(r.effective.slice(-2)).toEqual([
      ['audio', false],
      ['video', false],
    ]);
  });

  it('a RECEIVE-ONLY participant (empty publish set) gets an empty streams list: no fault, no counter', () => {
    const r = rig([]);
    r.listener(directive([]));
    expect(r.count(KIND_MISSING)).toBe(0);
    expect(r.faults).toEqual([]);
    expect(r.applied).toEqual([]);
    expect(r.videoApplied).toEqual([]);
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
      encoding: undefined,
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
  it('returns each published kind its own handle', () => {
    const metrics = new MediaMetrics({ clientVersion: 't', orgId: 'o' }, new InMemoryMetricsSink());
    expect(kindMetrics(metrics, 'audio')).toBe(metrics.audio);
    expect(kindMetrics(metrics, 'video')).toBe(metrics.video);
  });
});

const ENCODING_REFUSED = 'dt_client_media_send_directive_encoding_refused_total';

describe('video send directive (story 3 C-A2)', () => {
  it('a complete VP8 directive reaches the video publisher with the directed values, verbatim', () => {
    const r = rig(['audio', 'video']);
    r.listener(directive([audio([MH]), video([MH])]));
    expect(r.videoApplied).toEqual([
      {
        streamNumber: 1,
        encoding: {
          codec: 'vp8',
          width: 640,
          height: 360,
          frameRateHz: 30,
          maxBitrateBps: 800_000,
          minKeyframeIntervalMs: 500,
        },
        targets: [MH],
        headerVersion: HEADER_VERSION,
      },
    ]);
    expect(r.effective).toEqual([
      ['audio', true],
      ['video', true],
    ]);
    expect(r.count(ENCODING_REFUSED)).toBe(0);
  });

  it('an empty video target set is a withdrawal reaching the publisher (camera off), not a fault', () => {
    const r = rig(['audio', 'video']);
    r.listener(directive([audio([MH]), video([])]));
    expect(r.videoApplied.at(-1)?.targets).toEqual([]);
    expect(r.effective.at(-1)).toEqual(['video', false]);
    expect(r.faults).toEqual([]);
  });

  // One row per required field, each ABSENT and 0 (story 3 R-23: no client
  // default for any video encoding value).
  const rows: [string, SendStreamEncoding | undefined][] = [
    ['no EncodingParameters at all', undefined],
    ['codec unspecified', { ...VP8_ENCODING, codec: null }],
    ['codec not VP8', { ...VP8_ENCODING, codec: 'h264' }],
    ['width 0/absent', { ...VP8_ENCODING, width: 0 }],
    ['height 0/absent', { ...VP8_ENCODING, height: 0 }],
    ['frame_rate 0/absent', { ...VP8_ENCODING, frameRate: 0 }],
    ['max_bitrate_bps 0/absent', { ...VP8_ENCODING, maxBitrateBps: 0 }],
    ['min_keyframe_interval_ms absent', { ...VP8_ENCODING, minKeyframeIntervalMs: undefined }],
    ['min_keyframe_interval_ms 0', { ...VP8_ENCODING, minKeyframeIntervalMs: 0 }],
    // Above the compiled ceiling (MC's MIN_KEYFRAME_INTERVAL_MS_CEILING): a
    // uint32 past a timer's range would otherwise turn a wait into a spin.
    ['min_keyframe_interval_ms 2^31', { ...VP8_ENCODING, minKeyframeIntervalMs: 2 ** 31 }],
    ['min_keyframe_interval_ms 2^32-1', { ...VP8_ENCODING, minKeyframeIntervalMs: 2 ** 32 - 1 }],
  ];
  it.each(rows)('refuses a video stream with %s: counted, typed fault, fails closed', (_n, enc) => {
    const r = rig(['audio', 'video']);
    r.listener(directive([audio([MH]), { ...video([MH]), encoding: enc }]));
    expect(r.count(ENCODING_REFUSED)).toBe(1);
    expect(r.faults).toEqual([
      { stage: 'config', message: VIDEO_ENCODING_REFUSED_FAULT, fatal: false },
    ]);
    expect(r.videoApplied).toEqual([undefined]);
    expect(r.effective.at(-1)).toEqual(['video', false]);
    // Audio is untouched by a video refusal.
    expect(r.effective[0]).toEqual(['audio', true]);
    expect(r.warnings).toHaveLength(1);
    expect(r.warnings[0]).toMatch(/required encoding field [a-z_]+ is missing or invalid/);
  });

  it('validateVideoEncoding names the first missing field from the closed vocabulary', () => {
    expect(validateVideoEncoding({ ...VP8_ENCODING, height: 0 })).toEqual({
      ok: false,
      field: 'height',
    });
    expect(validateVideoEncoding({ ...VP8_ENCODING, minKeyframeIntervalMs: 50 }).ok).toBe(true);
  });

  it('the encoding fault is once per episode; every refusal is counted', () => {
    const r = rig(['audio', 'video']);
    const bad = video([MH], { ...VP8_ENCODING, width: 0 });
    r.listener(directive([audio([MH]), bad]));
    r.listener(directive([audio([MH]), bad]));
    expect(r.count(ENCODING_REFUSED)).toBe(2);
    expect(r.faults).toHaveLength(1);
    // One WARN per episode, not per receipt (MC re-sends directives).
    expect(r.warnings).toHaveLength(1);
    r.listener(directive([audio([MH]), video([MH])]));
    r.listener(directive([audio([MH]), bad]));
    expect(r.faults).toHaveLength(2);
  });
});
