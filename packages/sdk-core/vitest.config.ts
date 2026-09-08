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
      // `src/proto/**` is GENERATED protobuf-es code (gitignored, produced by
      // `proto-gen:codegen`) — not hand-written, so it is excluded from the
      // coverage gate (task #13). It is exercised indirectly by the signaling
      // tests but should not score against the ≥90% threshold.
      exclude: [
        'src/**/__tests__/**',
        'src/**/*.test.ts',
        'src/**/*.d.ts',
        'src/proto/**',
        // --- Browser-media construction seams (story task 19) ---
        //
        // These three are THIN BY CONSTRUCTION: acquisition, a support probe,
        // and delegation to a seam. Every decision with a branch in it lives in
        // a module that IS scored — the failure classification in
        // `setup/captureFailure.ts`, the tuning comparison in
        // `setup/opus.ts::effectiveTuning` (pure, and covered), the encoder
        // configuration in `config/clientConfig.ts`, the playback scheduling in
        // `pipeline/playbackSink.ts`. What remains is calls into
        // `getUserMedia`, `AudioEncoder`, `AudioDecoder`,
        // `MediaStreamTrackProcessor` and `AudioContext`, NONE of which exist
        // under this tier's `environment: 'node'`.
        //
        // Excluded EXPLICITLY and narrowly rather than offset by the rest of the
        // suite, per CLAUDE.md's fail-loudly rule: a reviewable exclusion with a
        // stated reason beats a hidden gap. The condition on it is that these
        // files stay logic-free — if one needs a branch, the branch moves to a
        // tested module rather than the exclusion widening. They are exercised
        // for real by the browser env-test in the web-app story.
        'src/media/setup/capture.ts',
        'src/media/setup/opus.ts',
        'src/media/setup/audioPlayback.ts',
      ],
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
