// File: packages/sdk-core/src/media/frame/receivePath.ts
//
// The composed receive path: decode -> verify -> unwrap -> open, plus the
// wrapped-key cache and the replay window.
//
// ---------------------------------------------------------------------------
// VERIFY-BEFORE-DECRYPT IS STRUCTURAL, NOT SEQUENTIAL
// ---------------------------------------------------------------------------
//
//   decodeFrame(bytes)              -> DecodedFrame     (slices; no keys, no crypto)
//   verifyFrame(decoded, publicKey) -> VerifiedFrame    (Ed25519 over publisher‖payload)
//   openVerifiedFrame(verified, …)  -> plaintext
//
// `VerifiedFrame` carries a private brand that only `verifyFrame` can apply, so
// `openVerifiedFrame` and the wrap-cache write are UNREACHABLE without having
// verified. Inverting the order is a type error, not a reading error.
//
// This matters because "we call verify first" is a convention a refactor inverts
// silently, and the property it protects is the only control standing between an
// insider and attributed forgery: every member holds the meeting KEK, so every
// member can unwrap every other member's transmit key and seal a frame under
// another sender's key id (ADR-0036 §3 — possession of a key is not authorship).
// The identity key is resolved from `key_id.sender_id` and NOTHING ELSE, and it
// is supplied by the caller: there is deliberately no "trust the key that came
// with the frame" mode, because a compromised MC publishes the roster.
//
// ADR-0036 §4 also requires that a wrap from a frame that FAILS verification is
// never cached. That is why the cache write lives here, downstream of the brand,
// and why `unwrapTransmitKey` in `./sframe.ts` is PURE — it returns a key and
// writes nothing. There is no `unwrapAndCache` and no cache-priming export: a
// door that exists only for tests is still a door, and the next person to use it
// will not be writing a test.
//
// ---------------------------------------------------------------------------
// `wrap_key_id_mismatch` NAMES A CONDITION THIS CODE CANNOT DETECT
// ---------------------------------------------------------------------------
//
// The wrapped-key block is `kek_generation(2) || wrapped_key(32) || tag(16)`.
// There is NO bound-key-id field on the wire. So a wrap bound to another key id
// and a wrap under the wrong KEK produce THE SAME GCM tag mismatch — one bit,
// indistinguishable. The predicate implemented below is therefore, exactly:
//
//     KEK-unwrap failed AND a usable transmit key for this kid is already cached.
//
// It is NOT "the code detected a mis-bound wrap". The token's name invites a
// check that cannot be implemented, and the natural way to attempt it — widening
// the unwrap AAD to carry a declared key id — would DESTROY the binding, because
// the binding IS that the AAD is the frame's own key id. A rename toward the
// observable is tracked in `docs/TODO.md`; the spelling is frozen for this task.

import type { DecodedFrame } from './frameCodec.js';
import { verifyFrameSignature } from './ed25519.js';
import { keyIdCacheKey, unpackKeyId, type KeyIdParts } from './keyId.js';
import { openSframe, parseSframe, unwrapTransmitKey, type SframeObject } from './sframe.js';
import { deriveSframeKeys, sframeNonce } from './sframeKeySchedule.js';
import { AES_256_KEY_BYTES, SFRAME_CIPHER_SUITE_ID, SFRAME_SALT_BYTES } from './sframe.js';
import { cryptoReject, keyReject, type RejectReason } from './rejectReason.js';
import type { Bytes } from './hex.js';

/** Brand applied only by `verifyFrame`. Not exported: that is what makes it a gate. */
declare const VERIFIED: unique symbol;

/**
 * A frame whose Ed25519 signature verified against the identity key resolved
 * from its own `key_id.sender_id`.
 *
 * Constructible ONLY by `verifyFrame`.
 */
export interface VerifiedFrame {
  readonly [VERIFIED]: true;
  readonly frame: DecodedFrame;
  readonly sframe: SframeObject;
  readonly keyId: Bytes;
  readonly keyIdParts: KeyIdParts;
}

