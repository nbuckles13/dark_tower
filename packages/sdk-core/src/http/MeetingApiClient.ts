// File: packages/sdk-core/src/http/MeetingApiClient.ts
//
// R-12: browser HTTP client for GC (global-controller) meeting endpoints. Exposes
// `joinMeeting` and `createMeeting` against the GC base URL, with the user JWT sent
// as `Authorization: Bearer <userToken>`. Responses use GC's camelCase wire shape
// (R-53). Failures map to typed `MeetingError` subtypes.
//
// R-23: the user token is sent only in the `Authorization` header — never in a URL,
// query string, log, or error. It is supplied per-call (token-source pattern), held
// in memory by the caller.
// R-31: the meeting code is regex-validated (12 base62) BEFORE the GET so a malformed
// code never reaches GC's DB lookup.

import { MeetingError } from '../errors/MeetingError.js';
import { validateDisplayName, validateMeetingCode } from '../validation/limits.js';
import { handleResponse, safeFetch } from './parse.js';
import type {
  CreateMeetingInput,
  CreateMeetingResponse,
  FetchLike,
  JoinMeetingResponse,
  UserTokenCredentials,
} from './types.js';

const MEETINGS_PATH = '/api/v1/meetings';

/** Construction options for {@link MeetingApiClient}. */
export interface MeetingApiClientOptions {
  /** GC base URL, e.g. `https://localhost:8444` (dev). Supplied by config. */
  readonly gcBaseUrl: string;
  /** `fetch` implementation; defaults to `globalThis.fetch`. Injected for tests. */
  readonly fetchImpl?: FetchLike;
}

/** Build the bearer auth header for a user token (R-23: header only, never a URL). */
function bearer(userToken: string): Record<string, string> {
  return { authorization: `Bearer ${userToken}` };
}

/** HTTP client for GC meeting join/create (R-12). */
export class MeetingApiClient {
  readonly #gcBaseUrl: string;
  readonly #fetchImpl: FetchLike;

  constructor(options: MeetingApiClientOptions) {
    this.#gcBaseUrl = options.gcBaseUrl;
    this.#fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
  }

  /**
   * Join a meeting by code, returning the meeting token + MC assignment.
   * `GET {gcBaseUrl}/api/v1/meetings/{code}` with `Authorization: Bearer <userToken>`.
   * @throws {MeetingError} on a mapped GC failure (401/403/404/...); the meeting code
   *   is validated client-side first ({@link ValidationError} on a bad code).
   */
  async joinMeeting(code: string, credentials: UserTokenCredentials): Promise<JoinMeetingResponse> {
    validateMeetingCode(code);
    const url = `${this.#gcBaseUrl}${MEETINGS_PATH}/${encodeURIComponent(code)}`;
    const response = await safeFetch(this.#fetchImpl, url, {
      method: 'GET',
      headers: { ...bearer(credentials.userToken) },
    });
    return handleResponse<JoinMeetingResponse>(response, MeetingError.fromResponse);
  }

  /**
   * Create a meeting in the authenticated user's org.
   * `POST {gcBaseUrl}/api/v1/meetings` with `Authorization: Bearer <userToken>` and a
   * camelCase body carrying only the defined fields.
   * @throws {MeetingError} on a mapped GC failure (400/401/403/...).
   */
  async createMeeting(
    input: CreateMeetingInput,
    credentials: UserTokenCredentials,
  ): Promise<CreateMeetingResponse> {
    validateDisplayName(input.displayName);
    const response = await safeFetch(this.#fetchImpl, `${this.#gcBaseUrl}${MEETINGS_PATH}`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...bearer(credentials.userToken),
      },
      // Send only defined fields; GC's CreateMeetingRequest uses deny_unknown_fields
      // and applies secure server-side defaults for omitted ones.
      body: JSON.stringify(buildCreateBody(input)),
    });
    return handleResponse<CreateMeetingResponse>(response, MeetingError.fromResponse);
  }
}

/** Construct the create-meeting request body, omitting `undefined` optional fields. */
function buildCreateBody(input: CreateMeetingInput): Record<string, unknown> {
  const body: Record<string, unknown> = { displayName: input.displayName };
  if (input.maxParticipants !== undefined) body['maxParticipants'] = input.maxParticipants;
  if (input.scheduledStartTime !== undefined) {
    body['scheduledStartTime'] = input.scheduledStartTime;
  }
  if (input.enableE2eEncryption !== undefined) {
    body['enableE2eEncryption'] = input.enableE2eEncryption;
  }
  if (input.requireAuth !== undefined) body['requireAuth'] = input.requireAuth;
  if (input.recordingEnabled !== undefined) body['recordingEnabled'] = input.recordingEnabled;
  if (input.allowGuests !== undefined) body['allowGuests'] = input.allowGuests;
  if (input.allowExternalParticipants !== undefined) {
    body['allowExternalParticipants'] = input.allowExternalParticipants;
  }
  if (input.waitingRoomEnabled !== undefined) {
    body['waitingRoomEnabled'] = input.waitingRoomEnabled;
  }
  return body;
}
