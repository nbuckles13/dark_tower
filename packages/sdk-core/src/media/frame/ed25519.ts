// File: packages/sdk-core/src/media/frame/ed25519.ts
//
// Ed25519 sign and verify for the per-frame sender authentication of
// ADR-0036 §3, through WebCrypto.
//
// ---------------------------------------------------------------------------
// THERE IS NO `@noble/ed25519` FALLBACK, AND THAT IS A MEASURED DECISION
// ---------------------------------------------------------------------------
//
// The task conditioned a fallback on the browser probe — "verify the target
// Chrome exposes WebCrypto Ed25519 unflagged; IF NOT, use the fallback". The
// probe was run and the antecedent resolved:
//
//   Google Chrome for Testing 151.0.7922.34, driven over http://127.0.0.1 so
//   `isSecureContext` is true (on a `data:` URL the origin is opaque and
//   `crypto.subtle` is `undefined` — the most likely way to repeat this probe
//   and wrongly conclude Ed25519 is missing):
//     generateKey({name:'Ed25519'})            -> ok, unflagged
//     importKey('pkcs8', <DER prefix||seed>)   -> ok
//     importKey('raw', <32-byte public key>)   -> ok
//     sign / verify                            -> ok, and DETERMINISTIC per RFC 8032
//
// So a fallback would be dead code on the only supported browser. Three reasons
// not to ship it anyway, the first being the decisive one:
//
//   1. The branch could never execute in any gate we run — `sdk-core`'s unit tier
//      is `environment: 'node'` and there is no browser lane for this package —
//      while still counting toward the >=90% branch-coverage threshold. An
//      unexecutable crypto branch scored as covered is coverage theatre on
//      exactly the code where coverage is supposed to mean something.
//   2. On the `@noble` path the identity private scalar is raw bytes in JS
//      memory, a real regression against ADR-0028 §5's non-extractable-`CryptoKey`
//      posture — taken to serve a platform that does not need it.
//   3. A shipped supply-chain surface, a lockfile regeneration that hard-fails
//      six `--frozen-lockfile` sites mid-branch, and a Layer-6 audit gate, for
//      code that never runs.
//
// Instead: a loud capability probe, `assertEd25519Available()`. **It is NOT YET
// INVOKED from production** — at story task 15 there is no session/receive loop
// to call it from; that lands at task 19, which owns the call site (tracked in
// `docs/TODO.md`). Until then the fail-closed SECURITY property still holds
// unconditionally — `verifyFrameSignature` returns false on any verifier
// failure, so an unsupported browser drops every frame — but the DIAGNOSABILITY
// half (a named error pointing at the runbook, rather than total media loss that
// reads as an attack) waits on that call site. Stated plainly so this header is
// not read as describing a live control: the probe exists, its wiring does not.
// (ADR-0036's own taxonomy: a control can be in scope and dead, which reads as
// coverage — so it is named as dead.)
//
// The public sign API takes a `CryptoKey` and never raw private bytes, so if a
// fallback ever does become necessary it will not force callers down to raw
// material.
//
// ---------------------------------------------------------------------------
// FAIL CLOSED ON THE RECEIVE PATH. THIS IS THE WHOLE POINT.
// ---------------------------------------------------------------------------
//
// The dangerous shape is NOT "cannot sign". It is "cannot construct a verifier,
// therefore the frame is unverifiable, therefore accept it". ANY failure to
// obtain or run a verifier — capability absent, `importKey` throwing, a throw
// from inside `verify` itself — maps to FALSE here, which the receive path maps
// to a DROP on the same arm as `signature_invalid`. Never to accept, never to a
// "verification unavailable" pass-through, never to warn-and-continue.
//
// A one-shot probe at init does not make a LATER failure inconclusive:
// `verifyFrameSignature` converts any thrown value to `false` rather than
// propagating something a caller might catch and treat as indeterminate.
//
// There is no unsigned-frame path in either direction, under any degradation.

import { SdkError, SdkErrorCode } from '../../errors/SdkError.js';
import type { Bytes } from './hex.js';

/**
 * Ed25519 public key length.
 *
 * BOUNDARY NOTE: equal to `AES_256_KEY_BYTES` and `MEETING_KEK_BYTES` by
 * coincidence, not derivation. Not hoisted to a shared constant — they are free
 * to move independently. Mirrors
 * `crates/mc-service/src/media_admission/identity_key.rs::IDENTITY_PUBLIC_KEY_BYTES`,
 * which records the same non-collapse.
 */
export const ED25519_PUBLIC_KEY_BYTES = 32;

/** Ed25519 signature length, and the frame's trailing field width. */
export const ED25519_SIGNATURE_BYTES = 64;

/** Ed25519 seed length (RFC 8032 §5.1.5). Test-tier use only; production imports keys. */
export const ED25519_SEED_BYTES = 32;

