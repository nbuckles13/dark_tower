import { describe, expect, it } from 'vitest';
import { base64UrlToBytes } from '../base64url.js';

const enc = (bytes: number[]): string =>
  btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, '-')
    .replace(/\//g, '_');

describe('base64UrlToBytes', () => {
  it.each([[[]], [[0]], [[0xfb]], [[0xfb, 0xff]], [[1, 2, 3]], [[0xfb, 0xff, 0xbf, 0x00]]])(
    'round-trips %j padded and unpadded',
    (bytes) => {
      const padded = enc(bytes);
      expect([...base64UrlToBytes(padded)]).toEqual(bytes);
      expect([...base64UrlToBytes(padded.replace(/=+$/, ''))]).toEqual(bytes);
    },
  );

  it('decodes the url-safe alphabet (- and _), not + and /', () => {
    expect([...base64UrlToBytes('-_8')]).toEqual([0xfb, 0xff]);
    expect(() => base64UrlToBytes('+/8')).toThrow(/alphabet/);
  });

  it('rejects an impossible length', () => {
    expect(() => base64UrlToBytes('A')).toThrow(/length/);
    expect(() => base64UrlToBytes('AAAAA')).toThrow(/length/);
  });

  it('never echoes the input in its error', () => {
    const secret = 'secret.value!';
    try {
      base64UrlToBytes(secret);
    } catch (err) {
      expect(String(err)).not.toContain(secret);
    }
  });
});
