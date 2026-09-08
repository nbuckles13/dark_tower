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

import { ED25519_PUBLIC_KEY_BYTES, importVerifyKey } from '../frame/ed25519.js';

/** One roster participant's verification material, as the media path needs it. */
export interface RosterIdentityEntry {
  /** Meeting-scoped numeric sender id, `1..=65535`. Absent means no key-id mapping. */
  readonly senderId: number;
  /**
   * The raw Ed25519 public key as MC published it.
   *
   * NOT attested, NOT verified, NOT trusted — this story performs no attestation
   * check of any kind, and `identity_public_key` is the honest spelling. A
   * signature verifying against it proves only that every frame came from the
   * same keyholder; it never proves WHO. The AC attestation that closes this is
   * story 2.
   *
   * EMPTY means no key published. See the module header.
   */
  readonly identityPublicKey: Uint8Array;
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
  /** Insertion-ordered for LRU. `undefined` records a KNOWN-KEYLESS participant. */
  readonly #keys = new Map<number, CryptoKey | undefined>();

  constructor(maxEntries: number) {
    this.#maxEntries = maxEntries;
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

    if (identityPublicKey.length !== ED25519_PUBLIC_KEY_BYTES) {
      // Covers BOTH the empty case (length 0, what MC publishes for a joiner
      // that sent none) and the malformed case. Neither is "skip verification".
      this.#insert(senderId, undefined);
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
    this.#insert(senderId, key);
  }

  /** Forget a participant, e.g. on `ParticipantLeft`. */
  remove(senderId: number): void {
    this.#keys.delete(senderId);
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
    return this.#keys.get(senderId);
  }

  /** Drop all entries. Public keys are not secret; this is a memory bound reset. */
  clear(): void {
    this.#keys.clear();
  }

  #insert(senderId: number, key: CryptoKey | undefined): void {
    // Delete-then-set so a refreshed entry moves to the end of the insertion
    // order and LRU eviction stays meaningful.
    this.#keys.delete(senderId);
    this.#keys.set(senderId, key);
    while (this.#keys.size > this.#maxEntries) {
      const oldest = this.#keys.keys().next().value;
      if (oldest === undefined) break;
      this.#keys.delete(oldest);
    }
  }
}
