// File: packages/sdk-core/src/signaling/__tests__/kekIntake.test.ts
//
// The KEK sink control's behavioural half. The mechanical half — that no source
// file passes a key-bearing subject to a serialising sink — is
// `src/__tests__/serverMessageSinkScan.test.ts`.

import { describe, expect, it } from 'vitest';

import { JoinResponseKekSource } from '../../media/setup/kekSource.js';
import { MEETING_KEK_BYTES } from '../../media/frame/sframe.js';
import { takeMeetingKek } from '../kekIntake.js';

function message(
  kek: Uint8Array,
  generation = 4,
): { meetingKek: Uint8Array; kekGeneration: number } {
  return { meetingKek: kek, kekGeneration: generation };
}

describe('takeMeetingKek', () => {
  it('installs the key and leaves NOTHING key-shaped on the decoded message', () => {
    // The leak this closes: protobuf-es has no `skip_debug`, so
    // `JSON.stringify(serverMessage)` prints the meeting KEK in the clear. After
    // intake the reachable object graph no longer contains it.
    const source = new JoinResponseKekSource();
    const msg = message(new Uint8Array(MEETING_KEK_BYTES).fill(0x11));
    const original = new Uint8Array(msg.meetingKek);

    expect(takeMeetingKek(msg, source).installed).toBe(true);
    expect(source.isProvisioned).toBe(true);
    expect(source.kekForGeneration(4)).toEqual(original);
    expect(msg.meetingKek.length).toBe(0);
    expect(JSON.stringify(msg)).not.toContain('17'); // 0x11 as a JSON byte
  });

  it('zeroes the ORIGINAL buffer, not merely the reference', () => {
    const source = new JoinResponseKekSource();
    const raw = new Uint8Array(MEETING_KEK_BYTES).fill(0x22);
    takeMeetingKek(message(raw), source);
    expect(raw.every((b) => b === 0)).toBe(true);
  });

  it('takes a COPY, so scrubbing the message cannot blank the installed key', () => {
    const source = new JoinResponseKekSource();
    const raw = new Uint8Array(MEETING_KEK_BYTES).fill(0x33);
    takeMeetingKek(message(raw, 7), source);
    expect(source.kekForGeneration(7)?.every((b) => b === 0x33)).toBe(true);
  });

  it('SCRUBS even when the value is not a usable key', () => {
    // Refusing to install and refusing to scrub are separate decisions.
    // Conflating them would leave the leak open on exactly the path where
    // something has already gone wrong.
    const source = new JoinResponseKekSource();
    const shortKey = new Uint8Array(16).fill(0x44);
    expect(takeMeetingKek(message(shortKey), source).installed).toBe(false);
    expect(shortKey.every((b) => b === 0)).toBe(true);
    expect(source.isProvisioned).toBe(false);
  });

  it('treats an ALL-ZERO key as unusable', () => {
    // `signaling.proto`: "an empty or all-zero KEK is never a usable key". A
    // zero key works cryptographically and is catastrophically wrong, so it must
    // fail at the boundary or not at all.
    const source = new JoinResponseKekSource();
    expect(takeMeetingKek(message(new Uint8Array(MEETING_KEK_BYTES)), source).installed).toBe(
      false,
    );
    expect(source.isProvisioned).toBe(false);
  });

  it('treats an EMPTY key as not-yet-provisioned rather than as an error', () => {
    // ADR-0036 §4 / `signaling.proto`: empty means NOT YET PROVISIONED, which is
    // a legitimate state. And the not-provisioned signal is the KEY's width, not
    // the generation — 0 is a plausible first generation, so gating on it would
    // give one field two meanings.
    const source = new JoinResponseKekSource();
    expect(takeMeetingKek(message(new Uint8Array(0), 0), source).installed).toBe(false);
    expect(source.isProvisioned).toBe(false);
  });
});

describe('JoinResponseKekSource', () => {
  it('answers only for the generation it holds', () => {
    const source = new JoinResponseKekSource();
    source.set(new Uint8Array(MEETING_KEK_BYTES).fill(0x55), 9);
    expect(source.kekForGeneration(9)).toBeDefined();
    // Previous-KEK retention is out of scope this story, so exactly one
    // generation is held. A frame carrying another is `no_kek_for_generation`
    // (dropped) or `kek_generation_not_held` (played off a cached key).
    expect(source.kekForGeneration(8)).toBeUndefined();
    expect(source.kekForGeneration(10)).toBeUndefined();
  });

  it('overwrites the buffer on clear', () => {
    const source = new JoinResponseKekSource();
    source.set(new Uint8Array(MEETING_KEK_BYTES).fill(0x66), 1);
    const held = source.kekForGeneration(1);
    source.clear();
    expect(held?.every((b) => b === 0)).toBe(true);
    expect(source.isProvisioned).toBe(false);
  });

  it('refuses a wrong-width key rather than handing it to WebCrypto', () => {
    // WebCrypto accepts a 16-byte AES-GCM key SILENTLY, so there is no platform
    // backstop here — this check is the whole control.
    const source = new JoinResponseKekSource();
    expect(() => source.set(new Uint8Array(16), 0)).toThrow(RangeError);
  });
});