/**
 * The PKCS#8 prefix that wraps a bare 32-byte Ed25519 seed.
 *
 * WebCrypto will not import a raw private key — `importKey('raw', ...)` is
 * public-key only for Ed25519 — so a seed must be presented as PKCS#8. These 16
 * bytes are the fixed DER header for `PrivateKeyInfo { version 0,
 * algorithm id-Ed25519, privateKey OCTET STRING(32) }`; nothing in them varies
 * with the key.
 *
 * Used by the conformance harness to make `identity_private_seed_hex`
 * load-bearing. Production never sees a seed.
 */
const PKCS8_ED25519_SEED_PREFIX = Uint8Array.of(
  0x30,
  0x2e,
  0x02,
  0x01,
  0x00,
  0x30,
  0x05,
  0x06,
  0x03,
  0x2b,
  0x65,
  0x70,
  0x04,
  0x22,
  0x04,
  0x20,
);

const ALGORITHM = { name: 'Ed25519' } as const;

let capabilityChecked = false;

/**
 * Assert the platform exposes WebCrypto Ed25519, loudly.
 *
 * Throws a named `SdkError` rather than degrading. CLAUDE.md's fail-loudly rule:
 * a browser that cannot verify frame signatures must produce a diagnosable error,
 * never a silent acceptance of unverified media.
 */
export async function assertEd25519Available(): Promise<void> {
  if (capabilityChecked) return;
  try {
    await crypto.subtle.generateKey(ALGORITHM, false, ['sign', 'verify']);
    capabilityChecked = true;
  } catch {
    // `SdkErrorCode.Media` rather than a new enum member: the existing codes are
    // a bounded, serialized taxonomy (`RedactedSdkError`), and this is a
    // media-path failure. The original cause is deliberately NOT attached —
    // `SdkErrorOptions` has no `cause`, and a platform error string on this path
    // would add nothing an operator can act on.
    throw new SdkError(
      SdkErrorCode.Media,
      'WebCrypto Ed25519 is unavailable, so media frame signatures cannot be verified. ' +
        'Dark Tower does not send or accept unsigned frames under any degradation. ' +
        'Chrome has shipped Ed25519 unflagged since Chrome 137; if this fires in a supported ' +
        'browser, check that the page is a secure context (crypto.subtle is undefined on an ' +
        'opaque origin such as a data: URL). See docs/runbooks/client-dev-local.md §2.1.',
    );
  }
}

/** A meeting-scoped identity signing keypair. */
export interface IdentityKeyPair {
  /**
   * The signing key. **NON-EXTRACTABLE**: `crypto.subtle.exportKey` on it
   * rejects, so "the private key cannot be logged or serialized" is a platform
   * guarantee rather than a review outcome. Signing does not need
   * extractability.
   */
  readonly privateKey: CryptoKey;
  /** The raw 32-byte public half, for `JoinRequest.identity_public_key`. */
  readonly publicKey: Bytes;
}

/**
 * Generate a fresh identity signing keypair (ADR-0036 §4 step 1).
 *
 * ---------------------------------------------------------------------------
 * CUSTODY, WHICH IS THE POINT OF THIS FUNCTION EXISTING SEPARATELY
 * ---------------------------------------------------------------------------
 *
 *   * **Platform CSPRNG.** `crypto.subtle.generateKey`. Production must NEVER
 *     route through `importSigningKeyFromSeed` — that exists so the conformance
 *     harness can reproduce the vectors' pinned signatures, and a seed path in
 *     production is a deterministic-key hazard.
 *   * **`extractable: false` on the private half.** See {@link IdentityKeyPair}.
 *   * **PER MEETING, NEVER PERSISTED.** ADR-0036 §4: *"Identity keys are scoped
 *     to ONE MEETING, not to a participant across meetings: a key reused across
 *     meetings makes a participant linkable by public key regardless of display
 *     name, which matters most for the guests who have the least identity
 *     assurance to begin with."* So: no `localStorage`, no `sessionStorage`, no
 *     `IndexedDB`, and no module-level singleton that outlives a
 *     `MeetingSession`. This function returns the pair; it caches nothing.
 *   * **Dropped at teardown**, alongside the meeting KEK.
 *
 * The public half is extracted through a SEPARATE extractable public key rather
 * than by exporting the private one, because the private one is not extractable
 * — which is the property we want, not an inconvenience to work around.
 */
export async function generateIdentityKeyPair(): Promise<IdentityKeyPair> {
  // `extractable: true` applies to the PAIR, and WebCrypto then honours it only
  // for the public half when we export `raw` from `publicKey`. To get a
  // non-extractable private half we generate extractable, export the public
  // bytes, then re-import the private half as non-extractable — which is not
  // possible without the private bytes. So instead: generate NON-extractable and
  // export the PUBLIC key object, which is always exportable regardless of the
  // pair's `extractable` flag (the flag governs private-key export only, per
  // the WebCrypto spec's `[[extractable]]` on each key object; `generateKey`
  // for Ed25519 marks the public key extractable unconditionally).
  const pair = (await crypto.subtle.generateKey(ALGORITHM, false, [
    'sign',
    'verify',
  ])) as CryptoKeyPair;
  const raw = new Uint8Array(await crypto.subtle.exportKey('raw', pair.publicKey));
  if (raw.length !== ED25519_PUBLIC_KEY_BYTES) {
    throw new RangeError(
      `generated Ed25519 public key is ${raw.length} bytes, expected ${ED25519_PUBLIC_KEY_BYTES}`,
    );
  }
  return { privateKey: pair.privateKey, publicKey: raw as Bytes };
}

