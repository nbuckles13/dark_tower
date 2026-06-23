// File: packages/sdk-core/src/http/__tests__/auth-api-client.test.ts
//
// R-42: MSW unit tests for AuthApiClient against the AC wire contract. Asserts the
// request shape (subdomain origin, method, headers, JSON body) MSW actually receives
// matches what AC accepts (camelCase, task #51), and that responses + error statuses
// map to the typed `AuthError` hierarchy. Also covers R-31/R-11 client-side validation
// short-circuiting before any network call.

import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { AuthApiClient } from '../AuthApiClient.js';
import { AuthRateLimitError, AuthUnauthorizedError } from '../../errors/AuthError.js';
import { ValidationError } from '../../errors/ValidationError.js';

const AC_TEMPLATE = 'https://{subdomain}.localhost:8443';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

function client(): AuthApiClient {
  return new AuthApiClient({ acOriginTemplate: AC_TEMPLATE });
}

describe('AuthApiClient.register', () => {
  it('posts camelCase body to the subdomain-qualified AC origin and parses the response', async () => {
    let seenUrl = '';
    let seenContentType: string | null = null;
    let seenBody: unknown;
    server.use(
      http.post('https://demo.localhost:8443/api/v1/auth/register', async ({ request }) => {
        seenUrl = request.url;
        seenContentType = request.headers.get('content-type');
        seenBody = await request.json();
        return HttpResponse.json({
          userId: '11111111-1111-1111-1111-111111111111',
          email: 'alice@example.com',
          displayName: 'Alice',
          accessToken: 'header.body.sig',
          tokenType: 'Bearer',
          expiresIn: 3600,
        });
      }),
    );

    const res = await client().register({
      subdomain: 'demo',
      email: 'alice@example.com',
      password: 'hunter2hunter2',
      displayName: 'Alice',
    });

    expect(seenUrl).toBe('https://demo.localhost:8443/api/v1/auth/register');
    expect(seenContentType).toBe('application/json');
    expect(seenBody).toEqual({
      email: 'alice@example.com',
      password: 'hunter2hunter2',
      displayName: 'Alice',
    });
    expect(res.userId).toBe('11111111-1111-1111-1111-111111111111');
    expect(res.accessToken).toBe('header.body.sig');
    expect(res.tokenType).toBe('Bearer');
    expect(res.expiresIn).toBe(3600);
  });

  it('maps 429 to AuthRateLimitError and captures Retry-After', async () => {
    server.use(
      http.post('https://demo.localhost:8443/api/v1/auth/register', () =>
        HttpResponse.json(
          { error: { code: 'RATE_LIMIT_EXCEEDED', message: 'Too many requests.' } },
          { status: 429, headers: { 'retry-after': '42' } },
        ),
      ),
    );

    const err = await client()
      .register({ subdomain: 'demo', email: 'a@b.co', password: 'pw-long-enough', displayName: 'A' })
      .catch((e: unknown) => e);

    expect(err).toBeInstanceOf(AuthRateLimitError);
    const rate = err as AuthRateLimitError;
    expect(rate.status).toBe(429);
    expect(rate.serverCode).toBe('RATE_LIMIT_EXCEEDED');
    expect(rate.retryAfterSeconds).toBe(42);
  });
});

describe('AuthApiClient.login', () => {
  it('posts {email,password} to /user/token and parses the token response', async () => {
    let seenUrl = '';
    let seenBody: unknown;
    server.use(
      http.post('https://acme.localhost:8443/api/v1/auth/user/token', async ({ request }) => {
        seenUrl = request.url;
        seenBody = await request.json();
        return HttpResponse.json({
          accessToken: 'a.b.c',
          tokenType: 'Bearer',
          expiresIn: 3600,
        });
      }),
    );

    const res = await client().login({
      subdomain: 'acme',
      email: 'bob@example.com',
      password: 'correct-horse',
    });

    expect(seenUrl).toBe('https://acme.localhost:8443/api/v1/auth/user/token');
    expect(seenBody).toEqual({ email: 'bob@example.com', password: 'correct-horse' });
    expect(res.accessToken).toBe('a.b.c');
  });

  it('maps 401 INVALID_CREDENTIALS to AuthUnauthorizedError and preserves serverCode', async () => {
    server.use(
      http.post('https://demo.localhost:8443/api/v1/auth/user/token', () =>
        HttpResponse.json(
          { error: { code: 'INVALID_CREDENTIALS', message: 'Invalid client credentials' } },
          { status: 401 },
        ),
      ),
    );

    const err = await client()
      .login({ subdomain: 'demo', email: 'x@y.co', password: 'whatever-pw' })
      .catch((e: unknown) => e);

    expect(err).toBeInstanceOf(AuthUnauthorizedError);
    expect((err as AuthUnauthorizedError).status).toBe(401);
    expect((err as AuthUnauthorizedError).serverCode).toBe('INVALID_CREDENTIALS');
  });
});

describe('AuthApiClient client-side validation (R-11 / R-31)', () => {
  it('rejects an invalid subdomain before any network call (injection prevention)', async () => {
    // onUnhandledRequest: 'error' means any fetch would fail the test; assert none fires.
    await expect(
      client().register({
        subdomain: 'Bad_Sub!',
        email: 'a@b.co',
        password: 'pw-long-enough',
        displayName: 'A',
      }),
    ).rejects.toBeInstanceOf(ValidationError);
  });

  it('rejects over-long inputs before any network call', async () => {
    const c = client();
    await expect(
      c.register({
        subdomain: 'demo',
        email: 'a@b.co',
        password: 'pw',
        displayName: 'x'.repeat(65),
      }),
    ).rejects.toBeInstanceOf(ValidationError);
    await expect(
      c.login({ subdomain: 'demo', email: 'e'.repeat(255), password: 'pw' }),
    ).rejects.toBeInstanceOf(ValidationError);
    await expect(
      c.login({ subdomain: 'demo', email: 'a@b.co', password: 'p'.repeat(129) }),
    ).rejects.toBeInstanceOf(ValidationError);
  });
});
