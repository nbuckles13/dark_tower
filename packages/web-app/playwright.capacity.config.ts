// File: packages/web-app/playwright.capacity.config.ts
//
// Story 3 R-28 capacity EVIDENCE run (task 21): the base harness config,
// unchanged (global setup, the once-per-run cohort, retries 0, Chromium only,
// the fake-media flags and NO cert-bypass flag), matching ONLY
// `e2e/capacity.evidence.ts`. A recorded run, not a gate: the default
// `test:e2e` (Layer 7) never matches that file. Run it with
// `pnpm nx run web-app:test:capacity-evidence` (through Nx so proto codegen runs
// first; `pnpm test:capacity-evidence` in this package is the same call); the record lands in
// the manual test plan's "Capacity evidence" section.

import { defineConfig } from 'playwright/test';
import base from './playwright.config.js';

export default defineConfig({
  ...base,
  testMatch: /capacity\.evidence\.ts$/,
});
