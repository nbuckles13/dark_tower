// File: packages/sdk-core/src/http/types.ts
//
// R-11/R-12: TS request/response shapes mirroring the EXACT camelCase wire contracts
// the Rust services accept/return (post task #51 single-rule camelCase). Confirmed
// against:
//   AC: crates/ac-service/src/handlers/auth_handler.rs (UserRegistrationRequest/
//       Response, UserTokenRequest), crates/ac-service/src/services/token_service.rs
//       (UserTokenResponse)
//   GC: crates/gc-service/src/models/mod.rs (JoinMeetingResponse, McAssignmentInfo,
//       CreateMeetingRequest, CreateMeetingResponse)
//
// R-23: tokens are typed as `string` here but are held in memory only and MUST NOT
// be placed on any error / `toJSON()` surface, logged, or put in URLs/query strings.

/** A function with the shape of the global `fetch`. Injected for testability (MSW). */
export type FetchLike = typeof fetch;

// ============================================================================
// AC — auth-controller
// ============================================================================

/** `AuthApiClient.register` input. `subdomain` selects the org (AC origin). */
export interface RegisterInput {
  readonly subdomain: string;
  readonly email: string;
  readonly password: string;
  readonly displayName: string;
}

/** `AuthApiClient.login` input. */
export interface LoginInput {
  readonly subdomain: string;
  readonly email: string;
  readonly password: string;
}

/**
 * AC token response. Login (`/user/token`) returns these three fields;
 * register (`/register`) returns these plus identity fields (see
 * {@link RegisterResponse}). Uniform camelCase (task #51).
 */
export interface AuthTokenResponse {
  readonly accessToken: string;
  readonly tokenType: string;
  readonly expiresIn: number;
}

/** AC `/register` response: identity + auto-login token (uniform camelCase). */
export interface RegisterResponse extends AuthTokenResponse {
  readonly userId: string;
  readonly email: string;
  readonly displayName: string;
}

// ============================================================================
// GC — global-controller
// ============================================================================

/** MC assignment block inside {@link JoinMeetingResponse}. */
export interface McAssignment {
  readonly mcId: string;
  /** Absent when GC has no WebTransport endpoint for the MC (skip-if-none on the wire). */
  readonly webtransportEndpoint?: string;
  readonly grpcEndpoint: string;
}

/** GC `GET /api/v1/meetings/:code` (join) response. */
export interface JoinMeetingResponse {
  readonly token: string;
  readonly expiresIn: number;
  readonly meetingId: string;
  readonly meetingName: string;
  readonly mcAssignment: McAssignment;
}

/**
 * GC `POST /api/v1/meetings` (create) input. `displayName` is required; all other
 * fields are optional and only sent when defined (GC applies secure server-side
 * defaults). Field names match `CreateMeetingRequest` (camelCase, deny_unknown_fields).
 */
export interface CreateMeetingInput {
  readonly displayName: string;
  readonly maxParticipants?: number;
  /** ISO-8601 timestamp; GC key is `scheduledStartTime`. */
  readonly scheduledStartTime?: string;
  readonly enableE2eEncryption?: boolean;
  readonly requireAuth?: boolean;
  readonly recordingEnabled?: boolean;
  readonly allowGuests?: boolean;
  readonly allowExternalParticipants?: boolean;
  readonly waitingRoomEnabled?: boolean;
}

/** GC `POST /api/v1/meetings` (create) 201 response. */
export interface CreateMeetingResponse {
  readonly meetingId: string;
  readonly meetingCode: string;
  readonly displayName: string;
  readonly status: string;
  readonly maxParticipants: number;
  readonly enableE2eEncryption: boolean;
  readonly requireAuth: boolean;
  readonly recordingEnabled: boolean;
  readonly allowGuests: boolean;
  readonly allowExternalParticipants: boolean;
  readonly waitingRoomEnabled: boolean;
  readonly createdAt: string;
}

/** Credentials carrier for GC calls. The token is held in memory only (R-23). */
export interface UserTokenCredentials {
  readonly userToken: string;
}
