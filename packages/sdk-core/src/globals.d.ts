// File: packages/sdk-core/src/globals.d.ts
//
// Ambient declaration for the build-time literal injected by Vite `define`.
// R-14: `__DEV_TRUST_FINGERPRINT__` is `false` in production bundles (so the
// dev-only `serverCertificateHashes` trust branch is statically eliminated /
// tree-shaken) and `true` in dev/test builds. See `vite.config.ts`.

declare const __DEV_TRUST_FINGERPRINT__: boolean;

// Build-time SDK version literal (task #12, R-24/R-26). Substituted by Vite
// `define` from `package.json` version at build, and mirrored under Vitest so
// `telemetryConfig`/`logger` resolve it in tests. Used as the OTel resource
// `service.version` attribute and the `client_version` log/metric field.
declare const __SDK_VERSION__: string;

// Story 2 R-7: the TEST-TONE build define. `false` in the SDK's library build
// (`vite.config.ts`) and in every vitest config; in the web app it is opt-in
// (`DT_TEST_TONE=1`) in dev and a hard `false` — with a build-time THROW on the
// opt-in — in production (`packages/web-app/vite/testTone.ts`). Read at exactly
// one site: the capture-source selection, `session/mediaSelection.ts:selectCaptureSource`.
declare const __DT_TEST_TONE__: boolean;
