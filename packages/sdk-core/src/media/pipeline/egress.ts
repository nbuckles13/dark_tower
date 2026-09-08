// File: packages/sdk-core/src/media/pipeline/egress.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction. Metric handles arrive pre-bound from `../setup/mediaMetrics.ts`.
//
// One encoded Opus frame -> one signed, encrypted v2 frame -> one QUIC datagram.
//
// ---------------------------------------------------------------------------
// THE ORDER IS FIXED BY ADR-0036 §4 AND IS NOT NEGOTIABLE
// ---------------------------------------------------------------------------
//
//   1. populate the FULL publisher region                (`buildPublisherRegion`)
//   2. generate or rotate the transmit key, wrap it under the meeting KEK
//   3. SFrame-encrypt with the AAD equal to the publisher-region slice
//   4. Ed25519-sign over publisher region ‖ payload
//   5. enqueue; assign the hop sequence at DEQUEUE; send as one datagram
//
// Steps 1 and 3 look circular — the region carries `payload_length`, and the
// payload is the sealed output — and are not: AES-GCM is length-preserving, so
// the sealed SFrame object's length (`key_id(8) + tag(16) + plaintext`) is known
// before sealing. The region is therefore built first, sealed under, and then
// handed to `buildUnsignedFrame`, which rebuilds it through the SAME function.
// One writer of header bytes; the AAD a sender seals under is byte-identical to
// the region the frame ships with by construction rather than by comparison.
//
// Signing is LAST, over the final bytes. The key-bearing flag and the presence
// of the wrap cannot disagree: `buildPublisherRegion` throws if they do, and
// audio sets the flag on EVERY frame (§4's cadence — audio switching at MH is
// instantaneous, so a receiver's first frame from a newly selected speaker must
// be decryptable with no wait at all).
//
// ---------------------------------------------------------------------------
// MUTE IS ENFORCED AT CAPTURE, NOT HERE
// ---------------------------------------------------------------------------
//
// While client-muted, no `AudioData` reaches the encoder, so no encoded frame
// reaches this module. That is why there is no `muted` check in `submit()` and
// no `muted` send-drop reason: nothing is dropped, because nothing was produced.
// "Count the frames we did not send" is the tempting wrong turn and it would
// spike the send-drop rate on every mute.

import {
  buildPublisherRegion,
  buildUnsignedFrame,
  finishFrame,
  writeHopSequence,
} from '../frame/frameCodec.js';
import { signFrame } from '../frame/ed25519.js';
import { sealSframe, serializeSframe, sframeObjectLength } from '../frame/sframe.js';
import { deriveSframeKeys, sframeNonce } from '../frame/sframeKeySchedule.js';
import { AES_256_KEY_BYTES, SFRAME_CIPHER_SUITE_ID, SFRAME_SALT_BYTES } from '../frame/sframe.js';
import type { Bytes } from '../frame/hex.js';
import type { KekForWrapping, TransmitKeyManager } from '../lifecycle/transmitKeys.js';
import type { MeetingIdentity } from '../setup/identity.js';
import { MEDIA_SEND_DROP_REASONS, type MediaMetrics } from '../setup/mediaMetrics.js';
import type { EncodedAudioFrame } from '../setup/seams.js';
import { BoundedDropOldestQueue } from './egressQueue.js';

/**
 * Thrown when a frame reaches the signing step after the meeting identity has
 * been released.
 *
 * Its own type rather than a bare `Error` so the lifecycle layer can tell a
 * teardown race from a genuine crypto fault: the first is expected at the tail
 * of a session, the second is not.
 */
export class EgressIdentityReleasedError extends Error {
  constructor() {
    super('the meeting identity was released before this frame could be signed');
    this.name = 'EgressIdentityReleasedError';
  }
}

/** A frame built and signed, waiting for its turn on the wire. */
interface PendingFrame {
  /** Complete wire bytes except the hop sequence, which is written at dequeue. */
  readonly bytes: Bytes;
  /** Offset of the relay region within `bytes`. */
  readonly relayRegionOffset: number;
}

/** Where a built frame is sent. One per media-handler target. */
export interface DatagramSender {
  /**
   * Send one datagram, resolving when the transport has accepted it.
   *
   * Implementations await the writer's `ready` first, so at most one write is in
   * flight and queue depth lives in OUR queue rather than the user agent's.
   */
  send(bytes: Uint8Array): Promise<void>;
  /** Largest datagram this transport will accept, if the platform reports one. */
  readonly maxDatagramSize: number | undefined;
  /** False once the transport has closed. */
  readonly isOpen: boolean;
}

