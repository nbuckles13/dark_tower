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
import {
  AES_256_KEY_BYTES,
  SFRAME_CIPHER_SUITE_ID,
  SFRAME_SALT_BYTES,
  fixedTimeEqual,
} from './sframe.js';
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
 * nothing else. Raw bytes, or an already-imported `CryptoKey` so a receiver can
 * import once per SENDER at roster update rather than once per frame. A frame carrying another sender's key id therefore fails HERE, at
 * verify, rather than later at decrypt — which is what makes the signature layer
 * load-bearing rather than decorative (ADR-0036 Assumption 4).
 *
 * @throws {FrameRejectedError} `signature_invalid` on any verification failure,
 * including an absent or unusable verifier. Fail closed: there is no
 * "unverifiable, therefore accepted" outcome.
 */
export async function verifyFrame(
  frame: DecodedFrame,
  identityPublicKey: Uint8Array | CryptoKey,
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
   * off the cache.
   *
   * WHAT PRODUCES IT — AND WHAT DOES NOT. Every cached transmit key records the
   * KEK generation it was unwrapped under (its SCOPE; see `TransmitKeyCache`),
   * and a key id resolves to exactly one scope. So this outcome needs ONE key id
   * arriving under a wrap that announces a generation OTHER than its scope — a
   * transmit key re-wrapped under another KEK. That condition has two halves,
   * forked purely by receiver state: this one (the announced generation is not
   * held) and `wrap_generation_conflict` (it is). In both the wrap is ignored and
   * the frame plays off the cache.
   *
   * THIS SDK NEVER DOES THAT, and an honest sender of it cannot reach this arm.
   * The wrap is computed once, at mint (`TransmitKeyManager`), and reused on
   * every frame of that generation; and on every KEK change the sender ROTATES
   * to a new transmit key synchronously with the install (R-13). A KEK change
   * therefore always arrives with a NEW key id — uncached, so a receiver
   * lacking that KEK DROPS the frame (`no_kek_for_generation` or
   * `kek_generation_stale`) rather than landing here. Cross-peer rotation lag
   * lands on the drop tokens, not on this outcome.
   *
   * What CAN reach it: a sender that re-wraps an existing transmit key under a
   * new KEK instead of rotating — which the frame format permits and which is
   * exactly the behaviour R-13 exists to end (a leaver holding the unwrapped key
   * keeps decrypting across the rotation). Read a sustained non-zero rate as a
   * NON-CONFORMING OR HOSTILE SENDER, never as normal rotation traffic. Do not
   * "restore" re-wrapping on the grounds that this arm expects it.
   *
   * NOT `kek_generation_stale`, the reject token one word away, and the two sit
   * on opposite sides of `received = accepted + sum(drops by reason)`. A wrap
   * announcing a KEK generation this receiver does not hold splits on ONE
   * question: is a usable transmit key for this frame's key id already cached?
   * CACHED means the frame is ACCEPTED and counted as the wrap outcome
   * kek_generation_not_held (reachable whether the unheld generation is newer or
   * older; here the cache decides, not the age). NOT CACHED
   * means the frame is DROPPED and counted as a reject reason, and only there
   * does direction matter: no_kek_for_generation when newer than the newest
   * held, kek_generation_stale when older than retention keeps.
   *
   * Named for its cause, and legitimately — unlike `wrap_key_id_mismatch`, which
   * this code cannot substantiate from a one-bit AEAD failure, this state is a
   * DIRECT query of receiver state (`kekForGeneration` returned undefined). It
   * pairs with the `no_kek_for_generation` reject token: same condition, split by
   * whether a transmit key was cached — dropped vs played — exactly as
   * `unwrap_failed` and `wrap_key_id_mismatch` split an unwrap failure.
   *
   * NOT `'absent'` (the enum's highest-volume value — every non-key-bearing
   * frame): folding a non-conforming-sender signal into that bucket buries it in
   * a noise floor whose stated semantics are "nothing to do", and ADR-0036 §4
   * requires the sustained case be observable. @observability owns this label and ruled
   * the spelling; task 19 counts it.
   */
  | 'kek_generation_not_held'
  /**
   * A key-bearing frame carried a wrap for an ALREADY-CACHED key id, announcing a
   * KEK generation this receiver DOES hold but which is not the generation that
   * key id was unwrapped under. The wrap is REFUSED — not unwrapped, not cached
   * into a second scope — and the frame plays off the cached key.
   *
   * The other half of `kek_generation_not_held`: same condition (one key id, a
   * wrap announcing a generation other than its scope), same cause (a sender
   * re-wrapping instead of rotating, which R-13 ends), forked only by whether
   * this receiver holds the announced generation. The refusal is what makes it a
   * conflict rather than an overwrite.
   *
   * WHY REFUSE (story 2 E-1, @security; ADR-0036 §4 invariant 3). Replay state
   * is scoped by the generation a key id was unwrapped under, and a frame with no
   * wrapped key finds its scope ONLY through the cached entry. Were this wrap
   * allowed to re-cache the key under the newer generation, the entry's scope
   * would move, and a captured wrapless frame already admitted under the old
   * scope would resolve to a bucket that never saw it and be admitted again —
   * valid signature, valid tag. One entry per key id, first-set-wins, closes it.
   *
   * An honest sender of this SDK cannot produce it (it rotates to a new key id
   * on every KEK change, R-13). Read a sustained non-zero rate as a
   * NON-CONFORMING OR HOSTILE SENDER. Deliberately not alerted: see its catalog
   * entry in `docs/observability/metrics/client.md`.
   */
  | 'wrap_generation_conflict'
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
  kekForGeneration(generation: number): Uint8Array | undefined;
  /**
   * Whether an unheld `generation` is OLDER than anything retained, rather than
   * newer than the current. Consulted only when a frame cannot be opened for want
   * of that generation's KEK, to choose between the two drop tokens.
   */
  isOlderThanRetained(generation: number): boolean;
}

