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
//
// ONE exception to "env-overridable value with a documented default":
// `E2E_ORG_SUBDOMAIN` is REQUIRED and has no default (R-7). See `readSubdomain`.

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

// ANCHOR (DRY): mirrors `SUBDOMAIN_REGEX` in
// packages/sdk-core/src/validation/limits.ts — the SSoT for the org-subdomain
// shape, itself anchored to AC's server-side rule
// (crates/ac-service/src/middleware/org_extraction.rs `extract_subdomain`:
// ASCII lowercase + digits + internal hyphens, DNS-label form; anything else
// yields None -> 400). Mirrored rather than imported: `validateSubdomain` is NOT
// exported from sdk-core's barrel (packages/sdk-core/src/index.ts), and widening
// the SDK's public API for a test-harness need is the wrong trade (@client +
// team-lead, task #3 Gate 1). Same anchoring convention limits.ts itself uses.
const SUBDOMAIN_REGEX = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;

/**
 * Read the REQUIRED per-run org subdomain. Deliberately NOT `readUrl`-shaped:
 * `readUrl` treats an empty string as "use the default", and the only value that
 * could be defaulted to here is the shared `demo` org — whose concurrent-meeting
 * cap of 10, against this suite's ~7 meetings per run, is the exact defect the
 * per-run org exists to remove (R-7). A silent fallback would leave every gate
 * green and the fix inert, so unset AND blank both throw.
 */
function readSubdomain(name: string): Resolved {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === '') {
    throw new Error(
      `${name} is required but ${raw === undefined ? 'unset' : 'blank'}. ` +
        `Layer 7 provisions a per-run organization and exports it (scripts/layer7.sh, ` +
        `Phase-2 export block). For a standalone run, provision one with ` +
        `infra/kind/scripts/setup.sh --provision-org and export the subdomain it ` +
        `reports. There is deliberately no default: falling back to the shared ` +
        `"demo" org re-creates the meeting-cap exhaustion this variable exists to avoid.`,
    );
  }
  // Validates `raw`, not a trimmed copy: stray whitespace fails loudly here
  // rather than being silently repaired into a different org's subdomain.
  if (!SUBDOMAIN_REGEX.test(raw)) {
    throw new Error(
      `${name}="${raw}" is not a valid org subdomain (DNS label: lowercase a-z, ` +
        `digits and internal hyphens, 1-63 chars). AC resolves the org from the ` +
        `Host header and rejects any other shape with a 400 — catching it here ` +
        `turns a cryptic mid-spec 400 into a named config-time failure.`,
    );
  }
  return { value: raw, source: 'env' };
}

const orgSubdomain = readSubdomain('E2E_ORG_SUBDOMAIN');
// SINGLE KNOB (@client, task #3 Gate 1). The page origin's host label and the org
// subdomain are ONE value, not two settings that happen to agree. Load-bearing,
// and invisible from this directory: the app builds the AC origin from the
// sign-up FORM FIELD, not from the page origin — src/lib/config.ts
// (`acOriginTemplate: 'http://{subdomain}.localhost:5173'`) feeds sdk-core's
// AuthApiClient `resolveAcOrigin(template, input.subdomain)`. The two collapsing
// to one origin is what keeps POST /api/v1/auth/register and /user/token
// SAME-ORIGIN. Let them diverge and every auth call becomes a cross-origin JSON
// POST with a CORS preflight through the Vite dev proxy — a path this suite has
// never exercised. So E2E_BASE_URL DEFAULTS from the subdomain, and if someone
// sets both they must agree (checked below). scripts/layer7.sh exports
// E2E_ORG_SUBDOMAIN ONLY — never E2E_BASE_URL, which would reintroduce exactly
// the drift this check exists to catch.
const baseUrl = readUrl('E2E_BASE_URL', `http://${orgSubdomain.value}.localhost:5173`);
const parsedBaseUrl = new URL(baseUrl.value);
const baseUrlLabel = parsedBaseUrl.hostname.split('.')[0] ?? '';
if (baseUrlLabel !== orgSubdomain.value) {
  throw new Error(
    `E2E_BASE_URL host label "${baseUrlLabel}" does not match ` +
      `E2E_ORG_SUBDOMAIN="${orgSubdomain.value}". These are one value, not two: ` +
      `the app derives the AC origin from the org subdomain, so a mismatch makes ` +
      `every auth call cross-origin. Set E2E_ORG_SUBDOMAIN and let E2E_BASE_URL ` +
      `default, or set both to the same subdomain.`,
  );
}
// baseUrl with the hostname swapped to loopback — derived ONCE here (not in
// consumers) so the Vite port is never encoded twice and Node-side probes
// follow an E2E_BASE_URL override. Node does not resolve `*.localhost` names
// (that is a browser behavior — see `toLoopbackUrl`); the browser itself still
// navigates to `baseUrl` with the org-subdomain Host.
const loopbackBase = new URL(baseUrl.value);
loopbackBase.hostname = '127.0.0.1';
const acUrl = readUrl('E2E_AC_URL', 'http://127.0.0.1:8443');
const gcUrl = readUrl('E2E_GC_URL', 'http://127.0.0.1:8444');
const prometheusUrl = readUrl('E2E_PROMETHEUS_URL', 'http://127.0.0.1:9090');

