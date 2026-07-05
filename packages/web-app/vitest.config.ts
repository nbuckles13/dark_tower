import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';
import { playwright } from '@vitest/browser-playwright';

// R-43: REAL Vitest 4 Browser Mode (Chromium) component tests for the demo
// views + the E2E bus. Mocks `MeetingSession`; no real WebTransport/WebCodecs.
// `retries: 0` per ADR-0028 flaky policy. All build-time defines are mirrored
// here (@observability/@test REQ-2) so telemetry-touching imports resolve.
export default defineConfig({
  define: {
    __SDK_VERSION__: JSON.stringify('0.0.0-test'),
    __DEV_TRUST_FINGERPRINT__: JSON.stringify(true),
    // Enabled so the `window.__darktower_test__` bus is exercisable at this tier.
    __E2E_HOOKS__: JSON.stringify(true),
    __DEV_CERT_SHA256_HASHES__: JSON.stringify([]),
  },
  resolve: {
    alias: {
      '@darktower/sdk-core': resolve(__dirname, '../sdk-core/src/index.ts'),
      '@darktower/sdk-svelte': resolve(__dirname, '../sdk-svelte/src/index.ts'),
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
