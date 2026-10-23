// File: packages/sdk-core/src/session/signalingMediaState.ts
//
// The per-session MEDIA STATE derived from signalling (story 3 C-A1), held
// apart from `MeetingSession` so the session class stays a facade:
//
//   * E3 — the LATEST send directive and slot assignments, cached from
//     signalling creation. MC may answer the join-carried declaration before
//     `startMedia()` builds the pipeline; the cached values are replayed ONCE
//     when the pipeline's listeners are attached (listeners installed first),
//     and later ones go straight through — no double-apply, no race.
//   * Server mute per participant, BOTH kinds, from `ParticipantMuteUpdate` —
//     the unchanged kind a host's per-kind request carries. Absent =
//     server-unmuted in both kinds: MC replays an update for every participant
//     server-muted in EITHER kind right after the join response
//     (`crates/mc-service/src/actors/meeting.rs::server_mute_replay`).
//   * The R-18 tracker, fed THIS participant's own mute updates from creation
//     and the effective per-kind send state from the directive listener.
//   * Client mute, both kinds — ONE `MuteState` shared by the pipeline and the
//     mute report.

import { MuteState, type MuteSnapshot } from '../media/lifecycle/muteState.js';
import type { SignalingClient } from '../signaling/SignalingClient.js';
import type {
  PublishKind,
  SendDirectiveEvent,
  StreamAssignmentsEvent,
} from '../signaling/events.js';
import { pairSlotsBySender, type SlotPair } from '../signaling/pairing.js';
import { ServerMuteResumeTracker } from './serverMuteResume.js';

/** The pipeline-side consumers of signalling media state. */
export interface MediaStateListeners {
  readonly onDirective: (directive: SendDirectiveEvent) => void;
  readonly onAssignments: (event: StreamAssignmentsEvent) => void;
}

/** See the module header. One per `MeetingSession`. */
export class SignalingMediaState {
  /** Client mute, both kinds. */
  readonly mute = new MuteState();
  readonly #resume: ServerMuteResumeTracker;
  /** This participant's own id, from the `JoinResponse`; selects the SELF mute update. */
  #selfParticipantId: string | undefined;
  readonly #serverMute = new Map<string, { audio: boolean; video: boolean }>();
  #latestDirective: SendDirectiveEvent | undefined;
  #latestAssignments: StreamAssignmentsEvent | undefined;
  #listeners: MediaStateListeners | undefined;
  /** The roster's participant <-> sender index lives on the signaling client (one index). */
  #signaling: Pick<SignalingClient, 'senderIdOf' | 'participantIdOf'> | undefined;
  /** Story 3 C-B: a sender's VIDEO server mute was lifted (the unmute-edge repair). */
  #onVideoServerMuteLifted: ((senderId: number) => void) | undefined;

  /** `onResume(kind)` is the session's `captureResumedAfterServerMute` emitter. */
  constructor(onResume: (kind: PublishKind) => void) {
    this.#resume = new ServerMuteResumeTracker((kind) => this.mute.isMuted(kind), onResume);
  }

  /**
   * Subscribe to `signaling`, at its creation. Registered BEFORE the session's
   * own event bridges, so the pipeline sees an update no later than an embedder.
   */
  observe(signaling: SignalingClient): void {
    this.#signaling = signaling;
    // `joined` is emitted before `join()` resolves, so the SELF mute replay that
    // follows the join response is recognised.
    signaling.on('joined', (e) => {
      this.#selfParticipantId = e.participantId;
    });
    signaling.on('participantLeft', (e) => this.#serverMute.delete(e.participantId));
    signaling.on('streamAssignments', (e) => {
      this.#latestAssignments = e;
      this.#listeners?.onAssignments(e);
    });
    signaling.on('sendDirective', (d) => {
      this.#latestDirective = d;
      this.#listeners?.onDirective(d);
    });
    signaling.on('participantMuteChanged', (e) => {
      const wasVideoMuted = this.#serverMute.get(e.participantId)?.video ?? false;
      this.#serverMute.set(e.participantId, {
        audio: e.audioServerMuted,
        video: e.videoServerMuted,
      });
      // R-18: only THIS participant's own update feeds the tracker; another
      // participant's never touches the self latch.
      if (e.participantId === this.#selfParticipantId) {
        this.#resume.apply({ type: 'selfServerMute', kind: 'audio', muted: e.audioServerMuted });
        this.#resume.apply({ type: 'selfServerMute', kind: 'video', muted: e.videoServerMuted });
      }
      // The video server-mute -> unmute EDGE (story 3 C-B, observability O2):
      // the video receiver re-requests a frame it dropped as muted at arrival.
      if (wasVideoMuted && !e.videoServerMuted) {
        const senderId = signaling.senderIdOf(e.participantId);
        if (senderId !== undefined) this.#onVideoServerMuteLifted?.(senderId);
      }
    });
  }

  /**
   * Attach the pipeline's listeners, then replay what MC already sent, ONCE.
   * Anything later arrives through them directly.
   */
  attach(listeners: MediaStateListeners): void {
    this.#listeners = listeners;
    if (this.#latestAssignments) listeners.onAssignments(this.#latestAssignments);
    if (this.#latestDirective) listeners.onDirective(this.#latestDirective);
  }

  /** Set `kind`'s client mute; the new both-kinds snapshot on a real change, else `undefined`. */
  setClientMute(kind: PublishKind, muted: boolean): MuteSnapshot | undefined {
    const changed =
      kind === 'audio' ? this.mute.setAudioMuted(muted) : this.mute.setVideoMuted(muted);
    return changed ? this.mute.snapshot() : undefined;
  }

  /** The R-18 feed: whether `kind` is EFFECTIVELY being sent (after validation). */
  effectiveTargets(kind: PublishKind, nonEmpty: boolean): void {
    this.#resume.apply({ type: 'effectiveTargets', kind, nonEmpty });
  }

  /** MC's latest send directive, or `undefined` before one arrives. */
  get latestDirective(): SendDirectiveEvent | undefined {
    return this.#latestDirective;
  }

  /**
   * Sender id -> the audio and video slots showing that sender (story 3 R-3):
   * MC's latest assignments plus `liveVideo` (video slot -> the VERIFIED sender
   * its live slot last rendered, story 3 C-B).
   */
  slotPairing(liveVideo?: ReadonlyMap<number, number>): ReadonlyMap<number, SlotPair> {
    return pairSlotsBySender(this.#latestAssignments?.assignments ?? [], liveVideo);
  }

  /**
   * A VERIFIED video sender's server-mute state per the latest
   * `ParticipantMuteUpdate`, through the roster's ONE participant <-> sender
   * index. `unmapped` when no roster participant maps to the sender — never
   * rendered (security S2). Absent update = unmuted (MC replays every muted
   * participant right after the join response).
   */
  videoMuteState(senderId: number): 'muted' | 'unmuted' | 'unmapped' {
    const participantId = this.#signaling?.participantIdOf(senderId);
    if (participantId === undefined) return 'unmapped';
    return this.#serverMute.get(participantId)?.video === true ? 'muted' : 'unmuted';
  }

  /** Subscribe the video receiver to video server-mute -> unmute edges. */
  onVideoServerMuteLifted(listener: ((senderId: number) => void) | undefined): void {
    this.#onVideoServerMuteLifted = listener;
  }
}
