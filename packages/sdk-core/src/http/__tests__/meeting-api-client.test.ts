// File: packages/sdk-core/src/http/__tests__/meeting-api-client.test.ts
//
// R-42: MSW unit tests for MeetingApiClient against the GC wire contract. Asserts the
// bearer header, URL, and camelCase request/response shapes match GC
// (crates/gc-service/src/models/mod.rs), and that error statuses map to typed
// `MeetingError` subtypes. Covers R-31 meeting-code validation short-circuit.

import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { MeetingApiClient } from '../MeetingApiClient.js';
import { ValidationError } from '../../errors/ValidationError.js';
import {
  MeetingForbiddenError,
  MeetingNotFoundError,
  MeetingUnauthorizedError,
} from '../../errors/MeetingError.js';

const GC_BASE = 'https://localhost:8444';
const VALID_CODE = 'Abc123Def456'; // 12-char base62

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

function client(): MeetingApiClient {
  return new MeetingApiClient({ gcBaseUrl: GC_BASE });
}

describe('MeetingApiClient.joinMeeting', () => {
  it('GETs the coded meeting URL with a bearer header and parses the join response', async () => {
    let seenAuth: string | null = null;
    server.use(
      http.get(`${GC_BASE}/api/v1/meetings/${VALID_CODE}`, ({ request }) => {
        seenAuth = request.headers.get('authorization');
        return HttpResponse.json({
          token: 'meeting.jwt.sig',
          expiresIn: 900,
          meetingId: '22222222-2222-2222-2222-222222222222',
          meetingName: 'Standup',
          mcAssignment: {
            mcId: 'mc-001',
            webtransportEndpoint: 'https://mc:443',
            grpcEndpoint: 'https://mc:50051',
          },
        });
      }),
    );

    const res = await client().joinMeeting(VALID_CODE, { userToken: 'user.jwt.sig' });

    expect(seenAuth).toBe('Bearer user.jwt.sig');
    expect(res.token).toBe('meeting.jwt.sig');
    expect(res.expiresIn).toBe(900);
    expect(res.mcAssignment.mcId).toBe('mc-001');
    expect(res.mcAssignment.webtransportEndpoint).toBe('https://mc:443');
    expect(res.mcAssignment.grpcEndpoint).toBe('https://mc:50051');
  });

  it('handles a join response with webtransportEndpoint absent (skip-if-none)', async () => {
    server.use(
      http.get(`${GC_BASE}/api/v1/meetings/${VALID_CODE}`, () =>
        HttpResponse.json({
          token: 't',
          expiresIn: 900,
          meetingId: '22222222-2222-2222-2222-222222222222',
          meetingName: 'Standup',
          mcAssignment: { mcId: 'mc-001', grpcEndpoint: 'https://mc:50051' },
        }),
      ),
    );

    const res = await client().joinMeeting(VALID_CODE, { userToken: 't' });
    expect(res.mcAssignment.webtransportEndpoint).toBeUndefined();
    expect(res.mcAssignment.grpcEndpoint).toBe('https://mc:50051');
  });

  it.each([
    [401, 'INVALID_TOKEN', MeetingUnauthorizedError],
    [403, 'FORBIDDEN', MeetingForbiddenError],
    [404, 'NOT_FOUND', MeetingNotFoundError],
  ])('maps %i to the typed MeetingError subtype', async (status, code, ctor) => {
    server.use(
      http.get(`${GC_BASE}/api/v1/meetings/${VALID_CODE}`, () =>
        HttpResponse.json({ error: { code, message: 'denied' } }, { status }),
      ),
    );
    const err = await client()
      .joinMeeting(VALID_CODE, { userToken: 't' })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ctor);
    expect((err as MeetingUnauthorizedError).status).toBe(status);
    expect((err as MeetingUnauthorizedError).serverCode).toBe(code);
  });

  it('rejects a malformed meeting code before any network call (R-31)', async () => {
    await expect(
      client().joinMeeting('short', { userToken: 't' }),
    ).rejects.toBeInstanceOf(ValidationError);
  });
});

describe('MeetingApiClient.createMeeting', () => {
  it('POSTs a camelCase body with a bearer header and parses the 201 response', async () => {
    let seenAuth: string | null = null;
    let seenBody: unknown;
    server.use(
      http.post(`${GC_BASE}/api/v1/meetings`, async ({ request }) => {
        seenAuth = request.headers.get('authorization');
        seenBody = await request.json();
        return HttpResponse.json(
          {
            meetingId: '33333333-3333-3333-3333-333333333333',
            meetingCode: VALID_CODE,
            displayName: 'Planning',
            status: 'scheduled',
            maxParticipants: 100,
            enableE2eEncryption: true,
            requireAuth: true,
            recordingEnabled: false,
            allowGuests: false,
            allowExternalParticipants: false,
            waitingRoomEnabled: true,
            createdAt: '2026-06-23T00:00:00Z',
          },
          { status: 201 },
        );
      }),
    );

    const res = await client().createMeeting(
      { displayName: 'Planning', scheduledStartTime: '2026-06-24T10:00:00Z' },
      { userToken: 'user.jwt.sig' },
    );

    expect(seenAuth).toBe('Bearer user.jwt.sig');
    expect(seenBody).toEqual({
      displayName: 'Planning',
      scheduledStartTime: '2026-06-24T10:00:00Z',
    });
    expect(res.meetingCode).toBe(VALID_CODE);
    expect(res.status).toBe('scheduled');
  });
});