/** Resolved browser-E2E environment (see module header for the default sources). */
export const e2eEnv = {
  /**
   * The Vite-served demo origin. The ORG SUBDOMAIN in the Host header is
   * load-bearing: the dev proxy forwards it verbatim (`changeOrigin: false`) to
   * AC for ADR-0020 org extraction. It carries the RUN's org, not a fixed
   * `demo` — Vite admits any `*.localhost` Host and Chromium resolves them
   * natively, so no proxy change is needed per run.
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
   * Org subdomain used by sign-up/sign-in — a PER-RUN organization provisioned
   * by layer 7 (R-7), NOT the shared `demo` org: this suite creates ~7 meetings
   * per run against `demo`'s concurrent-meeting cap of 10, so the second run
   * against a live cluster used to fail with a 403 the pipeline attributed to
   * the diff. ALWAYS equal to `baseUrl`'s first host label — read the
   * single-knob note above before changing either.
   */
  orgSubdomain: orgSubdomain.value,
} as const;

/**
 * Rewrite a URL the BROWSER issued against {@link e2eEnv.baseUrl} into the URL a
 * NODE-side fetch must use: hostname swapped to loopback, everything else
 * (scheme, port, path, query, fragment) preserved byte-for-byte.
 *
 * THE FAILURE CLASS THIS EXISTS TO KILL — Node resolver vs browser resolver for
 * `*.localhost`. Chromium resolves ANY `*.localhost` label internally, so the
 * page loads from `http://e2e-<hex>.localhost:5173` with no DNS and no hosts
 * file. Node has no such rule: it goes to the system resolver, which answers for
 * `*.localhost` only if the host is configured to. In the devloop image it is
 * not — the only reason the OLD fixed `demo` subdomain ever worked Node-side was
 * a hardcoded `127.0.0.1 demo.localhost` line in `/etc/hosts`
 * (`dns.lookup('demo.localhost')` -> 127.0.0.1;
 * `dns.lookup('e2e-<hex>.localhost')` -> ENOTFOUND, measured). A per-run random
 * subdomain (R-7) can never be pre-seeded there, and seeding it at runtime would
 * need root and make the suite depend on mutating the container. So EVERY
 * Node-side hop that replays a browser URL — `route.fetch()`,
 * `APIRequestContext`, a bare `fetch()` built from `baseUrl` — must come through
 * here or die with `getaddrinfo ENOTFOUND e2e-<hex>.localhost`. Story R-7 task
 * #3 Gate 2 hit exactly this: 7 browser-side specs passed and the one spec with
 * a Node-side hop failed, deterministically (ADR-0028 retries=0 — not a flake).
 *
 * This moves the URL only. The caller keeps ownership of the request's HEADERS,
 * including `Host` — see the call site in fixtures.ts for why Host is not
 * load-bearing on the GC path but IS on the AC path.
 *
 * Throws on a URL from any other origin: silently redirecting an unrelated
 * origin at the Vite dev server would send a request to the wrong process and
 * report the wrong server's answer.
 */
export function toLoopbackUrl(browserUrl: string): string {
  let parsed: URL;
  try {
    parsed = new URL(browserUrl);
  } catch {
    throw new Error(`toLoopbackUrl("${browserUrl}") is not a valid absolute URL`);
  }
  if (parsed.origin !== parsedBaseUrl.origin) {
    throw new Error(
      `toLoopbackUrl("${browserUrl}"): origin "${parsed.origin}" is not the browser ` +
        `origin "${parsedBaseUrl.origin}". This helper swaps ONLY the Vite dev server's ` +
        `hostname to loopback; pointing another origin at ${loopbackBase.origin} would ` +
        `silently query a different server.`,
    );
  }
  parsed.hostname = loopbackBase.hostname;
  return parsed.toString();
}

/** Provenance lines for the global-setup log (env override vs documented default). */
export function describeEnv(): string[] {
  const entries: Array<[string, Resolved]> = [
    // First: it is the value the base URL derives from, so a reader scanning the
    // global-setup log sees which org the run used before anything else.
    ['E2E_ORG_SUBDOMAIN', orgSubdomain],
    ['E2E_BASE_URL', baseUrl],
    ['E2E_AC_URL', acUrl],
    ['E2E_GC_URL', gcUrl],
    ['E2E_PROMETHEUS_URL', prometheusUrl],
  ];
  return entries.map(([name, r]) => `${name}=${r.value} (${r.source})`);
}
