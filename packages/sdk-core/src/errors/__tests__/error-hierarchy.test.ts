// File: packages/sdk-core/src/errors/__tests__/error-hierarchy.test.ts
//
// R-23: redaction + forward-compat coverage for the typed error hierarchy.
//   - toJSON()/JSON.stringify of representative errors emit ONLY the non-secret
//     allowlist and contain NO sentinel secret values.
//   - AuthError.fromResponse maps the 400 and 409 statuses to the typed subtypes.
//     AC cannot emit these today (it has no 400/409 variant — see AuthError.ts), so
//     these synthetic-envelope tests are the only coverage of those forward-compat
//     branches and are required for the >=90% target.

import { describe, expect, it } from 'vitest';
import {
  AuthBadRequestError,
  AuthConflictError,
  AuthError,
} from '../AuthError.js';
import { MeetingNotFoundError } from '../MeetingError.js';
import { NetworkError, NetworkErrorReason } from '../NetworkError.js';

const SENTINEL_JWT = 'SECRET.jwt.value.must.not.leak';
const SENTINEL_PASSWORD = 'super-secret-password';

describe('AuthError.fromResponse forward-compat branches', () => {
  it('maps a 400 envelope to AuthBadRequestError', () => {
    const err = AuthError.fromResponse(400, {
      error: { code: 'BAD_REQUEST', message: 'bad' },
    });
    expect(err).toBeInstanceOf(AuthBadRequestError);
    expect(err.status).toBe(400);
    expect(err.serverCode).toBe('BAD_REQUEST');
  });

  it('maps a 409 envelope to AuthConflictError', () => {
    const err = AuthError.fromResponse(409, {
      error: { code: 'CONFLICT', message: 'exists' },
    });
    expect(err).toBeInstanceOf(AuthConflictError);
    expect(err.status).toBe(409);
    expect(err.serverCode).toBe('CONFLICT');
  });

  it('falls back to the generic AuthError for an unmapped status', () => {
    const err = AuthError.fromResponse(500, undefined);
    expect(err).toBeInstanceOf(AuthError);
    expect(err.name).toBe('AuthError');
    expect(err.status).toBe(500);
  });
});

describe('toJSON() redaction (R-23)', () => {
  it('emits only the non-secret allowlist and never a token/password', () => {
    // Build errors as the clients would — from (status, envelope). Even if the
    // originating request carried secrets, they are never on the instance.
    const authErr = AuthError.fromResponse(401, {
      error: { code: 'INVALID_CREDENTIALS', message: 'Invalid client credentials' },
    });
    const meetErr = new MeetingNotFoundError('Meeting not found', 'NOT_FOUND');

    for (const err of [authErr, meetErr]) {
      const json = err.toJSON();
      // Exactly the allowlisted keys, no more.
      expect(new Set(Object.keys(json))).toEqual(
        new Set(['name', 'code', 'message', 'status', 'serverCode']),
      );
      const serialized = JSON.stringify(err);
      expect(serialized).not.toContain(SENTINEL_JWT);
      expect(serialized).not.toContain(SENTINEL_PASSWORD);
      expect(serialized).not.toContain('Bearer ');
    }

    // Concrete subclass name is reported, not the base name.
    expect(meetErr.toJSON().name).toBe('MeetingNotFoundError');
  });

  it('NetworkError uses a STATIC message and never leaks the raw cause', () => {
    // The raw cause carries a sentinel; it must stay off the serialized surface.
    const err = new NetworkError(NetworkErrorReason.MalformedResponse, {
      detail: SENTINEL_JWT,
    });
    expect(err.message).toBe('Response was not valid JSON');
    const serialized = JSON.stringify(err);
    expect(serialized).not.toContain(SENTINEL_JWT);
    // cause is preserved for local debugging but is non-enumerable.
    expect((err as { cause?: unknown }).cause).toEqual({ detail: SENTINEL_JWT });
  });
});
