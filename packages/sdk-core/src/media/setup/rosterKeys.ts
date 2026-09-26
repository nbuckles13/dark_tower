// File: packages/sdk-core/src/media/setup/rosterKeys.ts
//
// The `sender_id -> identity verification key` resolver (ADR-0036 §3).
//
// ---------------------------------------------------------------------------
// FAIL CLOSED ON AN ABSENT KEY. THIS IS THE ENTIRE REPLACEMENT CONTROL.
// ---------------------------------------------------------------------------
//
// MC ADMITS a joiner whose `JoinRequest.identity_public_key` is empty and
// publishes an EMPTY (length 0, never a zero-filled placeholder)
// `Participant.identity_public_key`. `signaling.proto` states the consumer rule:
// *"Empty means NO KEY PUBLISHED: consumers MUST fail closed and MUST NOT fall
// back to accepting unsigned frames."*
//
// Rejecting empty at admission was considered and REVERSED — validation is
// length-only with no proof of possession and no `cnf` binding, so a hostile
// client is admitted anyway with 32 random bytes, and the reject would exclude
// only honest clients that have not implemented the field. What that reversal
// gave up is exactly this: reject made "a keyless participant on the roster"
// unreachable, so no consumer could fail open on one. **That protection now
// lives here, and nothing in MC will catch a violation of it.**
//
// THE DANGEROUS IMPLEMENTATION IS NOT "FORGOT TO CHECK". It is reading empty as
// *"no key, so skip verification"*, which fails open and is the natural reading
// of an absent field. The correct behaviour is *"no key, so this participant's
// frames are undecodable-by-policy: DROP them"* — which is what
// `identityKeyFor` returning `undefined` means to the ingress path, where it
// becomes a counted `no_roster_entry` drop before any decrypt.
//
// A participant with a MALFORMED key (present but not 32 bytes) is treated
// identically: no usable key, frames dropped and counted. Never accepted, never
// coerced.
//
// ---------------------------------------------------------------------------
// BOUNDED, AND IMPORTED ONCE PER SENDER
// ---------------------------------------------------------------------------
//
// Roster membership's bar is a meeting link, so anything unbounded and keyed by
// `sender_id` is attacker-influenced memory growth. The map is bounded with LRU
// eviction — the same per-sender-bounded shape `ReplayWindow` and
// `TransmitKeyCache` already use, rather than a third eviction idiom.
//
// `crypto.subtle.importKey` runs ONCE PER SENDER at roster update, never on the
// per-frame path.
//
// ---------------------------------------------------------------------------
// SELF IS ON THIS ROSTER, AND THAT IS NOT A SELF-TRUST BRANCH
// ---------------------------------------------------------------------------
//
// MC does not include the joiner in its own `existing_participants`, so in a
// loopback the client must add its own `(sender_id, public key)` — both locally
// known — or its own returned frames find no entry. This is roster POPULATION,
// not a verification bypass: the frame still runs the identical
// resolve-then-Ed25519-verify path a peer's frame takes, and a frame that fails
// to verify against that key is dropped exactly as a peer's would be. There is
// no branch anywhere that trusts a frame because this client signed it.
//
// ---------------------------------------------------------------------------
// A REBIND PURGES THAT SENDER'S TRANSMIT KEYS — AND NEVER ITS REPLAY STATE
// ---------------------------------------------------------------------------
//
// Story 2 R-18. MC never legitimately rebinds a LIVE sender id to a different
// identity key. It DOES reissue a sender id to a new identity after a KEK-epoch
// reset (R-16) — but only an id whose holder has LEFT, and the old holder's
// `ParticipantLeft` reaches this client first, so the reissue arrives as a FIRST
// binding (see `remove()`), not a rebind. A rebind therefore means an MC defect,
// a cache-poisoning attempt, or a `ParticipantLeft` that MC dropped under
// outbound backpressure (`try_send`) so the removal never arrived — see
// `MEDIA_ROSTER_KEY_CHANGES` in `mediaMetrics.ts` for how the three are told
// apart, and `docs/TODO.md` for the delivery gap. In every case the transmit
// keys this client unwrapped for that sender under the OLD binding must not keep
// opening frames — and the rebind arm purges exactly what a removal would have. The decision compares the published BYTES (public, so a plain
// comparison), and it is made SYNCHRONOUSLY at `upsert` entry — before the
// `importKey` await — so two updates for one sender cannot resolve out of order
// and let the older key win.
//
//   * equal bytes                       -> no-op (an LRU touch only)
//   * not on the roster, or keyless     -> a FIRST BINDING, not a rebind
//   * key present -> different key      -> REBIND: purge + count
//   * key present -> empty / malformed  -> a DOWNGRADE, treated as a rebind:
//                                          purge + count, entry known-keyless
//
// On a rebind the entry goes KNOWN-KEYLESS immediately, so frames arriving while
// the new key imports fail closed as `no_roster_entry` rather than verifying
// against the old key. An import that fails leaves the entry known-keyless — it
// never leaves the previous `CryptoKey` in place.
//
// Replay state (contexts, bitmaps, generation high-water, in EVERY KEK-generation
// scope) is NOT touched by any path in this file: clearing it would permit a
// rebind-BACK replay, in which an attacker rebinds away and back and replays
// frames the window had rejected. A reissued id's new holder is not blocked by
// that retained state, because it sends under a NEWER KEK generation and replay
// state is scoped by generation (`ReplayWindow`); the old holder's scope ends
// when its generation leaves retention, not on any roster event.
//
// Forgetting a sender's bytes — `remove()`, or LRU eviction — also invalidates
// its transmit keys, because afterwards a re-add is indistinguishable from a
// first binding and the rebind check above would be blind to it. That purge is
// memory hygiene and is not counted as a rebind.
//
// The roster key stays TRUST-ON-FIRST-USE. Nothing here makes it anything more.