/**
 * Verify a decoded frame and admit it to the opening path.
 *
 * @param identityPublicKey resolved by the caller from `key_id.sender_id` and
 * nothing else. A frame carrying another sender's key id therefore fails HERE, at
 * verify, rather than later at decrypt — which is what makes the signature layer
 * load-bearing rather than decorative (ADR-0036 Assumption 4).
 *
 * @throws {FrameRejectedError} `signature_invalid` on any verification failure,
 * including an absent or unusable verifier. Fail closed: there is no
 * "unverifiable, therefore accepted" outcome.
 */
export async function verifyFrame(
  frame: DecodedFrame,
  identityPublicKey: Uint8Array,
): Promise<VerifiedFrame> {
  const sframe = parseSframe(frame.payload);
  const ok = await verifyFrameSignature(identityPublicKey, frame.signature, frame.signedRange);
  if (!ok) {
    throw cryptoReject(
      'signature_invalid',
      'frame signature did not verify against the sender key',
    );
  }
  return {
    frame,
    sframe,
    keyId: sframe.keyId,
    keyIdParts: unpackKeyId(sframe.keyId),
  } as VerifiedFrame;
}

/** What happened to the wrapped key carried by a frame, if any. */
export type WrapOutcome =
  /** No key-bearing flag on the frame; there was no wrap to process. */
  | 'absent'
  /** Unwrapped under this frame's own key id and cached. */
  | 'cached'
  /** Already held; the wrap was re-sent under the audio cadence. */
  | 'already_held'
  /**
   * A key-bearing frame carried a wrap under a KEK generation this receiver does
   * NOT hold, but a usable transmit key was already cached — so the frame PLAYS
   * off the cache. This is the KEK-ROTATION-LAG signal: the sender re-wrapped
   * under a generation whose push has not landed yet.
   *
   * Named for its cause, and legitimately — unlike `wrap_key_id_mismatch`, which
   * this code cannot substantiate from a one-bit AEAD failure, this state is a
   * DIRECT query of receiver state (`kekForGeneration` returned undefined). It
   * pairs with the `no_kek_for_generation` reject token: same condition, split by
   * whether a transmit key was cached — dropped vs played — exactly as
   * `unwrap_failed` and `wrap_key_id_mismatch` split an unwrap failure.
   *
   * NOT `'absent'` (the enum's highest-volume value — every non-key-bearing
   * frame): folding the rotation-lag signal into that bucket buries it in a noise
   * floor whose stated semantics are "nothing to do", and ADR-0036 §4 requires
   * the sustained case be observable. @observability owns this label and ruled
   * the spelling; task 19 counts it.
   */
  | 'kek_generation_not_held'
  /**
   * The wrap did not open under this frame's key id, but a usable transmit key
   * was already cached, so the frame is played and the wrap ignored.
   *
   * ADR-0036 §4. NOT a drop — it is the only token with `drops_frame: false`, and
   * it travels on this SUCCESS channel rather than in the thrown-error union so
   * that "count it as a drop" is not the path of least resistance. Collapsing it
   * into the drop counter breaks R-25's `received = played + sum(drops)` identity
   * in aggregate, long after the label set is frozen.
   */
  | 'wrap_key_id_mismatch';

/** A successfully opened frame. */
export interface OpenedFrame {
  readonly plaintext: Bytes;
  readonly wrapOutcome: WrapOutcome;
  /** Attribution. Derives from `key_id.sender_id` and nothing else. */
  readonly senderId: bigint;
}

/** Receiver state a caller supplies. */
export interface ReceiverKeys {
  /** Meeting KEKs by generation. */
  readonly kekForGeneration: (generation: number) => Uint8Array | undefined;
}

