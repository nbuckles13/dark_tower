// Story 3 R-28, the CI caller of the VP8 substrate probe: runs the REAL in-page
// probe (`e2e/vp8Probe.ts`) in this tier's Chromium (vitest browser mode, the
// `test:component` step of ci-client.yml's Chromium job), at the repo's BASE
// media-codec-profile — injected by `vitest.config.ts` through
// `e2e/configEnv.ts:vp8ProbeProfileDefine`, never restated here.
//
// NEVER SKIPS (R-28): a missing or malformed profile define THROWS, and the
// verdict must be exactly `pass` — a substrate that cannot encode and decode VP8
// fails the job, it is never degraded to audio-only. Test-only: lives under
// `src/__tests__` because that is this tier's include glob; nothing in the app
// imports it, and the probe module imports nothing from the app or the SDK.

import { describe, expect, it } from 'vitest';

import {
  assertProbeProfile,
  classifyVp8Probe,
  PROBE_STEP_BOUND_MS,
  PROBE_TOTAL_BOUND_MS,
  runVp8ProbeInPage,
} from '../../e2e/vp8Probe.js';

/** Headroom over the probe's own worst case for page and codec setup. */
const COMPONENT_TIMEOUT_MARGIN_MS = 5_000;

// Injected by vitest.config.ts (test runner only; not an app-build define).
declare const __E2E_VP8_PROBE_PROFILE__: unknown;

function injectedProfile(): unknown {
  if (typeof __E2E_VP8_PROBE_PROFILE__ === 'undefined') {
    throw new Error(
      '__E2E_VP8_PROBE_PROFILE__ is not defined: vitest.config.ts must inject the base ' +
        'media-codec-profile via vp8ProbeProfileDefine — the probe never runs without it',
    );
  }
  return __E2E_VP8_PROBE_PROFILE__;
}

describe('VP8 substrate probe (R-28), component tier', () => {
  it(
    'this Chromium encodes and decodes VP8 at the base media-codec-profile',
    async () => {
      const profile = injectedProfile();
      assertProbeProfile(profile);
      const observation = await runVp8ProbeInPage({ profile, stepBoundMs: PROBE_STEP_BOUND_MS });
      expect(
        classifyVp8Probe(observation, profile),
        `VP8 substrate probe failed (observation ${JSON.stringify(observation)})`,
      ).toBe('pass');
      // Derived, never a literal: above every step bound, so a hung step reports
      // its classified *_timeout verdict instead of vitest's generic timeout.
    },
    PROBE_TOTAL_BOUND_MS + COMPONENT_TIMEOUT_MARGIN_MS,
  );
});
