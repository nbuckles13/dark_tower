// File: packages/sdk-core/src/encoding/base64url.ts
//
// THE one base64url -> bytes decoder in sdk-core (RFC 4648 §5). Used by the
// Ed25519 JWK public-key export (`media/frame/ed25519.ts`) and by the meeting
// token's role read (`session/meetingRole.ts`). Padding is optional on input:
// JWK `x` and JWT segments are unpadded, but a padded value decodes the same.
//
// THROWS on anything outside the base64url alphabet, and on a length that no
// base64 encoding can produce (remainder 1). Callers decide what a failure means;
// nothing here logs, and the error never echoes the input.

const BASE64URL = /^[A-Za-z0-9_-]*={0,2}$/;

/** Decode a base64url string (padding optional) into bytes. Throws on malformed input. */
export function base64UrlToBytes(input: string): Uint8Array<ArrayBuffer> {
  if (!BASE64URL.test(input)) {
    throw new TypeError('base64url: input contains characters outside the base64url alphabet');
  }
  const unpadded = input.replace(/=+$/, '');
  if (unpadded.length % 4 === 1) {
    throw new TypeError('base64url: input length is not a valid base64 length');
  }
  const b64 =
    unpadded.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (unpadded.length % 4)) % 4);
  const raw = atob(b64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) out[i] = raw.charCodeAt(i);
  return out;
}
