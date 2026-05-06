import { afterEach, describe, expect, it, vi } from 'vitest';

describe('TestTokenSigner', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.resetModules();
  });

  it('round-trip sign/verify of a JOSE EdDSA JWT exposes only the public key', async () => {
    const { TestTokenSigner } = await import('../test-only/signer.js');
    const signer = await TestTokenSigner.generate();
    expect(signer.publicKeyJwk.kty).toBe('OKP');
    expect(signer.publicKeyJwk.crv).toBe('Ed25519');
    expect(signer.publicKeyJwk.x).toEqual(expect.any(String));
    expect(signer.kid).toMatch(/^[A-Za-z0-9_-]+$/); // base64url, no padding

    const jwt = await signer.sign({ sub: 'u1', iat: 0, exp: 9999, jti: 'j1' });
    const parts = jwt.split('.');
    expect(parts).toHaveLength(3);
    const headerJson = JSON.parse(Buffer.from(parts[0]!.replace(/-/g, '+').replace(/_/g, '/'), 'base64').toString('utf8'));
    expect(headerJson).toMatchObject({ alg: 'EdDSA', typ: 'JWT', kid: signer.kid });

    // Verify the signature using the same path that produced it. Web Crypto
    // path: re-import the public key as a CryptoKey and verify. @noble path:
    // use noble.verifyAsync. We assert verifiability via a single import of
    // @noble (the @noble fallback is universally available in the workspace).
    const { verifyAsync } = await import('@noble/ed25519');
    const sig = base64UrlDecode(parts[2]!);
    const signingInput = new TextEncoder().encode(`${parts[0]}.${parts[1]}`);
    const pub = base64UrlDecode(signer.publicKeyJwk.x as string);
    const ok = await verifyAsync(sig, signingInput, pub);
    expect(ok).toBe(true);

    // Round-trip exposes only the public key — the signer object itself does
    // not expose the private handle as an enumerable property.
    expect(Object.getOwnPropertyNames(signer)).not.toContain('privKey');
    expect(Object.getOwnPropertyNames(signer)).not.toContain('#privKey');
  });

  it('module-init guard fires at IMPORT time when NODE_ENV=production (scoped via vi.stubEnv)', async () => {
    vi.stubEnv('NODE_ENV', 'production');
    vi.resetModules();
    await expect(import('../test-only/signer.js')).rejects.toThrow(
      /cannot be loaded with NODE_ENV=production/,
    );
  });

  it('barrel does NOT expose TestTokenSigner — only the sub-path resolves', async () => {
    const barrel = await import('../index.js');
    expect('TestTokenSigner' in barrel).toBe(false);
    // Sub-path resolves and exposes the class.
    const subPath = await import('../test-only/signer.js');
    expect(typeof subPath.TestTokenSigner).toBe('function');
  });
});

function base64UrlDecode(s: string): Uint8Array {
  const b64 = s.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (s.length % 4)) % 4);
  const bin = Buffer.from(b64, 'base64');
  return new Uint8Array(bin);
}
