// File: packages/sdk-core/src/media/pipeline/__tests__/frameSeal.test.ts
//
// Security (C-A2 Gate 3, F1): the transmit key is copied SYNCHRONOUSLY at seal
// entry, so a key overwritten mid-seal (two rotations inside the HKDF's await)
// can never produce a frame sealed under zeros; an all-zero key fails loudly.

import { describe, expect, it } from 'vitest';

import { HEADER_VERSION } from '../../frame/wireConstants.js';
import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import type { Bytes } from '../../frame/hex.js';
import { kekHolderWith } from '../../__tests__/helpers.js';
import { MeetingIdentity } from '../../setup/identity.js';
import { TransmitKeyManager } from '../../lifecycle/transmitKeys.js';
import { sealAndSignFrame, type SealFrameInput } from '../frameSeal.js';

async function input(): Promise<SealFrameInput> {
  const keys = new TransmitKeyManager(258);
  const kek = kekHolderWith(new Uint8Array(AES_256_KEY_BYTES).fill(0x55), 0);
  const material = await keys.materialFor(1, kek);
  return {
    headerVersion: HEADER_VERSION,
    flags: { independentlyDecodable: true, discardable: false, keyBearing: true },
    streamSequence: 7,
    gopCounter: 0,
    material,
    streamId: 1,
    plaintext: Uint8Array.of(1, 2, 3, 4),
    identity: await MeetingIdentity.create(),
  };
}

describe('sealAndSignFrame — the key is read once, synchronously', () => {
  it('a key overwritten DURING the seal still seals under the announced key (never zeros)', async () => {
    const base = await input();
    const original = Uint8Array.from(base.material.key) as Bytes;
    const reference = await sealAndSignFrame({
      ...base,
      material: { ...base.material, key: Uint8Array.from(original) as Bytes },
    });
    const live = { ...base.material, key: Uint8Array.from(original) as Bytes };
    const sealing = sealAndSignFrame({ ...base, material: live });
    // The rotation-before-last overwrite, landing inside the HKDF's await.
    live.key.fill(0);
    const sealed = await sealing;
    // Same key, nonce, AAD, plaintext: the AES-GCM payload is identical to the
    // reference sealed under the real key.
    const payloadOf = (b: Uint8Array) => [...b.subarray(0, b.length - 64)].join();
    expect(payloadOf(sealed.bytes)).toBe(payloadOf(reference.bytes));
  });

  it('refuses LOUDLY to seal under an all-zero key', async () => {
    const base = await input();
    base.material.key.fill(0);
    await expect(sealAndSignFrame(base)).rejects.toThrow(/all-zero transmit key/);
  });
});
