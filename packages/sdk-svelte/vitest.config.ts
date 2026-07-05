import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';
import { playwright } from '@vitest/browser-playwright';

// R-43: REAL Vitest 4 Browser Mode (Chromium via the Playwright provider) — the
// new component test tier (sdk-core's `test:component` is a node-env bundle
// test, NOT this). `retries: 0` per ADR-0028 flaky policy. Runs `.svelte.ts` +
// `.svelte` through the svelte plugin so `$state` runes evaluate in the browser.
export default defineConfig({
  define: {
    // sdk-core source (aliased below) references these build-time literals; a
    // fixed `__SDK_VERSION__` keeps assertions stable across version bumps.
    __SDK_VERSION__: JSON.stringify('0.0.0-test'),
    __DEV_TRUST_FINGERPRINT__: JSON.stringify(true),
    __E2E_HOOKS__: JSON.stringify(true),
  },
  resolve: {
    alias: {
      // TEST-ONLY: resolve the sdk-core dependency to its TS source so component
      // tests run without first building sdk-core's `dist/`.
      '@darktower/sdk-core': resolve(__dirname, '../sdk-core/src/index.ts'),
    },
  },
  plugins: [svelte()],
  test: {
    include: ['src/**/__tests__/**/*.test.ts'],
    browser: {
      enabled: true,
      provider: playwright({
        launchOptions: {
          args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'],
        },
      }),
      instances: [{ browser: 'chromium' }],
      headless: true,
    },
    // ADR-0028 flaky policy: never mask failures with retries (`retry` is
    // Vitest's spelling of retries=0).
    retry: 0,
  },
});
