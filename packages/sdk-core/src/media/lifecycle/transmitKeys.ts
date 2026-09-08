// File: packages/sdk-core/src/media/lifecycle/transmitKeys.ts
//
// The send-side transmit-key manager: generation, rotation, and the wrap that
// rides every audio frame (ADR-0036 §4).
//
// ---------------------------------------------------------------------------
// THE SEQUENCE AND THE GENERATION LIVE ON ONE OBJECT, AND NEITHER CAN RESET
// ---------------------------------------------------------------------------
//
// This is the file where a (key, nonce) reuse would be introduced, so the
// invariant is structural rather than disciplined.
//
// ADR-0036 §4 permits a stream-sequence reset ONLY because the key id
// distinguishes generations; §2's rule is to keep counting regardless. If a path
// ever existed where the sequence returned to zero while the generation did NOT
// advance, that is a repeated (key, nonce) pair under AES-GCM — which does not
// merely expose those two frames, it leaks the authentication subkey and permits
// forgery.
//
// So:
//
//   * `#generation` and `#streamSequences` are fields on THIS object, which is
//     session-scoped and is not rebuilt on mute/unmute, on a directive change,
//     or on resume-from-empty.
//   * **There is no reset method.** No code path returns a sequence to zero.
//   * `rotate()` increments the generation UNCONDITIONALLY. There is no branch
//     under which a rotation fails to advance it — a conditional bump is exactly
//     the shape that produces the hazard above.
//   * No frame is renumbered after sealing. The only post-seal mutation is the
//     relay hop-sequence patch, which is outside both the AEAD associated data
//     and the signed range.
//
// A new counter set requires a new `MeetingSession`, which is a fresh join with
// a fresh identity and therefore a new key-id namespace. ADR-0036 §4: "Identity
// key, token, and generation counter share one lifetime."
//
// ---------------------------------------------------------------------------
// ROTATION TRIGGERS: WHICH ARE IMPLEMENTED, AND WHICH ARE UNREACHABLE
// ---------------------------------------------------------------------------
//
// ADR-0036 §4's table has seven. This story reaches four; the other three are
// named here so their absence reads as a decision rather than an oversight.
//
//   * **Every T for audio** — implemented, `T` from config, measured from
//     pipeline start (per-client phase, deliberately not wall-clock aligned).
//   * **Sender resumes from empty** — implemented: a send directive's target set
//     going empty -> non-empty.
//   * **Unmute after mute** — implemented. Not literally in §4's table; a mute is
//     a resume-from-empty for the stream, it costs one generation bump, and it
//     bounds a leaked transmit key across the silent window.
//   * **Participant leaves / joins** — not a transmit-key trigger at all; those
//     rotate the KEK, which is MC's.
//   * **Counter exhaustion** — UNREACHABLE RATHER THAN UNCHECKED, and stated in
//     the idiom `keyId.ts` uses for the 2^40 generation ceiling. The stream
//     sequence is a `uint32` (`decodeFrame` reads `getUint32(6)`), so at 50 fps a
//     wrap needs roughly 2.7 YEARS of continuous transmission on one
//     (sender, stream). No meeting reaches it. There is therefore no ceiling
//     check and no ceiling vector — and an author who adds one is adding dead
//     code, while an author who reads the omission as an oversight might instead
//     conclude the sequence may be reset, which is the actual hazard.
//   * **Reconnect** — UNREACHABLE THIS STORY. `MediaTransport` performs no
//     reconnect, `MeetingSession` is single-use per join, and no path restarts
//     the pipeline against a new transport while this object survives. It
//     becomes live with the handler-restart story, and when it does it is a
//     generation bump like any other.
//   * **MC migration / MH failover** — MC migration issues a fresh KEK, which
//     changes what we wrap under, not our generation. MH failover is explicitly
//     not a trigger: MH never held keys.
//
// ---------------------------------------------------------------------------
// MINTING IS SINGLE-FLIGHT PER STREAM, AND THAT IS A CRYPTO REQUIREMENT
// ---------------------------------------------------------------------------
//
// `materialFor` is called from a FIRE-AND-FORGET submit (`AudioPipeline` does
// `void egress.submit(frame)` from the encoder's output callback), so several
// calls are in flight at once and nothing serialises them.
//
// A naive read-modify-write — check the map, `await` the wrap, write the map —
// lets two callers both miss, both mint an independent random key, and both pack
// THE SAME KEY ID, because `#senderId`, `streamNumber` and `#generation` are all
// unchanged across the await. And `wrapNonce(keyId)` is a pure function of the
// key id, with the key id also as the associated data. So the two wraps are:
//
//     AES-GCM(KEK, nonce=f(kid), aad=kid, keyA)
//     AES-GCM(KEK, nonce=f(kid), aad=kid, keyB)
//
// One key, one nonce, two plaintexts — the catastrophic case ADR-0036 §4 names:
// *"Never reuse a (key, nonce) pair"*, *"catastrophic for AES-GCM, yielding
// authentication-key recovery rather than mere confidentiality loss."* It leaks
// `keyA XOR keyB` and permits recovery of the nonce-independent GHASH subkey
// under the meeting KEK.
//
// `receivePath.ts::TransmitKeyCache.set` PREDICTED this in terms — *"were that
// ever violated ... it would be a (key, nonce) repeat under the KEK"* — naming
// the caller obligation this file is the caller for.
//
// So the mint is memoised on the IN-FLIGHT PROMISE, not on its result: every
// concurrent caller for one stream receives the same promise and therefore the
// same key. The window a result-only cache leaves open is exactly the window
// that matters, because the mint path is entered at every rotation boundary —
// which is every T, every unmute, and every resume-from-empty.
//
// **Sequential tests cannot see this.** The concurrent cases in
// `__tests__/transmitKeys.test.ts` exist because the suite was green with the
// defect present.
//
// ---------------------------------------------------------------------------
// A ROTATION MUST NOT BE UNDONE BY A MINT THAT WAS ALREADY IN FLIGHT
// ---------------------------------------------------------------------------
//
// `rotate()` is synchronous and clears the current keys, but a mint that started
// before it resumes afterwards. If it installed its material, it would reinstate
// a key the rotation just discarded — no nonce reuse (the key id names the old
// generation consistently), but the rotation silently would not take effect for
// that stream, so the leaked-transmit-key window `rotate()` exists to bound
// would not be bounded.
//
// So `#mint` captures the generation at entry and installs ONLY if the manager
// is still on it, and `rotate()` clears the in-flight map so no later caller
// joins an orphaned mint. The orphaned mint's own caller still receives its
// material and emits one frame at the pre-rotation generation — which is
// self-consistent, monotonic (generations never decrease), and correct: that
// frame was committed to that generation before the rotation happened.
//
// ---------------------------------------------------------------------------
// THE WRAP IS COMPUTED ONCE PER GENERATION
// ---------------------------------------------------------------------------
//
// ADR-0036 §4: "One transmit key therefore wraps to one ciphertext —
// byte-identical from frame to frame within a generation". That determinism is
// not an optimisation for us; it is what the RECEIVER's `matchesCachedWrap` skip
// depends on. Re-wrapping per frame would still be correct but would make every
// receiver unwrap on every frame, and would break the property the
// `wrap_determinism_seq_lo`/`_hi` vector pair exists to pin.

