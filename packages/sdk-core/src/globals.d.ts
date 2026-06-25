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
