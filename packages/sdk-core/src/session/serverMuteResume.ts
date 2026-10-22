// File: packages/sdk-core/src/session/serverMuteResume.ts
//
// The R-18 fail-safe (story 3 R-18, signaling.proto `ParticipantMuteUpdate`
// "SELF COPY AND RESUMED CAPTURE"): when a host's server UNMUTE resumes this
// participant's capture or send for a kind, the participant is told — whatever
// order MC's messages arrive in, and even if the unmute update is lost. The UI
// notice itself is story 3 C-C1's; this module decides WHEN the event fires.
//
// A pure, per-kind state machine. Inputs, both from signaling creation onward:
//
//   * `selfServerMute` — THIS participant's own `ParticipantMuteUpdate` for a
//     kind (updates about other participants are never fed here);
//   * `effectiveTargets` — whether the kind is EFFECTIVELY being sent, AFTER
//     directive validation: a kind-missing fault, a refused header version and
//     shim (c)'s empty `streams` all count as empty. Before media starts nothing
//     is sent, so the first applied directive is an edge from empty.
//
// ---------------------------------------------------------------------------
// INVARIANT S1 — THE CAPTURE GATE (security, Gate 1). C-A2 MUST KEEP IT.
// ---------------------------------------------------------------------------
//
// The ONLY things that withhold or resume capture/send for a kind are the
// EFFECTIVE target set and client mute. A server-mute `ParticipantMuteUpdate`
// NEVER stops or starts capture by itself. The `selfServerMute(false)`-while-
// sending branch below clears the latch, and that is safe ONLY under this
// invariant: if any path ever stopped capture on update(true) — e.g. a camera
// stopped on the mute update rather than on the directive withdrawal — then
// update(false) with non-empty targets would clear the latch AND restart
// capture with no event: a real R-18 bypass. Story 3 C-A2 (task 13), which
// adds the camera, owns keeping this true.

import type { MediaKindToken } from '../media/mediaKind.js';

/** The kinds this tracks. */
export type ResumeKind = MediaKindToken;

/** One input to the machine. */
export type ServerMuteResumeInput =
  | { readonly type: 'selfServerMute'; readonly kind: ResumeKind; readonly muted: boolean }
  | { readonly type: 'effectiveTargets'; readonly kind: ResumeKind; readonly nonEmpty: boolean };

interface KindState {
  /** A server mute of this kind was recorded and not yet consumed by a resume. */
  isServerMuteRecorded: boolean;
  /** Whether the kind is effectively being sent (non-empty effective targets). */
  isSending: boolean;
}

/** Per-kind R-18 state. `onResume(kind)` is the "capture resumed after server mute" event. */
export class ServerMuteResumeTracker {
  readonly #state: Record<ResumeKind, KindState> = {
    audio: { isServerMuteRecorded: false, isSending: false },
    video: { isServerMuteRecorded: false, isSending: false },
  };
  readonly #isClientMuted: (kind: ResumeKind) => boolean;
  readonly #onResume: (kind: ResumeKind) => void;

  constructor(isClientMuted: (kind: ResumeKind) => boolean, onResume: (kind: ResumeKind) => void) {
    this.#isClientMuted = isClientMuted;
    this.#onResume = onResume;
  }

  apply(input: ServerMuteResumeInput): void {
    const state = this.#state[input.kind];
    switch (input.type) {
      case 'selfServerMute': {
        if (input.muted) {
          // Latch. Keyed on what was RECORDED, so the event fires whichever of
          // the unmute update and the resumed directive arrives first, and
          // still fires if the unmute update is lost.
          state.isServerMuteRecorded = true;
        } else if (state.isSending) {
          // Unmuted while the kind never stopped being sent (no empty period
          // since the mute was recorded — e.g. a controller that does not
          // withdraw on server mute): nothing was withheld, so nothing can
          // "resume". Clearing here is what stops a stale latch from raising
          // a false notice at a later unwatched -> watched edge. Safe ONLY
          // under invariant S1 (header).
          state.isServerMuteRecorded = false;
        }
        // Unmuted while NOT sending: keep the latch — the resume edge consumes it.
        return;
      }
      case 'effectiveTargets': {
        const wasSending = state.isSending;
        state.isSending = input.nonEmpty;
        if (wasSending || !input.nonEmpty) return;
        // The empty -> non-empty edge: capture/send resumes for this kind.
        if (state.isServerMuteRecorded) {
          state.isServerMuteRecorded = false;
          // Client-muted: nothing is captured or sent, so there is nothing to
          // notify about — the participant resumes it themselves.
          if (!this.#isClientMuted(input.kind)) this.#onResume(input.kind);
        }
        // RESIDUAL, BY DESIGN: with no server mute ever recorded, a resume
        // raises NO notice — the participant was never told they were muted,
        // so a resume (e.g. someone started watching) is not news to them.
        return;
      }
    }
  }
}
