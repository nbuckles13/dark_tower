// File: packages/sdk-core/src/session/__tests__/mhControlSink.test.ts
//
// Story 3 R-11 / security S6: an MH `KeyframeRequest` reaches the video
// keyframe gate ONLY when it names the directed stream number of the PUBLISHED
// video stream; everything else is refused and counted BEFORE any gate state is
// touched. Only the named stream keys.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { mhControlSink } from '../mediaWiring.js';
import { MediaMetrics } from '../../media/setup/mediaMetrics.js';
import type { PublishKind, SendDirectiveEvent } from '../../signaling/events.js';

const MH = 'https://mh-a';

function rig(publish: PublishKind[], withVideo = true) {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 't', orgId: 'o' }, sink);
  const routed: [string, number][] = [];
  const directive: SendDirectiveEvent = {
    headerVersion: 3,
    streams: [
      {
        streamNumber: 0,
        mediaKind: 'audio',
        maxBitrateBps: 32_000,
        encoding: undefined,
        targets: [MH],
      },
      {
        streamNumber: 1,
        mediaKind: 'video',
        maxBitrateBps: 800_000,
        encoding: undefined,
        targets: [MH],
      },
    ],
  };
  const control = mhControlSink({
    metrics,
    publish,
    latestDirective: () => directive,
    video: () =>
      withVideo
        ? {
            keyframeRequest: (url: string, gop: number) => {
              routed.push([url, gop]);
              return 'honoured' as const;
            },
          }
        : undefined,
  });
  const count = (name: string, labels: Record<string, string> = {}): number =>
    sink
      .getRecordedMetrics()
      .filter((m) => m.name === name && Object.entries(labels).every(([k, v]) => m.labels[k] === v))
      .reduce((n, m) => n + m.value, 0);
  return { control, routed, count };
}

describe('mhControlSink', () => {
  it('routes a request for the directed VIDEO stream to the gate with the CONNECTION url', () => {
    const r = rig(['audio', 'video']);
    r.control.keyframeRequest(MH, { streamNumber: 1, gopCounter: 4 });
    expect(r.routed).toEqual([[MH, 4]]);
  });

  it('a request naming the AUDIO stream is refused not_sending on the audio handle; nothing keys', () => {
    const r = rig(['audio', 'video']);
    r.control.keyframeRequest(MH, { streamNumber: 0, gopCounter: 4 });
    expect(r.routed).toEqual([]);
    expect(
      r.count('dt_client_media_keyframe_requests_received_total', {
        media_kind: 'audio',
        outcome: 'not_sending',
      }),
    ).toBe(1);
  });

  it('an unknown stream number is refused kind-free; nothing keys', () => {
    const r = rig(['audio', 'video']);
    r.control.keyframeRequest(MH, { streamNumber: 9, gopCounter: 4 });
    expect(r.routed).toEqual([]);
    expect(
      r.count('dt_client_media_keyframe_requests_unattributed_total', { outcome: 'not_sending' }),
    ).toBe(1);
  });

  it('malformed input is counted kind-free and never reaches the gate', () => {
    const r = rig(['audio', 'video']);
    r.control.malformed(MH);
    expect(r.routed).toEqual([]);
    expect(
      r.count('dt_client_media_keyframe_requests_unattributed_total', { outcome: 'malformed' }),
    ).toBe(1);
  });

  it('a video request when video is NOT published is refused kind-free', () => {
    const r = rig(['audio'], false);
    r.control.keyframeRequest(MH, { streamNumber: 1, gopCounter: 0 });
    expect(r.routed).toEqual([]);
    expect(
      r.count('dt_client_media_keyframe_requests_unattributed_total', { outcome: 'not_sending' }),
    ).toBe(1);
  });

  it('a video request before media started (no publisher yet) is refused not_sending on the video handle', () => {
    const r = rig(['audio', 'video'], false);
    r.control.keyframeRequest(MH, { streamNumber: 1, gopCounter: 0 });
    expect(
      r.count('dt_client_media_keyframe_requests_received_total', {
        media_kind: 'video',
        outcome: 'not_sending',
      }),
    ).toBe(1);
  });
});
