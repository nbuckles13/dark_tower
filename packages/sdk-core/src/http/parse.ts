// File: packages/sdk-core/src/http/parse.ts
//
// Shared HTTP response handling for the AC/GC clients. Centralizes: (a) executing a
// `fetch` and turning a fetch-reject into a typed `NetworkError(FETCH_FAILED)`,
// (b) parsing a JSON body (non-JSON -> `NetworkError(MALFORMED_RESPONSE)`), (c) on
// `!response.ok`, reading the `{error:{code,message}}` envelope and handing
// `(status, body)` to a caller-supplied `fromResponse` factory (AuthError / MeetingError).
//
// R-23: secret material is never threaded through here — error construction uses only
// `(status, parsed envelope)`, and `NetworkError` messages are STATIC.

import { NetworkError, NetworkErrorReason } from '../errors/NetworkError.js';
import type { SdkError } from '../errors/SdkError.js';
import type { FetchLike } from './types.js';

/** A factory mapping `(status, body)` to a typed SDK error (e.g. `AuthError.fromResponse`). */
export type ErrorFactory = (status: number, body: unknown, retryAfterSeconds?: number) => SdkError;

/** Run a `fetch`, converting a transport-level rejection into a typed `NetworkError`. */
export async function safeFetch(
  fetchImpl: FetchLike,
  input: string,
  init: RequestInit,
): Promise<Response> {
  try {
    return await fetchImpl(input, init);
  } catch (cause) {
    throw new NetworkError(NetworkErrorReason.FetchFailed, cause);
  }
}

/** Parse a JSON body, mapping a non-JSON/empty body to a typed `MALFORMED_RESPONSE`. */
async function parseJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (text.length === 0) {
    return undefined;
  }
  try {
    return JSON.parse(text);
  } catch (cause) {
    throw new NetworkError(NetworkErrorReason.MalformedResponse, cause);
  }
}

/**
 * Parse a `Retry-After` header as integer seconds. Returns `undefined` when absent
 * or non-numeric (HTTP-date form is not honored here — AC emits integer seconds).
 */
function parseRetryAfter(response: Response): number | undefined {
  const raw = response.headers.get('retry-after');
  if (raw === null) {
    return undefined;
  }
  const seconds = Number.parseInt(raw, 10);
  return Number.isFinite(seconds) ? seconds : undefined;
}

/**
 * Resolve a `Response` into a parsed `T` on success, or throw the appropriate typed
 * error on failure. `errorFactory` builds the domain error (AC/GC) from the status +
 * envelope; a missing/malformed success body maps to `MALFORMED_RESPONSE`.
 */
export async function handleResponse<T>(
  response: Response,
  errorFactory: ErrorFactory,
): Promise<T> {
  if (!response.ok) {
    // Best-effort envelope parse; never let a malformed error body mask the status.
    let body: unknown;
    try {
      body = await parseJson(response);
    } catch {
      body = undefined;
    }
    throw errorFactory(response.status, body, parseRetryAfter(response));
  }

  const body = await parseJson(response);
  if (body === undefined) {
    throw new NetworkError(NetworkErrorReason.MalformedResponse);
  }
  return body as T;
}