/**
 * Default bound on tracked `(stream, generation)` replay contexts per sender.
 *
 * NOT {@link DEFAULT_TRANSMIT_KEYS_PER_SENDER}. The two agree today BY COINCIDENCE
 * and are independent tuning decisions — how many contexts to track for duplicate
 * detection versus how many unwrapped keys to hold. Do NOT hoist them to one
 * shared constant: collapsing them would be a false single source of truth that
 * silently couples two unrelated decisions, so retuning one would move the other.
 *
 * Exported so `config/clientConfig.ts` imports it as its own default rather than
 * restating the number. The dependency direction is `config -> frame`; this
 * module must never import from `config/`, which would couple the pure
 * codec/crypto layer to SDK configuration.
 */
export const DEFAULT_REPLAY_CONTEXTS_PER_SENDER = 32;

/**
 * Default sliding duplicate-bitmap width.
 *
 * A third, unrelated number — not a variant of either per-sender bound above.
 */
export const DEFAULT_REPLAY_WINDOW_BITS = 64;

/**
 * Default bound on cached unwrapped transmit keys per sender.
 *
 * NOT {@link DEFAULT_REPLAY_CONTEXTS_PER_SENDER} — see that constant.
 */
export const DEFAULT_TRANSMIT_KEYS_PER_SENDER = 32;

