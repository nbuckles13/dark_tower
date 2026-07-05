// File: packages/web-app/vite/fingerprints.ts
//
// NODE-ONLY build helper (runs at Vite config time, never shipped to the
// browser). R-30: load the dev WebTransport cert fingerprints so the SDK can
// pin MC/MH self-signed certs via `serverCertificateHashes` in dev.
//
// The file `infra/docker/certs/fingerprints.json` is GENERATED (infra R-36,
// task #4) and gitignored — it does NOT exist on a fresh checkout. This loader
// degrades GRACEFULLY: absent/invalid file → empty hash list + an ACTIONABLE
// warning naming the fix (rather than a cryptic crash). With no hashes the demo
// still serves (HTTP views work); only the browser WebTransport handshake to
// MC/MH is refused — the log line says exactly that.

import { readFileSync } from 'node:fs';

/** A base64-encoded SHA-256 leaf-cert fingerprint (DER), one per MC/MH cert. */
export type CertSha256Base64 = string;

/** Shape of `infra/docker/certs/fingerprints.json` (tolerant — keys optional). */
interface FingerprintsFile {
  readonly mcCertSha256?: string;
  readonly mhCertSha256?: string;
  // Also accept the R-36 SCREAMING_SNAKE env-style keys.
  readonly MC_CERT_SHA256?: string;
  readonly MH_CERT_SHA256?: string;
}

/**
 * Read the dev cert fingerprints file and return the distinct MC+MH SHA-256
 * hashes (base64). The browser matches a presented cert against ANY hash in the
 * `serverCertificateHashes` list, so a single combined list covers both MC and
 * MH connects. Never throws — returns `[]` on any failure.
 */
export function loadCertFingerprints(path: string): CertSha256Base64[] {
  let raw: string;
  try {
    raw = readFileSync(path, 'utf8');
  } catch {
    console.warn(
      `[web-app] dev cert fingerprints not found at ${path} — WebTransport to MC/MH ` +
        `will be REFUSED by the browser (self-signed, unpinned). Fix: run ` +
        `scripts/generate-dev-certs.sh, then RESTART \`pnpm dev\` (fingerprints are read ` +
        `at Vite config time). HTTP sign-up/create/join views still work without it.`,
    );
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as FingerprintsFile;
    const hashes = [
      parsed.mcCertSha256 ?? parsed.MC_CERT_SHA256,
      parsed.mhCertSha256 ?? parsed.MH_CERT_SHA256,
    ].filter((h): h is string => typeof h === 'string' && h.length > 0);
    if (hashes.length === 0) {
      console.warn(`[web-app] ${path} contained no MC/MH SHA-256 fingerprints.`);
    }
    return hashes;
  } catch (err) {
    // Include the parse-error detail (bad token / position) to help a dev debug a
    // hand-edited / corrupt file. Leak-safe: build-time warning, cert hashes are
    // public and never shipped.
    const detail = err instanceof Error ? err.message : String(err);
    console.warn(
      `[web-app] failed to parse ${path} as JSON (${detail}) — ignoring dev fingerprints.`,
    );
    return [];
  }
}
