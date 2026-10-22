// File: packages/sdk-core/src/media/pipeline/videoEgress.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction. Metric handles arrive pre-bound from `../setup/mediaMetrics.ts`.
// Inside the ADR-0036 §11 directory-scoped deny (`media/__tests__/hotPathLayout.test.ts`).
//
// One encoded VP8 frame -> one signed, encrypted v3 frame -> written back to
// back on ONE unidirectional QUIC stream per GROUP per TARGET (story 3 R-12,
// R-20, wire freeze: `TRANSPORT_MODE_STREAM_PER_GROUP`).
//
// ---------------------------------------------------------------------------
// SEAL ONCE, FAN OUT N — the same rule as audio (`./egress.ts`)
// ---------------------------------------------------------------------------
//
// ONE `materialFor`, ONE `nextStreamSequence`, ONE seal, ONE sign per frame
// (`./frameSeal.ts`), above the per-target fan-out; identical bytes go to every
// target. Per-target state is the group stream, its capped backlog and the hop
// sequence ONLY. The `TransmitKeyManager` is the SESSION's single instance (one
// per sender — the nonce boundary; see `lifecycle/transmitKeys.ts`).
//
// SUBMITS ARE SERIALISED. A group stream carries frames in encode order, so —
// unlike audio datagrams, which tolerate reordering — two concurrently sealing
// frames must not finish out of order. Every submit joins one promise chain
// that awaits ONLY seal/sign; the per-target enqueue is synchronous and no link
// ever awaits a target's write, so a slow target can never back-pressure the
// chain, the encoder, another target or audio.
//
// ---------------------------------------------------------------------------
// GROUPS
// ---------------------------------------------------------------------------
//
// A KEY CHUNK (requested or not) starts a group on EVERY active target: the
// session generation rotates (§4 / R-14: every group start), the GoP counter
// advances through the one owner (`TransmitKeyManager.nextGopCounter`, never a
// local counter), each target's previous group stream is FINISHED and a new one
// opened with the keyframe first. A DELTA chunk goes only to targets whose
// group is in progress; a target with no group in progress receives nothing
// until the next key chunk.
//
// WHOEVER DECIDES AN ABANDONMENT REPAIRS IT (amendment 4):
//   * a target whose backlog crosses the cap on an OPEN group stream is RESET
//     (never finished — a half-written group never looks complete), set to
//     `awaiting_group`, counted, and reported via `onSelfAbandoned` so the
//     lifecycle keys a new group (bounded by the keyframe gate);
//   * a target MH stops or refuses (STOP_SENDING, any write rejection, a
//     rejected open) — or whose stream open never completed before its backlog
//     overflowed (MH's open-rate / MAX_STREAMS decision) — is `mh_stopped`: no
//     re-key, NOT counted as awaiting a group, and it rejoins at the next group
//     started for any other reason.
//
// HOP SEQUENCE (ADR-0036 §2: what the transmitter actually SENT) is per
// (target, stream), written into the frame when it is WRITTEN, never at
// enqueue; a backlog drop consumes no number; the counter lives on the target's
// lane, outside any QUIC stream object, so it continues across groups.

import { isSupportedHeaderVersion, writeHopSequence } from '../frame/frameCodec.js';
import { unpackKeyId } from '../frame/keyId.js';
import type { Bytes } from '../frame/hex.js';
import type { TransmitKeyManager } from '../lifecycle/transmitKeys.js';
import type { KekWrapSource } from '../setup/kekSource.js';
import type { MeetingIdentity } from '../setup/identity.js';
import {
  MEDIA_SEND_DROP_REASONS,
  MEDIA_SEND_GROUP_STOP_OUTCOMES,
  type MediaKindMetrics,
  type MediaSendGroupStopOutcome,
} from '../setup/mediaMetrics.js';
import type { EncodedVideoFrame } from '../setup/seams.js';
import type { EgressSendPlan } from './egress.js';
import { sealAndSignFrame } from './frameSeal.js';

/** Opens a group stream to a connected handler, or `undefined` (a selector, never a dialer). */
export type GroupStreamOpener = (
  mediaHandlerUrl: string,
) => Promise<WritableStream<Uint8Array>> | undefined;