/** Import a 32-byte raw Ed25519 public key for verification. */
export async function importVerifyKey(publicKey: Uint8Array): Promise<CryptoKey> {
  if (publicKey.length !== ED25519_PUBLIC_KEY_BYTES) {
    throw new RangeError(
      `Ed25519 public key must be ${ED25519_PUBLIC_KEY_BYTES} bytes, got ${publicKey.length}`,
    );
  }
  return crypto.subtle.importKey('raw', Uint8Array.from(publicKey), ALGORITHM, true, ['verify']);
}

/**
 * Import a signing key from a bare 32-byte seed, via PKCS#8.
 *
 * TEST-TIER ONLY. Production signs with a `CryptoKey` it never saw the private
 * bytes of; this exists so the conformance harness can reproduce the vectors'
 * pinned signatures from `identity_private_seed_hex` and thereby keep that field
 * load-bearing rather than dormant.
 */
export async function importSigningKeyFromSeed(seed: Uint8Array): Promise<CryptoKey> {
  if (seed.length !== ED25519_SEED_BYTES) {
    throw new RangeError(`Ed25519 seed must be ${ED25519_SEED_BYTES} bytes, got ${seed.length}`);
  }
  const pkcs8 = new Uint8Array(PKCS8_ED25519_SEED_PREFIX.length + seed.length);
  pkcs8.set(PKCS8_ED25519_SEED_PREFIX, 0);
  pkcs8.set(seed, PKCS8_ED25519_SEED_PREFIX.length);
  return crypto.subtle.importKey('pkcs8', pkcs8, ALGORITHM, true, ['sign']);
}

/** Derive the raw 32-byte public key from an extractable Ed25519 key. */
export async function exportPublicKey(key: CryptoKey): Promise<Bytes> {
  const jwk = await crypto.subtle.exportKey('jwk', key);
  if (!jwk.x) throw new Error('Ed25519 key export carries no public component');
  const b64 = jwk.x.replace(/-/g, '+').replace(/_/g, '/');
  const raw = atob(b64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) out[i] = raw.charCodeAt(i);
  return out;
}

/**
 * Sign a frame's signed range.
 *
 * Takes a `CryptoKey`, never raw private bytes — keeps the ADR-0028 §5
 * non-extractable posture available to callers.
 */
export async function signFrame(key: CryptoKey, signedRange: Uint8Array): Promise<Bytes> {
  const sig = new Uint8Array(
    await crypto.subtle.sign(ALGORITHM, key, Uint8Array.from(signedRange)),
  );
  if (sig.length !== ED25519_SIGNATURE_BYTES) {
    throw new Error(
      `Ed25519 signature is ${sig.length} bytes, expected ${ED25519_SIGNATURE_BYTES}`,
    );
  }
  return sig;
}

/**
 * Verify a frame signature. Returns `false` on ANY failure.
 *
 * FAIL CLOSED — see the module header. A thrown value from `verify` (a malformed
 * key, a revoked capability, an unexpected platform error) becomes `false`, not a
 * propagated exception, precisely so no caller can catch it and treat the result
 * as inconclusive. "Inconclusive" has no representation here on purpose: the only
 * two outcomes are verified and dropped.
 */
export async function verifyFrameSignature(
  publicKey: Uint8Array | CryptoKey,
  signature: Uint8Array,
  signedRange: Uint8Array,
): Promise<boolean> {
  if (signature.length !== ED25519_SIGNATURE_BYTES) return false;
  if (!(publicKey instanceof Uint8Array) && publicKey.type !== 'public') return false;
  if (publicKey instanceof Uint8Array && publicKey.length !== ED25519_PUBLIC_KEY_BYTES) {
    return false;
  }
  try {
    // ACCEPTS AN ALREADY-IMPORTED KEY so a receiver can import once per SENDER
    // at roster update instead of once per frame at 50 frames a second. The
    // fail-closed property is unchanged: a `CryptoKey` that is not a public
    // verification key is rejected above, and any throw below still becomes
    // `false` rather than a propagated exception.
    const key = publicKey instanceof Uint8Array ? await importVerifyKey(publicKey) : publicKey;
    return await crypto.subtle.verify(
      ALGORITHM,
      key,
      Uint8Array.from(signature),
      Uint8Array.from(signedRange),
    );
  } catch {
    return false;
  }
}
