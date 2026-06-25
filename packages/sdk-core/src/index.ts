// File: packages/sdk-core/src/index.ts
//
// Public barrel for `@darktower/sdk-core`. The package `exports` map is closed
// (no `./*` wildcard) — this barrel is the only entry point. Production browser
// SDK core: WebTransport transport abstraction (R-13/R-14) and length-prefix
// wire framing (R-16). The full SignalingClient (proto encode/decode, join
// flow) is a later task and intentionally absent here.
//
// Telemetry SCAFFOLDING (task #12, R-19/R-24/R-25/R-26) lands here: the
// `MetricsSink` contract + sinks, the `dt_client_*` name guard, the single
// global `configureTelemetry`, the W3C trace-injection helper, and the
// bounded-event join logger. The actual emission sites (metric increments,
// `dt_client.join` span creation, wire-level trace injection) land in LATER
// tasks (#13+) — task #12 only provides the surfaces those tasks consume.

export {
  encodeFrame,
  FrameDecoder,
  FramingError,
  FramingErrorCode,
  MAX_MESSAGE_SIZE,
} from './framing/length-prefix.js';

export type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
} from './transport/IWebTransport.js';

export type { DevCertificateHash, WebTransportConnectOptions } from './transport/types.js';

export { BrowserWebTransport, connect } from './transport/BrowserWebTransport.js';

export { TransportError, TransportErrorCode } from './transport/errors.js';

// --- HTTP API (R-11, R-12) ---

export { AuthApiClient } from './http/AuthApiClient.js';
export type { AuthApiClientOptions } from './http/AuthApiClient.js';

export { MeetingApiClient } from './http/MeetingApiClient.js';
export type { MeetingApiClientOptions } from './http/MeetingApiClient.js';

export type {
  AuthTokenResponse,
  CreateMeetingInput,
  CreateMeetingResponse,
  FetchLike,
  JoinMeetingResponse,
  LoginInput,
  McAssignment,
  RegisterInput,
  RegisterResponse,
  UserTokenCredentials,
} from './http/types.js';

// --- Typed error hierarchy (R-23) ---

export { SdkError, SdkErrorCode } from './errors/SdkError.js';
export type { RedactedSdkError } from './errors/SdkError.js';

export {
  AuthError,
  AuthBadRequestError,
  AuthConflictError,
  AuthForbiddenError,
  AuthRateLimitError,
  AuthUnauthorizedError,
} from './errors/AuthError.js';

export {
  MeetingBadRequestError,
  MeetingConflictError,
  MeetingError,
  MeetingForbiddenError,
  MeetingNotFoundError,
  MeetingUnauthorizedError,
} from './errors/MeetingError.js';

export { NetworkError, NetworkErrorReason } from './errors/NetworkError.js';

export { ValidationError } from './errors/ValidationError.js';

export { SignalingError, SignalingErrorCode } from './errors/SignalingError.js';
export type { RedactedSignalingError, SignalingErrorOptions } from './errors/SignalingError.js';

// --- Telemetry scaffolding (R-19, R-24, R-25, R-26) ---

export type { MetricLabels, MetricsSink } from './telemetry/MetricsSink.js';

export { assertClientMetricName } from './telemetry/nameGuard.js';
export type { GuardMode } from './telemetry/nameGuard.js';

export { NoopMetricsSink } from './telemetry/NoopMetricsSink.js';
export { ConsoleMetricsSink } from './telemetry/ConsoleMetricsSink.js';
export { OtelMetricsSink } from './telemetry/OtelMetricsSink.js';

export {
  configureTelemetry,
  getMeter,
  getTracer,
  getMetricsSink,
  resetTelemetryForTest,
} from './telemetry/telemetryConfig.js';
export type { TelemetryConfig, TelemetryEnv } from './telemetry/telemetryConfig.js';

export { injectIntoClientMessage } from './telemetry/tracePropagation.js';
export type { TraceCarrier } from './telemetry/tracePropagation.js';

export { JoinEvent, logJoinEvent } from './telemetry/logger.js';
export type { JoinLogRecord } from './telemetry/logger.js';

export { CloseReason, normalizeCloseReason } from './telemetry/closeReason.js';

// --- Signaling (R-16, R-17, R-18, R-19) ---

export { SignalingClient } from './signaling/SignalingClient.js';
export type {
  SignalingClientOptions,
  SignalingEventMap,
  SignalingLogger,
  WebTransportConnectFn,
} from './signaling/SignalingClient.js';

export { ParticipantLeaveReason } from './signaling/events.js';
export type {
  JoinedEvent,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
  RosterParticipant,
  SignalingCapabilities,
  SignalingJoinParams,
} from './signaling/events.js';
