// File: packages/sdk-core/src/transport/types.ts
//
// Connection-time options for the browser transport. Kept separate from the
// `IWebTransport` contract so the runtime instance shape stays identical to the
// interface (no connect-time concerns leak onto consumers of `IWebTransport`).

// R-14: this module is the dev-facing certificate-pinning API surface. The
// SDK-facing field below is deliberately named `devCertificateHashes` and NOT
// the browser's own option identifier. That browser identifier, and the
// dev-trust build-flag identifier, appear ONLY inside the dev-gated runtime
// branch of `connect` (where they are tree-shaken from prod executables) — they
// are intentionally kept out of every public `.d.ts` so the R-14 dt-guard's
// dist scan stays clean. See `BrowserWebTransport.ts` for the runtime gating.

/**
 * A dev-only certificate hash entry, structurally matching the browser
 * `WebTransportHash`. Used ONLY on the dev self-signed-cert trust path, which
 * is gated behind a build-time flag and tree-shaken out of production bundles.
 * The `value` (the actual fingerprint) is supplied by the caller at runtime via
 * config/env — never hardcoded.
 */
export interface DevCertificateHash {
  readonly algorithm: string;
  readonly value: BufferSource;
}

/**
 * Options accepted by the {@link BrowserWebTransport} connect factory.
 */
export interface WebTransportConnectOptions {
  /**
   * Dev-only: pinned certificate hashes for self-signed certs. IGNORED in
   * production builds (the consuming branch is dead-code-eliminated). When
   * provided in dev, the fingerprint value must come from runtime config —
   * passing a committed constant here will trip the no-secrets guard.
   */
  readonly devCertificateHashes?: ReadonlyArray<DevCertificateHash>;
}
