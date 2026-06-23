// File: packages/sdk-core/src/http/__tests__/http-branches.test.ts
//
// R-41/R-42: branch-coverage closure for the HTTP layer that the happy/error MSW
// tests don't all hit — fetch rejection -> NetworkError(FETCH_FAILED), malformed /
// empty success bodies -> NetworkError(MALFORMED_RESPONSE), the AC origin
// missing-`{subdomain}` misconfiguration throw, createMeeting optional-field body
// assembly, and the empty-input validation throws. Uses an injected `fetchImpl`
// (no network) rather than MSW where a transport-level outcome is being forced.

import { describe, expect, it } from 'vitest';
import { AuthApiClient } from '../AuthApiClient.js';
import { MeetingApiClient } from '../MeetingApiClient.js';
import { resolveAcOrigin } from '../origin.js';
import { ValidationError } from '../../errors/ValidationError.js';
import { NetworkError, NetworkErrorReason } from '../../errors/NetworkError.js';
import type { FetchLike } from '../types.js';

const AC_TEMPLATE = 'https://{subdomain}.localhost:8443';
const GC_BASE = 'https://localhost:8444';
const VALID_CODE = 'Abc123Def456';

describe('safeFetch: fetch rejection -> NetworkError(FETCH_FAILED)', () => {
  it('wraps a rejecting fetch into a typed NetworkError without leaking the cause', async () => {
    const rejectingFetch: FetchLike = () => Promise.reject(new Error('SECRET-dns-detail'));
    const client = new MeetingApiClient({ gcBaseUrl: GC_BASE, fetchImpl: rejectingFetch });

    const err = await client
      .joinMeeting(VALID_CODE, { userToken: 'tok' })
      .catch((e: unknown) => e);

    expect(err).toBeInstanceOf(NetworkError);
    expect((err as NetworkError).reason).toBe(NetworkErrorReason.FetchFailed);
    expect(JSON.stringify(err)).not.toContain('SECRET-dns-detail');
  });
});

describe('handleResponse: malformed / empty success bodies', () => {
  function fetchReturning(body: string, status = 200): FetchLike {
    return () =>
      Promise.resolve(
        new Response(body, { status, headers: { 'content-type': 'application/json' } }),
      );
  }

  it('maps a non-JSON success body to NetworkError(MALFORMED_RESPONSE)', async () => {
    const client = new AuthApiClient({
      acOriginTemplate: AC_TEMPLATE,
      fetchImpl: fetchReturning('<<not json>>'),
    });
    const err = await client
      .login({ subdomain: 'demo', email: 'a@b.co', password: 'pw-ok-here' })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(NetworkError);
    expect((err as NetworkError).reason).toBe(NetworkErrorReason.MalformedResponse);
  });

  it('maps an empty success body to NetworkError(MALFORMED_RESPONSE)', async () => {
    const client = new AuthApiClient({
      acOriginTemplate: AC_TEMPLATE,
      fetchImpl: fetchReturning(''),
    });
    const err = await client
      .login({ subdomain: 'demo', email: 'a@b.co', password: 'pw-ok-here' })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(NetworkError);
    expect((err as NetworkError).reason).toBe(NetworkErrorReason.MalformedResponse);
  });

  it('tolerates a malformed ERROR body, still throwing the status-mapped error', async () => {
    const client = new MeetingApiClient({
      gcBaseUrl: GC_BASE,
      fetchImpl: () => Promise.resolve(new Response('<<garbage>>', { status: 404 })),
    });
    const err = await client
      .joinMeeting(VALID_CODE, { userToken: 'tok' })
      .catch((e: unknown) => e);
    // 404 -> MeetingNotFoundError even though the body wasn't parseable.
    expect((err as { status?: number }).status).toBe(404);
    expect((err as { name: string }).name).toBe('MeetingNotFoundError');
  });
});

describe('resolveAcOrigin misconfiguration', () => {
  it('throws when the template lacks the {subdomain} placeholder', () => {
    expect(() => resolveAcOrigin('https://localhost:8443', 'demo')).toThrow(
      /\{subdomain\} placeholder/,
    );
  });

  it('throws ValidationError for an invalid subdomain before substitution', () => {
    let thrown: unknown;
    try {
      resolveAcOrigin(AC_TEMPLATE, 'BAD!');
    } catch (e) {
      thrown = e;
    }
    expect(thrown).toBeInstanceOf(ValidationError);
    // Domain-neutral discriminant: code VALIDATION, field tagged, status 400, no serverCode.
    expect((thrown as ValidationError).code).toBe('VALIDATION');
    expect((thrown as ValidationError).field).toBe('subdomain');
    expect((thrown as ValidationError).status).toBe(400);
    expect((thrown as ValidationError).serverCode).toBeUndefined();
  });
});

describe('createMeeting optional-field body assembly', () => {
  it('includes every defined optional and omits undefined ones', async () => {
    let seenBody: unknown;
    const captureFetch: FetchLike = async (_url, init) => {
      seenBody = JSON.parse(String(init?.body));
      return new Response(
        JSON.stringify({
          meetingId: '1',
          meetingCode: VALID_CODE,
          displayName: 'Full',
          status: 'scheduled',
          maxParticipants: 5,
          enableE2eEncryption: false,
          requireAuth: false,
          recordingEnabled: true,
          allowGuests: true,
          allowExternalParticipants: true,
          waitingRoomEnabled: false,
          createdAt: '2026-06-23T00:00:00Z',
        }),
        { status: 201 },
      );
    };
    const client = new MeetingApiClient({ gcBaseUrl: GC_BASE, fetchImpl: captureFetch });

    await client.createMeeting(
      {
        displayName: 'Full',
        maxParticipants: 5,
        scheduledStartTime: '2026-06-24T10:00:00Z',
        enableE2eEncryption: false,
        requireAuth: false,
        recordingEnabled: true,
        allowGuests: true,
        allowExternalParticipants: true,
        waitingRoomEnabled: false,
      },
      { userToken: 'tok' },
    );

    expect(seenBody).toEqual({
      displayName: 'Full',
      maxParticipants: 5,
      scheduledStartTime: '2026-06-24T10:00:00Z',
      enableE2eEncryption: false,
      requireAuth: false,
      recordingEnabled: true,
      allowGuests: true,
      allowExternalParticipants: true,
      waitingRoomEnabled: false,
    });
  });
});

describe('empty-input validation throws (R-31)', () => {
  const client = new AuthApiClient({ acOriginTemplate: AC_TEMPLATE });

  it('rejects empty email, empty password, and empty display name pre-network', async () => {
    await expect(
      client.login({ subdomain: 'demo', email: '', password: 'pw' }),
    ).rejects.toBeInstanceOf(ValidationError);
    await expect(
      client.login({ subdomain: 'demo', email: 'a@b.co', password: '' }),
    ).rejects.toBeInstanceOf(ValidationError);
    await expect(
      client.register({ subdomain: 'demo', email: 'a@b.co', password: 'pw', displayName: '   ' }),
    ).rejects.toBeInstanceOf(ValidationError);
  });
});
