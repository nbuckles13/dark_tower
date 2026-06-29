// File: packages/sdk-core/src/validation/__tests__/limits.test.ts
//
// R-60/R-23 (G2): the MH-status byte-cap. `capUtf8Bytes`/`capMhUrl` must truncate
// at a UTF-8 CODEPOINT boundary (never split a multi-byte char) and count BYTES,
// not `.length` — matching MC's 256-byte `floor_char_boundary` bound.

import { describe, expect, it } from 'vitest';

import { capMhUrl, capUtf8Bytes, MAX_MH_URL_BYTES } from '../limits.js';

const utf8 = (s: string): number => new TextEncoder().encode(s).length;

describe('capUtf8Bytes / capMhUrl (G2)', () => {
  it('returns short ASCII unchanged', () => {
    expect(capUtf8Bytes('https://mh.example:4433', 256)).toBe('https://mh.example:4433');
  });

  it('returns a value that exactly fills the cap unchanged', () => {
    const s = 'a'.repeat(256);
    expect(capUtf8Bytes(s, 256)).toBe(s);
  });

  it('truncates over-long ASCII to exactly the byte cap', () => {
    const out = capUtf8Bytes('a'.repeat(300), 256);
    expect(out).toHaveLength(256);
    expect(utf8(out)).toBe(256);
  });

  it('truncates multibyte input on a CODEPOINT boundary (never splits a char)', () => {
    // Each 😀 is 4 UTF-8 bytes; 100 of them = 400 bytes.
    const input = '😀'.repeat(100);
    expect(utf8(input)).toBe(400);
    const out = capUtf8Bytes(input, 256);
    // 256 / 4 = 64 whole emoji, exactly filling the cap with no split.
    expect(out).toBe('😀'.repeat(64));
    expect(utf8(out)).toBe(256);
    // No replacement char / lone surrogate introduced.
    expect(out.includes('�')).toBe(false);
  });

  it('stops BEFORE the cap when the next codepoint would overflow it', () => {
    // Cap 255 with 4-byte chars → 63 chars (252 bytes); the 64th (→256) is dropped.
    const out = capUtf8Bytes('😀'.repeat(64), 255);
    expect(out).toBe('😀'.repeat(63));
    expect(utf8(out)).toBe(252);
  });

  it('capMhUrl applies the 256-byte MH bound', () => {
    expect(MAX_MH_URL_BYTES).toBe(256);
    const out = capMhUrl('x'.repeat(1000));
    expect(utf8(out)).toBe(256);
  });
});
