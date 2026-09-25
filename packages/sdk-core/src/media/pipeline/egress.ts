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

/**
 * One media-handler target: its own queue, its own hop sequence, its own buffers.
 *
 * ---------------------------------------------------------------------------
 * WHY PER-TARGET STATE IS EXACTLY THE QUEUE AND THE HOP SEQUENCE
 * ---------------------------------------------------------------------------
 *
 * The frame format draws this line for us, and it draws it between the two
 * REGIONS rather than between destinations:
 *
 *   * PUBLISHER region — `stream_sequence` is per (sender, stream), never reset,
 *     and it is the AEAD NONCE INPUT. There is exactly ONE of it per frame, and
 *     it is allocated upstream of the fan-out in `submit()`.
 *   * RELAY region — `hop_sequence` is "per (connection, media stream) count of
 *     what the transmitter actually sent" and is explicitly unauthenticated and
 *     harmless to reset (`crates/media-protocol/src/frame.rs`, the doc comments
 *     on `stream_sequence` and `hop_sequence`, which close with "do not build a
 *     symmetric counter type for the two").
 *
 * So one counter per lane here is not a convenience: a single hop counter shared
 * across two lanes would make each media handler see every other number missing
 * and read ~50% uplink loss. Three independent encodings of this scope agree —
 * the format doc above, the receive-side `hopSequenceMonitor.ts` (per connection,
 * per stream), and MH's own per-egress-stream state in
 * `crates/mh-service/src/media/forwarder.rs`.
 *
 * ---------------------------------------------------------------------------
 * EACH LANE OWNS ITS BUFFERS EXCLUSIVELY
 * ---------------------------------------------------------------------------
 *
 * `writeHopSequence` stamps IN PLACE at `relayRegionOffset`, and lanes drain
 * concurrently. So the copy is taken at ENQUEUE and each lane holds its own
 * buffer for that buffer's whole life: the race cannot FORM, rather than being
 * avoided by careful sequencing. The single-flight `#drain` guarantee is
 * per-lane and would NOT have covered cross-lane interleaving. The first active
 * lane takes the original, so N-1 copies are made and the N=1 case — every
 * participant in an all-connected meeting — allocates nothing extra.
 */
interface Lane {
  /** Whether the latest send directive still names this target. */
  active: boolean;
  /** This lane's bounded queue. The `maxQueueFrames` bound is PER LANE. */
  readonly queue: BoundedDropOldestQueue<PendingFrame>;
  /** What this transport actually carried. Never reset while the lane lives. */
  hopSequence: number;
  /** Single-flight drain flag, per lane. */
  draining: boolean;
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
  /** Bound on the application egress queue, in frames. PER TARGET. */
  readonly maxQueueFrames: number;
  /** The publisher's stream index from the send directive (8-bit semantics). */
  readonly streamNumber: number;
  /** The relay-region `stream_id` to stamp. Subscriber-scoped; rewritten by MH. */
  readonly streamId: number;
  /**
   * Resolves the datagram channel for a media-handler URL, or `undefined`.
   *
   * A SELECTOR, NEVER A DIALER: it resolves through
   * `MediaTransport.getDatagramChannel`, which only looks up transports
   * `connectAll` already opened. A directed target can pick among those
   * transports; it can never create one.
   */
  readonly senderFor: (mediaHandlerUrl: string) => DatagramSender | undefined;
  /**
   * Called when MC directs this client at a target it has no transport for.
   *
   * Bounded and event-once at the CALLER (the lifecycle layer dedupes by
   * (stage, message)), so a persistent condition cannot become per-frame
   * telemetry. Under the ADR-0036 §9 edge model this is a SERVER-side defect:
   * MC derives a sender's targets from the handlers owning that sender's edges,
   * and an edge exists only where both parties are connected to that handler.
   */
  readonly onTargetNotConnected: () => void;
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
  readonly #maxQueueFrames: number;
  readonly #streamNumber: number;
  readonly #streamId: number;
  readonly #senderFor: (mediaHandlerUrl: string) => DatagramSender | undefined;
  readonly #onTargetNotConnected: () => void;

