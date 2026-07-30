// File: packages/sdk-core/src/validation/__tests__/limits.test.ts
//
// R-60/R-23 (G2): the MH-status byte-cap. `capUtf8Bytes`/`capMhUrl` must truncate
// at a UTF-8 CODEPOINT boundary (never split a multi-byte char) and count BYTES,
// not `.length` — matching MC's 256-byte `floor_char_boundary` bound.

import { describe, expect, it } from 'vitest';

import {
  capMhUrl,
  capUtf8Bytes,
  MAX_MH_URL_BYTES,
  MAX_PASSWORD_LENGTH,
  MAX_USER_TOKEN_LENGTH,
  validateUserToken,
} from '../limits.js';
import { ValidationError } from '../../errors/ValidationError.js';

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

// ---------------------------------------------------------------------------
// validateUserToken (task #58) — a header-injection guard, NOT token verification
// ---------------------------------------------------------------------------

describe('validateUserToken', () => {
  it('accepts a realistic JWT compact serialization', () => {
    // base64url segments joined by `.` are a strict subset of RFC 7235 token68, so
    // the charset is tight without rejecting any legitimate token.
    const jwt =
      'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1LTEiLCJleHAiOjE3MDAwMDAwMDB9.q-_A1b2C3d4E5f6G7h8I9j0KlMnOpQrStUvWxYz';
    expect(() => validateUserToken(jwt)).not.toThrow();
  });

  it('rejects CRLF, so header splitting is structurally impossible rather than filtered', () => {
    expect(() => validateUserToken('abc\r\nX-Injected: 1')).toThrow(ValidationError);
    expect(() => validateUserToken('abc\nX-Injected: 1')).toThrow(ValidationError);
    expect(() => validateUserToken('abc\rX')).toThrow(ValidationError);
  });

  it('rejects spaces and control characters', () => {
    expect(() => validateUserToken('abc def')).toThrow(ValidationError);
    expect(() => validateUserToken('abc\tdef')).toThrow(ValidationError);
    expect(() => validateUserToken('abc\x00def')).toThrow(ValidationError);
  });

  it('rejects empty and over-long tokens', () => {
    expect(() => validateUserToken('')).toThrow(ValidationError);
    expect(() => validateUserToken('a'.repeat(MAX_USER_TOKEN_LENGTH + 1))).toThrow(ValidationError);
  });

  it('the length bound is sized for a real JWT, not copied from the password bound', () => {
    // A bound copied from MAX_PASSWORD_LENGTH (128) would reject legitimate tokens,
    // and because the credential guard ships with no bypass there would be no
    // workaround — it would present as an inexplicable join failure (@security).
    expect(MAX_USER_TOKEN_LENGTH).toBeGreaterThan(MAX_PASSWORD_LENGTH * 8);
    expect(() => validateUserToken('a'.repeat(2048))).not.toThrow();
  });
});