/** Where one target stands with respect to the current group. */
export type VideoLaneState = 'awaiting_group' | 'in_group' | 'mh_stopped';

/** One sealed frame waiting in a target's backlog. Each target owns its buffer. */
interface PendingFrame {
  readonly bytes: Bytes;
  readonly relayRegionOffset: number;
}

/** One group stream toward one target. */
interface Group {
  writer: WritableStreamDefaultWriter<Uint8Array> | undefined;
  /** The open has not completed yet. */
  opening: boolean;
  readonly queue: PendingFrame[];
  bytes: number;
  draining: boolean;
  /** Finish (FIN) once the in-flight write settles. */
  finishRequested: boolean;
  /** Reset, finished or failed: write nothing more. */
  dead: boolean;
}

/** One media-handler target. RETAINED across directives, so the hop count persists. */
interface Lane {
  active: boolean;
  state: VideoLaneState;
  /** Frames this target actually received on this stream. Never reset while the lane lives. */
  hopSequence: number;
  group: Group | undefined;
}

/** Construction options for {@link VideoEgress}. */
export interface VideoEgressOptions {
  readonly metrics: MediaKindMetrics;
  /** The SESSION's single transmit-key manager. */
  readonly transmitKeys: TransmitKeyManager;
  readonly kek: KekWrapSource;
  readonly identity: MeetingIdentity;
  /** MC's send id for this stream (the key id's stream field). */
  readonly streamNumber: number;
  /** The uplink relay `stream_id`, from `./sendId.ts:uplinkRelayStreamId`. */
  readonly streamId: number;
  /** N: frames after ANY key change that carry the wrap (`clientConfig`). */
  readonly keyBearingFrames: number;
  /** Per-target backlog caps (`clientConfig`). */
  readonly maxBacklogFrames: number;
  readonly maxBacklogBytes: number;
  readonly openStream: GroupStreamOpener;
  /** A target was abandoned BY THE PUBLISHER (backlog cap on an open group). */
  readonly onSelfAbandoned: () => void;
  /** Every key chunk actually emitted, solicited or not — the gate stamps its clock. */
  readonly onKeyChunk: () => void;
  /** MC directed a target this client holds no transport to (event-once at the caller). */
  readonly onTargetNotConnected: () => void;
  /** A frame could not be sealed or signed; the stream has stopped. */
  readonly onBuildFailed: (err: unknown) => void;
}

/** Builds and sends one video stream. See the module header. */
export class VideoEgress {
  readonly #o: VideoEgressOptions;
  readonly #lanes = new Map<string, Lane>();
  #tail: Promise<void> = Promise.resolve();
  #headerVersion: number | undefined;
  #stopped = false;
  #exhausted = false;
  /** Bumped on every group teardown edge; output encoded under an older epoch is dropped. */
  #epoch = 0;
  /** The GoP counter of the group in progress, or undefined before the first. */
  #currentGop: number | undefined;
  /** Non-secret key-change detector: the generation of the last key used (never the material). */
  #lastGeneration: bigint | undefined;
  #framesSinceKeyChange = 0;
  #framesSent = 0;

  constructor(options: VideoEgressOptions) {
    this.#o = options;
  }

  /** The current output epoch. The lifecycle captures it at ENCODE time and passes it to {@link submit}. */
  get epoch(): number {
    return this.#epoch;
  }

  /**
   * Settles once every submit already chained has finished building (seal and
   * sign). Teardown awaits it so the session's key `clear()` races no seal.
   */
  get drained(): Promise<void> {
    return this.#tail;
  }

  /** Frames actually written, summed over targets (monotone). Sampled, never per frame. */
  get framesSent(): number {
    return this.#framesSent;
  }

  /** Active targets with no group in progress that the PUBLISHER owes a group (never `mh_stopped`). */
  targetsAwaitingGroup(): number {
    let n = 0;
    for (const lane of this.#lanes.values()) {
      if (lane.active && lane.state === 'awaiting_group') n += 1;
    }
    return n;
  }

  /** The lane state of `url`, for assertion. */
  laneState(url: string): VideoLaneState | undefined {
    const lane = this.#lanes.get(url);
    return lane?.active ? lane.state : undefined;
  }