import { ED25519_PUBLIC_KEY_BYTES, importVerifyKey } from '../frame/ed25519.js';

/** One roster participant's verification material, as the media path needs it. */
export interface RosterIdentityEntry {
  /** Meeting-scoped numeric sender id, `1..=65535`. Absent means no key-id mapping. */
  readonly senderId: number;
  /**
   * The raw Ed25519 public key as MC published it.
   *
   * Trust-on-first-use and nothing more: this story performs no attestation of
   * any kind, and `identity_public_key` is the honest spelling. A signature
   * verifying against it proves only that every frame came from the same
   * keyholder; it never proves WHO. Closing that is the attestation story
   * (ADR-0036 addendum).
   *
   * EMPTY means no key published. See the module header.
   */
  readonly identityPublicKey: Uint8Array;
}

/** Why a sender's cached transmit keys must be dropped. */
export type TransmitKeyInvalidation =
  /** A live sender was bound to DIFFERENT, well-formed key bytes. Counted. */
  | 'rebind'
  /**
   * A live sender's key was replaced by an empty, wrong-width or otherwise
   * unusable one. Counted SEPARATELY from `rebind`: the two point at different
   * fixes (a true rebind is an MC defect or an injected roster update; a
   * downgrade is MC publishing an empty key, which the tree documents occurring).
   */
  | 'downgrade'
  /** The roster forgot the sender's bytes (removal or LRU eviction). Not counted. */
  | 'forgotten';

/** Receives transmit-key invalidations. Wired by the session to the pipeline's cache. */
export type TransmitKeyInvalidationListener = (
  senderId: number,
  cause: TransmitKeyInvalidation,
) => void;

/** One roster entry. `key` undefined records a KNOWN-KEYLESS participant. */
interface RosterEntry {
  readonly key: CryptoKey | undefined;
  /** The published bytes, copied. Empty for keyless and malformed entries. */
  readonly bytes: Uint8Array;
}

/**
 * Bounded `sender_id -> CryptoKey` resolver.
 *
 * The ONLY input to a lookup is a sender id unpacked from a frame's own key id.
 * The slot assignment, the relay `stream_id`, and any notion of a "current
 * speaker" are structurally unable to reach it: this class has no method that
 * takes them.
 */
export class RosterIdentityKeys {
  readonly #maxEntries: number;
  /** Insertion-ordered for LRU. */
  readonly #entries = new Map<number, RosterEntry>();
  /**
   * The latest upsert per sender. An import resolving for an OLDER upsert is
   * discarded, so out-of-order `importKey` completions cannot install a stale key.
   */
  readonly #latestUpsert = new Map<number, number>();
  #upsertSeq = 0;
  #onInvalidate: TransmitKeyInvalidationListener | undefined;

  constructor(maxEntries: number) {
    this.#maxEntries = maxEntries;
  }

  /** Wire where transmit-key invalidations go. One listener; replaces any earlier one. */
  setTransmitKeyInvalidationListener(listener: TransmitKeyInvalidationListener | undefined): void {
    this.#onInvalidate = listener;
  }

