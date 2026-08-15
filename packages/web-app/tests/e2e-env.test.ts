// R-7 (story task #3): pin `packages/web-app/e2e/env.ts`'s org-subdomain contract.
//
// WHY THIS TIER. `env.ts` resolves its whole configuration AT MODULE LOAD, and
// `playwright.config.ts` imports it at config-load time — so a missing or malformed
// `E2E_ORG_SUBDOMAIN` throws before any browser launches. That is the intended shape, and
// it is exactly why the browser E2E tier cannot test it: by the time a spec could assert
// anything, the throw has already aborted the run. These cases live in the node tier
// (`pnpm test:unit` -> `vitest.node.config.ts`, `include: tests/**/*.test.ts`) alongside
// `bundle-content.test.ts`, where the module can be loaded deliberately and repeatedly with
// controlled environment.
//
// WHAT IS BEING PROTECTED. The variable is REQUIRED with no default. A silent fallback to
// the shared `demo` org is R-7 itself: that org's concurrent-meeting cap is 10, this suite
// creates ~7 meetings per run, so the second run against a live cluster fails with a 403 the
// pipeline attributes to the diff. A fallback would leave every gate green and the fix inert
// — which is a failure mode no browser spec can observe, because the suite would PASS.

import { afterEach, beforeEach, expect, test, vi } from 'vitest';

/** Every variable `env.ts` reads — cleared before each case, restored after. */
const OWNED_VARS = [
  'E2E_ORG_SUBDOMAIN',
  'E2E_BASE_URL',
  'E2E_AC_URL',
  'E2E_GC_URL',
  'E2E_PROMETHEUS_URL',
] as const;

let saved: Partial<Record<(typeof OWNED_VARS)[number], string | undefined>> = {};

beforeEach(() => {
  saved = {};
  for (const name of OWNED_VARS) {
    saved[name] = process.env[name];
    delete process.env[name];
  }
});

afterEach(() => {
  for (const name of OWNED_VARS) {
    if (saved[name] === undefined) delete process.env[name];
    else process.env[name] = saved[name];
  }
});

/**
 * Load a FRESH copy of `env.ts` under the current `process.env`.
 *
 * `vi.resetModules()` before every import is load-bearing, not hygiene: `env.ts` resolves
 * everything in module scope, so a cached copy would make the FIRST case decide the outcome
 * of all of them. The suite would still go green — it would just be asserting one resolution
 * ten times. The specifier is static because Vite cannot analyse a fully dynamic one.
 */
async function loadEnvModule(): Promise<typeof import('../e2e/env.js')> {
  vi.resetModules();
  return await import('../e2e/env.js');
}

test('E2E_ORG_SUBDOMAIN unset throws, and does not fall back to a default', async () => {
  await expect(loadEnvModule()).rejects.toThrow(/E2E_ORG_SUBDOMAIN is required but unset/);
  // The error must be actionable: an operator seeing it needs to know who normally sets the
  // variable and how to get one for a standalone run.
  await expect(loadEnvModule()).rejects.toThrow(/layer7\.sh/);
  await expect(loadEnvModule()).rejects.toThrow(/--provision-org/);
  // Naming `demo` is the point of the message — it records WHY there is no default.
  await expect(loadEnvModule()).rejects.toThrow(/demo/);
});

test('E2E_ORG_SUBDOMAIN blank throws — an empty value is NOT "use the default"', async () => {
  // Deliberately NOT `readUrl`'s shape. `readUrl` treats '' exactly like unset and returns
  // its fallback, so reusing it here would silently restore `demo` on a blank variable — the
  // subtlest way to re-introduce R-7, and one that leaves the suite green. A shell exporting
  // the result of a failed command substitution produces precisely this value.
  process.env.E2E_ORG_SUBDOMAIN = '';
  await expect(loadEnvModule()).rejects.toThrow(/E2E_ORG_SUBDOMAIN is required but blank/);
});