  /** Whether any target is directed. */
  get hasActiveTarget(): boolean {
    for (const lane of this.#lanes.values()) if (lane.active) return true;
    return false;
  }

  /**
   * Apply MC's target set. EMPTY MEANS SEND NOTHING (§5). A newly named target
   * starts `awaiting_group`; a dropped target's group is finished and the lane
   * retained (hop count persists).
   *
   * @throws {RangeError} for a non-empty plan in a header version this codec cannot send.
   */
  setTargets(plan: EgressSendPlan): void {
    if (this.#stopped) return;
    const wanted = new Set(plan.targets.filter((url) => url !== ''));
    if (wanted.size > 0) {
      if (!isSupportedHeaderVersion(plan.headerVersion)) {
        throw new RangeError('VideoEgress: the directed frame header version is not sendable');
      }
      this.#headerVersion = plan.headerVersion;
    }
    for (const [url, lane] of this.#lanes) {
      if (wanted.has(url) || !lane.active) continue;
      lane.active = false;
      this.#finishGroup(lane);
    }
    for (const url of wanted) {
      const lane = this.#lanes.get(url);
      if (lane) {
        if (!lane.active) {
          lane.active = true;
          lane.state = 'awaiting_group';
        }
        continue;
      }
      this.#lanes.set(url, {
        active: true,
        state: 'awaiting_group',
        hopSequence: 0,
        group: undefined,
      });
    }
  }

  /**
   * End every group (client mute or a withdrawal): finish each stream after its
   * in-flight write, discard unsent backlog, and drop any output still in the
   * encoder (the epoch advances). Every active target then awaits a group.
   */
  endGroups(): void {
    this.#epoch += 1;
    for (const lane of this.#lanes.values()) {
      this.#finishGroup(lane);
      if (lane.active) lane.state = 'awaiting_group';
    }
  }

  /** Stop for good: nothing more is built or written. */
  stop(): void {
    this.#stopped = true;
    this.#epoch += 1;
    for (const lane of this.#lanes.values()) {
      lane.active = false;
      this.#finishGroup(lane);
    }
  }