/**
 * Bounded replay window plus a transmit-key-generation high-water mark, both
 * SCOPED by `(kek_generation, sender_id)`.
 *
 * ---------------------------------------------------------------------------
 * WHY THE BUDGET IS PER (SCOPE, SENDER) AND WHY THE HIGH-WATER MARK EXISTS
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
 *   * The context budget is PER `(scope, sender)`, so eviction pressure from one
 *     sender cannot reach another's state — and, since scoping added a KEK
 *     generation axis, a sender flooding its own NEW scope cannot evict its own
 *     RETAINED one either. A budget shared across scopes would re-open the same
 *     attack with `generation` in place of `stream`: flood g+1, evict g, replay a
 *     captured g frame.
 *   * A per-(scope, sender, stream) HIGH-WATER MARK on the transmit-key
 *     generation rejects any frame below the highest already seen there. It
 *     survives context eviction, which turns the LRU from a security boundary
 *     into a plain MEMORY BOUND, which is what it should be.
 *
 * Per-scope budgets do not grow without bound: a scope exists only for a KEK
 * generation that was HELD when it was created, and is discarded when that
 * generation leaves retention (see "LIFETIME" below), so a sender has at most
 * two live scopes and the bound is twice the per-scope budget.
 *
 * ---------------------------------------------------------------------------
 * WHERE CROSS-GENERATION PROTECTION COMES FROM (story 2 R-16; ADR-0036 §4)
 * ---------------------------------------------------------------------------
 *
 * The high-water is monotonic WITHIN one scope and nowhere else. Transmit-key
 * generations restart across scopes: when MC exhausts the 16-bit sender-id space
 * it resets the KEK epoch and reissues ids, and a reissued id's new holder
 * starts again at transmit-key generation 0. An unscoped high-water held from the
 * DEPARTED holder would drop every one of the new holder's frames, permanently.
 *
 * So nothing here protects ACROSS generations, and nothing needs to: that is KEK
 * RETENTION's job. A scope lives exactly as long as its KEK generation is held
 * (current, or the one retained previous), and a frame under a generation that
 * has left retention is refused before it reaches this object at all
 * (`kek_generation_stale`). Within that bound, a frame resolves to the scope its
 * own wrap or its cached key was unwrapped under — never the "current" one, so a
 * captured frame always lands in the bucket that already saw it.
 *
 * A CONSEQUENCE, DELIBERATE: a frame sealed under g that arrives after the
 * sender's first g+1 frame is ACCEPTED while g is retained (subject to g's own
 * window). That is what ADR-0036 §4 buys retention for — "frames in flight
 * across a rotation" — and the unscoped high-water silently negated it. Do not
 * "tighten" it back. Reordering WITHIN one generation, across a transmit-key
 * rotation, still drops on the high-water: there no independent bound backs a
 * looser rule, because the high-water IS the control that keeps the LRU a memory
 * bound. Different treatment because different guarantees are available, and
 * the within-generation direction fails closed.
 *
 * THE MC INVARIANT THIS RESTS ON (ADR-0036 §4 invariant 1): within one KEK
 * generation a `sender_id` is bound to at most one identity, EVER — never
 * reissued even after its holder leaves; the only reissue path is an epoch reset,
 * which always bumps the generation first. Were an id reissued inside one
 * generation, its new holder would land in the old holder's scope, under the old
 * holder's high-water, and S-1 would recur. MC gets this from a skip set computed
 * ONCE at the reset and immutable for the generation's life; the PROHIBITED
 * implementation is a liveness query in the allocation path ("skip ids held by
 * live members"), which lets an id whose holder leaves mid-generation be
 * reissued inside it. MC's side of this contract is in
 * `crates/mc-service/src/media_admission/sender_id.rs`.
 *
 * ---------------------------------------------------------------------------
 * LIFETIME
 * ---------------------------------------------------------------------------
 *
 * Derived state with no lifetime of its own. `discardGeneration` is called from
 * the one listener on `MeetingKekHolder`'s generation-dropped event, together
 * with `TransmitKeyCache.discardGeneration`; there is no other timer.
 */
export class ReplayWindow {
  readonly #maxContextsPerSender: number;
  readonly #windowBits: number;
  /** `kek generation -> sender -> state`. */
  readonly #scopes = new Map<number, Map<string, ScopedSenderReplay>>();

  /**
   * @param maxContextsPerSender bound on tracked `(stream, generation)` contexts
   * per sender PER SCOPE. Config, not a constant: the right value depends on
   * deployment (simulcast layers, expected rotation cadence), and CLAUDE.md
   * prefers config over hardcoding.
   * @param windowBits width of the sliding duplicate bitmap.
   */
  constructor(
    maxContextsPerSender = DEFAULT_REPLAY_CONTEXTS_PER_SENDER,
    windowBits = DEFAULT_REPLAY_WINDOW_BITS,
  ) {
    this.#maxContextsPerSender = maxContextsPerSender;
    this.#windowBits = windowBits;
  }

