import { defineConfig } from 'vitest/config';

// Node-env tier (R-14 / @security): the production bundle-content test runs a
// real `vite build` and asserts the E2E-hook + dev-trust surfaces are absent
// from the prod bundle. Kept OUT of the browser `test:component` glob; wired as
// web-app's `test:unit` target so CI's `pnpm test:unit` runs it.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
    // A cold production `vite build` dominates this tier's runtime.
    testTimeout: 180_000,
    hookTimeout: 180_000,
  },
});
