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
/** Story 2 R-7 test-tone build define; `false` under this package's vitest. */
declare const __DT_TEST_TONE__: boolean;

// Story 3 R-27: the TEST-PATTERN camera define. Same rules as `__DT_TEST_TONE__`:
// `false` in the SDK library build and every vitest config, opt-in
// (`DT_TEST_PATTERN=1`) in web-app dev, a hard `false` with a build-time THROW on
// the opt-in in production. Read at exactly one site:
// `session/mediaSelection.ts:selectVideoCaptureSource`.
declare const __DT_TEST_PATTERN__: boolean;