test('E2E_ORG_SUBDOMAIN whitespace-only throws', async () => {
  process.env.E2E_ORG_SUBDOMAIN = '   ';
  await expect(loadEnvModule()).rejects.toThrow(/E2E_ORG_SUBDOMAIN is required but blank/);
});

test('E2E_ORG_SUBDOMAIN uppercase throws rather than being lowercased', async () => {
  // AC lowercases nothing: it resolves the org from the Host header with an exact match, so
  // a silently-normalized value would address a DIFFERENT organization than the one layer 7
  // provisioned and exported. Reject, never repair.
  process.env.E2E_ORG_SUBDOMAIN = 'E2E-ABCDEF0123456789';
  await expect(loadEnvModule()).rejects.toThrow(/is not a valid org subdomain/);
});

test.each([
  ['leading hyphen', '-leading'],
  ['trailing hyphen', 'trailing-'],
  ['underscore', 'has_underscore'],
  ['embedded space', 'has space'],
  ['leading whitespace', ' e2e-0123456789abcdef'],
  ['non-ASCII (Cyrillic U+0430)', 'аbc'],
  ['64 characters', 'a'.repeat(64)],
])('E2E_ORG_SUBDOMAIN rejects %s', async (_label, value) => {
  // Each row is a distinct way to violate the anchored DNS-label rule, not a respelling of
  // one. The whitespace row matters most: `readSubdomain` validates the RAW value rather
  // than a trimmed copy, so stray whitespace fails loudly here instead of being repaired
  // into some other organization's subdomain.
  process.env.E2E_ORG_SUBDOMAIN = value;
  await expect(loadEnvModule()).rejects.toThrow(/is not a valid org subdomain/);
});

test('a well-formed per-run subdomain resolves and E2E_BASE_URL DERIVES from it', async () => {
  // The single-knob property, and it is a CORRECTNESS constraint rather than tidiness: the
  // app builds the AC origin from the sign-up FORM FIELD (src/lib/config.ts's
  // `acOriginTemplate` -> sdk-core's `resolveAcOrigin`), so page origin and org subdomain
  // collapsing to one host is what keeps POST /api/v1/auth/register and /user/token
  // SAME-ORIGIN. Let them diverge and every auth call becomes a cross-origin JSON POST with
  // a CORS preflight through the Vite dev proxy — a path this suite has never exercised.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  const { e2eEnv } = await loadEnvModule();
  expect(e2eEnv.orgSubdomain).toBe('e2e-0123456789abcdef');
  expect(e2eEnv.baseUrl).toBe('http://e2e-0123456789abcdef.localhost:5173');
  // Node does not necessarily resolve `*.localhost`, so Node-side probes get loopback —
  // derived once here rather than in each consumer, so the Vite port is encoded once.
  expect(e2eEnv.loopbackBaseUrl).toBe('http://127.0.0.1:5173');
});

test('E2E_BASE_URL whose host label disagrees with the subdomain throws', async () => {
  // scripts/layer7.sh exports E2E_ORG_SUBDOMAIN ONLY, never E2E_BASE_URL, precisely so this
  // cannot trip in the pipeline. The check exists for a human who sets both by hand: a
  // mismatch is silent at config time and surfaces as unexplained auth failures mid-suite.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  process.env.E2E_BASE_URL = 'http://demo.localhost:5173';
  await expect(loadEnvModule()).rejects.toThrow(/does not match/);
  await expect(loadEnvModule()).rejects.toThrow(/E2E_ORG_SUBDOMAIN/);
});

test('E2E_BASE_URL that AGREES with the subdomain is accepted', async () => {
  // The negative twin of the case above. Without it, a check that threw unconditionally
  // would still pass the mismatch test — and would break every explicit-base-URL run.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  process.env.E2E_BASE_URL = 'http://e2e-0123456789abcdef.localhost:5199';
  const { e2eEnv } = await loadEnvModule();
  expect(e2eEnv.baseUrl).toBe('http://e2e-0123456789abcdef.localhost:5199');
  expect(e2eEnv.loopbackBaseUrl).toBe('http://127.0.0.1:5199');
});

