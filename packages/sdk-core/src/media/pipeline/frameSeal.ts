// File: packages/sdk-core/src/media/pipeline/frameSeal.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no labels.
//
// THE ONE seal-and-sign sequence for an outgoing media frame, shared by audio
// (`./egress.ts`) and video (`./videoEgress.ts`). Steps 1-4 of ADR-0036 §4's
// fixed order, which is not negotiable:
//
//   1. populate the FULL publisher region                (`buildPublisherRegion`)
//   2. (caller) generate or rotate the transmit key, wrap it under the KEK
//   3. SFrame-encrypt with the AAD equal to the publisher-region slice
//   4. Ed25519-sign over publisher region ‖ payload
//
// Step 5 — enqueue, then assign the hop sequence when the frame is WRITTEN — is
// each egress's own, because the two kinds' transports differ (one datagram per
// frame vs. frames back to back on a per-group stream).
//
// Extracted rather than copied (story 3 C-A2): a second copy of a crypto order
// is a second thing to drift. The caller has already taken ONE stream sequence
// and ONE GoP value for this frame, above any fan-out; nothing here allocates a
// nonce input.
//
// Steps 1 and 3 look circular — the region carries `payload_length`, and the
// payload is the sealed output — and are not: AES-GCM is length-preserving, so
// the sealed SFrame object's length is known before sealing. The region is
// built first, sealed under, and then handed to `buildUnsignedFrame`, which
// rebuilds it through the SAME function: the AAD a sender seals under is
// byte-identical to the region the frame ships with by construction.

import { buildPublisherRegion, buildUnsignedFrame, finishFrame } from '../frame/frameCodec.js';
import { signFrame } from '../frame/ed25519.js';
import { sealSframe, serializeSframe, sframeObjectLength } from '../frame/sframe.js';
import { deriveSframeKeys, sframeNonce } from '../frame/sframeKeySchedule.js';
import { AES_256_KEY_BYTES, SFRAME_CIPHER_SUITE_ID, SFRAME_SALT_BYTES } from '../frame/sframe.js';
import type { Bytes } from '../frame/hex.js';
import type { TransmitKeyMaterial } from '../lifecycle/transmitKeys.js';
import type { MeetingIdentity } from '../setup/identity.js';

/**
 * Thrown when a frame reaches the signing step after the meeting identity has
 * been released.
 *
 * Its own type rather than a bare `Error` so the lifecycle layer can tell a
 * teardown race from a genuine crypto fault: the first is expected at the tail
 * of a session, the second is not.
 */
export class EgressIdentityReleasedError extends Error {
  constructor() {
    super('the meeting identity was released before this frame could be signed');
    this.name = 'EgressIdentityReleasedError';
  }
}

/** The publisher-region flags for one frame. */
export interface SealFlags {
  readonly independentlyDecodable: boolean;
  readonly discardable: boolean;
  /** When true the wrapped transmit key rides this frame; `buildPublisherRegion` enforces agreement. */
  readonly keyBearing: boolean;
}

/** Everything one seal needs. */
export interface SealFrameInput {
  readonly headerVersion: number;
  readonly flags: SealFlags;
  /** Taken ONCE above the fan-out (`TransmitKeyManager.nextStreamSequence`): the AEAD nonce input. */
  readonly streamSequence: number;
  /** Taken ONCE per group above the fan-out (`TransmitKeyManager.nextGopCounter`). */
  readonly gopCounter: number;
  /** The current transmit key. Read here and NOT retained by this module. */
  readonly material: TransmitKeyMaterial;
  /** The relay-region `stream_id` (`./sendId.ts:uplinkRelayStreamId`). */
  readonly streamId: number;
  /** The encoded codec payload. */
  readonly plaintext: Uint8Array;
  /** The meeting identity HOLDER, read at the point of use so teardown revokes it. */
  readonly identity: MeetingIdentity;
}

