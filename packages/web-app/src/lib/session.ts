// File: packages/web-app/src/lib/session.ts
//
// R-29/R-12: builders that wire the demo to the SDK. The join view uses
// `MeetingSession`; sign-up/sign-in use `AuthApiClient`; create-meeting uses
// `MeetingApiClient.createMeeting` directly (no MeetingSession for create).
//
// R-14: the injected `connect` wraps the SDK's dev-gated `connect`, threading the
// dev cert fingerprints into `serverCertificateHashes` for the MC/MH WebTransport
// handshake. In prod builds `__DEV_TRUST_FINGERPRINT__` is false, so the trust
// branch inside `connect` is tree-shaken and the hashes are ignored.

import {
  AuthApiClient,
  MeetingApiClient,
  MeetingSession,
  connect as baseConnect,
} from '@darktower/sdk-core';
import type { WebTransportConnectFn } from '@darktower/sdk-core';
import type { DemoConfig } from './config.js';

/** A `connect` that pins the dev MC/MH cert fingerprints (dev only). */
function makeConnect(config: DemoConfig): WebTransportConnectFn {
  return (url, options) =>
    baseConnect(url, { ...options, devCertificateHashes: config.devCertHashes });
}

/** Construct the AC HTTP client (sign-up / sign-in views). */
export function buildAuthClient(config: DemoConfig): AuthApiClient {
  return new AuthApiClient({ acOriginTemplate: config.acOriginTemplate });
}

/** Construct the GC HTTP client (create-meeting view). */
export function buildMeetingClient(config: DemoConfig): MeetingApiClient {
  return new MeetingApiClient({ gcBaseUrl: config.gcBaseUrl });
}

/** Construct a fresh single-use {@link MeetingSession} (meeting-join view). */
export function buildMeetingSession(config: DemoConfig): MeetingSession {
  return new MeetingSession({
    acOriginTemplate: config.acOriginTemplate,
    gcBaseUrl: config.gcBaseUrl,
    connect: makeConnect(config),
  });
}

/**
 * Configure SDK telemetry ONCE at startup — but ONLY when an endpoint is
 * configured (@observability Q1: telemetry OFF by default in `pnpm dev`; the SDK
 * stays on NoopMetricsSink otherwise). `env` comes from config (Vite mode),
 * never hardcoded `production`.
 */
export function configureTelemetryIfEnabled(config: DemoConfig): void {
  if (!config.telemetryEndpoint) return;
  MeetingSession.configure({ env: config.env, telemetryEndpoint: config.telemetryEndpoint });
}