  /**
   * Record `streamSequence` for a key id within `scope`. Returns `false` if it is
   * a replay.
   *
   * A replayed frame carries a valid signature and a valid authentication tag —
   * it WAS legitimately produced — so no cryptographic check can reject it. Only
   * the receiver's window can.
   *
   * @param scope the KEK generation this frame's transmit key was unwrapped
   * under — its PROVENANCE, from `TransmitKeyCache.scopeOf`. Never the receiver's
   * current generation: that would drop a captured frame replayed just after a
   * rotation into a fresh, empty bucket.
   */
  admit(scope: number, parts: KeyIdParts, streamSequence: number): boolean {
    const sender = parts.senderId.toString(16);
    const stream = parts.stream.toString(16);

    const bySender = this.#scopes.get(scope) ?? new Map<string, ScopedSenderReplay>();
    this.#scopes.set(scope, bySender);
    let state = bySender.get(sender);
    if (!state) {
      state = { contexts: new Map(), highWater: new Map() };
      bySender.set(sender, state);
    }

    const highestGeneration = state.highWater.get(stream);
    if (highestGeneration !== undefined && parts.generation < highestGeneration) {
      // Below a generation already seen for this (scope, sender, stream).
      // Monotonicity within a scope makes this impossible for an honest sender,
      // and this check survives context eviction — so it holds even when the
      // window below has been recycled.
      return false;
    }
    if (highestGeneration === undefined || parts.generation > highestGeneration) {
      state.highWater.set(stream, parts.generation);
    }

    const contextKey = `${stream}:${parts.generation.toString(16)}`;
    let ctx = state.contexts.get(contextKey);
    if (!ctx) {
      ctx = { highest: -1, bits: 0n };
      state.contexts.set(contextKey, ctx);
      // Per-(scope, sender) LRU eviction. Bounded so an attacker-influenced key
      // space cannot exhaust memory; per sender AND per scope so it can evict
      // neither anyone else's state nor the sender's own retained scope.
      while (state.contexts.size > this.#maxContextsPerSender) {
        const oldest = state.contexts.keys().next().value;
        if (oldest === undefined) break;
        state.contexts.delete(oldest);
      }
    }

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
   * Drop every sender's replay state in the scope of KEK `generation`.
   *
   * Called ONLY with that generation leaving retention, in the same synchronous
   * listener that discards its transmit keys (C-3). Never on a roster change: a
   * rebind or a leave must keep replay state, or a rebind-BACK replay opens.
   */
  discardGeneration(generation: number): void {
    this.#scopes.delete(generation);
  }

  /**
   * Drop all replay state.
   *
   * No key material here — bitmaps and generation counters only — so this is a
   * memory-bound reset rather than a credential cleanup. Present for symmetry
   * with `TransmitKeyCache.clear()` so the session-teardown obligation covers
   * both, and so its absence is not read as deliberate.
   */
  clear(): void {
    this.#scopes.clear();
  }
}

/** One sender's replay state within one scope. */
interface ScopedSenderReplay {
  /** `(stream|generation) -> {highest, bitmap}`, insertion-ordered for LRU. */
  readonly contexts: Map<string, { highest: number; bits: bigint }>;
  /** `stream -> highest transmit-key generation seen`. Survives context eviction. */
  readonly highWater: Map<string, bigint>;
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
  /**
   * The KEK generation this key was unwrapped under: its SCOPE. Recorded at
   * unwrap time from the wrap that produced it, and FIXED for the entry's life.
   *
   * This is the ONLY way a frame without a wrapped key finds its replay scope —
   * the generation is not on every frame, only on key-bearing ones. It is never
   * recomputed from the receiver's "current" generation.
   */
  readonly generation: number;
}

/**
 * The transmit-key cache.
 *
 * Keyed by the RECEIVED key-id bytes, so two distinct wire key ids can never
 * collide regardless of how they decompose. That is true but NO LONGER
 * SUFFICIENT: once MC reissues sender ids (story 2 R-16), a departed holder and a
 * new holder of one id produce byte-identical key ids under DIFFERENT KEK
 * generations. What keeps them apart is (a) each entry's recorded scope, (b)
 * ONE ENTRY PER KEY ID — the map is keyed by key id, so a second scope for one
 * key id is unrepresentable, and `set` refuses rather than moves an entry across
 * generations — and (c) the departed holder's entries being purged on its roster
 * removal (`purgeSender`) before the new holder's key can verify.
 *
 * Bounded per sender with LRU eviction, for the same reason as the replay
 * window, and cleared on session teardown — nothing is retained past the
 * session. Eviction here is a memory bound only: an evicted key id's replay
 * state is untouched, so a re-established entry lands in the scope that already
 * saw its frames.
 *
 * The KEK itself is NOT held here: it is passed per call by the caller, so this
 * seam never becomes a KEK home.
 */
export class TransmitKeyCache {
  readonly #maxPerSender: number;
  readonly #bySender = new Map<string, Map<string, CachedTransmitKey>>();

