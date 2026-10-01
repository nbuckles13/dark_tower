// File: packages/web-app/vite/sourceAliases.ts
//
// NODE-ONLY test helper. Both vitest tiers resolve the workspace SDKs to their
// SOURCE, so tests never depend on a prior `dist` build (CI's unit-test job
// builds none). One map, imported by vitest.config.ts and vitest.node.config.ts.
import { resolve } from 'node:path';

export const sdkSourceAliases = {
  '@darktower/sdk-core': resolve(__dirname, '../../sdk-core/src/index.ts'),
  '@darktower/sdk-svelte': resolve(__dirname, '../../sdk-svelte/src/index.ts'),
};