/**
 * Bounded per-sender replay window plus a per-(sender, stream) generation
 * high-water mark.
 *
 * ---------------------------------------------------------------------------
 * WHY THE BUDGET IS PER SENDER AND WHY THE HIGH-WATER MARK EXISTS
 * ---------------------------------------------------------------------------
 *
 * A single global LRU over `(sender, stream, generation)` is NOT a security
 * boundary. `stream` is 8 bits and `generation` is 40, so any roster member —
 * and the bar for membership is a meeting link — can mint an unbounded number of
 * frames across their OWN triples, walk a global LRU until another sender's
 * window is evicted, then replay that sender's captured frame successfully:
 * signature valid, tag valid, window gone. Nothing reds; the replay vector still
 * passes, because it never floods.
 *
 * Two properties close it:
 *   * The context budget is PER `sender_id`, so eviction pressure from one sender
 *     cannot reach another's state. A sender exhausting its own budget is
 *     self-harm; exhausting someone else's is the bug.
 *   * A per-(sender, stream) GENERATION HIGH-WATER MARK rejects any frame below
 *     the highest generation already seen for that pair. ADR-0036 §4 makes
 *     generation monotonic per sender across its membership and never reset, so
 *     this is free — and it survives window eviction, which turns the LRU from a
 *     security boundary into a plain MEMORY BOUND, which is what it should be.
 */
export class ReplayWindow {
  readonly #maxContextsPerSender: number;
  readonly #windowBits: number;
  /** `sender -> (stream|generation) -> {highest, bitmap}`, insertion-ordered for LRU. */
  readonly #bySender = new Map<string, Map<string, { highest: number; bits: bigint }>>();
  /** `sender -> stream -> highest generation seen`. Survives context eviction. */
  readonly #generationHighWater = new Map<string, Map<string, bigint>>();

  /**
   * @param maxContextsPerSender bound on tracked `(stream, generation)` contexts
   * per sender. Config, not a constant: the right value depends on deployment
   * (simulcast layers, expected rotation cadence), and CLAUDE.md prefers config
   * over hardcoding.
   * @param windowBits width of the sliding duplicate bitmap.
   */
  constructor(maxContextsPerSender = 32, windowBits = 64) {
    this.#maxContextsPerSender = maxContextsPerSender;
    this.#windowBits = windowBits;
  }

