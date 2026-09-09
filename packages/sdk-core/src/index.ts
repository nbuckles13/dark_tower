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

export { MediaConnectionError, MediaConnectionErrorCode } from './errors/MediaConnectionError.js';
export type {
  MediaConnectionErrorOptions,
  RedactedMediaConnectionError,
} from './errors/MediaConnectionError.js';

// --- Telemetry scaffolding (R-19, R-24, R-25, R-26) ---

export type { MetricLabels, MetricsSink } from './telemetry/MetricsSink.js';

export { assertClientMetricName } from './telemetry/nameGuard.js';
export type { GuardMode } from './telemetry/nameGuard.js';

export { NoopMetricsSink } from './telemetry/NoopMetricsSink.js';
export { ConsoleMetricsSink } from './telemetry/ConsoleMetricsSink.js';
export { OtelMetricsSink } from './telemetry/OtelMetricsSink.js';

export {
  configureTelemetry,
  flushMetrics,
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
// Public vocabulary of `SignalingCapabilities.supportedCodecs`; root-exported to
// mirror `ParticipantLeaveReason` so an external consumer can name the type
// (const object carries both value and type). Without it the field ships with a
// nameless vocabulary.
export { SignalingCodec } from './signaling/events.js';
export type {
  JoinedEvent,
  MhConnectionStatusReport,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
  RosterParticipant,
  SignalingCapabilities,
  SignalingJoinParams,
} from './signaling/events.js';

// --- Shared typed event emitter (R-22 base for the media/session emitters) ---

export { TypedEventEmitter } from './events/TypedEventEmitter.js';
export type { EventListener } from './events/TypedEventEmitter.js';

// --- Media transport (R-20, R-21, R-58, R-60 SDK side) ---

export { MediaTransport } from './media/MediaTransport.js';
export { DEFAULT_MH_CONNECT_TIMEOUT_MS } from './media/events.js';
export type {
  MediaConnectionState,
  MediaTransportEventMap,
  MediaTransportOptions,
  RunInContext,
} from './media/events.js';

// --- MeetingSession facade (R-22) — the public SDK entry point ---

export { MeetingSession } from './session/MeetingSession.js';
export { MeetingSessionState } from './session/events.js';
export type {
  JoinCredentials,
  JoinOptions,
  LoginCredentials,
  MeetingSessionEventMap,
  MeetingSessionOptions,
  RegisterCredentials,
  TokenCredentials,
} from './session/events.js';

// --- ADR-0036 §2/§3/§4 media frame v2 + SFrame stack (story task 15) ---
//
// Three separable modules, so the vendored external sframe-wg vectors reach the
// exact code they validate: the RFC 9605 key schedule, the AEAD/SFrame object,
// and the framing that owns our deviations from the RFC.
//
// NOTE the deliberate omissions. `TransmitKeyCache.set` is reachable only through
// `openVerifiedFrame`, and there is no cache-priming or `unwrapAndCache` export:
// ADR-0036 §4 requires that a wrap from a frame that fails verification is never
// cached, and a bypass that exists only for tests is still a bypass.

export {
  decodeFrame,
  buildUnsignedFrame,
  finishFrame,
  PROTOCOL_VERSION,
} from './media/frame/frameCodec.js';
export type {
  DecodedFrame,
  EncodeFrameInput,
  FrameExtension,
  FrameFlags,
  UnsignedFrame,
  WrappedTransmitKey,
} from './media/frame/frameCodec.js';

export {
  MAX_PAYLOAD_BYTES,
  CIPHER_SUITE_ID,
  HEADER_VERSION,
  WIRE_CONSTANTS,
} from './media/frame/wireConstants.js';

export { packKeyId, unpackKeyId, KeyIdRangeError } from './media/frame/keyId.js';
export type { KeyIdParts } from './media/frame/keyId.js';

export { deriveSframeKeys, sframeNonce } from './media/frame/sframeKeySchedule.js';
export {
  sealSframe,
  openSframe,
  parseSframe,
  serializeSframe,
  unwrapTransmitKey,
  wrapNonce,
  AES_256_KEY_BYTES,
} from './media/frame/sframe.js';
export type { SframeObject } from './media/frame/sframe.js';

export { assertEd25519Available, signFrame, verifyFrameSignature } from './media/frame/ed25519.js';

export {
  ReplayWindow,
  TransmitKeyCache,
  openVerifiedFrame,
  verifyFrame,
} from './media/frame/receivePath.js';
export type {
  OpenedFrame,
  ReceiverKeys,
  VerifiedFrame,
  WrapOutcome,
} from './media/frame/receivePath.js';

export { FrameRejectedError, ALL_REJECT_REASONS } from './media/frame/rejectReason.js';
export type { RejectReason, RejectLayer, RejectDetail } from './media/frame/rejectReason.js';

// --- ADR-0036 §1/§4/§5/§6/§11 single-client audio media pipeline (story task 19) ---
//
// The public surface is the SEAMS plus the lifecycle object. The hot path
// (`media/pipeline/**`) is deliberately NOT exported: an embedder that could
// construct an ingress or egress pipeline directly could bypass the
// `VerifiedFrame` brand's ordering guarantee, and a door that exists only for
// convenience is still a door.

export {
  DEFAULT_CLIENT_CONFIG,
  DEFAULT_METRIC_EXPORT_INTERVAL_MS,
  MIN_AUDIO_ROTATION_PERIOD_MS,
  ClientConfigError,
  validateMediaConfig,
} from './config/clientConfig.js';
export type {
  AudioConfig,
  ClientConfig,
  EgressConfig,
  IngressConfig,
  KeyRotationConfig,
  MediaConfig,
  OpusApplication,
  OpusSignal,
  ReceiverStateConfig,
  TelemetryCadenceConfig,
} from './config/clientConfig.js';

export { AudioPipeline, MediaFaultStage } from './media/lifecycle/AudioPipeline.js';
export type {
  AudioPipelineEventMap,
  AudioPipelineOptions,
  AudioPipelineStartOptions,
  MediaFrameCounts,
  AudioSendDirective,
  MediaFault,
} from './media/lifecycle/AudioPipeline.js';
export { MuteState } from './media/lifecycle/muteState.js';
export type { MuteSnapshot } from './media/lifecycle/muteState.js';

export {
  MEDIA_KEK_SOURCES,
  MEDIA_MUTE_ACTIONS,
  MEDIA_SEND_DROP_REASONS,
  MediaMetrics,
  mediaMetricLabels,
} from './media/setup/mediaMetrics.js';
export type {
  MediaKekSource,
  MediaMetricIdentity,
  MediaMuteAction,
  MediaSendDropReason,
  ReportableWrapOutcome,
} from './media/setup/mediaMetrics.js';

export { JoinResponseKekSource } from './media/setup/kekSource.js';
export type { MeetingKekSource } from './media/setup/kekSource.js';
export { RosterIdentityKeys } from './media/setup/rosterKeys.js';
export type { RosterIdentityEntry } from './media/setup/rosterKeys.js';
export { CaptureFailure, MediaCaptureError, listMicrophones } from './media/setup/capture.js';
export { AudioCodecUnsupportedError } from './media/setup/opus.js';
export { MediaPlaybackError, PlaybackFailure } from './media/setup/audioPlayback.js';
export type {
  AudioDecoderFactory,
  AudioDecoderSeam,
  AudioEncoderFactory,
  AudioEncoderSeam,
  CaptureSource,
  CaptureSourceFactory,
  EncodedAudioFrame,
  PlaybackSink,
  PlaybackSinkFactory,
} from './media/setup/seams.js';

// `generateIdentityKeyPair` and `IdentityKeyPair` are deliberately NOT exported.
// `MeetingIdentity` is the seam; the factory behind it is not public surface.
// A public factory returning a bare keypair record would let an embedder hold one
// outside `MeetingIdentity`, outside `clear()`, outside the per-meeting
// never-persisted property, and outside `__tests__/serverMessageSinkScan.test.ts`
// — which scans `packages/sdk-core/src` and cannot see consumer code. That
// re-creates in application code exactly the retained-bare-record shape this SDK
// removed from itself, in the one place no guard of ours runs. Same argument as
// the hot path above: a door that exists only for convenience is still a door.
export { MeetingIdentity } from './media/setup/identity.js';
export { sframeObjectLength } from './media/frame/sframe.js';
export { buildPublisherRegion, writeHopSequence } from './media/frame/frameCodec.js';
export type { PublisherRegionInput } from './media/frame/frameCodec.js';

export { DEFAULT_AUDIO_SLOT_ID } from './session/events.js';
export type { StartMediaOptions } from './session/events.js';
export type {
  ReceiveSlotDeclaration,
  SendDirectiveEvent,
  SendStreamDirective,
  StreamAssignmentEvent,
  StreamAssignmentsEvent,
} from './signaling/events.js';
