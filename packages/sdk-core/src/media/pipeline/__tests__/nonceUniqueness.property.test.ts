// File: packages/sdk-core/src/media/pipeline/__tests__/nonceUniqueness.property.test.ts
//
// Story 3 R-14 / ADR-0036 §2, §4: a NONCE-UNIQUENESS PROPERTY across audio AND
// video sharing the session's ONE TransmitKeyManager, under randomized
// interleavings of frames, group starts, T rotations and KEK-style rotations.
//
// The AEAD nonce is derived from (transmit key, stream sequence), and the
// transmit key is bound to its packed key id `sender | stream | generation`.
// So the property, stated in wire terms, is: no two frames ever sent carry the
// same (key id, stream sequence). Deterministic: a fixed seed list, the seed
// printed in every failure message — no unseeded randomness.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink, MockWebTransport } from '@darktower/test-utils';

import { DEFAULT_CLIENT_CONFIG } from '../../../config/clientConfig.js';
import { HEADER_VERSION } from '../../frame/wireConstants.js';
import { decodeFrame } from '../../frame/frameCodec.js';
import { bytesToHex } from '../../frame/hex.js';
import { AES_256_KEY_BYTES, parseSframe } from '../../frame/sframe.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { MediaKindMetrics } from '../../setup/mediaMetrics.js';
import { MediaKind } from '../../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { TransmitKeyManager } from '../../lifecycle/transmitKeys.js';
import { EgressPipeline, type DatagramSender } from '../egress.js';
import { VideoEgress } from '../videoEgress.js';

const SEEDS = [1, 7, 42, 1337, 0xc0ffee, 20261022];
const MH = 'https://mh-0.example:4433';

/** mulberry32: a tiny deterministic PRNG. */
function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 20; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

describe('nonce uniqueness across audio and video (one manager per sender)', () => {
  it.each(SEEDS)('seed %i: no (key id, stream sequence) pair is ever sent twice', async (seed) => {
    const rand = prng(seed);
    const keys = new TransmitKeyManager(258);
    const kek = kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(0x44), 0);
    const identity = await MeetingIdentity.create();
    const sink = new InMemoryMetricsSink();
    const audioMetrics = new MediaKindMetrics(
      { clientVersion: 'v', orgId: 'o' },
      MediaKind.AUDIO,
      sink,
    );
    const videoMetrics = new MediaKindMetrics(
      { clientVersion: 'v', orgId: 'o' },
      MediaKind.VIDEO_CAMERA,
      sink,
    );
    const datagrams: Uint8Array[] = [];
    const sender: DatagramSender = {
      isOpen: true,
      maxDatagramSize: undefined,
      send: async (bytes) => {
        datagrams.push(new Uint8Array(bytes));
      },
    };
    const audio = new EgressPipeline({
      metrics: audioMetrics,
      transmitKeys: keys,
      kek,
      identity,
      maxQueueFrames: DEFAULT_CLIENT_CONFIG.media.egress.maxQueueFrames,
      streamNumber: 0,
      streamId: 0,
      senderFor: () => sender,
      onTargetNotConnected: () => undefined,
    });
    audio.setTargets({ targets: [MH], headerVersion: HEADER_VERSION });
    const mock = new MockWebTransport();
    const video = new VideoEgress({
      metrics: videoMetrics,
      transmitKeys: keys,
      kek,
      identity,
      streamNumber: 1,
      streamId: 1,
      keyBearingFrames: 3,
      maxBacklogFrames: 1_000,
      maxBacklogBytes: 10_000_000,
      openStream: () => mock.createUnidirectionalStream(),
      onSelfAbandoned: () => undefined,
      onKeyChunk: () => undefined,
      onTargetNotConnected: () => undefined,
      onBuildFailed: (err) => {
        throw err;
      },
    });
    video.setTargets({ targets: [MH], headerVersion: HEADER_VERSION });

    const inFlight: Promise<void>[] = [];
    let ts = 0;
    let sentVideo = 0;
    for (let step = 0; step < 120; step += 1) {
      const roll = rand();
      ts += 1;
      if (roll < 0.4) {
        // Audio is fire-and-forget and concurrent: the race the manager's
        // single-flight mint exists for.
        inFlight.push(audio.submit({ data: Uint8Array.of(step & 0xff), timestampUs: ts }));
      } else if (roll < 0.8) {
        const key = sentVideo === 0 || rand() < 0.15;
        sentVideo += 1;
        inFlight.push(
          video.submit({ data: Uint8Array.of(step & 0xff, 1), timestampUs: ts, key }, video.epoch),
        );
      } else if (roll < 0.9) {
        // A T or KEK receipt, landing anywhere.
        keys.rotate();
      } else if (roll < 0.95) {
        // Video withdraw / mute (groups end) and resume with a key: the stream
        // sequence must keep counting across it.
        video.endGroups();
        video.setTargets({ targets: [], headerVersion: HEADER_VERSION });
        video.setTargets({ targets: [MH], headerVersion: HEADER_VERSION });
        inFlight.push(
          video.submit(
            { data: Uint8Array.of(step & 0xff, 2), timestampUs: ts, key: true },
            video.epoch,
          ),
        );
      } else {
        // Audio withdraw and resume.
        audio.setTargets({ targets: [], headerVersion: HEADER_VERSION });
        audio.setTargets({ targets: [MH], headerVersion: HEADER_VERSION });
      }
      if (rand() < 0.2) await Promise.resolve();
    }
    await Promise.all(inFlight);
    await settle();

    const frames = [...datagrams, ...mock.getOutboundUniStreams().flatMap((s) => s.chunks)].map(
      decodeFrame,
    );
    expect(frames.length, `seed ${seed}: positive control`).toBeGreaterThan(40);
    const pairs = frames.map(
      (f) => `${bytesToHex(parseSframe(f.payload).keyId)}#${f.streamSequence}`,
    );
    const duplicates = pairs.filter((p, i) => pairs.indexOf(p) !== i);
    expect(duplicates, `seed ${seed}: a (key id, sequence) pair repeated`).toEqual([]);
    // Each stream's sequence is strictly per stream and never reused.
    for (const stream of [0, 1]) {
      const seqs = frames.filter((f) => f.streamId === stream).map((f) => f.streamSequence);
      expect(new Set(seqs).size, `seed ${seed}: stream ${stream} reused a sequence`).toBe(
        seqs.length,
      );
    }
  });
});
