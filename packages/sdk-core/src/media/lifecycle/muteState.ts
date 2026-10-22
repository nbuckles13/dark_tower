// File: packages/sdk-core/src/media/lifecycle/muteState.ts
//
// Client mute, as ADR-0036 §5 defines it.
//
// ---------------------------------------------------------------------------
// TWO STATES THAT MUST NOT COLLAPSE
// ---------------------------------------------------------------------------
//
// §5 distinguishes two things that both stop media leaving:
//
//   * **"MC has not asked you to send"** — an empty target set on the send
//     directive. Resuming is a server round trip.
//   * **"you have muted yourself"** — client mute. Resuming is a LOCAL decision.
//
// They have different resumption costs, so they are separate fields here. MC
// MAY leave the send directive in place while a client reports itself muted;
// the client suppresses locally. A server mute or an unwatched kind withdraws
// the directive, so a client-unmute resumes locally only while the kind is
// still directed (signaling.proto `SendDirective`; ADR-0036 §5, correction
// 2026-10-13).
//
// ---------------------------------------------------------------------------
// ENFORCED AT CAPTURE, AND THE SIGNAL IS INFORMATIONAL
// ---------------------------------------------------------------------------
//
// §5: "Under client mute, no media leaves the device. This is enforced
// client-side at capture and must not depend on the server honouring it."
//
// So {@link MuteState} is consulted by the capture callback, BEFORE the encoder
// — not by the egress queue and not by the transport. `MuteRequest` is sent to
// MC afterwards, informationally: the local state changes first and does not
// wait for, or depend on, the report being delivered.
//
// Muting signals OUT OF BAND rather than transmitting substitute media. Audio
// simply stops; there is no keyframe to resume from, so restart is immediate.
// This is also why the encoder is configured with DTX off — with DTX, absence of
// frames becomes a signal, which is precisely what out-of-band signalling exists
// to avoid.
//
// DTX-off has a second consequence, load-bearing elsewhere: because the frame
// rate is constant and independent of speech, a DATAGRAM COUNT is not a
// voice-activity trace, which is what makes `EgressPipeline.framesSent` (a read
// of `#datagramsSent`) and `IngressPipeline.framesAccepted` safe to expose where
// ADR-0036 §11 bars per-frame sizes. Turning DTX on would silently turn both of
// those — and the production `dt_client_media_frames_sent_total` — into that
// trace.
//
// The unit is datagrams rather than captured frames, because a sender whose edges
// span two handlers sends each frame to both (§9). That does NOT weaken the
// argument above: the per-lane count is still constant and speech-independent, so
// the sum over lanes is too. What would break it is DTX, exactly as before.
//
// ---------------------------------------------------------------------------
// CLIENT MUTE DOES NOT RELEASE THE CAPTURE DEVICE, AND MUST NOT
// ---------------------------------------------------------------------------
//
// The track stays open while muted, so the browser and OS microphone indicators
// STAY LIT. That looks wrong next to a UI that says "muted", and it is not: the
// invariant §5 states is that **no media leaves the device**, not that the
// device is closed. Suppression happens at the capture callback, before the
// encoder — see `AudioPipeline.#onCapturedFrame`.
//
// Written here because the obvious "fix" is destructive. Stopping the track on
// mute means unmute has to re-acquire it — a `getUserMedia` round trip, possibly
// a permission prompt — which replaces the local, instantaneous resume this
// whole design exists to provide with a resumption path that can fail. Nothing
// in the mute path may call `stop()` on the capture.

// ---------------------------------------------------------------------------
// PER KIND, INDEPENDENT (story 3 R-15, R-17)
// ---------------------------------------------------------------------------
//
// Audio and video client mute are two independent fields: a request about one
// kind never changes the other. ONE instance per session, shared by every
// pipeline and by the mute report to MC, so the report always carries both
// kinds' CURRENT state (`signaling/muteRequest.ts`). Video client mute stops
// the camera track (story 3 R-17) — there is no video capture until C-A2, so
// today the video field is state and report only.

import type { MediaKindToken } from '../mediaKind.js';

/** Immutable snapshot of the local mute state. */
export interface MuteSnapshot {
  /** The participant muted their audio. Enforced at capture. */
  readonly audioMuted: boolean;
  /** The participant muted their video. Enforced at capture (camera stops; C-A2). */
  readonly videoMuted: boolean;
}

/**
 * Client-mute state for one session.
 *
 * Deliberately tiny and synchronous: the capture callback reads it once per
 * frame, so it must be a field read, never a call into anything that can block
 * or allocate.
 */
export class MuteState {
  #audioMuted = false;
  #videoMuted = false;

  /** Whether audio capture output is currently suppressed. Read once per frame. */
  get audioMuted(): boolean {
    return this.#audioMuted;
  }

  /** Whether video is client-muted. */
  get videoMuted(): boolean {
    return this.#videoMuted;
  }

  /** A snapshot for an event payload, and the both-kinds mute report. */
  snapshot(): MuteSnapshot {
    return { audioMuted: this.#audioMuted, videoMuted: this.#videoMuted };
  }

  /** Whether `kind` is client-muted. */
  isMuted(kind: MediaKindToken): boolean {
    return kind === 'audio' ? this.#audioMuted : this.#videoMuted;
  }

  /**
   * Set the local mute state.
   *
   * @returns `true` when the state actually changed. The caller counts the
   * transition and reports it to MC only on a real change, so a UI that calls
   * `setAudioMuted(true)` twice does not produce two transitions or two
   * signalling messages.
   */
  setAudioMuted(muted: boolean): boolean {
    if (this.#audioMuted === muted) return false;
    this.#audioMuted = muted;
    return true;
  }

  /** Set the local VIDEO mute state. Never touches audio. Same return contract. */
  setVideoMuted(muted: boolean): boolean {
    if (this.#videoMuted === muted) return false;
    this.#videoMuted = muted;
    return true;
  }
}
