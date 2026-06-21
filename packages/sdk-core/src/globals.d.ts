// File: packages/sdk-core/src/globals.d.ts
//
// Ambient declaration for the build-time literal injected by Vite `define`.
// R-14: `__DEV_TRUST_FINGERPRINT__` is `false` in production bundles (so the
// dev-only `serverCertificateHashes` trust branch is statically eliminated /
// tree-shaken) and `true` in dev/test builds. See `vite.config.ts`.

declare const __DEV_TRUST_FINGERPRINT__: boolean;