  /**
   * One lane per media-handler URL this pipeline has ever been directed at.
   *
   * Entries are RETAINED when a target leaves the directive rather than deleted,
   * because `hop_sequence` is per CONNECTION: a target dropped and later
   * re-directed onto the same transport must not restart its count, which any
   * receiver tracking that hop would read as a publisher restart. Bounded by the
   * meeting's registered handler set, which MC froze at the first join.
   */
  readonly #lanes = new Map<string, Lane>();
  /**
   * Datagrams actually put on a transport, summed over lanes (monotone).
   *
   * ---------------------------------------------------------------------------
   * ONE INCREMENT PER LANE, ONE FIELD, FOUR READERS
   * ---------------------------------------------------------------------------
   *
   * This used to BE `#hopSequence` — one value serving as both the wire hop
   * counter and the telemetry count. That collapse was correct only while the
   * two scopes coincided, which they did while there was exactly one target. Now
   * hop is per (connection, stream) and this is per SENDER, so a getter over one
   * lane's hop counter would report a two-handler sender's sends at HALF their
   * true number — and the egress-advances proofs would then watch one lane while
   * the other went unobserved.
   *
   * The discipline the old collapse existed for is unchanged and is why this
   * increment sits on the line ADJACENT to `this.#metrics.frameSent()`: two
   * encodings of "how many datagrams went out" that agree today will drift the
   * first time a drop path lands between the two sites, with the bus field and
   * `dt_client_media_frames_sent_total` disagreeing and nothing failing.
   *
   * UNIT: DATAGRAMS, NOT CAPTURED FRAMES. A sender whose edges span two handlers
   * advances this twice per captured frame. That is deliberate and it is what
   * keeps it usable as the denominator of the send-drop ratio, whose numerator
   * (`dt_client_media_send_dropped_total`) is per send ATTEMPT and therefore
   * per-lane: counting frames here would report 50% loss for a frame that one
   * lane delivered and the other refused. `docs/observability/metrics/client.md`
   * carries the same statement, and the received side has counted datagrams at
   * the wire since story 1.
   */
  #datagramsSent = 0;
  #stopped = false;

  constructor(options: EgressPipelineOptions) {
    this.#metrics = options.metrics;
    this.#transmitKeys = options.transmitKeys;
    this.#kek = options.kek;
    this.#identity = options.identity;
    this.#maxQueueFrames = options.maxQueueFrames;
    this.#streamNumber = options.streamNumber;
    this.#streamId = options.streamId;
    this.#senderFor = options.senderFor;
    this.#onTargetNotConnected = options.onTargetNotConnected;
  }

  /**
   * Datagrams actually sent since construction (monotone).
   *
   * Advances at DEQUEUE, never at enqueue, and never for a frame a transport
   * refused. See `#datagramsSent` for why this is no longer a read of the hop
   * counter, and for the unit.
   *
   * Exposed so an embedder can SAMPLE it. It must never become a per-frame
   * emission: this file is the hot path under ADR-0036 §11's layout constraint,
   * whose per-frame invariant is zero allocation and zero registry lookup.
   *
   * A frame COUNT is exposable where a per-frame SIZE would not be (§11's
   * voice-activity trace) only because the encoder runs with DTX off — see
   * `lifecycle/muteState.ts`. With DTX the frame rate becomes speech-dependent
   * and this, along with the production counter, becomes that trace.
   */
  get framesSent(): number {
    return this.#datagramsSent;
  }