/** A sealed, signed frame: wire bytes with a placeholder hop sequence. */
export interface SealedFrame {
  /** Complete wire bytes except the hop sequence, which is written at send time. */
  readonly bytes: Bytes;
  /** Offset of the relay region within `bytes` (for `writeHopSequence`). */
  readonly relayRegionOffset: number;
}

/**
 * Seal and sign ONE frame. Called once per frame, above any per-target fan-out.
 *
 * @throws {EgressIdentityReleasedError} when the identity was released, and
 *   whatever the crypto layer throws. Never swallowed: a frame that silently
 *   fails to build is indistinguishable from a muted source.
 */
export async function sealAndSignFrame(input: SealFrameInput): Promise<SealedFrame> {
  const { headerVersion, flags, streamSequence, gopCounter, material } = input;
  // COPY THE TRANSMIT KEY SYNCHRONOUSLY, before the first await. The manager
  // overwrites a key retired by the rotation before last (`transmitKeys.ts:
  // rotate`), and rotation sources are many (audio T, video T, every video group
  // start, KEK receipt): two rotations inside the HKDF's `await importKey` would
  // otherwise seal this frame under HKDF(zeros, public kid) — derivable by
  // anyone holding the frame, i.e. a confidentiality loss to the relay. The copy
  // makes the read window zero; an all-zero copy fails LOUDLY (nothing sent).
  const baseKey = Uint8Array.from(material.key) as Bytes;
  if (baseKey.every((b) => b === 0)) {
    throw new Error('refusing to seal a media frame under an all-zero transmit key');
  }
  try {
    return await sealWithKey(input, headerVersion, flags, streamSequence, gopCounter, baseKey);
  } finally {
    baseKey.fill(0);
  }
}

async function sealWithKey(
  input: SealFrameInput,
  headerVersion: number,
  flags: SealFlags,
  streamSequence: number,
  gopCounter: number,
  baseKey: Bytes,
): Promise<SealedFrame> {
  const { material } = input;
  const wrappedTransmitKey = flags.keyBearing ? material.wrapped : null;
  // GCM is length-preserving, so the sealed object's length is known now.
  // Computed by the SFrame module from the same constants its serializer uses,
  // never by arithmetic here.
  const sealedLength = sframeObjectLength(input.plaintext.length);
  const publisherRegion = buildPublisherRegion(
    { headerVersion, flags, streamSequence, gopCounter, wrappedTransmitKey, extensions: [] },
    sealedLength,
  );

  const derived = await deriveSframeKeys({
    hash: 'SHA-512',
    baseKey,
    // The packed key id AS IT WILL APPEAR ON THE WIRE, never a value re-packed
    // from decomposed fields — a re-pack always succeeds and is merely wrong.
    kid: material.keyId,
    cipherSuiteId: SFRAME_CIPHER_SUITE_ID,
    keyBytes: AES_256_KEY_BYTES,
    saltBytes: SFRAME_SALT_BYTES,
  });

  const sframe = await sealSframe({
    key: derived.key,
    nonce: sframeNonce(derived.salt, BigInt(streamSequence)),
    // THE PUBLISHER REGION, and only the publisher region. The relay region is
    // excluded because MH rewrites it.
    aad: publisherRegion,
    plaintext: input.plaintext,
    keyId: material.keyId,
  });
  const payload = serializeSframe(sframe);

  const unsigned = buildUnsignedFrame({
    headerVersion,
    flags,
    streamSequence,
    gopCounter,
    wrappedTransmitKey,
    extensions: [],
    streamId: input.streamId,
    // Placeholder: overwritten when the frame is sent, so the hop sequence
    // counts only frames actually sent.
    hopSequence: 0,
    payload,
  });
  // Read at the point of use, so a released identity cannot be signed with.
  const signer = input.identity.signer;
  if (!signer) throw new EgressIdentityReleasedError();
  const signature = await signFrame(signer, unsigned.signedRange);
  return { bytes: finishFrame(unsigned, signature), relayRegionOffset: unsigned.relayRegionOffset };
}