  constructor(maxPerSender = DEFAULT_TRANSMIT_KEYS_PER_SENDER) {
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
   * The KEK generation this key id's transmit key was unwrapped under, or
   * `undefined` if it is not cached. The replay scope for every frame under it.
   */
  scopeOf(parts: KeyIdParts, keyId: Uint8Array): number | undefined {
    return this.#entry(parts, keyId)?.generation;
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
   * Install a transmit key unwrapped under KEK `generation`. Returns `false` if
   * REFUSED.
   *
   * Deliberately NOT exported as a standalone priming helper — the only caller is
   * `openVerifiedFrame`, which requires a `VerifiedFrame`. Tests construct a
   * receiver already holding a key rather than mutating one through a production
   * entry point.
   *
   * ONE ENTRY PER KEY ID, FIRST-SET-WINS (story 2 E-1; ADR-0036 §4 invariant 3):
   *
   *   * No entry                              -> installed, scope = `generation`.
   *   * Entry under a DIFFERENT generation    -> REFUSED; `key` is zeroized and
   *     the entry is untouched. Moving the entry would move its replay scope, and
   *     a captured wrapless frame would then resolve to a bucket that never saw
   *     it. `openVerifiedFrame` refuses this case before unwrapping; this arm
   *     keeps the rule true across that function's await, where a concurrent
   *     open may have installed the key id first.
   *   * Same generation, IDENTICAL key bytes  -> the entry is kept (scope and key
   *     buffer), only its wrap is updated; the duplicate buffer is zeroized. No
   *     zeroization of the kept key is needed, because it is the same key — the
   *     part of the old no-zero-on-replace rule @security ruled, and still true.
   *   * Same generation, DIFFERENT key bytes  -> the old buffer is zeroized, then
   *     replaced. This is two transmit keys under one key id under one KEK, which
   *     ADR-0036 §4 invariant 1 plus transmit-key monotonicity FORECLOSES for
   *     honest parties; the branch is retained as defence in depth so that, were
   *     it ever reached, no superseded key survives un-zeroized. (The rule it
   *     replaces skipped zeroization on replace on the premise "one key id is one
   *     transmit key", which R-16's reissue made false across generations.)
   */
  set(
    parts: KeyIdParts,
    keyId: Uint8Array,
    key: Uint8Array,
    wrap: Uint8Array,
    generation: number,
  ): boolean {
    const sender = parts.senderId.toString(16);
    const forSender = this.#bySender.get(sender) ?? new Map<string, CachedTransmitKey>();
    const cacheKey = keyIdCacheKey(keyId);
    const existing = forSender.get(cacheKey);
    if (existing) {
      if (existing.generation !== generation) {
        key.fill(0);
        return false;
      }
      if (fixedTimeEqual(existing.key, key)) {
        if (existing.key !== key) key.fill(0);
        forSender.set(cacheKey, { key: existing.key, wrap: Uint8Array.from(wrap), generation });
        return true;
      }
      existing.key.fill(0);
    }
    forSender.set(cacheKey, { key, wrap: Uint8Array.from(wrap), generation });
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
    return true;
  }

  /**
   * Drop every transmit key cached for ONE sender, IN EVERY SCOPE, overwriting
   * the buffers first.
   *
   * Called when that sender's roster binding changes or is forgotten (story 2
   * R-18). ALL SCOPES, not the current one (@security F-8): on a rebind or a
   * leave at id X, the keys under an OLDER, still-retained generation belong to
   * the DEPARTED holder — precisely the material the purge exists to destroy — so
   * a purge that read "this sender" as "this sender in the current scope" would
   * leave it resident. Structural here: a sender's entries of every scope share
   * one per-sender map.
   *
   * Touches NOTHING else — in particular not the replay window, in ANY scope,
   * which a separate object owns and which must survive a rebind or a
   * rebind-back replay opens.
   *
   * A RACE THIS DOES NOT CLOSE, AND WHY IT DOES NOT NEED TO: a frame that
   * verified under the old key just before the rebind can finish its unwrap and
   * `set()` a transmit key after this purge. That key can only ever open frames
   * that VERIFY against the sender's CURRENT roster key — verification precedes
   * opening structurally (`VerifiedFrame`) — and every member holds the KEK and
   * can unwrap any wrap anyway, so the residual grants nothing a member lacks.
   * A per-sender epoch check at `set()` would close it; it is not worth the
   * state.
   */
  purgeSender(senderId: bigint): void {
    const key = senderId.toString(16);
    const forSender = this.#bySender.get(key);
    if (!forSender) return;
    for (const entry of forSender.values()) entry.key.fill(0);
    this.#bySender.delete(key);
  }

  /**
   * Drop every transmit key unwrapped under KEK `generation`, for every sender,
   * overwriting the buffers first.
   *
   * Called ONLY with that generation leaving retention, in the same synchronous
   * listener that discards its replay scope (C-3), so a scope's keys and its
   * replay state go together — never one without the other.
   */
  discardGeneration(generation: number): void {
    for (const [sender, forSender] of this.#bySender) {
      for (const [cacheKey, entry] of forSender) {
        if (entry.generation !== generation) continue;
        entry.key.fill(0);
        forSender.delete(cacheKey);
      }
      if (forSender.size === 0) this.#bySender.delete(sender);
    }
  }

  /**
   * Drop every transmit key, overwriting the buffers first.
   *
   * The CALLER must invoke this on session teardown — ADR-0028 §5's "explicit
   * token cleanup on disconnect/logout". `AudioPipeline` registers it with the
   * teardown registry.
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
 * ---------------------------------------------------------------------------
 * SCOPE FIRST, REPLAY SECOND — A SECURITY ORDERING, NOT A REFACTOR
 * ---------------------------------------------------------------------------
 *
 * Replay state is scoped by the KEK generation a frame's transmit key was
 * unwrapped under (see `ReplayWindow`), so the scope must be known before the
 * window can be consulted — and for a key id not yet cached, the scope comes
 * into existence only by unwrapping. So `admit` runs AFTER key resolution. This
 * trades away "replay check before any AEAD work"; @security ruled the trade:
 *
 *   * A `VerifiedFrame` has ALREADY passed Ed25519 verification against a roster
 *     key, so anyone reaching the unwrap is a rostered member — the unwrap is not
 *     an unauthenticated amplifier.
 *   * The added work is ONE AES-GCM open over a 48-byte block, on the uncached
 *     path only, beside an Ed25519 verify already spent on the same frame. The
 *     DoS ratio does not move.
 *   * The common path does not regress: a cached key id resolves its scope by
 *     map lookup, with no AEAD before `admit`.
 *
 * What it buys is an unambiguous scope, which is what closes story 2 S-1. Do not
 * restore the old order on DoS grounds without re-deriving the above.
 *
 * A consequence: a replay of a frame whose generation has left retention drops
 * as `kek_generation_stale`, not `replay_detected` — refused before any replay
 * state is consulted, and so never re-creating a discarded scope.
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

  // The key id's scope, if cached — and the first place RETENTION IS ENFORCED,
  // not merely observed.
  //
  // WHY THIS RUNS ON EVERY FRAME, KEY-BEARING OR NOT. `min(W/2, ceiling)` is a
  // SECURITY bound: it caps how long a departed member's KEK stays usable at a
  // receiver, which is half the leave-exposure figure ADR-0036 §4 accounts for. A
  // timer alone cannot hold that bound in a browser — background tabs throttle
  // timers hard — so a backgrounded participant would otherwise keep opening
  // frames under a previous generation long past the window. `kekForGeneration`
  // is the holder's LAZY backstop, so calling it here ties the bound to the very
  // thing that would exercise it: a frame cannot open under an expired scope,
  // because opening is what checks. Audio is key-bearing every frame, but video
  // is not, so a key-bearing-only check would leave a video receiver as exactly
  // the unbounded case.
  //
  // The call can discard this scope SYNCHRONOUSLY (the holder's drop event fires
  // the pipeline's listener, which zeroizes this entry's key). That is why the
  // generation is captured BEFORE the check and the throw uses the captured
  // value: nothing downstream may touch an entry the listener has already
  // zeroized.
  let scope = cache.scopeOf(keyIdParts, keyId);
  if (scope !== undefined && !keys.kekForGeneration(scope)) {
    // Fail closed on the CACHED scope, without re-reading the entry. A frame
    // whose key id is bound to an expired generation is `kek_generation_stale`
    // whether or not it carries a wrap — and for a wrapless frame this is the
    // only path that can name the generation at all, so re-reading (and finding
    // the entry gone) would downgrade a retention expiry to `no_transmit_key`.
    // It also fails closed on a key id bound to an EXPIRED generation, which no
    // conforming sender ever has (R-13 gives a new key id on every KEK change).
    // That is NOT a fix for the re-wrap residual, and must not be read as one
    // (@security): the expired entry is discarded at the FIRST frame after
    // expiry, so this covers about one frame's opportunity. After that the key id
    // is unbound, a wrap under the live generation is a FIRST BINDING rather than
    // a conflict, and a fresh entry and bucket are created. Closing that residual
    // would mean remembering expired key ids indefinitely — the unbounded
    // per-sender state the context budget exists to prevent — and its impact is
    // confined to a non-conforming sender's own media. Accepted for that reason,
    // not for being small.
    //
    // One MECHANISM, three DISPOSITIONS — the pattern is a prompt to ask, never a
    // precedent for the answer (@security). Discarding state necessarily discards
    // the ability to tell a legitimate new binding from a returning one, because
    // the binding WAS the memory. Here that blind spot is ACCEPTED. At
    // `setup/rosterKeys.ts`'s forget path it is MITIGATED — the transmit keys are
    // purged so the blind spot is harmless. At `TransmitKeyCache` eviction it is
    // SAFE BY CONSTRUCTION — re-deriving the key needs a signed key-bearing frame,
    // which an attacker able to produce did not need the cache for. Do not read
    // the first as licence for a fourth site.
    throw kekDrop(keys, scope);
  }

  let wrapOutcome: WrapOutcome = 'absent';
  /** Unwrapped by THIS call and not yet cached: installed below, synchronously. */
  let fresh: { readonly key: Bytes; readonly generation: number } | undefined;
  const wrapped = frame.wrappedTransmitKey;
  if (wrapped) {
    if (scope !== undefined && wrapped.kekGeneration !== scope) {
      // One key id, one scope (E-1). A wrap announcing any OTHER generation is
      // never unwrapped into a second scope; the frame plays off the cached key.
      // The two halves are forked purely by receiver state — see both outcomes.
      wrapOutcome = keys.kekForGeneration(wrapped.kekGeneration)
        ? 'wrap_generation_conflict'
        : 'kek_generation_not_held';
    } else if (
      scope !== undefined &&
      cache.matchesCachedWrap(keyIdParts, keyId, wrapped.wrappedKeyWithTag)
    ) {
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
      // Nothing cached, or cached under THIS generation with a different wrap.
      // Either way the generation is the wrap's own — the key id's provenance.
      const kek = keys.kekForGeneration(wrapped.kekGeneration);
      if (!kek) {
        // Reached only with nothing cached (a cached scope was checked held
        // above), so the frame cannot be opened. Direction matters only here:
        // older than retention keeps and newer than the current are different
        // faults with different remedies. No generation value reaches either
        // message.
        throw kekDrop(keys, wrapped.kekGeneration);
      }
      // The received key-id SLICE feeds the unwrap AAD. Never a value re-packed
      // from `keyIdParts` — that always succeeds and is merely wrong.
      const unwrapped = await unwrapTransmitKey({
        kek,
        keyId,
        wrappedKeyWithTag: wrapped.wrappedKeyWithTag,
      });
      if (unwrapped) {
        fresh = { key: unwrapped, generation: wrapped.kekGeneration };
      } else {
        wrapOutcome = 'wrap_key_id_mismatch';
      }
    }
  }

  // ---- SYNCHRONOUS FROM HERE TO `set`: no await may separate the scope's
  // retention check from the replay state it gates or the cache entry it
  // installs. ----

  if (fresh && !keys.kekForGeneration(fresh.generation)) {
    // The generation left retention DURING the unwrap. Admitting or caching now
    // would create a scope after its discard already ran, which nothing would
    // ever discard (C-3: never leaked).
    fresh.key.fill(0);
    throw kekDrop(keys, fresh.generation);
  }
  const installedMeanwhile = fresh && cache.scopeOf(keyIdParts, keyId);
  if (fresh && installedMeanwhile !== undefined && installedMeanwhile !== fresh.generation) {
    // A concurrent open installed this key id under ANOTHER generation during
    // the unwrap. First-set-wins holds across the await: this unwrap is
    // discarded and the frame resolves to the existing entry's scope.
    fresh.key.fill(0);
    fresh = undefined;
    wrapOutcome = 'wrap_generation_conflict';
  }

  scope = fresh?.generation ?? cache.scopeOf(keyIdParts, keyId);
  if ((fresh?.key ?? cache.get(keyIdParts, keyId)) === undefined || scope === undefined) {
    // The unwrap failed AND nothing usable was cached, so the frame cannot be
    // opened. This is where `unwrap_failed` and `wrap_key_id_mismatch` diverge —
    // by RECEIVER STATE, not by anything observable about the failure.
    if (wrapOutcome === 'wrap_key_id_mismatch') {
      throw cryptoReject('unwrap_failed', 'the carried wrapped transmit key did not unwrap');
    }
    throw cryptoReject('no_transmit_key', 'no transmit key for this frame');
  }
  if (!keys.kekForGeneration(scope)) {
    fresh?.key.fill(0);
    throw kekDrop(keys, scope);
  }
  // That check is itself the lazy backstop, so it may have just discarded this
  // scope and zeroized the cached key. Re-read AFTER it, and never hold a
  // reference taken before it: opening against a zeroized buffer would fail the
  // AEAD and report `decrypt_failed`, blaming a crypto fault for a retention
  // expiry.
  const transmitKey = fresh?.key ?? cache.get(keyIdParts, keyId);
  if (!transmitKey) {
    fresh?.key.fill(0);
    throw kekDrop(keys, scope);
  }

  if (replay && !replay.admit(scope, keyIdParts, frame.streamSequence)) {
    // A frame the window rejects changes NO key state: a freshly unwrapped key is
    // discarded rather than cached, so a replay cannot re-populate a cache a
    // roster purge just emptied.
    fresh?.key.fill(0);
    throw cryptoReject('replay_detected', 'frame replays a stream sequence already accepted');
  }

  if (fresh && wrapped) {
    // Cached ONLY for the key id of the frame carrying it. That binding is the
    // AEAD itself, checked at the unwrap site — the wrap opens under this frame's
    // own key id or it does not open at all. Still synchronous with the checks
    // above, so `set` cannot meet an entry under another generation here; its
    // own refusal arm is the backstop, and the key is re-read from the cache so
    // a refused (zeroized) buffer is never used.
    wrapOutcome = cache.set(
      keyIdParts,
      keyId,
      fresh.key,
      wrapped.wrappedKeyWithTag,
      fresh.generation,
    )
      ? 'cached'
      : 'wrap_generation_conflict';
  }
  // No await since `transmitKey` was read, so it is still the live buffer.
  const openingKey = transmitKey;

  const derived = await deriveSframeKeys({
    hash: 'SHA-512',
    baseKey: openingKey,
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

/**
 * The drop for a frame that needs a KEK generation this receiver does not hold,
 * split by direction: older than retention keeps, or newer than the current.
 */
function kekDrop(keys: ReceiverKeys, generation: number) {
  if (keys.isOlderThanRetained(generation)) {
    return keyReject(
      'kek_generation_stale',
      'the carried KEK generation is older than this receiver retains',
    );
  }
  return keyReject('no_kek_for_generation', 'no meeting KEK for the carried KEK generation');
}

/** Every reject reason `openVerifiedFrame` and `verifyFrame` can produce. */
export const RECEIVE_PATH_REASONS: readonly RejectReason[] = [
  'signature_invalid',
  'replay_detected',
  'no_kek_for_generation',
  'kek_generation_stale',
  'no_transmit_key',
  'unwrap_failed',
  'decrypt_failed',
] as const;
