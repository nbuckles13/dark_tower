// Ambient declarations for `@darktower/sdk-svelte`.
//
// The Svelte 5 rune globals (`$state`, `$derived`, …) used in `.svelte.ts`
// modules are provided by the `svelte` types reference below so `svelte-check`
// and `vite-plugin-dts` (tsc) both resolve them.
/// <reference types="svelte" />

// Build-time Vite `define` literals. Only referenced transitively via the
// aliased `@darktower/sdk-core` source under Vitest (the lib build externalizes
// sdk-core, so these never appear in this package's own bundle) — declared here
// so type-checking resolves them.
declare const __SDK_VERSION__: string;
declare const __DEV_TRUST_FINGERPRINT__: boolean;
declare const __E2E_HOOKS__: boolean;