  /**
   * Add or replace a participant's verification key.
   *
   * A participant with an empty or wrong-width key is recorded as KNOWN-KEYLESS
   * rather than skipped, so `identityKeyFor` returns `undefined` and the ingress
   * drops-and-counts. Recording the absence is what keeps the empty branch
   * distinguishable from "not on the roster yet" for a reader of this code; both
   * fail closed identically at the call site.
   *
   * Ignores an entry with no `senderId` mapping: a participant MC has not
   * allocated a sender id for cannot be the subject of any frame's key id.
   */
  async upsert(entry: RosterIdentityEntry): Promise<void> {
    const { senderId, identityPublicKey } = entry;
    if (!Number.isInteger(senderId) || senderId <= 0) return;

    const wellFormed = identityPublicKey.length === ED25519_PUBLIC_KEY_BYTES;
    const bytes = wellFormed ? Uint8Array.from(identityPublicKey) : new Uint8Array(0);
    const existing = this.#entries.get(senderId);

    // ---- SYNCHRONOUS: the rebind decision, before any await. ----
    if (existing && existing.bytes.length > 0 && wellFormed && sameBytes(existing.bytes, bytes)) {
      // The same key again. An LRU touch; no import, no purge.
      this.#insert(senderId, existing);
      return;
    }
    const hadKey = existing !== undefined && existing.bytes.length > 0;
    if (hadKey) {
      // A different key, or a downgrade to none. Either way the old binding's
      // transmit keys must not keep opening frames; the two are reported apart.
      this.#onInvalidate?.(senderId, wellFormed ? 'rebind' : 'downgrade');
    }

    const seq = (this.#upsertSeq += 1);
    this.#latestUpsert.set(senderId, seq);
    // Known-keyless NOW, so nothing verifies against the old key while the new
    // one imports. Frames arriving meanwhile are `no_roster_entry` — fail closed.
    this.#insert(senderId, { key: undefined, bytes });
    if (!wellFormed) {
      // Covers BOTH the empty case (length 0, what MC publishes for a joiner
      // that sent none) and the malformed case. Neither is "skip verification".
      return;
    }

    let key: CryptoKey | undefined;
    try {
      key = await importVerifyKey(identityPublicKey);
    } catch {
      // A key of the right length that WebCrypto refuses is not a usable key.
      // Fail closed; the cause is deliberately not retained — it would be a
      // platform string with nothing an operator can act on, on a path where
      // `rejectReason.ts`'s no-key-material-in-messages rule applies.
      key = undefined;
    }
    // A later upsert for this sender supersedes this one; so does a removal.
    if (this.#latestUpsert.get(senderId) !== seq) return;
    if (!this.#entries.has(senderId)) return;
    // Unimportable keeps the entry known-keyless, bytes and all: it is still
    // this sender's published key for rebind comparison.
    this.#insert(senderId, { key, bytes });
  }

  /**
   * Forget a participant, e.g. on `ParticipantLeft`.
   *
   * The sender's transmit keys go with it (see the module header): a leaver's
   * frames then fail at `no_roster_entry` BEFORE verify, independently of the
   * slot-edge gate. Replay state is kept.
   */
  remove(senderId: number): void {
    this.#latestUpsert.delete(senderId);
    if (this.#entries.delete(senderId)) this.#onInvalidate?.(senderId, 'forgotten');
  }

  /**
   * The verification key for `senderId`, or `undefined` if this client has none.
   *
   * `undefined` covers three cases that are DELIBERATELY indistinguishable to
   * the caller — unknown participant, participant with an empty key, participant
   * with an unusable key — because all three mean the same thing: this frame
   * cannot be verified, so it is dropped and counted as `no_roster_entry`. A
   * caller that could tell them apart would be a caller that could treat one of
   * them permissively.
   */
  identityKeyFor(senderId: number): CryptoKey | undefined {
    return this.#entries.get(senderId)?.key;
  }

  /** Drop all entries. Public keys are not secret; this is a memory bound reset. */
  clear(): void {
    this.#entries.clear();
    this.#latestUpsert.clear();
  }

  #insert(senderId: number, entry: RosterEntry): void {
    // Delete-then-set so a refreshed entry moves to the end of the insertion
    // order and LRU eviction stays meaningful.
    this.#entries.delete(senderId);
    this.#entries.set(senderId, entry);
    while (this.#entries.size > this.#maxEntries) {
      const oldest = this.#entries.keys().next().value;
      if (oldest === undefined) break;
      this.#entries.delete(oldest);
      this.#latestUpsert.delete(oldest);
      // Its bytes are gone, so a later re-add would read as a first binding.
      this.#onInvalidate?.(oldest, 'forgotten');
    }
  }
}

/** Plain byte equality. Public key material, so timing reveals nothing. */
function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) if (a[i] !== b[i]) return false;
  return true;
}