// ---------------------------------------------------------------------------
// toLoopbackUrl — the Node-side hop (R-7 task #3 Gate 2 regression)
//
// Gate 2 failed on `route.fetch: getaddrinfo ENOTFOUND e2e-88ef8994f5c08774.localhost`:
// `route.fetch()` runs in NODE and replayed the browser's org-subdomain URL verbatim.
// Chromium resolves any `*.localhost` label internally, Node does not — the old fixed
// `demo` subdomain only ever worked Node-side because of a hardcoded `/etc/hosts` entry,
// which a per-run random subdomain can never have. These cases pin the swap. They are the
// node tier deliberately: a browser spec can only observe the SYMPTOM (one ENOTFOUND), and
// only for the one path a spec happens to intercept.
// ---------------------------------------------------------------------------

test('toLoopbackUrl swaps ONLY the hostname, preserving port, path, query and fragment', async () => {
  // Everything but the hostname is load-bearing on this path: the port reaches the same Vite
  // dev server, and the path is the GC join route whose response the caller rewrites.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  const { toLoopbackUrl } = await loadEnvModule();
  expect(
    toLoopbackUrl('http://e2e-0123456789abcdef.localhost:5173/api/v1/meetings/eqMW1rf87OQp'),
  ).toBe('http://127.0.0.1:5173/api/v1/meetings/eqMW1rf87OQp');
  expect(toLoopbackUrl('http://e2e-0123456789abcdef.localhost:5173/a/b?x=1&y=2#frag')).toBe(
    'http://127.0.0.1:5173/a/b?x=1&y=2#frag',
  );
});

test('toLoopbackUrl follows an E2E_BASE_URL port override rather than re-encoding 5173', async () => {
  // The Vite port has exactly one encoding (env.ts). A helper that hardcoded it would send
  // the Node-side hop to a server the browser is not using — and report ITS answer.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  process.env.E2E_BASE_URL = 'http://e2e-0123456789abcdef.localhost:5199';
  const { toLoopbackUrl } = await loadEnvModule();
  expect(toLoopbackUrl('http://e2e-0123456789abcdef.localhost:5199/api/v1/meetings/abc')).toBe(
    'http://127.0.0.1:5199/api/v1/meetings/abc',
  );
});

test.each([
  ['a different host label', 'http://demo.localhost:5173/api/v1/meetings/abc'],
  ['a different port', 'http://e2e-0123456789abcdef.localhost:5199/api/v1/meetings/abc'],
  ['a different scheme', 'https://e2e-0123456789abcdef.localhost:5173/api/v1/meetings/abc'],
  ['an unrelated origin', 'http://127.0.0.1:8444/api/v1/meetings/abc'],
])('toLoopbackUrl throws on %s — it must not silently retarget another origin', async (_l, url) => {
  // Fail loudly, never mask: pointing an unrelated origin at the Vite dev server would query
  // a DIFFERENT process and report that process's answer as if it were the one under test.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  const { toLoopbackUrl } = await loadEnvModule();
  expect(() => toLoopbackUrl(url)).toThrow(/is not the browser origin/);
});

test('toLoopbackUrl throws on a non-absolute URL', async () => {
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  const { toLoopbackUrl } = await loadEnvModule();
  expect(() => toLoopbackUrl('/api/v1/meetings/abc')).toThrow(/not a valid absolute URL/);
});

test('describeEnv reports the org subdomain first, with its provenance', async () => {
  // global-setup prints these lines; the org is the value the base URL derives from, so a
  // reader scanning the log must see which organization the run used before anything else.
  process.env.E2E_ORG_SUBDOMAIN = 'e2e-0123456789abcdef';
  const { describeEnv } = await loadEnvModule();
  const lines = describeEnv();
  expect(lines[0]).toBe('E2E_ORG_SUBDOMAIN=e2e-0123456789abcdef (env)');
  expect(lines[1]).toBe('E2E_BASE_URL=http://e2e-0123456789abcdef.localhost:5173 (default)');
});
