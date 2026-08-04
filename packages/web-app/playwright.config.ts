// File: packages/web-app/playwright.config.ts
//
// Task #18: browser E2E harness config (ADR-0028 E2E tier, browser driver of the
// Env-Test tier). Runs against the LIVE host-side Kind cluster (ADR-0030) via
// the Vite dev server — see e2e/README.md for prerequisites and division of
// responsibilities vs the Rust env-tests.
//
// Runner note: the already-installed `playwright` package's `./test` export IS
// the @playwright/test runner (verified: package.json exports + test.d.ts) —
// same version as the Vitest browser provider, zero extra dependencies.

import { defineConfig } from 'playwright/test';
import { e2eEnv } from './e2e/env.js';

export default defineConfig({
  testDir: './e2e',
  outputDir: 'test-results',

  // ADR-0028 flaky policy: retries=0 — a flaky test is fixed or deleted, never
  // masked by retry. Do not override per-spec.
  retries: 0,
  // One worker, in-order: a single shared cluster + Vite server backs every
  // spec, and AC's registration rate limit (5/hour) leaves no headroom for
  // parallel credential churn. Escalation path is "more shards" (task #19),
  // not parallel workers against one cluster.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env['CI'],

  // Per-test ceiling so a wedged WebTransport handshake can never hang the
  // suite. Individual waits carry their own tighter, assertion-meaningful
  // timeouts (5s roster propagation, 60s Prometheus budget, ...).
  timeout: 120_000,
  expect: { timeout: 5_000 },

  reporter: [['list']],
  globalSetup: './e2e/global-setup.ts',

  use: {
    baseURL: e2eEnv.baseUrl,
    // Playwright's default actionTimeout is 0 (UNBOUNDED auto-wait): a single
    // locator action on a conditionally-rendered element could otherwise eat
    // the whole 120s test budget and mask the assertion-level failure message.
    // 15s comfortably exceeds every legitimate UI action in this suite.
    actionTimeout: 15_000,
    // retain-on-failure, NOT on-first-retry: with retries=0 a retry-gated trace
    // would never be captured. Traces contain this suite's synthetic-credential
    // traffic — gitignored, local-only (see e2e/README.md).
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    launchOptions: {
      // ADR-0028 fake-media flags. NO cert-bypass flags: MC/MH trust flows
      // exclusively through serverCertificateHashes pinning — a bypass would
      // mask exactly the invariant assertion (b) exists to prove.
      args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'],
    },
  },

  // Chromium only: the sole WebTransport-capable engine (ADR-0028).
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],

  webServer: {
    // Dev mode is load-bearing: `__E2E_HOOKS__` (the window.__darktower_test__
    // bus) and the dev cert-fingerprint define are compile-time gates that only
    // exist in dev builds. Never point this at `pnpm preview`/a prod build.
    command: 'pnpm dev',
    // Readiness via the loopback derivation of baseURL (single port encoding,
    // follows an E2E_BASE_URL override; Node does not necessarily resolve
    // *.localhost). The browser itself navigates to baseURL with the org
    // subdomain Host.
    url: e2eEnv.loopbackBaseUrl,
    reuseExistingServer: !process.env['CI'],
    timeout: 60_000,
  },
});