import { aesGcmSeal, AES_256_KEY_BYTES, wrapNonce } from '../frame/sframe.js';
import { packKeyId } from '../frame/keyId.js';
import type { WrappedTransmitKey } from '../frame/frameCodec.js';
import type { Bytes } from '../frame/hex.js';
import type { MeetingKekSource } from '../setup/kekSource.js';

/** Everything the egress path needs to seal and announce one frame. */
export interface TransmitKeyMaterial {
  /** The 8-byte packed key id: `sender_id(16) | stream(8) | generation(40)`. */
  readonly keyId: Bytes;
  /** The raw AES-256 transmit key. */
  readonly key: Bytes;
  /** The publisher-region block, computed once per generation. */
  readonly wrapped: WrappedTransmitKey;
}

/** Thrown when a transmit key cannot be produced. Bounded reasons only. */
export class TransmitKeyError extends Error {
  /** `no_kek` when the meeting KEK has not arrived; `sender_id` when unassigned. */
  readonly reason: 'no_kek' | 'sender_id';

  constructor(reason: 'no_kek' | 'sender_id', message: string) {
    super(message);
    this.name = 'TransmitKeyError';
    this.reason = reason;
  }
}

/** Read side of the KEK seam plus the generation to wrap under. */
export interface KekForWrapping {
  readonly source: MeetingKekSource;
  /** The generation to announce on every wrapped block. */
  readonly generation: number;
}

