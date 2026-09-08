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
// They have different resumption costs, so they are separate fields here and MC
// keeps the send directive ACTIVE while a client reports itself muted. The
// client suppresses locally; MC does not withdraw the instruction. That is what
// makes unmute instantaneous.
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

/** Immutable snapshot of the local mute state. */
export interface MuteSnapshot {
  /** The participant muted themselves. Enforced here, at capture. */
  readonly audioMuted: boolean;
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

  /** Whether capture output is currently suppressed. Read once per frame. */
  get audioMuted(): boolean {
    return this.#audioMuted;
  }

  /** A snapshot for an event payload. */
  snapshot(): MuteSnapshot {
    return { audioMuted: this.#audioMuted };
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
}