/** Construction options for {@link EgressPipeline}. */
export interface EgressPipelineOptions {
  readonly metrics: MediaMetrics;
  readonly transmitKeys: TransmitKeyManager;
  readonly kek: KekForWrapping;
  /**
   * The meeting identity, passed as the HOLDER rather than as a bare signing
   * capability.
   *
   * The distinction is a lifetime one, not a style one. Holding an independent
   * `CryptoKey` reference here would let this pipeline keep signing after
   * `MeetingIdentity.clear()` had released it at teardown — the capability would
   * outlive the object that owns it, which is the half of ADR-0028 §5's "clear
   * all references AND overwrite key buffers" that a copy silently defeats.
   * Reading `identity.signer` at the point of use means teardown revokes it.
   *
   * It is also the shape every other piece of media key material already uses:
   * the meeting KEK arrives as `MeetingKekSource`, peer keys as
   * `RosterIdentityKeys`. The identity was the one still passed as a raw handle
   * through two layers.
   */
  readonly identity: MeetingIdentity;
  /** Bound on the application egress queue, in frames. */
  readonly maxQueueFrames: number;
  /** The publisher's stream index from the send directive (8-bit semantics). */
  readonly streamNumber: number;
  /** The relay-region `stream_id` to stamp. Subscriber-scoped; rewritten by MH. */
  readonly streamId: number;
}

/**
 * Builds, queues and sends encrypted audio frames.
 *
 * `submit()` is fire-and-forget from the encoder's output callback: it returns
 * once the frame is built and queued, never once it is on the wire, so a stalled
 * transport applies back-pressure to the QUEUE rather than to the encoder.
 */
export class EgressPipeline {
  readonly #metrics: MediaMetrics;
  readonly #transmitKeys: TransmitKeyManager;
  readonly #kek: KekForWrapping;
  readonly #identity: MeetingIdentity;
  readonly #queue: BoundedDropOldestQueue<PendingFrame>;
  readonly #streamNumber: number;
  readonly #streamId: number;

  #sender: DatagramSender | undefined;
  /**
   * Counts only frames ACTUALLY SENT (ADR-0036 §2 / R-8), which is why it
   * advances at dequeue and never at enqueue. See `writeHopSequence`'s header
   * for the two-service consequence of getting this backwards.
   */
  #hopSequence = 0;
  #draining = false;
  #stopped = false;

  constructor(options: EgressPipelineOptions) {
    this.#metrics = options.metrics;
    this.#transmitKeys = options.transmitKeys;
    this.#kek = options.kek;
    this.#identity = options.identity;
    this.#queue = new BoundedDropOldestQueue<PendingFrame>(options.maxQueueFrames);
    this.#streamNumber = options.streamNumber;
    this.#streamId = options.streamId;
  }

  /** Attach (or detach, with `undefined`) the transport this pipeline sends on. */
  setSender(sender: DatagramSender | undefined): void {
    this.#sender = sender;
    if (sender) void this.#drain();
  }

  /** Stop accepting frames and discard anything queued. */
  stop(): void {
    this.#stopped = true;
    this.#queue.drain();
    this.#metrics.sendQueueDepth(0);
  }