/**
 * Session-scoped transmit keys for one sender.
 *
 * One instance per `MeetingSession`. Constructed with the sender id MC assigned
 * at join, which is fixed for the meeting.
 */
export class TransmitKeyManager {
  readonly #senderId: bigint;
  readonly #randomBytes: (out: Bytes) => void;

  /**
   * MONOTONIC, NEVER RESET. Only {@link rotate} changes it, and only upward.
   * Starts at 0 so the first key of a session is generation 0.
   */
  #generation = 0n;

  /**
   * Per-stream sequence counters. NEVER RESET — not on rotation, not on mute,
   * not on a directive change. §2 requires counting to continue across
   * stop/start; §4's key id is what makes a reset merely unnecessary rather than
   * required.
   */
  readonly #streamSequences = new Map<number, number>();

  /** The current key per stream number, invalidated wholesale by `rotate()`. */
  #current = new Map<number, TransmitKeyMaterial>();

  /**
   * Mints in flight, by stream number.
   *
   * Memoised on the PROMISE rather than on its result. See the module header:
   * a result-only cache leaves the whole await open, and two callers entering it
   * produce two keys under one key id, which is a (key, nonce) repeat under the
   * meeting KEK.
   */
  readonly #inFlight = new Map<number, Promise<TransmitKeyMaterial>>();

  /**
   * Materials superseded by the MOST RECENT rotation, held for one rotation
   * period so they can be overwritten without racing a frame still using them.
   *
   * See {@link rotate} for why they are not overwritten at the moment they are
   * superseded, and why one period is the bound.
   */
  #retired: TransmitKeyMaterial[] = [];

  constructor(senderId: number, randomBytes?: (out: Bytes) => void) {
    if (!Number.isInteger(senderId) || senderId <= 0) {
      // `packKeyId` refuses sender id 0 because a shared zero is N colliding key
      // ids; refusing it here as well means the failure names the cause rather
      // than surfacing as a pack-time RangeError from three frames deep.
      throw new TransmitKeyError(
        'sender_id',
        'no sender id was assigned for this meeting, so no key id can be formed',
      );
    }
    this.#senderId = BigInt(senderId);
    this.#randomBytes = randomBytes ?? ((out) => crypto.getRandomValues(out));
  }

  /** The current generation. Exposed for assertion; never for a metric label. */
  get generation(): bigint {
    return this.#generation;
  }

