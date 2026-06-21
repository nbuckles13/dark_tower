import { defineConfig } from 'vitest/config';

// Component tier (ADR-0028 §test tiers): slower checks that exercise real
// build artifacts. `tests/bundle-content.test.ts` runs a real production
// `vite build` and asserts the dev-trust path is tree-shaken out (R-14). Kept
// OUT of the unit `include` glob so it never runs on the every-save inner
// loop; wired into CI via `nx affected -t test:unit test:component`
// (scripts/lang/ts/test.sh).
export default defineConfig({
  test: {
    environment: 'node',
    globals: false,
    include: ['tests/**/*.test.ts'],
    // A cold `vite build` dominates the runtime of this tier.
    testTimeout: 120_000,
    hookTimeout: 120_000,
  },
});
