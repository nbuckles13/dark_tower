// File: packages/sdk-core/src/media/frame/__tests__/hex.test.ts
//
// The byte primitives' refusals. Every one of these throws rather than coercing,
// and that choice is load-bearing rather than fastidious: a lenient decoder turns
// a fixture defect into a crypto mismatch 300 bytes downstream, where it reads as
// an implementation bug.

import { describe, expect, it } from 'vitest';

import {
  bigUintToBytesBE,
  bytesEqual,
  bytesToHex,
  concatBytes,
  hexToBytes,
  xorBytes,
} from '../hex.js';

describe('hexToBytes rejects rather than coercing', () => {
  it('round-trips canonical hex', () => {
    expect(bytesToHex(hexToBytes('00ff10'))).toBe('00ff10');
    expect(hexToBytes('')).toHaveLength(0);
  });

  it('rejects odd-length input', () => {
    expect(() => hexToBytes('abc')).toThrow(/odd-length/);
  });

  it('rejects uppercase and 0x prefixes', () => {
    // The vectors' convention is lowercase and unprefixed; accepting anything
    // else would let a hand-edited fixture through.
    expect(() => hexToBytes('ABCD')).toThrow(/canonical/);
    expect(() => hexToBytes('0xabcd')).toThrow(/canonical/);
  });

  it('rejects non-hex characters', () => {
    expect(() => hexToBytes('zz')).toThrow(/canonical/);
  });

  it('rejects a non-string', () => {
    expect(() => hexToBytes(undefined as unknown as string)).toThrow(TypeError);
  });

  it('names the field in the error', () => {
    expect(() => hexToBytes('q', 'kek_hex')).toThrow(/kek_hex/);
  });
});

describe('bigUintToBytesBE never truncates', () => {
  it('encodes big-endian at the requested width', () => {
    expect(bytesToHex(bigUintToBytesBE(1n, 4))).toBe('00000001');
    expect(bytesToHex(bigUintToBytesBE(0xffffffffffffffffn, 8))).toBe('ffffffffffffffff');
  });

  it('throws rather than masking a value that does not fit', () => {
    // A masking encoder aliases distinct key ids onto one another, which is a
    // key-id collision and therefore a nonce reuse.
    expect(() => bigUintToBytesBE(256n, 1)).toThrow(/does not fit/);
  });

  it('rejects a negative value', () => {
    expect(() => bigUintToBytesBE(-1n, 4)).toThrow(/negative/);
  });
});

describe('xorBytes fails on a length mismatch', () => {
  it('xors equal-length runs', () => {
    expect(bytesToHex(xorBytes(Uint8Array.of(0xf0, 0x0f), Uint8Array.of(0xff, 0xff)))).toBe('0ff0');
  });

  it('throws rather than defaulting a missing byte to zero', () => {
    // Under `noUncheckedIndexedAccess` the tempting fix is `a[i] ?? 0`, which
    // would silently produce a WRONG NONCE out of a length bug — the one error
    // class this whole vector exercise exists to prevent.
    expect(() => xorBytes(new Uint8Array(2), new Uint8Array(3))).toThrow(/length mismatch/);
  });
});

describe('concatBytes and bytesEqual', () => {
  it('concatenates in order', () => {
    expect(bytesToHex(concatBytes(Uint8Array.of(1), Uint8Array.of(2, 3)))).toBe('010203');
    expect(concatBytes()).toHaveLength(0);
  });

  it('compares by value and length', () => {
    expect(bytesEqual(Uint8Array.of(1, 2), Uint8Array.of(1, 2))).toBe(true);
    expect(bytesEqual(Uint8Array.of(1, 2), Uint8Array.of(1, 3))).toBe(false);
    expect(bytesEqual(Uint8Array.of(1), Uint8Array.of(1, 2))).toBe(false);
  });
});