  /**
   * Advance to a new generation, discarding every current key.
   *
   * UNCONDITIONAL. There is no argument, no predicate, and no early return: a
   * rotation that could decline to advance the generation is the shape that
   * permits a (key, nonce) repeat.
   */
  rotate(): void {
    this.#generation += 1n;
    // ---------------------------------------------------------------------
    // OVERWRITE THE GENERATION BEFORE LAST, NOT THE ONE BEING SUPERSEDED NOW
    // ---------------------------------------------------------------------
    //
    // A transmit key is read AFTER AN AWAIT by the frame being sealed with it:
    // `EgressPipeline.submit` -> `deriveSframeKeys` -> `hkdfExtract` -> `hmac`,
    // and `hmac` reads the material at its `crypto.subtle.sign(...)` call, which
    // is downstream of `await crypto.subtle.importKey(...)`. Rotation fires from
    // an interval timer, so it can land inside that await.
    //
    // Overwriting the just-superseded key there would seal that frame under an
    // ALL-ZERO key while its wrapped block announces the real one. The receiver
    // unwraps correctly, derives correctly, and fails the tag — surfacing as
    // `decrypt_failed`, whose triage points at "the key schedule or the sender".
    // A memory-hygiene measure would present as a crypto fault at every rotation
    // boundary, in the service whose failures are hardest to localise.
    //
    // So retirement is deferred by one rotation. **The bound is the interval
    // between two CONSECUTIVE ROTATIONS FROM ANY TRIGGER — not the configured
    // rotation period**, and the distinction is the whole of it: `rotate()` has
    // three call sites in `AudioPipeline`, and only one of them is the timer.
    //
    //   * the rotation interval        (`audioRotationPeriodMs`)
    //   * resume-from-empty            (a send directive going empty -> non-empty)
    //   * unmute                       (`setAudioMuted(false)`)
    //
    // So two `setAudioMuted(false)` calls in quick succession, or a directive
    // flapping empty -> non-empty -> empty -> non-empty, rotate twice
    // milliseconds apart and the second overwrites what the first retired. That
    // interval is CALLER-CONTROLLED and cannot be bounded from here.
    //
    // What IS bounded from here is the read window it races: `material.key` is
    // read exactly once after `materialFor` resolves — as the HMAC input inside
    // `deriveSframeKeys` -> `hkdfExtract` -> `hmac`, at
    // `sframeKeySchedule.ts`'s `crypto.subtle.sign('HMAC', ...)`, downstream of
    // that function's `await crypto.subtle.importKey`. One HKDF-extract.
    // (`material.wrapped` is ciphertext and is not affected.)
    //
    // A BOUND, NOT A GUARANTEE, and now stated against the right quantity: the
    // periodic half is validated — `validateMediaConfig` enforces a floor on
    // `audioRotationPeriodMs` well above one HKDF-extract — while the
    // event-driven half rests on a caller not toggling mute twice inside a
    // WebCrypto round trip. The earlier version of this comment claimed "six
    // orders of magnitude of headroom" from the 60 s default, which is false for
    // the mute path and would have led a reader who checked it to discard the
    // paragraph including the part that matters.
    //
    // `clear()` catches whatever is still held at teardown, where there is no
    // in-flight reader to race.
    for (const material of this.#retired) material.key.fill(0);
    this.#retired = [...this.#current.values()];
    this.#current = new Map();
    // No later caller may join a mint that was started at the old generation.
    // The mint's existing callers still get their material; `#mint` refuses to
    // install it, so the rotation takes effect regardless.
    this.#inFlight.clear();
  }

  /**
   * The key material for `streamNumber` at the current generation, minting and
   * wrapping it on first use.
   *
   * @throws {TransmitKeyError} `no_kek` when the meeting KEK has not arrived —
   * the client cannot announce a key it cannot wrap, and sending an unwrapped or
   * unannounced frame is not an available degradation.
   */
  async materialFor(streamNumber: number, kek: KekForWrapping): Promise<TransmitKeyMaterial> {
    const existing = this.#current.get(streamNumber);
    if (existing) return existing;

    // SINGLE-FLIGHT. Every concurrent caller for this stream joins the SAME
    // mint and therefore receives the SAME key — which is what keeps one key id
    // bound to one key, and therefore one (key, nonce) pair under the KEK.
    const pending = this.#inFlight.get(streamNumber);
    if (pending) return pending;

    const minted = this.#mint(streamNumber, kek, this.#generation);
    this.#inFlight.set(streamNumber, minted);
    const release = (): void => {
      // Identity-checked: a `rotate()` between start and settle already cleared
      // the map and a newer mint may own the slot. Deleting unconditionally
      // would evict the live one.
      if (this.#inFlight.get(streamNumber) === minted) this.#inFlight.delete(streamNumber);
    };
    // Both arms, so a rejected mint does not wedge the slot. Attaching handlers
    // here also means this derived promise never rejects, so it cannot become an
    // unhandled rejection; the ORIGINAL is returned and handled by the caller.
    minted.then(release, release);
    return minted;
  }

  /**
   * Mint, wrap, and (if still current) install one transmit key.
   *
   * @param generation captured by the CALLER at entry, not read from `this`
   * here. That is the whole point: it must be the generation the key id was
   * packed under, so the install check below compares like with like.
   */
  async #mint(
    streamNumber: number,
    kek: KekForWrapping,
    generation: bigint,
  ): Promise<TransmitKeyMaterial> {
    const kekBytes = kek.source.kekForGeneration(kek.generation);
    if (!kekBytes) {
      throw new TransmitKeyError(
        'no_kek',
        'no meeting key-encryption key is available, so a transmit key cannot be wrapped',
      );
    }

    const keyId = packKeyId({
      senderId: this.#senderId,
      stream: BigInt(streamNumber),
      generation,
    });
    const key = new Uint8Array(AES_256_KEY_BYTES) as Bytes;
    this.#randomBytes(key);

    // The wrap: AES-256-GCM under the KEK, nonce DERIVED from the key id
    // (`0x00000000 || key_id`), and the key id itself as associated data. The
    // AAD is what BINDS the wrap to this key id — a receiver accepts a wrap only
    // for the key id of the frame carrying it, and that binding is the AEAD
    // rather than a field, which is why widening the AAD would destroy it.
    //
    // NOTE the nonce is a pure function of the key id. That is exactly why the
    // single-flight guard above is a crypto requirement rather than an
    // optimisation.
    const wrappedKeyWithTag = await aesGcmSeal({
      key: kekBytes,
      nonce: wrapNonce(keyId),
      aad: keyId,
      plaintext: key,
    });

    const material: TransmitKeyMaterial = {
      keyId,
      key,
      wrapped: { kekGeneration: kek.generation, wrappedKeyWithTag },
    };
    // Install ONLY if no rotation happened while this was in flight. Installing
    // a superseded generation would reinstate a key the rotation discarded, so
    // the rotation would silently not take effect for this stream.
    //
    // The material is still RETURNED, and its key is NOT zeroed: the caller is
    // mid-frame and is about to derive from it. Zeroing a key we are handing out
    // would seal that frame under zeros while its wrap announces the real key —
    // trading a bounded-window concern for guaranteed audio loss.
    if (this.#generation === generation) {
      this.#current.set(streamNumber, material);
    }
    return material;
  }

  /**
   * Take the next stream sequence for `streamNumber`.
   *
   * Monotonic per (sender, stream) for the life of the session. Deliberately has
   * no reset, no seek, and no argument.
   */
  nextStreamSequence(streamNumber: number): number {
    const next = this.#streamSequences.get(streamNumber) ?? 0;
    this.#streamSequences.set(streamNumber, next + 1);
    return next;
  }

  /**
   * Drop every key, overwriting the buffers first.
   *
   * ADR-0028 §5's explicit cleanup. Note this deliberately does NOT reset the
   * generation or the sequences: teardown ends the session, and a manager that
   * survived teardown with reset counters would be the hazard this module's
   * header describes.
   *
   * ---------------------------------------------------------------------------
   * OVERWRITING HAPPENS HERE AND **NOT** IN `rotate()`, DELIBERATELY
   * ---------------------------------------------------------------------------
   *
   * A transmit key is read AFTER AN AWAIT by the frame being sealed with it. The
   * chain is `EgressPipeline.submit` -> `deriveSframeKeys` -> `hkdfExtract` ->
   * `hmac`, and `hmac` reads the key material at its `crypto.subtle.sign(...)`
   * call, which is downstream of `await crypto.subtle.importKey(...)`. So a
   * rotation firing from its interval timer during that await would zero a key
   * a frame is mid-way through using.
   *
   * The result would be a frame sealed under an ALL-ZERO key while its wrapped
   * block announces the real one — the receiver unwraps correctly, derives
   * correctly, and fails the tag. That surfaces as `decrypt_failed`, whose
   * triage points at "the key schedule or the sender", so a memory-hygiene
   * measure would present as a crypto fault at every rotation boundary.
   *
   * `clear()` runs at teardown, after `stop()` has halted submits, so it has no
   * in-flight reader to race and overwrites unconditionally — both the current
   * generation and anything {@link rotate} retired but has not yet reached.
   * Recorded rather than left as an asymmetry for someone to "correct".
   */
  clear(): void {
    for (const material of this.#current.values()) material.key.fill(0);
    for (const material of this.#retired) material.key.fill(0);
    this.#current = new Map();
    this.#retired = [];
    this.#inFlight.clear();
  }
}