  /**
   * Build, sign and queue one encoded audio frame.
   *
   * @throws whatever the crypto layer throws. The caller (the lifecycle
   * orchestrator) surfaces it as a typed media error; a build failure is never
   * swallowed, because a frame that silently fails to build is indistinguishable
   * from a muted microphone.
   */
  async submit(frame: EncodedAudioFrame): Promise<void> {
    if (this.#stopped) return;

    const material = await this.#transmitKeys.materialFor(this.#streamNumber, this.#kek);
    const streamSequence = this.#transmitKeys.nextStreamSequence(this.#streamNumber);

    const flags = {
      // Every Opus frame stands alone.
      independentlyDecodable: true,
      // Audio is never discardable: there is no dependent frame to protect by
      // dropping this one, so the flag would only invite a relay to shed audio.
      discardable: false,
      // EVERY audio frame carries the key (§4's cadence). Not a policy choice
      // made here: `buildPublisherRegion` throws if the flag and the wrap
      // disagree, so the two cannot drift apart.
      keyBearing: true,
    } as const;

    // GCM is length-preserving, so the sealed object's length is known now.
    // Computed by the SFrame module from the same constants its serializer uses,
    // never by arithmetic here — a second derivation of this length is a second
    // wire format that agrees only by review.
    const sealedLength = sframeObjectLength(frame.data.length);
    const publisherRegion = buildPublisherRegion(
      {
        flags,
        streamSequence,
        wrappedTransmitKey: material.wrapped,
        extensions: [],
      },
      sealedLength,
    );

    const derived = await deriveSframeKeys({
      hash: 'SHA-512',
      baseKey: material.key,
      // The packed key id AS IT WILL APPEAR ON THE WIRE, never a value re-packed
      // from decomposed fields — a re-pack always succeeds and is merely wrong.
      kid: material.keyId,
      cipherSuiteId: SFRAME_CIPHER_SUITE_ID,
      keyBytes: AES_256_KEY_BYTES,
      saltBytes: SFRAME_SALT_BYTES,
    });

    const sframe = await sealSframe({
      key: derived.key,
      nonce: sframeNonce(derived.salt, BigInt(streamSequence)),
      // THE PUBLISHER REGION, and only the publisher region. The relay region is
      // excluded because MH rewrites it.
      aad: publisherRegion,
      plaintext: frame.data,
      keyId: material.keyId,
    });
    const payload = serializeSframe(sframe);

    const unsigned = buildUnsignedFrame({
      flags,
      streamSequence,
      wrappedTransmitKey: material.wrapped,
      extensions: [],
      streamId: this.#streamId,
      // Placeholder: overwritten at dequeue so the hop sequence counts only
      // frames actually sent.
      hopSequence: 0,
      payload,
    });
    // Read at the point of use, so a released identity cannot be signed with.
    const signer = this.#identity.signer;
    if (!signer) {
      // Fail loudly rather than emitting anything. Reaching here means a frame
      // was still being built when the session tore down; there is no
      // degradation available, because this SDK does not send unsigned frames.
      throw new EgressIdentityReleasedError();
    }
    const signature = await signFrame(signer, unsigned.signedRange);
    const bytes = finishFrame(unsigned, signature);

    if (this.#stopped) return;
    const evicted = this.#queue.push({ bytes, relayRegionOffset: unsigned.relayRegionOffset });
    if (evicted !== undefined) {
      // The drop MH structurally cannot see, counted where the decision is made.
      this.#metrics.sendDropped(MEDIA_SEND_DROP_REASONS.EgressQueueOverflow);
    }
    this.#metrics.sendQueueDepth(this.#queue.depth);
    void this.#drain();
  }

  /**
   * Send queued frames until the queue empties or the transport stalls.
   *
   * Single-flight: a second call while draining returns immediately, so frames
   * cannot interleave and the hop sequence stays monotonic on the wire.
   */
  async #drain(): Promise<void> {
    if (this.#draining) return;
    this.#draining = true;
    try {
      for (;;) {
        const sender = this.#sender;
        if (!sender) {
          // Nothing to send on. Frames stay queued and age out through the
          // drop-oldest policy rather than being counted as failures here: a
          // send was never attempted.
          return;
        }
        const pending = this.#queue.shift();
        if (pending === undefined) return;
        this.#metrics.sendQueueDepth(this.#queue.depth);

        if (!sender.isOpen) {
          this.#metrics.sendDropped(MEDIA_SEND_DROP_REASONS.ConnectionClosed);
          continue;
        }
        const max = sender.maxDatagramSize;
        if (max !== undefined && pending.bytes.length > max) {
          // Counted SEPARATELY from `transport_send_refused`, whose fleet
          // contract is "reads zero forever". Without this token a raised
          // bitrate ceiling would present as a transport fault and poison an
          // invariant counter — every audio frame is key-bearing, so each
          // carries the wrapped-key block and the signature on top of the Opus
          // payload.
          this.#metrics.sendDropped(MEDIA_SEND_DROP_REASONS.OversizeDatagram);
          continue;
        }

        // Assigned HERE, at dequeue, so it counts only frames actually sent.
        writeHopSequence(pending.bytes, pending.relayRegionOffset, this.#hopSequence);
        try {
          await sender.send(pending.bytes);
        } catch {
          // The transport refused a datagram it should have accepted. The cause
          // is deliberately not retained: it is a platform string on a path
          // where `rejectReason.ts`'s no-key-material rule applies, and the
          // bounded token is what an operator acts on.
          this.#metrics.sendDropped(MEDIA_SEND_DROP_REASONS.TransportSendRefused);
          continue;
        }
        this.#hopSequence += 1;
        this.#metrics.frameSent();
      }
    } finally {
      this.#draining = false;
    }
  }
}
