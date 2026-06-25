// File: packages/sdk-core/src/http/AuthApiClient.ts
//
// R-11: browser HTTP client for AC (auth-controller). Exposes `register` and `login`
// against the subdomain-qualified AC origin, using the uniform camelCase wire
// contract (task #51). Standard `fetch` is used (injectable for tests). Inputs are
// bounded-validated and the subdomain is regex-validated BEFORE URL interpolation
// (R-31 / R-11). Failures map to typed `AuthError` subtypes via `AuthError.fromResponse`.
//
// R-23: tokens returned here are held in memory by the caller only; this client never
// logs them, stores them, or places them on an error.

import { AuthError } from '../errors/AuthError.js';
import { validateDisplayName, validateEmail, validatePassword } from '../validation/limits.js';
import { resolveAcOrigin } from './origin.js';
import { handleResponse, safeFetch } from './parse.js';
import type {
  AuthTokenResponse,
  FetchLike,
  LoginInput,
  RegisterInput,
  RegisterResponse,
} from './types.js';

const REGISTER_PATH = '/api/v1/auth/register';
const LOGIN_PATH = '/api/v1/auth/user/token';

const JSON_HEADERS: Readonly<Record<string, string>> = {
  'content-type': 'application/json',
};

/** Construction options for {@link AuthApiClient}. */
export interface AuthApiClientOptions {
  /**
   * AC origin template containing a `{subdomain}` placeholder, e.g.
   * `https://{subdomain}.localhost:8443` (dev). Supplied by config — never hardcoded.
   */
  readonly acOriginTemplate: string;
  /** `fetch` implementation; defaults to `globalThis.fetch`. Injected for tests. */
  readonly fetchImpl?: FetchLike;
}

/** HTTP client for AC register/login (R-11). */
export class AuthApiClient {
  readonly #acOriginTemplate: string;
  readonly #fetchImpl: FetchLike;

  constructor(options: AuthApiClientOptions) {
    this.#acOriginTemplate = options.acOriginTemplate;
    this.#fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);
  }

  /**
   * Register a new user, returning identity + an auto-login token.
   * `POST {acOrigin}/api/v1/auth/register` with body `{email, password, displayName}`.
   * @throws {AuthError} on a mapped AC failure; {@link ValidationError} for
   *   client-side validation failures (thrown before any network call).
   */
  async register(input: RegisterInput): Promise<RegisterResponse> {
    validateEmail(input.email);
    validatePassword(input.password);
    validateDisplayName(input.displayName);
    const origin = resolveAcOrigin(this.#acOriginTemplate, input.subdomain);

    const response = await safeFetch(this.#fetchImpl, `${origin}${REGISTER_PATH}`, {
      method: 'POST',
      headers: { ...JSON_HEADERS },
      body: JSON.stringify({
        email: input.email,
        password: input.password,
        displayName: input.displayName,
      }),
    });
    return handleResponse<RegisterResponse>(response, AuthError.fromResponse);
  }

  /**
   * Log in with email + password, returning a user access token.
   * `POST {acOrigin}/api/v1/auth/user/token` with body `{email, password}`.
   * @throws {AuthError} on a mapped AC failure (e.g. {@link AuthUnauthorizedError} 401).
   */
  async login(input: LoginInput): Promise<AuthTokenResponse> {
    validateEmail(input.email);
    validatePassword(input.password);
    const origin = resolveAcOrigin(this.#acOriginTemplate, input.subdomain);

    const response = await safeFetch(this.#fetchImpl, `${origin}${LOGIN_PATH}`, {
      method: 'POST',
      headers: { ...JSON_HEADERS },
      body: JSON.stringify({ email: input.email, password: input.password }),
    });
    return handleResponse<AuthTokenResponse>(response, AuthError.fromResponse);
  }
}
