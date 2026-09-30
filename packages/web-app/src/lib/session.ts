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
import type { E2EInstrumentation } from './e2eAnalysis.js';
import { blockingConnect, type TestLevers } from './testLevers.js';

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

/**
 * Construct a fresh single-use {@link MeetingSession} (meeting-join view).
 *
 * `receiveSlots` is N from `VITE_DT_RECEIVE_SLOTS` (see `config.ts`), unless a
 * test-levers build overrides it for this context. `e2e` is
 * the test build's receive-side instrumentation (`e2eAnalysis.ts`); it is
 * `undefined` in production, so neither the verification recorder nor the
 * analysing playback wrapper is ever injected there.
 */
export function buildMeetingSession(
  config: DemoConfig,
  e2e?: E2EInstrumentation,
  levers?: TestLevers,
): MeetingSession {
  // `levers` exists only in a `DT_TEST_LEVERS=1` build (`testLevers.ts`). The
  // define is tested AT EACH USE so the lever code is statically dead — and
  // tree-shaken — in every other build, not merely unreached.
  const connect = makeConnect(config);
  const active = __DT_TEST_LEVERS__ ? levers : undefined;
  return new MeetingSession({
    acOriginTemplate: config.acOriginTemplate,
    gcBaseUrl: config.gcBaseUrl,
    connect: __DT_TEST_LEVERS__ && active ? blockingConnect(connect, active) : connect,
    receiveSlots: (__DT_TEST_LEVERS__ ? active?.receiveSlots : undefined) ?? config.receiveSlots,
    ...(e2e ? { receiveVerification: e2e.recorder, playbackFactory: e2e.playbackFactory } : {}),
  });
}

/**
 * Configure SDK telemetry ONCE at startup — but ONLY when an endpoint is
 * configured (@observability Q1: telemetry OFF by default in `pnpm dev`; the SDK
 * stays on NoopMetricsSink otherwise). `env` comes from config (Vite mode),
 * never hardcoded `production`.
 */
export function configureTelemetryIfEnabled(
  config: DemoConfig,
  authTokenProvider: () => string | undefined,
): void {
  if (!config.telemetryEndpoint) return;
  MeetingSession.configure({
    env: config.env,
    telemetryEndpoint: config.telemetryEndpoint,
    // A GETTER READING `App.svelte`'s SESSION STATE — never a copy of the token.
    // GC's telemetry proxy requires a bearer credential, but telemetry is
    // configured once at mount while the token arrives at sign-in, changes on
    // re-auth and is cleared on sign-out and on the 401 drop. A second holder of
    // that credential would need its own clearing on both those paths and would
    // drift from `AuthSession.userToken` the moment one was cleared and the other
    // was not — the defect class `lib/types.ts` documents. The closure stores
    // nothing, so the token keeps exactly one home.
    authTokenProvider,
  });
}
