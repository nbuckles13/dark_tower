// File: packages/sdk-core/src/signaling/__tests__/kekIntake.test.ts
//
// The KEK sink control's behavioural half. The mechanical half — that no source
// file passes a key-bearing subject to a serialising sink — is
// `src/__tests__/serverMessageSinkScan.test.ts`.

import { describe, expect, it } from 'vitest';

import { emptyKekHolder, NOMINAL_DEBOUNCE_SECONDS } from '../../media/__tests__/helpers.js';
import { MEETING_KEK_BYTES } from '../../media/frame/sframe.js';
import { takeMeetingKek } from '../kekIntake.js';

function message(
  kek: Uint8Array,
  generation = 4,
): { meetingKek: Uint8Array; kekGeneration: number; kekRotationDebounceSeconds: number } {
  return {
    meetingKek: kek,
    kekGeneration: generation,
    kekRotationDebounceSeconds: NOMINAL_DEBOUNCE_SECONDS,
  };
}

describe('takeMeetingKek', () => {
  it('installs the key and leaves NOTHING key-shaped on the decoded message', () => {
    // The leak this closes: protobuf-es has no `skip_debug`, so
    // `JSON.stringify(serverMessage)` prints the meeting KEK in the clear. After
    // intake the reachable object graph no longer contains it.
    const holder = emptyKekHolder();
    const msg = message(new Uint8Array(MEETING_KEK_BYTES).fill(0x11));
    const original = new Uint8Array(msg.meetingKek);

    expect(takeMeetingKek(msg, holder, 'join_response').result).toBe('installed');
    expect(holder.isProvisioned).toBe(true);
    expect(holder.kekForGeneration(4)).toEqual(original);
    expect(msg.meetingKek.length).toBe(0);
    expect(JSON.stringify(msg)).not.toContain('17'); // 0x11 as a JSON byte
  });

  it('zeroes the ORIGINAL buffer, not merely the reference', () => {
    const holder = emptyKekHolder();
    const raw = new Uint8Array(MEETING_KEK_BYTES).fill(0x22);
    takeMeetingKek(message(raw), holder, 'join_response');
    expect(raw.every((b) => b === 0)).toBe(true);
  });

  it('takes a COPY, so scrubbing the message cannot blank the installed key', () => {
    const holder = emptyKekHolder();
    const raw = new Uint8Array(MEETING_KEK_BYTES).fill(0x33);
    takeMeetingKek(message(raw, 7), holder, 'join_response');
    expect(holder.kekForGeneration(7)?.every((b) => b === 0x33)).toBe(true);
  });

  it('SCRUBS even when the value is not a usable key', () => {
    // Refusing to install and refusing to scrub are separate decisions.
    // Conflating them would leave the leak open on exactly the path where
    // something has already gone wrong.
    const holder = emptyKekHolder();
    const shortKey = new Uint8Array(16).fill(0x44);
    const msg = message(shortKey);
    takeMeetingKek(msg, holder, 'kek_update');
    expect(shortKey.every((b) => b === 0)).toBe(true);
    expect(msg.meetingKek.length).toBe(0);
    expect(holder.isProvisioned).toBe(false);
  });

  it.each([
    ['too short', 16],
    ['too long', 48],
  ])('routes a %s NON-EMPTY key to the holder, which refuses and COUNTS it', (_l, width) => {
    // Only EMPTY means "not yet provisioned". A non-empty wrong-width key is
    // malformed, and filtering it out here would scrub it without a trace — a
    // fail-quietly path on exactly the input that is wrong.
    const refusals: string[] = [];
    const holder = emptyKekHolder({
      observer: {
        kekArrived: () => {},
        retentionAnomaly: () => {},
        installRefused: (o) => refusals.push(o),
        generationRetained: () => {},
        retentionViolation: () => {},
        warn: () => {},
      },
    });
    const result = takeMeetingKek(message(new Uint8Array(width).fill(0x45)), holder, 'kek_update');
    expect(result.result).toBe('refused');
    expect(refusals).toEqual(['malformed']);
  });

  it('SCRUBS and replaces the reference even if the sink THROWS on a defect', () => {
    // Both scrub steps sit in a `finally`: a defect in the sink must not leave
    // the key on the decoded message.
    const raw = new Uint8Array(MEETING_KEK_BYTES).fill(0x45);
    const msg = message(raw);
    const throwing = {
      install(): never {
        throw new Error('defect');
      },
    };
    expect(() => takeMeetingKek(msg, throwing, 'kek_update')).toThrow('defect');
    expect(raw.every((b) => b === 0)).toBe(true);
    expect(msg.meetingKek.length).toBe(0);
  });

  it('treats an ALL-ZERO key as unusable, on the push path as well as the join', () => {
    // `signaling.proto`: "an empty or all-zero KEK is never a usable key". A
    // zero key works cryptographically and is catastrophically wrong, so it must
    // fail at the boundary or not at all — on BOTH arrival paths.
    for (const source of ['join_response', 'kek_update'] as const) {
      const holder = emptyKekHolder();
      expect(
        takeMeetingKek(message(new Uint8Array(MEETING_KEK_BYTES)), holder, source).result,
      ).toBe('refused');
      expect(holder.isProvisioned).toBe(false);
    }
  });

  it('treats an EMPTY key as not-yet-provisioned rather than as an error', () => {
    // ADR-0036 §4 / `signaling.proto`: empty means NOT YET PROVISIONED, which is
    // a legitimate state. And the not-provisioned signal is the KEY's width, not
    // the generation — 0 is a plausible first generation, so gating on it would
    // give one field two meanings.
    const holder = emptyKekHolder();
    expect(takeMeetingKek(message(new Uint8Array(0), 0), holder, 'join_response').result).toBe(
      undefined,
    );
    expect(holder.isProvisioned).toBe(false);
  });

  it("passes the SAME message's W to the holder, not one learned earlier", () => {
    const calls: number[] = [];
    const recording = {
      install(_kek: Uint8Array, _g: number, w: number) {
        calls.push(w);
        return 'installed' as const;
      },
    };
    const msg = {
      ...message(new Uint8Array(MEETING_KEK_BYTES).fill(1)),
      kekRotationDebounceSeconds: 17,
    };
    takeMeetingKek(msg, recording, 'kek_update');
    expect(calls).toEqual([17]);
  });
});
