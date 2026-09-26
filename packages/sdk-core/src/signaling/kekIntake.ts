// File: packages/sdk-core/src/signaling/kekIntake.ts
//
// THE TS-SIDE KEK SINK CONTROL.
//
// ---------------------------------------------------------------------------
// WHAT IT CLOSES
// ---------------------------------------------------------------------------
//
// protobuf-es has no `skip_debug` equivalent, so a decoded `ServerMessage`
// carrying `JoinResponse.meeting_kek` or `MeetingKekUpdate.meeting_kek` prints
// the meeting KEK in the clear under `JSON.stringify`, under `structuredClone`
// into a worker, and under any template interpolation. The Rust side suppresses
// derived `Debug` for those messages in `crates/proto-gen/build.rs`; TypeScript
// has no equivalent, and `scripts/guards/semantic/checks.md`'s Rust-only items
// (11-13) do not reach here.
//
// This module removes the material from the object graph the moment it is
// decoded: the KEK is copied into the private KEK-source seam, the decoded
// field's BYTES ARE ZEROED, and the field is replaced with an empty array. A
// later stringify of that message prints nothing.
//
// ---------------------------------------------------------------------------
// BOUNDARY VERSUS RESIDENCY, AND WHY BOTH NEED A CONTROL
// ---------------------------------------------------------------------------
//
// This closes the WIRE-DECODE leak completely. It does NOT close residency: the
// KEK is then alive inside the seam for the meeting's whole life, and a sink
// reached from that object — a log of the holder, an error carrying it in a
// context bag — is a different leak on a different path. Residency is the longer
// exposure and its control is the source scan
// (`src/__tests__/serverMessageSinkScan.test.ts`), which covers the seam objects
// as subjects, not only the wire types.
//
// ---------------------------------------------------------------------------
// ZEROING A DECODED FIELD IS NOT A GUARANTEE, AND IS STILL WORTH DOING
// ---------------------------------------------------------------------------
//
// JavaScript cannot promise no engine-internal copy survives: protobuf-es may
// have sliced the field out of the frame buffer, the frame buffer itself is not
// ours to zero, and a JIT may have copied. ADR-0028 §5 names the PRACTICE, not
// the guarantee, and this is the cheap place to honour it. What the control
// actually buys is deterministic: after intake, the reachable object graph the
// SDK hands anywhere no longer contains the key.

import type { KekArrival, KekInstallResult } from '../media/setup/kekSource.js';

/**
 * Write side of the KEK seam, declared STRUCTURALLY rather than imported.
 *
 * Keeps signaling free of a dependency on the media pipeline's concrete seam:
 * this module needs one method, and naming the method is a smaller contract than
 * naming the class. `MeetingKekHolder` satisfies it.
 */
export interface MeetingKekSink {
  /**
   * Install the KEK. Never throws on wire input: a refusal is reported through
   * the result and the holder's own counters.
   *
   * @param debounceSeconds W from the SAME message (`kek_rotation_debounce_seconds`).
   */
  install(
    kek: Uint8Array,
    generation: number,
    debounceSeconds: number,
    source: KekArrival,
  ): KekInstallResult;
}

/**
 * The two key-bearing message shapes, structurally: `JoinResponse` (including
 * the join-shaped reconnect response) and `MeetingKekUpdate`. Both carry W.
 */
interface KekBearing {
  meetingKek: Uint8Array;
  kekGeneration: number;
  kekRotationDebounceSeconds: number;
}

/** What intake did. Never the key, never its generation. */
export interface KekIntakeOutcome {
  /**
   * The holder's verdict, or `undefined` when the message carried no key (MC's
   * "not yet provisioned", a legitimate state).
   */
  readonly result: KekInstallResult | undefined;
}

/**
 * Take the KEK out of a decoded message and into the seam, then scrub the
 * message.
 *
 * SCRUBS UNCONDITIONALLY — including when the field is not a usable key. A
 * wrong-width or all-zero value is still key-shaped material on the wire and
 * still prints; refusing to install it is a separate decision from refusing to
 * scrub it, and conflating them would leave the leak open on exactly the path
 * where something has already gone wrong.
 *
 * ADR-0036 §4 / `signaling.proto`: an empty `meeting_kek` means NOT YET
 * PROVISIONED, which is a legitimate state, not an error. There is deliberately
 * no sentinel on `kek_generation` — 0 is a plausible first generation — so the
 * not-provisioned signal is the key being EMPTY, and this function must not
 * gate on the generation instead. A non-empty key of the wrong width is not
 * "unprovisioned": it is malformed, and the holder refuses and counts it.
 */
export function takeMeetingKek(
  message: KekBearing,
  sink: MeetingKekSink,
  source: KekArrival,
): KekIntakeOutcome {
  const raw = message.meetingKek;
  let result: KekInstallResult | undefined;
  try {
    // Only EMPTY means "not yet provisioned". Every non-empty value goes to the
    // holder — including a wrong-width one, which it refuses and COUNTS as
    // `malformed`. Filtering on the exact width here would scrub such a value
    // silently: a fail-quietly path on exactly the input that is wrong.
    if (raw.length !== 0) {
      // The seam TAKES A COPY, which is what makes the zeroing below safe: the
      // seam is not left holding a view of the buffer we are about to overwrite.
      result = sink.install(raw, message.kekGeneration, message.kekRotationDebounceSeconds, source);
    }
  } finally {
    // In a `finally`, so the scrub runs even if the sink throws on a defect —
    // the leak is closed on exactly the path where something has gone wrong.
    raw.fill(0);
    // Replace the reference too. Zeroing alone leaves a 32-byte all-zero array
    // on the message, which a reader could mistake for a real key; an empty
    // array is unambiguous and matches what MC sends when unprovisioned.
    message.meetingKek = new Uint8Array(0);
  }
  return { result };
}
