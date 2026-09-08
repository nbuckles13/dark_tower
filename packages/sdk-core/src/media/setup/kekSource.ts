// File: packages/sdk-core/src/media/setup/kekSource.ts
//
// THE KEK-SOURCE SEAM (ADR-0036 §4).
//
// "The client obtains the KEK through a seam, as §10's transport and measurement
// seams; today its one implementation is the join response and the KEK-push
// message. Nothing in the frame format or the wrap depends on where the KEK came
// from, so a key server outside MC — should operator exclusion ever be required
// — changes the source and nothing else."
//
// ---------------------------------------------------------------------------
// WHERE THE KEK LIVES, AND WHERE IT DOES NOT
// ---------------------------------------------------------------------------
//
// It lives in ONE private field, on ONE object, for the life of one meeting. It
// is NOT:
//
//   * on `JoinedEvent` or any other public event payload — those are projected
//     to embedders and, in test builds, onto the web app's e2e bus;
//   * on `TransmitKeyCache` — the codec layer deliberately keeps the KEK a
//     per-call parameter to `unwrapTransmitKey` so that cache never becomes a
//     KEK home, and that property is preserved here;
//   * on any error, log, span attribute, or metric label;
//   * persisted anywhere.
//
// It is zeroed and dropped at session teardown.
//
// ---------------------------------------------------------------------------
// WHAT THIS SEAM DOES NOT CLAIM
// ---------------------------------------------------------------------------
//
// ADR-0036 §4 records the trust decision in its own words: media is encrypted
// between clients; MH, transport and storage cannot read it; MC can. This is
// accepted operator custody. Nothing here may be described as end-to-end against
// the operator, or as zero-trust; telemetry carries `key_custody=operator` in
// place of any such boolean.
//
// PREVIOUS-KEK RETENTION IS OUT OF SCOPE for this story, so exactly one
// generation is held at a time. A frame carrying a different generation is
// `no_kek_for_generation` (dropped) or `kek_generation_not_held` (played off a
// cached transmit key) — both are expected transients at join and rotation, and
// both are signals when sustained.

import { MEETING_KEK_BYTES } from '../frame/sframe.js';

/**
 * Read side of the seam, as the receive path consumes it.
 *
 * Deliberately the exact shape `openVerifiedFrame`'s `ReceiverKeys` needs, so no
 * adapter sits between the seam and the crypto.
 */
export interface MeetingKekSource {
  /** The KEK for `generation`, or `undefined` if this client does not hold it. */
  kekForGeneration(generation: number): Uint8Array | undefined;
}

/** Where a KEK arrived from. Mirrors the metric's bounded `source` vocabulary. */
export type KekArrival = 'join_response';

/**
 * The one implementation: a single meeting KEK, supplied by the signaling layer
 * from `JoinResponse.meeting_kek`.
 *
 * Constructed empty. `set()` is called by the signaling layer's KEK intake, and
 * `clear()` by teardown.
 */
export class JoinResponseKekSource implements MeetingKekSource {
  #kek: Uint8Array | undefined;
  #generation: number | undefined;

  /**
   * Install the meeting KEK.
   *
   * FAILS CLOSED on width: `signaling.proto` states the consumer rule as "never
   * wrap or unwrap under a KEK of any length other than 32; an empty or all-zero
   * KEK is never a usable key", and MC cannot enforce a consumer's behaviour.
   * A wrong-width value is refused here rather than passed to WebCrypto, which
   * would accept a 16-byte AES-GCM key silently.
   *
   * TAKES A COPY. The caller's buffer is a slice of a decoded protobuf message
   * whose bytes the intake path zeroes immediately afterwards; holding the slice
   * would mean holding a view of memory that is about to be overwritten.
   *
   * @throws {RangeError} if `kek` is not exactly {@link MEETING_KEK_BYTES}.
   */
  set(kek: Uint8Array, generation: number): void {
    if (kek.length !== MEETING_KEK_BYTES) {
      // The LENGTH only — a fixed-size key-material length is safe to state
      // because it is a compile-time constant rather than a function of the
      // secret. No bytes, no generation-derived value, nothing else.
      throw new RangeError(
        `meeting KEK must be exactly ${MEETING_KEK_BYTES} bytes, got ${kek.length}`,
      );
    }
    // An all-zero KEK is never a usable key (`signaling.proto`). Refused rather
    // than used: a zero key "works" cryptographically and is catastrophically
    // wrong, so it must fail loudly at the boundary or not at all.
    if (kek.every((b) => b === 0)) {
      throw new RangeError('meeting KEK is all zero, which is never a usable key');
    }
    this.#zero();
    this.#kek = Uint8Array.from(kek);
    this.#generation = generation;
  }

  /** Whether a usable KEK is held. Never exposes the key or its generation. */
  get isProvisioned(): boolean {
    return this.#kek !== undefined;
  }

  kekForGeneration(generation: number): Uint8Array | undefined {
    return this.#generation === generation ? this.#kek : undefined;
  }

  /**
   * Drop the KEK, overwriting the buffer first.
   *
   * ADR-0028 §5's "explicit token cleanup on disconnect/logout". JavaScript
   * cannot guarantee no engine-internal copy survives; the ADR names the
   * practice, not the guarantee, and this is the cheap place to honour it.
   */
  clear(): void {
    this.#zero();
    this.#kek = undefined;
    this.#generation = undefined;
  }

  #zero(): void {
    this.#kek?.fill(0);
  }
}
