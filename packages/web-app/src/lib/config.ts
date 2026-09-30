// File: packages/web-app/src/lib/config.ts
//
// R-30: runtime demo configuration derived from Vite env + build-time defines.
// No secrets here — origins/ports only. The AC origin is subdomain-qualified
// (ADR-0020) and served same-origin through the dev proxy (see vite.config.ts);
// GC is same-origin too. Telemetry is OFF unless an endpoint is configured
// (@observability): the SDK stays on NoopMetricsSink until `configure` is called.

import { parseReceiveSlots } from '@darktower/sdk-core';
import type { DevCertificateHash, ParsedReceiveSlots, TelemetryEnv } from '@darktower/sdk-core';

/** Resolved demo configuration. */
export interface DemoConfig {
  /** AC origin template with `{subdomain}` placeholder (R-11). */
  readonly acOriginTemplate: string;
  /** GC base URL (empty = same-origin, proxied in dev). */
  readonly gcBaseUrl: string;
  /** Telemetry environment — derived from Vite mode, NEVER hardcoded (@observability REQ-1). */
  readonly env: TelemetryEnv;
  /** GC `/api/v1/telemetry` endpoint; when absent, telemetry stays OFF. */
  readonly telemetryEndpoint?: string;
  /** Dev MC/MH cert fingerprints for WebTransport pinning (empty in prod). */
  readonly devCertHashes: readonly DevCertificateHash[];
  /**
   * N — the audio receive slots this client declares, from
   * `VITE_DT_RECEIVE_SLOTS`, with whether it was configured or defaulted.
   *
   * N IS A REQUEST bounded by MC's `MC_MAX_RECEIVE_SLOTS` (advertised on
   * `JoinResponse.max_receive_slots`). Above the cap MC rejects the WHOLE
   * declaration — never clamps — so the SDK refuses it loudly at `startMedia()`
   * and the in-meeting view shows the error; nothing shrinks N to fit. Two
   * quantities, not a duplicate: no copy of the cap exists here.
   *
   * A BROWSER-SIDE BUILD KNOB, NOT COVERED BY `dt-guard env-config` (which reads
   * `infra/services/**` only). The browser suite asserts the effective N at
   * runtime (the E2E bus `receiveSlots` event) instead. `scripts/dev-web.sh`
   * always exports it (its demo topology, `DEMO_RECEIVE_SLOTS`); absent, the
   * SDK's own default applies and is reported as `source: 'default'`.
   */
  readonly receiveSlots: ParsedReceiveSlots;
}

/** Map the Vite mode string to the SDK's bounded telemetry env. */
function toTelemetryEnv(mode: string): TelemetryEnv {
  if (mode === 'production') return 'production';
  if (mode === 'test') return 'test';
  return 'development';
}

// The `<ArrayBuffer>` argument is load-bearing: `serverCertificateHashes` wants a
// `BufferSource`, and since TS 6 a bare `Uint8Array` widens to
// `Uint8Array<ArrayBufferLike>`, which admits `SharedArrayBuffer` and so is not
// assignable. Allocating from a length always yields a plain `ArrayBuffer`.
/** Decode a base64 SHA-256 fingerprint into bytes for `serverCertificateHashes`. */
function decodeBase64(b64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/** Build the {@link DemoConfig} from `import.meta.env` + the cert-hash define. */
export function loadConfig(): DemoConfig {
  const env = import.meta.env;
  const devCertHashes: DevCertificateHash[] = __DEV_CERT_SHA256_HASHES__.map((b64) => ({
    algorithm: 'sha-256',
    value: decodeBase64(b64),
  }));

  // THROWS on a present-but-malformed value (`0`, `03`, `3.0`, ` 3`, empty,
  // non-numeric) — the same class `scripts/dev-web.sh` refuses — rather than
  // falling back: a silently substituted N is a participant who hears fewer
  // people than configured with nothing saying so.
  const receiveSlots = parseReceiveSlots(env.VITE_DT_RECEIVE_SLOTS);
  const telemetryEndpoint = env.VITE_TELEMETRY_ENDPOINT;
  const gcBaseUrl = env.VITE_GC_BASE_URL ?? '';
  if (telemetryEndpoint) {
    assertTelemetrySharesGcOrigin(telemetryEndpoint, gcBaseUrl);
  }
  return {
    acOriginTemplate: env.VITE_AC_ORIGIN_TEMPLATE ?? 'http://{subdomain}.localhost:5173',
    gcBaseUrl,
    env: toTelemetryEnv(env.MODE),
    devCertHashes,
    receiveSlots,
    ...(telemetryEndpoint ? { telemetryEndpoint } : {}),
  };
}

/**
 * Catch a telemetry endpoint that DIVERGES from the GC API's origin.
 *
 * READ WHAT THIS DOES AND DOES NOT DO. On the default path it is VACUOUSLY TRUE:
 * the browser endpoint is relative and `gcBaseUrl` defaults to `''`, so both
 * resolve to `location.origin` and the comparison cannot fail whatever either
 * value contains. What keeps telemetry same-origin on that path is the RELATIVE
 * SPELLING, not this assertion. Do not read it as a guarantee, and in particular
 * do not make the endpoint absolute on the reasoning that this will catch a
 * mistake — on the default configuration it cannot fire.
 *
 * What it DOES catch is divergence once either side is absolute: a baked
 * per-subdomain endpoint against a live page origin, or an endpoint repointed at
 * another host.
 *
 * Every metric export carries the user's bearer token (GC's telemetry proxy is
 * behind `require_user_auth`), so this value decides who receives a live user
 * credential. The invariant is deliberately "the same service we ALREADY give
 * this token to", not "the same origin as the page" — the latter would break a
 * CDN-hosted app for no security gain, since the page origin never sees the token.
 *
 * THROWN, NOT WARNED, AND CHECKED AT CONFIG LOAD. The failure it guards is a
 * deploy misconfiguration pointing telemetry at a third-party collector, which
 * ships user JWTs off-estate silently and works perfectly from the app's point of
 * view. A doc comment does not run at deploy time and a console warning at
 * startup is read by nobody; failing to boot is the only signal that cannot be
 * ignored.
 *
 * Both values default to RELATIVE (`gcBaseUrl` is `''`, and the telemetry
 * endpoint is `/api/v1/telemetry`), so both are resolved against
 * `location.origin` before comparing. That makes both-relative, both-absolute
 * and mixed cases behave the same way instead of needing three branches.
 */
function assertTelemetrySharesGcOrigin(telemetryEndpoint: string, gcBaseUrl: string): void {
  const base = typeof location === 'undefined' ? 'http://localhost' : location.origin;
  const telemetryOrigin = new URL(telemetryEndpoint, base).origin;
  const gcOrigin = new URL(gcBaseUrl, base).origin;
  if (telemetryOrigin !== gcOrigin) {
    throw new Error(
      `VITE_TELEMETRY_ENDPOINT (${telemetryOrigin}) must share an origin with the GC API ` +
        `(${gcOrigin}). Client metric exports carry the user's bearer token, so a telemetry ` +
        `endpoint on another origin would send a live user credential to a service that is ` +
        `not GC. Point it at the GC origin, or leave it unset to disable telemetry.`,
    );
  }
}
