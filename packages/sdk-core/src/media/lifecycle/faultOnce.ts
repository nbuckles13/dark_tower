// File: packages/sdk-core/src/media/lifecycle/faultOnce.ts
//
// THE event-once fault dedupe, shared by the audio pipeline and the video
// publisher (story 3 C-A2): a repeating fault must not become per-frame
// telemetry.
//
// Keyed on (stage, message), NOT stage alone. Several faults share a stage with
// genuinely different remedies — on `transport`, "not connected to this
// handler" and "assignments moved to a different handler mid-session" (an MC
// placement defect) — and a stage-only key let whichever fired first silence
// the rest for the session.
//
// BOUNDED BY CONSTRUCTION, and that is the caller's obligation: the message is
// part of the dedupe key, so it MUST come from a fixed set — a string literal at
// its call site, or interpolation of static names only (teardown's
// `TeardownRegistry` name). Never a platform error string or any per-frame
// value, which would make a repeating fault unbounded.

/** Event-once (per episode) fault reporting. */
export class FaultOnce {
  readonly #reported = new Set<string>();

  /** `true` the first time (stage, message) is reported in an episode; `false` after. */
  report(stage: string, message: string): boolean {
    const key = `${stage}\u0000${message}`;
    if (this.#reported.has(key)) return false;
    this.#reported.add(key);
    return true;
  }

  /**
   * End a fault's episode: the next occurrence is reported again. For a fault
   * whose condition is RECOVERABLE by a later input, so "event-once" means once
   * per episode rather than once per session.
   */
  rearm(stage: string, message: string): void {
    this.#reported.delete(`${stage}\u0000${message}`);
  }
}
