// File: packages/web-app/e2e/global-setup.ts
//
// Task #18: fail-loud preconditions for the browser E2E suite. Every check here
// either passes or THROWS with the exact remediation — never skip, never
// warn-and-continue (a masked precondition becomes a cryptic mid-spec timeout).
//
// Order:
//   1. Dev WebTransport cert fingerprints exist (MC + MH) — without them the
//      browser refuses the MC/MH handshakes and assertion (b) can only time out.
//   2. Host-side Kind cluster reachable: AC + GC health, Prometheus healthy
//      (Prometheus is REQUIRED — assertion (d) reads MC's counter through it).

import { fileURLToPath } from 'node:url';
// Reuse the ONE fingerprints parser (writer: scripts/generate-dev-certs.sh, which
// emits MC_CERT_SHA256 / MH_CERT_SHA256 into BOTH fingerprints.json and the
// shell-sourceable fingerprints.env — same canonical pair, one writer). The Vite
// loader degrades gracefully by design (dev server must still serve without
// certs); this wrapper adds the fail-loud contract the E2E suite needs.
import { loadCertFingerprints } from '../vite/fingerprints.js';
import { describeEnv, e2eEnv } from './env.js';

// ESM-safe repo-relative path (this file runs under Playwright's ESM loader —
// no __dirname).
const FINGERPRINTS_JSON = fileURLToPath(
  new URL('../../../infra/docker/certs/fingerprints.json', import.meta.url),
);

/** Probe one HTTP endpoint with a bounded timeout; return an error line or null. */
async function probe(label: string, url: string, remediation: string): Promise<string | null> {
  try {
    const response = await fetch(url, { signal: AbortSignal.timeout(5000) });
    if (!response.ok) {
      return `${label}: ${url} responded ${response.status}. ${remediation}`;
    }
    return null;
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    return `${label}: ${url} unreachable within 5s (${detail}). ${remediation}`;
  }
}

export default async function globalSetup(): Promise<void> {
  console.log('[e2e] resolved environment:');
  for (const line of describeEnv()) {
    console.log(`[e2e]   ${line}`);
  }

  // --- 1. Cert fingerprints (R-36) ---
  const hashes = loadCertFingerprints(FINGERPRINTS_JSON);
  if (hashes.length < 2) {
    throw new Error(
      `[e2e] dev WebTransport cert fingerprints missing or incomplete at ` +
        `${FINGERPRINTS_JSON} (found ${hashes.length}, need MC + MH). ` +
        `Fix: run scripts/generate-dev-certs.sh, then restart the Vite dev server ` +
        `(fingerprints are read at Vite config time). Without them the browser ` +
        `refuses the MC/MH self-signed certs and every join spec fails at the ` +
        `WebTransport handshake.`,
    );
  }
  console.log(`[e2e] cert fingerprints OK (${hashes.length} hashes: MC + MH)`);

  // --- 2. Cluster reachability ---
  const clusterFix =
    'Is the host-side Kind cluster up? Bring it up with infra/kind/scripts/setup.sh ' +
    '(see e2e/README.md prerequisites).';
  const prometheusFix =
    'The observability stack is REQUIRED by this suite (assertion (d) reads ' +
    "mc_participant_mh_status_total via Prometheus) — do NOT use setup.sh's " +
    '--skip-observability option.';
  const failures = (
    await Promise.all([
      probe('AC health', `${e2eEnv.acUrl}/health`, clusterFix),
      probe('GC health', `${e2eEnv.gcUrl}/health`, clusterFix),
      probe('Prometheus health', `${e2eEnv.prometheusUrl}/-/healthy`, prometheusFix),
    ])
  ).filter((f): f is string => f !== null);
  if (failures.length > 0) {
    throw new Error(`[e2e] cluster preconditions failed:\n  - ${failures.join('\n  - ')}`);
  }
  console.log('[e2e] cluster preconditions OK (AC, GC, Prometheus)');

  // --- Informational: squatting-server note (see also waitForJoined's failure text) ---
  // reuseExistingServer means an already-running server on the Vite port is
  // reused as-is. It MUST be `pnpm dev` (dev mode => __E2E_HOOKS__ bus present);
  // a `pnpm preview` / prod build has the bus dead-code-eliminated and every
  // spec would time out in waitForJoined.
  // Probe via the loopback derivation of baseUrl (env.ts owns the one
  // hostname-swap: Node does not necessarily resolve `*.localhost` names, and
  // this note must not silently vanish on hosts without an /etc/hosts entry).
  const probeUrl = new URL(e2eEnv.loopbackBaseUrl);
  try {
    await fetch(probeUrl, { signal: AbortSignal.timeout(1000) });
    console.log(
      `[e2e] note: a server is already running on port ${probeUrl.port} and will be ` +
        `reused — it must be a DEV-mode Vite server (pnpm dev), not preview/prod.`,
    );
  } catch {
    // Nothing listening yet — Playwright's webServer will start `pnpm dev` itself.
  }
}