  /**
   * Apply MC's target set for this stream — EVERY handler owning one of this
   * sender's edges (ADR-0036 §9; the wire already carries `repeated SendTarget`).
   *
   * An empty set means SEND NOTHING (§5). Lanes not named go inactive and their
   * queues are discarded; a lane named again later resumes on its retained hop
   * counter.
   */
  setTargets(mediaHandlerUrls: readonly string[]): void {
    if (this.#stopped) return;
    const wanted = new Set(mediaHandlerUrls.filter((url) => url !== ''));
    for (const [url, lane] of this.#lanes) {
      if (wanted.has(url)) continue;
      lane.active = false;
      lane.queue.drain();
    }
    for (const url of wanted) {
      const existing = this.#lanes.get(url);
      if (existing) {
        existing.active = true;
        void this.#drain(url, existing);
        continue;
      }
      const lane: Lane = {
        active: true,
        queue: new BoundedDropOldestQueue<PendingFrame>(this.#maxQueueFrames),
        hopSequence: 0,
        draining: false,
      };
      this.#lanes.set(url, lane);
    }
    this.#reportQueueDepth();
  }

  /** Stop accepting frames and discard anything queued, on every lane. */
  stop(): void {
    this.#stopped = true;
    for (const lane of this.#lanes.values()) {
      lane.active = false;
      lane.queue.drain();
    }
    this.#metrics.sendQueueDepth(0);
  }

  /**
   * Build, sign and queue one encoded audio frame — ONCE, then fan the identical
   * bytes out to every directed target.
   *
   * ---------------------------------------------------------------------------
   * SEAL ONCE, TRANSMIT N. THE NONCE IS ALLOCATED HERE, ABOVE THE FAN-OUT.
   * ---------------------------------------------------------------------------
   *
   * The normative statement is `proto/dark_tower/signaling/v1/signaling.proto`
   * on `SendStream`: "A stream sent to several targets is encoded and encrypted
   * ONCE and transmitted N times", because the alternative "would require either
   * per-destination encryption (nonce reuse — catastrophic) or two counters per
   * frame." Read it there; it is not restated here, because a second wording of
   * a crypto invariant is a second thing to drift.
   *
   * What that means for this method: ONE `materialFor`, ONE
   * `nextStreamSequence`, ONE `sealSframe`, ONE `signFrame`, ONE `finishFrame` —
   * all of it above `#fanOut`. Per-lane state is the queue and the hop sequence
   * ONLY. NO PER-LANE COUNTER MAY EVER REACH `sframeNonce`.
   *
   * THE INVARIANT LIVES ON THE MANAGER, NOT ON THIS CLASS. ADR-0036 §2 fixes the
   * counting scope at per (sender, stream), while `nextStreamSequence` keys its
   * state per MANAGER INSTANCE by stream number. Those two scopes coincide only
   * because exactly one `TransmitKeyManager` exists per sender — so constructing
   * a SECOND manager for the same sender is a nonce repeat under one key no
   * matter how the pipelines above it are arranged, and under AES-GCM a repeat
   * leaks the authentication subkey and permits forgery rather than merely
   * exposing two frames (§2). See `lifecycle/transmitKeys.ts`, where that
   * boundary is stated at the object that carries it.
   *
   * THE REFACTOR THAT BREAKS THIS, NAMED SO IT IS NOT REDISCOVERED: "one
   * `EgressPipeline` per target" is the decomposition this file's own structure
   * suggests, and it is FORBIDDEN. Because `submit()` calls `nextStreamSequence`
   * itself, N pipelines means N sequence allocations for one frame at best, and
   * N managers — nonce reuse — at worst. It is an example of the violation, not
   * the rule: the rule is the manager's scope above.
   *
   * @throws whatever the crypto layer throws. The caller (the lifecycle
   * orchestrator) surfaces it as a typed media error; a build failure is never
   * swallowed, because a frame that silently fails to build is indistinguishable
   * from a muted microphone.
   */
  async submit(frame: EncodedAudioFrame): Promise<void> {
    if (this.#stopped) return;
    // MC directed no targets: "send nothing" (§5). Nothing is built, so no
    // sequence is consumed and no drop is counted — "count the frames we did not
    // send" is the tempting wrong turn, exactly as on the mute path.
    if (!this.#hasActiveTarget()) return;

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
    this.#fanOut(bytes, unsigned.relayRegionOffset);
  }

  /** True when at least one lane is still named by the latest directive. */
  #hasActiveTarget(): boolean {
    for (const lane of this.#lanes.values()) {
      if (lane.active) return true;
    }
    return false;
  }

  /**
   * Hand ONE sealed, signed frame to every active lane.
   *
   * The first active lane with a transport takes the ORIGINAL buffer; each
   * further lane takes its own copy, so no two lanes ever hold the same bytes
   * and the in-place `writeHopSequence` at dequeue needs no coordination. See
   * {@link Lane}.
   */
  #fanOut(bytes: Bytes, relayRegionOffset: number): void {
    let originalTaken = false;
    for (const [url, lane] of this.#lanes) {
      if (!lane.active) continue;
      if (!this.#senderFor(url)) {
        // MC directed us at a handler we hold no transport to. Counted with the
        // token whose documented fleet contract is "reads zero forever", and
        // reported ONCE through the caller's bounded fault path. Deliberately
        // NOT queued: frames aging out of a lane that can never send would be
        // counted as `egress_queue_overflow`, which names the mechanism and
        // actively misdescribes the cause.
        this.#metrics.sendDropped(MEDIA_SEND_DROP_REASONS.NotConnected);
        this.#onTargetNotConnected();
        continue;
      }
      const laneBytes = originalTaken ? (bytes.slice() as Bytes) : bytes;
      originalTaken = true;
      const evicted = lane.queue.push({ bytes: laneBytes, relayRegionOffset });
      if (evicted !== undefined) {
        // The drop MH structurally cannot see, counted where the decision is made.
        this.#metrics.sendDropped(MEDIA_SEND_DROP_REASONS.EgressQueueOverflow);
      }
      void this.#drain(url, lane);
    }
    this.#reportQueueDepth();
  }

  /**
   * Report the DEEPEST lane, not the sum.
   *
   * `maxQueueFrames` bounds each lane independently, so depth-against-bound —
   * which is what a back-pressure reader is asking — is only meaningful per
   * lane. A sum would also make the same audio read as twice the back-pressure
   * purely because a sender spans two handlers.
   */
  #reportQueueDepth(): void {
    let deepest = 0;
    for (const lane of this.#lanes.values()) {
      if (lane.queue.depth > deepest) deepest = lane.queue.depth;
    }
    this.#metrics.sendQueueDepth(deepest);
  }

  /**
   * Send one lane's queued frames until it empties or its transport stalls.
   *
   * Single-flight PER LANE: a second call while that lane is draining returns
   * immediately, so its frames cannot interleave and its hop sequence stays
   * monotonic on its own wire. Lanes drain independently of each other — that is
   * what makes a partial send partial: a stalled, closed or missing transport on
   * one lane neither blocks nor fails the others.
   */
  async #drain(url: string, lane: Lane): Promise<void> {
    if (lane.draining) return;
    lane.draining = true;
    try {
      for (;;) {
        const sender = this.#senderFor(url);
        if (!sender) {
          // Nothing to send on. Frames stay queued and age out through the
          // drop-oldest policy rather than being counted as failures here: a
          // send was never attempted.
          return;
        }
        const pending = lane.queue.shift();
        if (pending === undefined) return;
        this.#reportQueueDepth();

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

        // Assigned HERE, at dequeue, so it counts only frames actually sent on
        // THIS lane's transport. Safe in place: this buffer belongs to this lane
        // alone (see `Lane`).
        writeHopSequence(pending.bytes, pending.relayRegionOffset, lane.hopSequence);
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
        // THREE INCREMENTS, TWO SCOPES, ONE SITE. The lane's hop counter is WIRE
        // scope (per connection, per stream); `#datagramsSent` and
        // `dt_client_media_frames_sent_total` are SENDER scope. They were one
        // value until multi-target send existed, and splitting them is what keeps
        // a two-handler sender from reporting half its sends — see
        // `#datagramsSent`. They stay on adjacent lines for the reason the old
        // collapse existed: two encodings of "how many datagrams went out" drift
        // the first time a drop path lands between the sites, with the bus field
        // and the production counter disagreeing and nothing failing.
        // All three are AFTER a successful `send()`: every branch above
        // `continue`s, so none counts an attempted-but-refused send.
        lane.hopSequence += 1;
        this.#datagramsSent += 1;
        this.#metrics.frameSent();
      }
    } finally {
      lane.draining = false;
    }
  }
}
