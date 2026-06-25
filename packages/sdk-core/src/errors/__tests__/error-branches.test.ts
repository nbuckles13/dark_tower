// File: packages/sdk-core/src/errors/__tests__/error-branches.test.ts
//
// R-41/R-42: branch-coverage closure for the error hierarchy + envelope parsing —
// the cheap, status-driven branches that the MSW client tests don't all exercise
// (e.g. MeetingError 400/409, AuthForbidden 403, the default-status fallthroughs,
// the tolerant `readServerErrorEnvelope` paths, and the no-cause NetworkError).

import { describe, expect, it } from 'vitest';
import { AuthError, AuthBadRequestError, AuthForbiddenError } from '../AuthError.js';
import { MeetingError, MeetingBadRequestError, MeetingConflictError } from '../MeetingError.js';
import { NetworkError, NetworkErrorReason } from '../NetworkError.js';
import { ValidationError } from '../ValidationError.js';
import { readServerErrorEnvelope } from '../SdkError.js';

describe('MeetingError.fromResponse status mapping', () => {
  it('maps 400 to MeetingBadRequestError', () => {
    const err = MeetingError.fromResponse(400, {
      error: { code: 'BAD_REQUEST', message: 'bad' },
    });
    expect(err).toBeInstanceOf(MeetingBadRequestError);
    expect(err.status).toBe(400);
    expect(err.serverCode).toBe('BAD_REQUEST');
  });

  it('maps 409 to MeetingConflictError', () => {
    const err = MeetingError.fromResponse(409, {
      error: { code: 'CONFLICT', message: 'exists' },
    });
    expect(err).toBeInstanceOf(MeetingConflictError);
    expect(err.status).toBe(409);
  });

  it('falls back to the generic MeetingError for an unmapped status', () => {
    const err = MeetingError.fromResponse(503, undefined);
    expect(err).toBeInstanceOf(MeetingError);
    expect(err.name).toBe('MeetingError');
    expect(err.status).toBe(503);
    // No envelope -> generic message, no serverCode.
    expect(err.serverCode).toBeUndefined();
  });
});

describe('AuthError.fromResponse 403 + generic message', () => {
  it('maps 403 to AuthForbiddenError', () => {
    const err = AuthError.fromResponse(403, {
      error: { code: 'INSUFFICIENT_SCOPE', message: 'Requires scope: x' },
    });
    expect(err).toBeInstanceOf(AuthForbiddenError);
    expect(err.status).toBe(403);
  });

  it('uses a generic message when the envelope has no message', () => {
    const err = AuthError.fromResponse(400, { error: { code: 'X' } });
    expect(err).toBeInstanceOf(AuthBadRequestError);
    expect(err.message).toContain('HTTP 400');
    expect(err.serverCode).toBe('X');
  });
});

describe('readServerErrorEnvelope tolerance', () => {
  it('returns empty for a non-object / missing-error body', () => {
    expect(readServerErrorEnvelope(undefined)).toEqual({});
    expect(readServerErrorEnvelope('not-json')).toEqual({});
    expect(readServerErrorEnvelope({ notError: 1 })).toEqual({});
    // `error` present but not an object.
    expect(readServerErrorEnvelope({ error: 'oops' })).toEqual({});
    // Partial: code present, message wrong type.
    expect(readServerErrorEnvelope({ error: { code: 'C', message: 5 } })).toEqual({ code: 'C' });
  });
});

describe('NetworkError no-cause path', () => {
  it('constructs without a cause and still uses the static message', () => {
    const err = new NetworkError(NetworkErrorReason.MalformedResponse);
    expect(err.message).toBe('Response was not valid JSON');
    expect((err as { cause?: unknown }).cause).toBeUndefined();
  });
});

describe('ValidationError shape (domain-neutral, code-reviewer F1)', () => {
  it('carries code VALIDATION + field + status 400, no serverCode, and is NOT an Auth/Meeting error', () => {
    const err = new ValidationError('meetingCode', 'Invalid meeting code');
    expect(err.code).toBe('VALIDATION');
    expect(err.field).toBe('meetingCode');
    expect(err.status).toBe(400);
    expect(err.name).toBe('ValidationError');
    // Domain-neutral: a meeting/auth `instanceof` filter must NOT catch it.
    expect(err).not.toBeInstanceOf(AuthError);
    expect(err).not.toBeInstanceOf(MeetingError);
    // toJSON allowlist: status present, serverCode omitted (nothing came from a server).
    const json = err.toJSON();
    expect(new Set(Object.keys(json))).toEqual(new Set(['name', 'code', 'message', 'status']));
  });
});