  /**
   * Seal and send one encoded frame. Fire-and-forget from the encoder output;
   * returns the chain link (tests await it). Never rejects: a build failure is
   * reported through `onBuildFailed` and stops the stream.
   *
   * @param epoch {@link epoch} captured when the frame was handed to the
   *   ENCODER. Output from an older epoch (encoded before a mute or withdrawal)
   *   is dropped before sealing.
   */
  submit(frame: EncodedVideoFrame, epoch: number): Promise<void> {
    const link = this.#tail.then(() => this.#build(frame, epoch));
    this.#tail = link.catch((err: unknown) => {
      if (this.#stopped) return;
      this.stop();
      this.#o.onBuildFailed(err);
    });
    return this.#tail;
  }

  async #build(frame: EncodedVideoFrame, epoch: number): Promise<void> {
    if (this.#stopped || epoch !== this.#epoch || !this.hasActiveTarget) return;
    const headerVersion = this.#headerVersion;
    if (headerVersion === undefined) {
      throw new Error('VideoEgress: an active target without a send plan');
    }
    if (this.#exhausted) {
      this.#countPerActiveLane();
      return;
    }
    const keys = this.#o.transmitKeys;
    const stream = this.#o.streamNumber;
    if (frame.key) {
      this.#o.onKeyChunk();
      // EVERY GROUP START ROTATES (R-14), before this keyframe takes its key.
      keys.rotate();
      const gop = keys.nextGopCounter(stream);
      if (gop === null) {
        // u32::MAX was the last legal value: this stream stops and never wraps (§2).
        this.#exhausted = true;
        this.#countPerActiveLane();
        this.endGroups();
        return;
      }
      this.#currentGop = gop;
    } else if (this.#currentGop === undefined || !this.#anyInGroup()) {
      // Nothing to attach a delta frame to: build nothing, consume no nonce.
      return;
    }
    const gopCounter = this.#currentGop;
    const material = await keys.materialFor(stream, this.#o.kek);
    const streamSequence = keys.nextStreamSequence(stream);
    // KEY-BEARING on the first N frames after ANY key change (R-14), detected by
    // the non-secret generation field, never by holding the material.
    const { generation } = unpackKeyId(material.keyId);
    if (generation !== this.#lastGeneration) {
      this.#lastGeneration = generation;
      this.#framesSinceKeyChange = 0;
    }
    const keyBearing = this.#framesSinceKeyChange < this.#o.keyBearingFrames;
    this.#framesSinceKeyChange += 1;
    const sealed = await sealAndSignFrame({
      headerVersion,
      flags: {
        independentlyDecodable: frame.key,
        // Never discardable: VP8 deltas are references, and a key-bearing frame
        // must never be (the receiver rejects key-bearing + discardable).
        discardable: false,
        keyBearing,
      },
      streamSequence,
      gopCounter,
      material,
      streamId: this.#o.streamId,
      plaintext: frame.data,
      identity: this.#o.identity,
    });
    if (this.#stopped || epoch !== this.#epoch) return;
    this.#fanOut(sealed.bytes, sealed.relayRegionOffset, frame.key);
  }

  #anyInGroup(): boolean {
    for (const lane of this.#lanes.values()) {
      if (lane.active && lane.state === 'in_group') return true;
    }
    return false;
  }

  #countPerActiveLane(): void {
    for (const lane of this.#lanes.values()) {
      if (lane.active) this.#o.metrics.sendDropped(MEDIA_SEND_DROP_REASONS.GopCounterExhausted);
    }
  }

  #fanOut(bytes: Bytes, relayRegionOffset: number, key: boolean): void {
    let originalTaken = false;
    for (const [url, lane] of this.#lanes) {
      if (!lane.active) continue;
      if (key) {
        if (!this.#startGroup(url, lane)) continue;
      } else if (lane.state !== 'in_group') {
        continue;
      }
      const laneBytes = originalTaken ? (bytes.slice() as Bytes) : bytes;
      originalTaken = true;
      this.#enqueue(lane, { bytes: laneBytes, relayRegionOffset });
    }
  }

  /** Finish the lane's previous group and open a new one. False if no transport. */
  #startGroup(url: string, lane: Lane): boolean {
    this.#finishGroup(lane);
    const opened = this.#o.openStream(url);
    if (opened === undefined) {
      // Directed at a handler we hold no transport to. MC directs only handlers
      // MH reported this client connected on, so this is a placement race or a
      // transport replaced before MC observed it — not a backlog the publisher
      // decided to abandon. Failing toward NO re-key keeps any such condition
      // from becoming a keying loop; the lane rejoins at the next group (an MH
      // request or a self-key owed elsewhere). Counted on send_dropped
      // {not_connected} and faulted once, never on groups_stopped.
      this.#o.metrics.sendDropped(MEDIA_SEND_DROP_REASONS.NotConnected);
      this.#o.onTargetNotConnected();
      lane.state = 'mh_stopped';
      return false;
    }
    const group: Group = {
      writer: undefined,
      opening: true,
      queue: [],
      bytes: 0,
      draining: false,
      finishRequested: false,
      dead: false,
    };
    lane.group = group;
    lane.state = 'in_group';
    opened.then(
      (stream) => {
        group.opening = false;
        if (group.dead) {
          void stream.abort().catch(() => undefined);
          return;
        }
        group.writer = stream.getWriter();
        void this.#drain(lane, group);
      },
      () => {
        // A rejected open is MH's (or the transport's) decision: no re-key.
        group.opening = false;
        this.#failGroup(lane, group, MEDIA_SEND_GROUP_STOP_OUTCOMES.OpenFailed);
      },
    );
    return true;
  }

  #enqueue(lane: Lane, pending: PendingFrame): void {
    const group = lane.group;
    if (!group || group.dead) return;
    // The caps measure backlog BEHIND an admitted frame: a frame into an EMPTY
    // queue is always admitted, so a keyframe larger than the byte cap (a
    // raised directed profile) is still delivered rather than blacking out
    // every target at once.
    if (
      group.queue.length > 0 &&
      (group.queue.length + 1 > this.#o.maxBacklogFrames ||
        group.bytes + pending.bytes.length > this.#o.maxBacklogBytes)
    ) {
      this.#overflow(lane, group);
      return;
    }
    group.queue.push(pending);
    group.bytes += pending.bytes.length;
    void this.#drain(lane, group);
  }

  /** The target cannot keep up. Who decided it decides the repair. */
  #overflow(lane: Lane, group: Group): void {
    group.dead = true;
    group.queue.length = 0;
    group.bytes = 0;
    lane.group = undefined;
    if (group.opening) {
      // The open never completed: MH's open-rate / MAX_STREAMS decision, not
      // ours. Whatever exists is reset when the open settles; no re-key and no
      // abandonment count — counted as `open_stalled`, the only signal for it.
      this.#stopLane(lane, MEDIA_SEND_GROUP_STOP_OUTCOMES.OpenStalled);
      return;
    }
    // PUBLISHER-DECIDED: reset (never finish), count, and ask for a new group.
    if (group.writer) void group.writer.abort().catch(() => undefined);
    lane.state = 'awaiting_group';
    this.#o.metrics.sendGroupAbandoned();
    this.#o.onSelfAbandoned();
  }

  /**
   * End the lane's group. FIN only a group whose every queued frame was
   * written (after its in-flight write); a group with unsent backlog is RESET
   * instead, so a truncated group never looks complete downstream (R-12). Not
   * an abandonment: the group simply ended (a new group, mute, withdrawal).
   */
  #finishGroup(lane: Lane): void {
    const group = lane.group;
    lane.group = undefined;
    if (!group || group.dead) return;
    if (group.queue.length > 0) {
      group.queue.length = 0;
      group.bytes = 0;
      group.dead = true;
      // An open still pending is aborted when it settles (`dead`).
      if (group.writer) void group.writer.abort().catch(() => undefined);
      return;
    }
    group.finishRequested = true;
    if (!group.draining) this.#closeGroup(group);
  }

  #closeGroup(group: Group): void {
    if (group.dead) return;
    group.dead = true;
    // An open still in flight is closed when it settles (`dead` above).
    if (group.writer) void group.writer.close().catch(() => undefined);
  }

  /** MH stopped or refused this group (or the write failed in any way): no re-key. */
  #failGroup(lane: Lane, group: Group, outcome: MediaSendGroupStopOutcome): void {
    group.dead = true;
    group.queue.length = 0;
    group.bytes = 0;
    if (lane.group === group) {
      lane.group = undefined;
      if (lane.active) this.#stopLane(lane, outcome);
    }
  }

  /** Into `mh_stopped`, counted ONCE per transition (bounded: per group, never per frame). */
  #stopLane(lane: Lane, outcome: MediaSendGroupStopOutcome): void {
    if (lane.state === 'mh_stopped') return;
    lane.state = 'mh_stopped';
    this.#o.metrics.sendGroupStopped(outcome);
  }

  async #drain(lane: Lane, group: Group): Promise<void> {
    if (group.draining || !group.writer) return;
    group.draining = true;
    try {
      for (;;) {
        if (group.dead) return;
        const pending = group.queue.shift();
        if (pending === undefined) break;
        group.bytes -= pending.bytes.length;
        // Assigned HERE, when the frame is written: backlog drops consume no
        // number, and the count continues across groups on this lane. CLAIMED
        // SYNCHRONOUSLY (security F4): an old group's write may still be pending
        // when the next group's first frame is written, and both must never
        // carry one number.
        const hop = lane.hopSequence;
        lane.hopSequence += 1;
        writeHopSequence(pending.bytes, pending.relayRegionOffset, hop);
        try {
          await group.writer.write(pending.bytes);
        } catch {
          // Not sent: hand the number back if nothing claimed one since, so the
          // count stays "what the transmitter actually sent".
          if (lane.hopSequence === hop + 1) lane.hopSequence = hop;
          // STOP_SENDING, a reset by us (dead), or any other failure. Ours is
          // already accounted; anything else is MH's decision — fail toward no re-key.
          if (!group.dead) this.#failGroup(lane, group, MEDIA_SEND_GROUP_STOP_OUTCOMES.PeerStopped);
          return;
        }
        this.#framesSent += 1;
        this.#o.metrics.frameSent();
      }
    } finally {
      group.draining = false;
    }
    if (group.finishRequested) this.#closeGroup(group);
  }
}
