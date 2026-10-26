// File: packages/sdk-core/src/media/pipeline/__tests__/videoFixtures.ts
//
// Shared fixtures for the video receive tests (story 3 C-B). REAL crypto: frames
// are built with the codec and SFrame stack and signed with real Ed25519 keys
// (`frameFixtures.ts`); only the VP8 codec is faked (`@darktower/test-utils`).

import { expect } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import {
  ALL_REJECT_REASONS,
  VIDEO_POST_DECODE_REJECT_REASONS,
  type RejectReason,
} from '../../frame/rejectReason.js';
import { MediaKindMetrics } from '../../setup/mediaMetrics.js';
import { MediaKind } from '../../../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { FrameCounts } from '../frameAccounting.js';
import { buildTestFrame, type TestFrameSpec, type TestIdentity } from './frameFixtures.js';

/** The meeting KEK most fixtures use, at generation 0. */
export const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);
/** The video key-id stream (MC's video send id). */
export const VIDEO_STREAM = 1;

/** A VP8 KEY frame's plaintext declaring `width` x `height` (RFC 6386 §9.1). */
export function vp8Key(width = 640, height = 360, fill = 0xab): Uint8Array {
  const out = new Uint8Array(24).fill(fill);
  out[0] = 0x00; // frame tag bit 0 = 0: key frame
  out[1] = 0;
  out[2] = 0;
  out[3] = 0x9d;
  out[4] = 0x01;
  out[5] = 0x2a;
  out[6] = width & 0xff;
  out[7] = (width >> 8) & 0x3f;
  out[8] = height & 0xff;
  out[9] = (height >> 8) & 0x3f;
  return out;
}

/** A VP8 DELTA frame's plaintext. */
export function vp8Delta(fill = 0xcd): Uint8Array {
  const out = new Uint8Array(16).fill(fill);
  out[0] = 0x01; // not a key frame
  return out;
}

/** What one video frame fixture needs. */
export interface VideoFrameSpec {
  readonly sender: number;
  readonly signer: TestIdentity;
  /** The relay stream id = this subscriber's slot id. */
  readonly slot: number;
  readonly gop: number;
  readonly seq: number;
  readonly key: boolean;
  /** Carries the wrapped transmit key. Default: keyframes do. */
  readonly keyBearing?: boolean;
  readonly generation?: number;
  readonly transmitKey?: Uint8Array;
  readonly kek?: Uint8Array;
  readonly kekGeneration?: number;
  readonly width?: number;
  readonly height?: number;
  readonly plaintext?: Uint8Array;
}

/** Build one signed, encrypted v3 video frame as MH would forward it. */
export function videoFrame(spec: VideoFrameSpec): Promise<Uint8Array> {
  const frameSpec: TestFrameSpec = {
    keyIdSenderId: spec.sender,
    stream: VIDEO_STREAM,
    generation: spec.generation ?? 0,
    streamSequence: spec.seq,
    ...(spec.transmitKey ? { transmitKey: spec.transmitKey } : {}),
    kek: spec.kek ?? KEK,
    kekGeneration: spec.kekGeneration ?? 0,
    signer: spec.signer,
    plaintext: spec.plaintext ?? (spec.key ? vp8Key(spec.width, spec.height) : vp8Delta()),
    streamId: spec.slot,
    hopSequence: spec.seq,
    keyBearing: spec.keyBearing ?? spec.key,
    gopCounter: spec.gop,
    independentlyDecodable: spec.key,
  };
  return buildTestFrame(frameSpec);
}

/** A video metrics handle on an in-memory sink. */
export function videoMetrics(): { metrics: MediaKindMetrics; sink: InMemoryMetricsSink } {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaKindMetrics(
    { clientVersion: 'v', orgId: 'o' },
    MediaKind.VIDEO_CAMERA,
    sink,
  );
  return { metrics, sink };
}

/** The sum of a counter's recorded values, optionally filtered by one label. */
export function counterSum(
  sink: InMemoryMetricsSink,
  name: string,
  filter?: (labels: Readonly<Record<string, string>>) => boolean,
): number {
  return sink
    .getRecordedMetrics()
    .filter((m) => m.name === name && (filter ? filter(m.labels) : true))
    .reduce((sum, m) => sum + m.value, 0);
}

/** Drops on `frames_dropped_total` by reason. */
export function dropsByReason(sink: InMemoryMetricsSink): Map<RejectReason, number> {
  const out = new Map<RejectReason, number>();
  for (const m of sink.getRecordedMetrics()) {
    if (m.name !== 'dt_client_media_frames_dropped_total') continue;
    const reason = m.labels['reason'] as RejectReason;
    out.set(reason, (out.get(reason) ?? 0) + m.value);
  }
  return out;
}

/**
 * THE ACCOUNTING IDENTITY at the parse boundary, from the REASON-LABELLED
 * counter: `received = accepted + sum(drops{reason not in
 * VIDEO_POST_DECODE_REJECT_REASONS})`, cross-checked against the sampled
 * getters. `excluded` is the post-decode set (video); pass `[]` for audio.
 */
export function expectIdentity(
  sink: InMemoryMetricsSink,
  counts: FrameCounts,
  excluded: readonly RejectReason[] = VIDEO_POST_DECODE_REJECT_REASONS,
): void {
  const received = counterSum(sink, 'dt_client_media_frames_received_total');
  const accepted = counterSum(sink, 'dt_client_media_frames_accepted_total');
  let drops = 0;
  let allDrops = 0;
  for (const [reason, n] of dropsByReason(sink)) {
    expect(ALL_REJECT_REASONS).toContain(reason);
    allDrops += n;
    if (!excluded.includes(reason)) drops += n;
  }
  expect(received, 'received = accepted + sum(drops at the parse boundary)').toBe(accepted + drops);
  expect(counts.framesReceived).toBe(received);
  expect(counts.framesAccepted).toBe(accepted);
  expect(counts.framesDropped).toBe(allDrops);
}

/** A manual clock. */
export function manualClock(start = 1_000): {
  now: () => number;
  advance: (ms: number) => void;
  set: (ms: number) => void;
} {
  let t = start;
  return {
    now: () => t,
    advance: (ms) => {
      t += ms;
    },
    set: (ms) => {
      t = ms;
    },
  };
}

/** A readable stream the test feeds chunk by chunk, then ends or resets. */
export function controlledStream(): {
  readonly readable: ReadableStream<Uint8Array>;
  enqueue(bytes: Uint8Array): void;
  close(): void;
  reset(): void;
  readonly cancelled: () => boolean;
} {
  let controller!: ReadableStreamDefaultController<Uint8Array>;
  let cancelled = false;
  const readable = new ReadableStream<Uint8Array>({
    start(c) {
      controller = c;
    },
    cancel() {
      cancelled = true;
    },
  });
  return {
    readable,
    enqueue: (bytes) => controller.enqueue(bytes),
    close: () => controller.close(),
    reset: () => controller.error(new Error('stream reset')),
    cancelled: () => cancelled,
  };
}

/** Let every queued promise continuation run (no wall-clock wait). */
export async function flush(rounds = 20): Promise<void> {
  for (let i = 0; i < rounds; i += 1) await Promise.resolve();
}
