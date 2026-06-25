import { defineConfig } from 'vitest/config';
import { resolve } from 'node:path';

// Unit tier (R-9 / ADR-0028 §test tiers): fast, in-memory, no real build.
// Scoped to co-located `src/**/__tests__/**` specs. The build-artifact
// contract test (`tests/bundle-content.test.ts`) lives in the slower
// `test:component` tier (vitest.component.config.ts) so it stays off the unit
// watch / inner loop.
export default defineConfig({
  define: {
    // Unit tests exercise the dev branch; mirror the non-prod Vite define so
    // `__DEV_TRUST_FINGERPRINT__` resolves under Vitest's transform.
    __DEV_TRUST_FINGERPRINT__: JSON.stringify(true),
    // Mirror the build-time SDK version literal (task #12) so `telemetryConfig`
    // / `logger` resolve `__SDK_VERSION__` under Vitest. A fixed test value
    // keeps assertions stable across version bumps.
    __SDK_VERSION__: JSON.stringify('0.0.0-test'),
  },
  resolve: {
    alias: {
      // Resolve the test-only dependency to its TS source so unit tests run
      // without first building test-utils' `dist/` (Nx `test:unit` does not
      // `dependsOn: ["^build"]`). This is a TEST-ONLY alias — it never affects
      // the production bundle, which has no edge to test-utils at all (R-13
      // Pattern A). The `src/index.ts` barrel exports `MockWebTransport` and
      // does NOT expose the test-only signer (which lives behind a sub-path).
      '@darktower/test-utils': resolve(__dirname, '../test-utils/src/index.ts'),
    },
  },
  test: {
    environment: 'node',
    globals: false,
    include: ['src/**/__tests__/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/__tests__/**', 'src/**/*.test.ts', 'src/**/*.d.ts'],
      // ≥90% gate (R-41 / R-47). Enforced here so `vitest run --coverage`
      // exits non-zero if any metric drops below threshold — CI does not
      // need a separate check step. src/index.ts barrel is included in
      // coverage (re-export lines score as covered); actual numbers
      // verified 2026-06-23: stmts 100%, branches 95.83%, funcs 100%,
      // lines 100%.
      thresholds: {
        statements: 90,
        branches: 90,
        functions: 90,
        lines: 90,
      },
    },
  },
});