  /**
   * Record `streamSequence` for a key id. Returns `false` if it is a replay.
   *
   * A replayed frame carries a valid signature and a valid authentication tag —
   * it WAS legitimately produced — so no cryptographic check can reject it. Only
   * the receiver's window can.
   */
  admit(parts: KeyIdParts, streamSequence: number): boolean {
    const sender = parts.senderId.toString(16);
    const stream = parts.stream.toString(16);

    const hwBySender = this.#generationHighWater.get(sender) ?? new Map<string, bigint>();
    const highestGeneration = hwBySender.get(stream);
    if (highestGeneration !== undefined && parts.generation < highestGeneration) {
      // Below a generation already seen for this (sender, stream). Monotonicity
      // makes this impossible for an honest sender, and this check survives
      // context eviction — so it holds even when the window below has been
      // recycled.
      return false;
    }
    hwBySender.set(
      stream,
      highestGeneration === undefined
        ? parts.generation
        : parts.generation > highestGeneration
          ? parts.generation
          : highestGeneration,
    );
    this.#generationHighWater.set(sender, hwBySender);

    const contexts =
      this.#bySender.get(sender) ?? new Map<string, { highest: number; bits: bigint }>();
    const contextKey = `${stream}:${parts.generation.toString(16)}`;
    let ctx = contexts.get(contextKey);
    if (!ctx) {
      ctx = { highest: -1, bits: 0n };
      contexts.set(contextKey, ctx);
      // Per-sender LRU eviction. Bounded so an attacker-influenced key space
      // cannot exhaust memory; per sender so it cannot evict anyone else.
      while (contexts.size > this.#maxContextsPerSender) {
        const oldest = contexts.keys().next().value;
        if (oldest === undefined) break;
        contexts.delete(oldest);
      }
    }
    this.#bySender.set(sender, contexts);

    if (streamSequence > ctx.highest) {
      const shift = BigInt(streamSequence - ctx.highest);
      ctx.bits = shift >= BigInt(this.#windowBits) ? 1n : (ctx.bits << shift) | 1n;
      ctx.highest = streamSequence;
      const mask = (1n << BigInt(this.#windowBits)) - 1n;
      ctx.bits &= mask;
      return true;
    }
    const behind = ctx.highest - streamSequence;
    if (behind >= this.#windowBits) return false;
    const bit = 1n << BigInt(behind);
    if ((ctx.bits & bit) !== 0n) return false;
    ctx.bits |= bit;
    return true;
  }

  /**
   * Drop all replay state.
   *
   * No key material here — bitmaps and generation counters only — so this is a
   * memory-bound reset rather than a credential cleanup. Present for symmetry
   * with `TransmitKeyCache.clear()` so the session-teardown obligation covers
   * both, and so its absence is not read as deliberate. Caller wires it at task
   * 19 alongside the cache clear.
   */
  clear(): void {
    this.#bySender.clear();
    this.#generationHighWater.clear();
  }
}

/** A cached transmit key, with the wrapped block it was unwrapped from. */
interface CachedTransmitKey {
  readonly key: Uint8Array;
  /**
   * The exact wrapped block that produced `key`. Retained ONLY to answer "is this
   * frame's wrap the same one I already accepted?" — see `matchesCachedWrap`. It
   * is public ciphertext, already on the wire in every frame.
   */
  readonly wrap: Uint8Array;
}

/**
 * The transmit-key cache.
 *
 * Keyed by the RECEIVED key-id bytes, so two distinct wire key ids can never
 * collide regardless of how they decompose. Bounded per sender with LRU
 * eviction, for the same reason as the replay window, and cleared on session
 * teardown — nothing is retained past the session.
 *
 * The KEK itself is NOT held here: it is passed per call by the caller, so this
 * seam never becomes a KEK home.
 */
export class TransmitKeyCache {
  readonly #maxPerSender: number;
  readonly #bySender = new Map<string, Map<string, CachedTransmitKey>>();

  constructor(maxPerSender = 32) {
    this.#maxPerSender = maxPerSender;
  }

  #entry(parts: KeyIdParts, keyId: Uint8Array): CachedTransmitKey | undefined {
    return this.#bySender.get(parts.senderId.toString(16))?.get(keyIdCacheKey(keyId));
  }

  get(parts: KeyIdParts, keyId: Uint8Array): Uint8Array | undefined {
    return this.#entry(parts, keyId)?.key;
  }

  has(parts: KeyIdParts, keyId: Uint8Array): boolean {
    return this.#entry(parts, keyId) !== undefined;
  }

  /**
   * Whether `wrappedKeyWithTag` is byte-identical to the block this key was
   * unwrapped from.
   *
   * This is what lets the receive path honour ADR-0036 §4:413 — "a receiver
   * holding the key does no per-frame unwrap" — WITHOUT going blind to a
   * mis-bound wrap. Audio carries the wrap on every frame, and §4 guarantees one
   * transmit key wraps to ONE byte-identical ciphertext within a generation
   * (pinned independently by the `wrap_determinism_seq_lo`/`_hi` vector pair). So
   * in the steady state this comparison is equal and the AEAD is never invoked;
   * only a wrap that DIFFERS from the one already accepted costs an unwrap.
   *
   * DO NOT SIMPLIFY THIS INTO A KEY-PRESENCE CHECK (`has()`). It reads like the
   * same thing and is not:
   *
   *   * A presence-only skip never LOOKS at the wrap once a key is cached, so a
   *     frame carrying a wrap bound to another key id is accepted with the wrap
   *     silently ignored and never examined. `wrap_key_id_mismatch` then becomes
   *     unreachable IN PRINCIPLE rather than merely rare — the vector row
   *     `wrap_for_different_key_id` cannot produce its declared `outcome`, and no
   *     receiver could ever observe a mis-bound wrap at all.
   *   * Every other assertion on that row still passes under a presence-only
   *     skip, so the control evaporates with the suite green. That is what makes
   *     this worth a warning rather than a note.
   *
   * WHY BLOCK-IDENTITY IS SOUND IN BOTH FAILURE DIRECTIONS (@protocol, as the
   * contract owner): a mis-bound wrap can NEVER be byte-identical to a correctly
   * bound cached block, because the AAD differs, so the tag differs, so the bytes
   * differ. And a sender that VIOLATES determinism — a fresh wrap nonce per
   * frame, the bug the `wrap_determinism_seq_lo`/`_hi` pair exists to catch —
   * merely makes the receiver unwrap on every frame: correctness preserved, only
   * the skip lost. A KEK rotation likewise changes the block and forces a
   * re-unwrap under the new KEK. Difference-then-unwrap is always the safe
   * fallback; identity-then-skip extends no new trust.
   *
   * A byte comparison, not a cryptographic one: the wrapped block is public
   * ciphertext, so there is nothing here to leak by timing.
   */
  matchesCachedWrap(parts: KeyIdParts, keyId: Uint8Array, wrappedKeyWithTag: Uint8Array): boolean {
    const entry = this.#entry(parts, keyId);
    if (!entry) return false;
    if (entry.wrap.length !== wrappedKeyWithTag.length) return false;
    return entry.wrap.every((b, i) => b === wrappedKeyWithTag[i]);
  }

  /**
   * Install a transmit key.
   *
   * Deliberately NOT exported as a standalone priming helper — the only caller is
   * `openVerifiedFrame`, which requires a `VerifiedFrame`. Tests construct a
   * receiver already holding a key rather than mutating one through a production
   * entry point.
   */
  set(parts: KeyIdParts, keyId: Uint8Array, key: Uint8Array, wrap: Uint8Array): void {
    const sender = parts.senderId.toString(16);
    const forSender = this.#bySender.get(sender) ?? new Map<string, CachedTransmitKey>();
    // No zero on replace: the key id encodes the generation, so the same key id
    // is the same transmit key and the replaced buffer holds identical bytes.
    // This DEPENDS on the caller obligation in `unwrapTransmitKey` — one transmit
    // key per key id — and were that ever violated the missing zeroisation would
    // be the smallest consequence (it would be a (key, nonce) repeat under the
    // KEK). Only EVICTION and CLEAR zero, because those drop keys that will never
    // be re-derived; the asymmetry with the S-3 fill(0) paths is deliberate, not
    // an oversight to "correct" (@security).
    forSender.set(keyIdCacheKey(keyId), { key, wrap: Uint8Array.from(wrap) });
    while (forSender.size > this.#maxPerSender) {
      const oldestKey = forSender.keys().next().value;
      if (oldestKey === undefined) break;
      const evicted = forSender.get(oldestKey);
      // ADR-0028 §5: "clear all references AND overwrite key buffers." JS cannot
      // guarantee no engine-internal copy survives, but the ADR names the
      // practice, not the guarantee, and this is the cheap place to honour it.
      if (evicted) evicted.key.fill(0);
      forSender.delete(oldestKey);
    }
    this.#bySender.set(sender, forSender);
  }

  /**
   * Drop every transmit key, overwriting the buffers first.
   *
   * The CALLER must invoke this on session teardown — ADR-0028 §5's "explicit
   * token cleanup on disconnect/logout". At story task 15 there is no session
   * object to hook it into (the receive loop lands at task 19), so this is the
   * obligation and not yet its wiring; task 19 owns the call site. Tracked in
   * `docs/TODO.md`.
   */
  clear(): void {
    for (const forSender of this.#bySender.values()) {
      for (const entry of forSender.values()) entry.key.fill(0);
    }
    this.#bySender.clear();
  }
}

/**
 * Open a verified frame.
 *
 * Only reachable with a `VerifiedFrame`, so ADR-0036 §4's "a wrap from a frame
 * that fails verification is not cached" holds by construction.
 *
 * @throws {FrameRejectedError} with a bounded reason.
 */
export async function openVerifiedFrame(
  verified: VerifiedFrame,
  cache: TransmitKeyCache,
  keys: ReceiverKeys,
  replay?: ReplayWindow,
): Promise<OpenedFrame> {
  const { frame, sframe, keyId, keyIdParts } = verified;

  if (replay && !replay.admit(keyIdParts, frame.streamSequence)) {
    throw cryptoReject('replay_detected', 'frame replays a stream sequence already accepted');
  }

  let wrapOutcome: WrapOutcome = 'absent';
  const wrapped = frame.wrappedTransmitKey;
  if (wrapped) {
    if (cache.matchesCachedWrap(keyIdParts, keyId, wrapped.wrappedKeyWithTag)) {
      // The steady state. ADR-0036 §4:413: "a receiver holding the key does no
      // per-frame unwrap." Audio carries the wrap on EVERY frame, and §4
      // guarantees the block is byte-identical within a generation, so this arm
      // is the common case and the AEAD is not invoked.
      //
      // The comparison — rather than a bare `has()` — is what keeps a mis-bound
      // wrap observable. A receiver that skipped on presence alone would accept a
      // frame whose wrap is bound elsewhere without ever looking at it, and
      // `wrap_key_id_mismatch` would be unreachable in principle rather than
      // merely rare.
      wrapOutcome = 'already_held';
    } else {
      const kek = keys.kekForGeneration(wrapped.kekGeneration);
      if (!kek) {
        // Only fatal if the frame cannot be opened anyway. A receiver that already
        // holds this key id's transmit key can play the frame through a KEK
        // rotation whose push has not landed yet — exactly the transient §4
        // describes at join and at rotation. Dropping a playable frame because a
        // key we do not need is unavailable would turn a benign race into an
        // audio gap.
        if (!cache.has(keyIdParts, keyId)) {
          throw keyReject('no_kek_for_generation', 'no meeting KEK for the carried KEK generation');
        }
        // Playable off the cache, but the missing KEK is a real condition that
        // must not be reported as `'absent'` (which means "no wrap at all").
        wrapOutcome = 'kek_generation_not_held';
      } else {
        // The received key-id SLICE feeds the unwrap AAD. Never a value re-packed
        // from `keyIdParts` — that always succeeds and is merely wrong.
        const unwrapped = await unwrapTransmitKey({
          kek,
          keyId,
          wrappedKeyWithTag: wrapped.wrappedKeyWithTag,
        });
        if (unwrapped) {
          // Cached ONLY for the key id of the frame carrying it. That binding is
          // the AEAD itself, checked here at the unwrap site — the wrap opens
          // under this frame's own key id or it does not open at all.
          cache.set(keyIdParts, keyId, unwrapped, wrapped.wrappedKeyWithTag);
          wrapOutcome = 'cached';
        } else {
          wrapOutcome = 'wrap_key_id_mismatch';
        }
      }
    }
  }

  const transmitKey = cache.get(keyIdParts, keyId);
  if (!transmitKey) {
    // The unwrap failed AND nothing usable was cached, so the frame cannot be
    // opened. This is where `unwrap_failed` and `wrap_key_id_mismatch` diverge —
    // by RECEIVER STATE, not by anything observable about the failure.
    if (wrapOutcome === 'wrap_key_id_mismatch') {
      throw cryptoReject('unwrap_failed', 'the carried wrapped transmit key did not unwrap');
    }
    throw cryptoReject('no_transmit_key', 'no transmit key for this frame');
  }

  const derived = await deriveSframeKeys({
    hash: 'SHA-512',
    baseKey: transmitKey,
    // The ORIGINAL received 8-byte key id, not a re-pack. This is the RFC's
    // canonical derivation input, and NOT a compressed encoding of anything —
    // nobody converts it to the header's form, because the header has no key id.
    kid: keyId,
    cipherSuiteId: SFRAME_CIPHER_SUITE_ID,
    // Not bare literals: the derived key IS the AES-256-GCM transmit key, so
    // `keyBytes` must equal the AEAD key length by construction, and the derived
    // salt IS the nonce input, so its width must equal the GCM nonce width. Two
    // places encoding one value drift; these reference the one home.
    keyBytes: AES_256_KEY_BYTES,
    saltBytes: SFRAME_SALT_BYTES,
  });

  const plaintext = await openSframe({
    key: derived.key,
    nonce: sframeNonce(derived.salt, BigInt(frame.streamSequence)),
    // Deviation (1): the publisher region only, taken as a slice of the received
    // buffer by `decodeFrame`.
    aad: frame.aeadAad,
    object: sframe,
  });
  if (!plaintext) {
    throw cryptoReject('decrypt_failed', 'SFrame payload failed authentication');
  }

  return { plaintext, wrapOutcome, senderId: keyIdParts.senderId };
}

/** Every reject reason `openVerifiedFrame` and `verifyFrame` can produce. */
export const RECEIVE_PATH_REASONS: readonly RejectReason[] = [
  'signature_invalid',
  'replay_detected',
  'no_kek_for_generation',
  'no_transmit_key',
  'unwrap_failed',
  'decrypt_failed',
] as const;
