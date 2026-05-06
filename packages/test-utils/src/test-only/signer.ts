// File: packages/test-utils/src/test-only/signer.ts
//
// TEST-ONLY. Imported only via @darktower/test-utils/test-only/signer
// sub-path; never re-exported from the package barrel.
// Throws on import when NODE_ENV=production.
// Ephemeral keypair per signer instance. Never persisted, never logged,
// never serialized.
// Private key handle is non-extractable on the Web Crypto path.
// On the @noble/ed25519 fallback path the private scalar is raw bytes —
// the asymmetry is intentional; consumers must still treat the private
// handle as opaque and never serialize, log, or persist it.

// ---------------------------------------------------------------------------
// Module-init guard. Throws at top-level evaluation time (NOT first-call).
// `typeof process` defensive wrapper so the check itself does not crash in
// non-Node contexts where `process` is undefined.
// ---------------------------------------------------------------------------
if (typeof process !== 'undefined' && process.env?.NODE_ENV === 'production') {
  throw new Error('test-utils/test-only/signer cannot be loaded with NODE_ENV=production');
}

const TEXT_ENCODER = new TextEncoder();

interface SignerImpl {
  /** Sign 64 bytes of EdDSA, returning the raw signature. */
  sign(messageBytes: Uint8Array): Promise<Uint8Array>;
  /** JWK representation of the public key. */
  publicJwk(): Promise<JsonWebKey>;
}

/**
 * Test-only Ed25519 JWT signer. Generates an ephemeral keypair on construction
 * and produces JOSE `EdDSA` JWTs.
 *
 * **Key handling**:
 *   - Web Crypto path: `subtle.generateKey({ name: 'Ed25519' }, false, …)` —
 *     private key handle is non-extractable. Public key is exported as JWK.
 *   - `@noble/ed25519` fallback path: library operates on raw scalar bytes;
 *     the asymmetry is intentional and documented in the module banner.
 *
 * **Never** serialize, log, or persist the signer instance. The
 * `publicKeyJwk` and `kid` are safe to expose; private material is not.
 *
 * @example
 * import { TestTokenSigner } from '@darktower/test-utils/test-only/signer';
 * const signer = await TestTokenSigner.generate();
 * const jwt = await signer.sign({ sub: 'u1', iat: 0, exp: 9999, jti: 'j1' });
 */
export class TestTokenSigner {
  readonly publicKeyJwk: JsonWebKey;
  readonly kid: string;
  /**
   * @internal
   * NEVER serialize, log, persist, or export this handle. Web Crypto path
   * stores `extractable=false` CryptoKey; @noble path stores raw 32-byte
   * scalar — both must be treated as opaque.
   */
  readonly #impl: SignerImpl;

  private constructor(impl: SignerImpl, publicKeyJwk: JsonWebKey, kid: string) {
    this.#impl = impl;
    this.publicKeyJwk = publicKeyJwk;
    this.kid = kid;
  }

  /**
   * Generate an ephemeral Ed25519 keypair and return a fresh signer.
   * Tries Web Crypto first; falls back to `@noble/ed25519` if Web Crypto
   * Ed25519 is unavailable in the runtime.
   */
  static async generate(): Promise<TestTokenSigner> {
    const impl = (await tryWebCrypto()) ?? (await loadNobleImpl());
    const jwk = await impl.publicJwk();
    const kid = await deriveKid(jwk);
    return new TestTokenSigner(impl, jwk, kid);
  }

  /**
   * Sign the given claims as a JOSE `EdDSA` JWT. Header is `{ alg: 'EdDSA',
   * typ, kid }`. Signature bytes are NEVER logged or surfaced separately
   * from the JWT compact serialization.
   */
  async sign(claims: object, header: { typ?: string } = {}): Promise<string> {
    const fullHeader = {
      alg: 'EdDSA' as const,
      typ: header.typ ?? 'JWT',
      kid: this.kid,
    };
    const headerB64 = base64UrlEncode(TEXT_ENCODER.encode(JSON.stringify(fullHeader)));
    const payloadB64 = base64UrlEncode(TEXT_ENCODER.encode(JSON.stringify(claims)));
    const signingInput = `${headerB64}.${payloadB64}`;
    const sig = await this.#impl.sign(TEXT_ENCODER.encode(signingInput));
    return `${signingInput}.${base64UrlEncode(sig)}`;
  }
}

