// File: packages/web-app/src/lib/config.ts
//
// R-30: runtime demo configuration derived from Vite env + build-time defines.
// No secrets here — origins/ports only. The AC origin is subdomain-qualified
// (ADR-0020) and served same-origin through the dev proxy (see vite.config.ts);
// GC is same-origin too. Telemetry is OFF unless an endpoint is configured
// (@observability): the SDK stays on NoopMetricsSink until `configure` is called.

import type { DevCertificateHash, TelemetryEnv } from '@darktower/sdk-core';

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
}

/** Map the Vite mode string to the SDK's bounded telemetry env. */
function toTelemetryEnv(mode: string): TelemetryEnv {
  if (mode === 'production') return 'production';
  if (mode === 'test') return 'test';
  return 'development';
}

/** Decode a base64 SHA-256 fingerprint into bytes for `serverCertificateHashes`. */
function decodeBase64(b64: string): Uint8Array {
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

  const telemetryEndpoint = env.VITE_TELEMETRY_ENDPOINT;
  return {
    acOriginTemplate: env.VITE_AC_ORIGIN_TEMPLATE ?? 'http://{subdomain}.localhost:5173',
    gcBaseUrl: env.VITE_GC_BASE_URL ?? '',
    env: toTelemetryEnv(env.MODE),
    devCertHashes,
    ...(telemetryEndpoint ? { telemetryEndpoint } : {}),
  };
}
