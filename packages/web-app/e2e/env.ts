// File: packages/web-app/e2e/env.ts
//
// Task #18 (R-40/R-44/R-46): the ONE host-topology config module for the browser
// E2E harness. Every URL the harness needs lives here as an env-overridable value
// with a documented default — specs and fixtures import from this module and
// NEVER hardcode a host or port (mirrors `ClusterPorts::from_env()` in
// `crates/env-tests/src/cluster.rs`, ADR-0030).
//
// Default sources (greppable trail for a port change — keep all three in sync):
//   - infra/kind/kind-config.yaml:26-35,45-46 — the SSoT host port mappings:
//     AC 127.0.0.1:8443, GC 127.0.0.1:8444, Prometheus 127.0.0.1:9090
//   - packages/web-app/vite.config.ts:21-22 — the dev-proxy consumer of the same
//     AC/GC defaults (VITE_AC_PROXY_TARGET / VITE_GC_PROXY_TARGET)
//   - crates/env-tests/src/cluster.rs — the Rust env-test counterpart
//     (ENV_TEST_AC_URL / ENV_TEST_GC_URL over port-forward defaults)
//
// MC/MH endpoints are deliberately ABSENT: the SDK learns them exclusively from
// the GC join response's `media_servers` (ADR-0030 — topology never computed
// client-side).

/** One resolved setting plus where it came from (env override vs default). */
interface Resolved {
  readonly value: string;
  readonly source: 'env' | 'default';
}

/**
 * Read an env-overridable URL. Fails loudly on a malformed override — a bad URL
 * must stop the run at config time, not surface as a cryptic fetch error mid-spec.
 */
function readUrl(name: string, fallback: string): Resolved {
  const raw = process.env[name];
  if (raw === undefined || raw === '') {
    return { value: fallback, source: 'default' };
  }
  const trimmed = raw.replace(/\/+$/, '');
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error(`${name}="${raw}" is not a valid URL (expected e.g. "${fallback}")`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`${name}="${raw}" must be http(s), got "${parsed.protocol}"`);
  }
  return { value: trimmed, source: 'env' };
}

const baseUrl = readUrl('E2E_BASE_URL', 'http://demo.localhost:5173');
// baseUrl with the hostname swapped to loopback — derived ONCE here (not in
// consumers) so the Vite port is never encoded twice and Node-side probes
// follow an E2E_BASE_URL override. Node does not necessarily resolve
// `*.localhost` names (that is a browser behavior); the browser itself still
// navigates to `baseUrl` with the org-subdomain Host.
const loopbackBase = new URL(baseUrl.value);
loopbackBase.hostname = '127.0.0.1';
const acUrl = readUrl('E2E_AC_URL', 'http://127.0.0.1:8443');
const gcUrl = readUrl('E2E_GC_URL', 'http://127.0.0.1:8444');
const prometheusUrl = readUrl('E2E_PROMETHEUS_URL', 'http://127.0.0.1:9090');

/** Resolved browser-E2E environment (see module header for the default sources). */
export const e2eEnv = {
  /**
   * The Vite-served demo origin. The `demo` subdomain in the Host header is
   * load-bearing: the dev proxy forwards it to AC for ADR-0020 org extraction
   * (vite.config.ts:52-57). Chromium resolves `*.localhost` natively.
   */
  baseUrl: baseUrl.value,
  /**
   * `baseUrl` on loopback, for Node-side probes only (Playwright webServer
   * readiness, global-setup's squatting-server check) — see derivation comment
   * above. Never hand this to the browser: the org-subdomain Host is
   * load-bearing for navigation.
   */
  loopbackBaseUrl: loopbackBase.origin,
  /** AC NodePort (health probe only — auth traffic flows through the Vite proxy). */
  acUrl: acUrl.value,
  /** GC NodePort (health probe + `bootstrapMeeting`'s direct POST /api/v1/meetings). */
  gcUrl: gcUrl.value,
  /** Prometheus (assertion (d): `mc_participant_mh_status_total` — see mcMetrics.ts). */
  prometheusUrl: prometheusUrl.value,
  /**
   * Org subdomain used by sign-up/sign-in. `demo` is seeded by
   * `infra/kind/scripts/setup.sh:seed_demo_org` (R-38) — the demo UI's default.
   */
  orgSubdomain: 'demo',
} as const;

/** Provenance lines for the global-setup log (env override vs documented default). */
export function describeEnv(): string[] {
  const entries: Array<[string, Resolved]> = [
    ['E2E_BASE_URL', baseUrl],
    ['E2E_AC_URL', acUrl],
    ['E2E_GC_URL', gcUrl],
    ['E2E_PROMETHEUS_URL', prometheusUrl],
  ];
  return entries.map(([name, r]) => `${name}=${r.value} (${r.source})`);
}