// ---------------------------------------------------------------------------
// Web Crypto path
// ---------------------------------------------------------------------------

async function tryWebCrypto(): Promise<SignerImpl | null> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return null;
  try {
    const pair = (await subtle.generateKey(
      { name: 'Ed25519' } as AlgorithmIdentifier,
      /* extractable */ false,
      ['sign', 'verify'],
    )) as CryptoKeyPair;
    // WebCrypto's `extractable=false` for an asymmetric `generateKey` applies
    // to the private key only — the public key is always exportable. We
    // export the public JWK here once and cache it.
    const publicKeyExtractable = await subtle.exportKey('jwk', pair.publicKey);
    return {
      async sign(message): Promise<Uint8Array> {
        const sig = await subtle.sign({ name: 'Ed25519' }, pair.privateKey, message);
        return new Uint8Array(sig);
      },
      async publicJwk(): Promise<JsonWebKey> {
        return publicKeyExtractable;
      },
    };
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// @noble/ed25519 fallback path
// ---------------------------------------------------------------------------

async function loadNobleImpl(): Promise<SignerImpl> {
  // Dynamic import so the dep is loaded only when Web Crypto Ed25519 is
  // unavailable. test-utils' devDependency on @noble/ed25519 ensures the
  // package is present in the workspace, but never reaches a production
  // module graph (test-utils itself is a devDependency of consumers).
  const noble = await import('@noble/ed25519');
  // Generate a 32-byte private scalar. Use Web Crypto getRandomValues if
  // available; otherwise fall back to Node's `node:crypto`.
  const priv = await randomBytes(32);
  const pub = await noble.getPublicKeyAsync(priv);
  const jwk: JsonWebKey = {
    kty: 'OKP',
    crv: 'Ed25519',
    x: base64UrlEncode(pub),
  };
  return {
    async sign(message): Promise<Uint8Array> {
      return noble.signAsync(message, priv);
    },
    async publicJwk(): Promise<JsonWebKey> {
      return jwk;
    },
  };
}

async function randomBytes(n: number): Promise<Uint8Array> {
  const out = new Uint8Array(n);
  if (typeof globalThis.crypto?.getRandomValues === 'function') {
    globalThis.crypto.getRandomValues(out);
    return out;
  }
  // Last-resort fallback for very old Node — should be unreachable on
  // Node 22+. Imported lazily to avoid bundler complaints in browser builds.
  const nodeCrypto = await import('node:crypto');
  const buf = nodeCrypto.randomBytes(n);
  out.set(buf);
  return out;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function deriveKid(jwk: JsonWebKey): Promise<string> {
  // RFC 7638 thumbprint shape: lexically-sorted required-fields JSON,
  // SHA-256, base64url. For OKP keys: { crv, kty, x }.
  const subtle = globalThis.crypto?.subtle;
  const required: Record<string, unknown> = {
    crv: jwk.crv,
    kty: jwk.kty,
    x: jwk.x,
  };
  const canonical = JSON.stringify(required, Object.keys(required).sort());
  const bytes = TEXT_ENCODER.encode(canonical);
  if (subtle) {
    const digest = await subtle.digest('SHA-256', bytes);
    return base64UrlEncode(new Uint8Array(digest));
  }
  // Last-resort node fallback.
  const nodeCrypto = await import('node:crypto');
  const hash = nodeCrypto.createHash('sha256').update(bytes).digest();
  return base64UrlEncode(new Uint8Array(hash));
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i] ?? 0);
  }
  const b64 = typeof btoa === 'function' ? btoa(binary) : Buffer.from(binary, 'binary').toString('base64');
  return b64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
