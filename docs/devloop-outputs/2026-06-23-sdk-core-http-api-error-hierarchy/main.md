# Devloop Output: sdk-core HTTP API + error hierarchy

**Date**: 2026-06-23
**Task**: AuthApiClient.register/login + MeetingApiClient.joinMeeting/createMeeting (uniform camelCase wire), client-side subdomain regex validation, typed error hierarchy with toJSON() redaction, MSW unit tests (R-11, R-12, R-23, R-31, R-42). Cross-boundary: add `accessToken` to dt-guard CATEGORY_A.
**Specialist**: client
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-11`
**Duration**: ~45m (Gate 1 → commit)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7bc9aa4547308a621fa689fe55bc64ff3049b65e` |
| Branch | `feature/browser-client-join-task-11` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-8796c9f0` |
| Implementing Specialist | `client` |
| Iteration | `2` (review→impl: test coverage findings) |
| Security | `security@session-8796c9f0` |
| Test | `test@session-8796c9f0` |
| Observability | `observability@session-8796c9f0` |
| Code Quality | `code-reviewer@session-8796c9f0` |
| DRY | `dry-reviewer@session-8796c9f0` |
| Operations | `operations@session-8796c9f0` |
| Semantic Guard | `semantic-guard@session-8796c9f0` |

### Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed — dt-guard `accessToken` at **Minor-judgment** (security initially said Mechanical, then self-corrected per code-reviewer's word-boundary argument; see Lead ruling below), explicit owner sign-off + trailer at commit; entry NOT redundant (consumers word-boundary+exact match, so `token`/`access_token` miss camelCase); place Wave-2 cohort + inline tag + sign-off `@security 2026-06-23`, add to `category_a_includes_wave2_additions` test |
| Test | confirmed (req: add synthetic 400/409 fromResponse tests; consolidate to ≤15) |
| Observability | confirmed (note: document toJSON `message` PII-safety as transitive AC/GC upstream contract) |
| Code Quality | confirmed (post-ruling) — ESCALATEd dt-guard classification; Lead upgraded to Minor-judgment. Relays: subclasses override `this.name`, `serverCode` readonly, `exactOptionalPropertyTypes` omit-not-undefined for optional fields |
| DRY | confirmed (note: add anchor comments to Rust SoT for subdomain/meeting-code regexes) |
| Operations | confirmed (nit: consistent msw version pin + commit lockfile) |
| Semantic Guard | confirmed (req-at-Gate2: NetworkError MALFORMED_RESPONSE must use STATIC message, never interpolate raw body/fetch detail) |

---

## Task Overview

### Objective
Implement the sdk-core HTTP API surface (`AuthApiClient`, `MeetingApiClient`) and the typed SDK error hierarchy with token-redacting `toJSON()`, against AC/GC's uniform camelCase wire contract (post task #51). Plus the cross-boundary Mechanical edit adding `accessToken` (camelCase) to `crates/dt-guard/src/common/pii_vocabulary.rs` CATEGORY_A.

### Scope
- **Service(s)**: client (`packages/sdk-core/`), dt-guard (`crates/dt-guard/` — cross-boundary)
- **Schema**: No
- **Cross-cutting**: Yes (one Rust cross-boundary edit in dt-guard)

### Requirements
- R-11: AuthApiClient.register/login → AC, camelCase wire, subdomain-qualified origin, subdomain regex validation, typed AuthError subtypes (400/401/403/409/429)
- R-12: MeetingApiClient.joinMeeting/createMeeting → GC, typed errors (400/401/403/404/409)
- R-23: tokens in memory only; SDK error objects redact token fields (toJSON() redaction)
- R-31: bounded input validation (display name ≤64, email ≤254, password ≤128); meeting code regex (12 base62) client-side
- R-42: ~10-15 MSW unit tests for HTTP API client

### Debate Decision
NOT NEEDED — wire-shape decision settled by task #51 (Clarification Question 6, user ruling 2026-06-19). This task codes against the established camelCase contract.

---

## Cross-Boundary Classification

**Mechanism restatement (per planning rule):** This task adds a browser-side HTTP
adapter that issues `fetch` calls against two existing Rust HTTP surfaces (AC, GC)
whose wire contracts are already frozen (camelCase, locked by serde wire-shape
tests on the Rust side). The mechanism is: (a) construct request URL/headers/body
strings, (b) parse JSON response bodies into typed TS objects, (c) translate HTTP
status codes into a typed error class hierarchy, (d) guarantee secret strings
(JWTs) never escape via error serialization. No Rust behavior changes. The single
Rust edit is a string-list append to a guard vocabulary. The class is NOT wider
than the task's nouns — every file is either new TS under `packages/sdk-core/` or
the one pre-identified guard-vocabulary line.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/src/http/*.ts` (new HTTP API clients) | Mine | — |
| `packages/sdk-core/src/errors/*.ts` (new typed error hierarchy) | Mine | — |
| `packages/sdk-core/src/validation/*.ts` (bounded input + subdomain/code regex) | Mine | — |
| `packages/sdk-core/src/index.ts` (barrel: add new public symbols) | Mine | — |
| `packages/sdk-core/src/**/__tests__/*.test.ts` (MSW unit tests) | Mine | — |
| `packages/sdk-core/package.json` (add `msw` devDependency) | Mine | — |
| `pnpm-lock.yaml` (lockfile update from adding `msw`) | Mine, Mechanical | — (infra owns lockfile policy; pure generated delta) |
| `crates/dt-guard/src/common/pii_vocabulary.rs` (add `accessToken` to CATEGORY_A) | Not mine, Minor-judgment | security |

**Lead ruling (Gate 1 ESCALATE resolution, 2026-06-23):** code-reviewer challenged the
`pii_vocabulary.rs` row as mis-classified Mechanical, arguing (correctly, and corroborated
by security's own word-boundary analysis) that the append is a behavior change to a
detection vocabulary — it newly flags camelCase `accessToken` at log/error sites that
previously passed — so it is NOT value-neutral and fails the sed-test. The need for owner
domain judgment (genuine secret → CATEGORY_A vs. substring-exception → ALLOWLIST like
`token_type`) with bounded impact is the textbook Minor-judgment case. **Lead upholds the
upgrade: Mechanical → Minor-judgment** (monotonic, allowed). dt-guard is not a §6.4 GSA
path, so owner-implements is not forced. Minor-judgment's requirement (owner confirms at
Gate 1 + Gate 3) is satisfied by security being a panel reviewer: security confirmed at
Gate 1 with explicit sign-off and will re-confirm via the Ownership Lens at Gate 3.
Implementer will add the optional `Approved-Cross-Boundary: security <reason>` commit
trailer (§6.7) as a durable audit breadcrumb for this auth-adjacent edit.

**Convergence:** security (named owner) subsequently conceded the framing error and
self-reclassified Mechanical → Minor-judgment, matching this ruling — so no live
disagreement remained; Gate 1 closed confirmed at Minor-judgment (not blocked). Owner
hunk-ACK trailer to land at commit (security's authored wording):
`Approved-Cross-Boundary: security CATEGORY_A += accessToken — camelCase counterpart of access_token; word-boundary matcher does not cover it (verified), genuine secret identifier, @security 2026-06-23`
No `CATEGORY_A_ALLOWLIST` entry needed (`accessToken` is a genuine secret, not a
substring-exception like `token_type`). The "redundant because `token` already present"
open question is struck — factually wrong per the `\b…\b` word-boundary matcher.

<!-- Note (Lead): dt-guard is NOT a GSA path (not in ADR-0024 §6.4 manifest). The
     file's own doc requires security sign-off for CATEGORY_A additions, so security
     is the de-facto owner and is on the review panel. Implementer self-classified
     Mechanical per the task framing; security reviewer adjudicates / may upgrade to
     Minor-judgment at Gate 1. Final classification to be confirmed during planning. -->

---

## Planning

### Confirmed wire contracts (read from source, not assumed)

**AC — `crates/ac-service/src/handlers/auth_handler.rs` + `services/token_service.rs`:**
- `POST /api/v1/auth/register` — request body keys: `{ email, password, displayName }`
  (`UserRegistrationRequest`, `rename_all = "camelCase"`, `display_name` → `displayName`).
  Response (`UserRegistrationResponse`): `{ userId, email, displayName, accessToken, tokenType, expiresIn }`.
- `POST /api/v1/auth/user/token` (login) — request body keys: `{ email, password }`
  (`UserTokenRequest`). Response (`UserTokenResponse`): `{ accessToken, tokenType, expiresIn }`.
- AC error envelope (`errors.rs`): `{ "error": { "code": "<UPPER_SNAKE>", "message": "<str>" } }`.
  AC status set actually emitted: **401** (`INVALID_CREDENTIALS`, `INVALID_TOKEN`),
  **403** (`INSUFFICIENT_SCOPE`), **404** (`NOT_FOUND`), **429** (`RATE_LIMIT_EXCEEDED`,
  `TOO_MANY_REQUESTS`), **500** (`DATABASE_ERROR`, `CRYPTO_ERROR`, `INTERNAL_ERROR`).
  NOTE: AC has **no 400 and no 409 variant today**. The task asks AuthError to cover
  400/401/403/409/429 — I map **by HTTP status** (defensive/forward-compatible), so a
  future AC 400/409 still lands on a typed subtype rather than a generic fallthrough.
- `429` may carry `Retry-After` (seconds) header on `TOO_MANY_REQUESTS` — captured into
  the typed error as `retryAfterSeconds?`.

**GC — `crates/gc-service/src/handlers/meetings.rs` + `models/mod.rs` + `errors.rs`:**
- `GET /api/v1/meetings/:code` (join) — header `Authorization: Bearer <userToken>`, no body.
  Response (`JoinMeetingResponse`, camelCase): `{ token, expiresIn, meetingId, meetingName,
  mcAssignment: { mcId, webtransportEndpoint?, grpcEndpoint } }`. (`webtransportEndpoint`
  is `skip_serializing_if = Option::is_none` → may be absent.)
- `POST /api/v1/meetings` (create) — header `Authorization: Bearer <userToken>`, request body
  (`CreateMeetingRequest`, camelCase, `deny_unknown_fields`): required `displayName`; optional
  `maxParticipants, scheduledStartTime, enableE2eEncryption, requireAuth, recordingEnabled,
  allowGuests, allowExternalParticipants, waitingRoomEnabled`. Response 201
  (`CreateMeetingResponse`, camelCase): `{ meetingId, meetingCode, displayName, status,
  maxParticipants, enableE2eEncryption, requireAuth, recordingEnabled, allowGuests,
  allowExternalParticipants, waitingRoomEnabled, createdAt }`.
  NOTE the request key is `scheduledStartTime` (not `scheduledStart` as the task summary
  abbreviated) and `displayName` (not `title`) — I follow the actual GC struct.
- GC error envelope identical shape to AC. GC status set: 400/401/403/404/409/429/500/502/503.

### File-by-file plan

**New TS source (all under `packages/sdk-core/src/`, ESM `.js` import specifiers per existing barrel):**

1. `errors/SdkError.ts` — base `SdkError extends Error` with:
   - `readonly code: SdkErrorCode` (const-object union, mirroring `TransportErrorCode` idiom).
   - `readonly status?: number` (HTTP status when applicable).
   - `toJSON()` returning a redacted plain object (`{ name, code, status, message }`) —
     **never** includes any token/credential field. This is the R-23 redaction guarantee:
     even if an error is built carrying contextual data, `toJSON()` is the only serialization
     surface and it emits a fixed allowlist of non-secret keys. (Also set a non-enumerable
     internal field convention so accidental `JSON.stringify(err)` can't leak — `toJSON`
     takes precedence for `JSON.stringify`, and we add no enumerable secret props anyway.)
   - `Object.setPrototypeOf` fix (matches `TransportError`).
2. `errors/AuthError.ts` — `AuthError extends SdkError` + subtypes mapped by status:
   - `AuthBadRequestError` (400), `AuthUnauthorizedError` (401), `AuthForbiddenError` (403),
     `AuthConflictError` (409), `AuthRateLimitError` (429, carries `retryAfterSeconds?`).
   - A `fromResponse(status, parsedBody)` factory returning the right subtype; falls back to
     a generic `AuthError` for unmapped statuses.
3. `errors/MeetingError.ts` — `MeetingError extends SdkError` + subtypes by status:
   - `MeetingBadRequestError` (400), `MeetingUnauthorizedError` (401),
     `MeetingForbiddenError` (403), `MeetingNotFoundError` (404), `MeetingConflictError` (409).
   - `fromResponse(status, parsedBody)` factory.
4. `errors/NetworkError.ts` — `NetworkError extends SdkError` for `fetch` rejection
   (DNS/TLS/offline) and for non-JSON / malformed-envelope bodies (`code: NETWORK` /
   `MALFORMED_RESPONSE`). Keeps the hierarchy aligned with R-41's `NetworkError` boundary.
5. `errors/index.ts` (internal re-export) — not the public barrel; convenience for `http/`.
6. `validation/limits.ts` — bounded validators (R-31):
   - `MAX_DISPLAY_NAME = 64`, `MAX_EMAIL = 254`, `MAX_PASSWORD = 128` consts.
   - `validateDisplayName`, `validateEmail`, `validatePassword` → throw
     `AuthBadRequestError` (client-side, before any network call) on violation.
   - `validateSubdomain(s)`: regex `^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`, anchored, applied
     **before** URL interpolation (injection prevention, R-11). Throws on mismatch.
   - `validateMeetingCode(c)`: regex `^[0-9A-Za-z]{12}$` (12-char base62, matches GC's
     `BASE62_CHARS` + `MEETING_CODE_LENGTH`). Throws on mismatch before request.
   - Regexes defined as module-level `const` literals (no dynamic `RegExp(userInput)`).
7. `http/origin.ts` — `acOriginFor(subdomain, baseTemplate)` helper: validates subdomain,
   then builds the subdomain-qualified AC origin (e.g. `https://demo.localhost:8443`). Origin
   template comes from config passed into the client (not hardcoded). GC origin is a plain
   configured base URL (no subdomain interpolation for GC).
8. `http/types.ts` — TS interfaces for request inputs + response shapes mirroring the
   confirmed contracts above (`RegisterInput`, `LoginInput`, `AuthTokenResponse`,
   `RegisterResponse`, `JoinMeetingResponse`, `CreateMeetingInput`, `CreateMeetingResponse`,
   `McAssignment`). `webtransportEndpoint?` optional. `userToken`/tokens typed as `string`
   but NEVER placed on any error/`toJSON` surface.
9. `http/parse.ts` — shared helper: given a `Response`, parse JSON (guard non-JSON →
   `NetworkError(MALFORMED_RESPONSE)`), and on `!response.ok` read the `{error:{code,message}}`
   envelope and hand `(status, body)` to the appropriate `fromResponse` factory.
10. `http/AuthApiClient.ts` — `AuthApiClient`:
    - Constructed with `{ acOriginTemplate, fetchImpl?: typeof fetch }` (token-source/injection
      pattern; default `globalThis.fetch`). `fetchImpl` injection is what MSW/tests hook.
    - `register({ subdomain, email, password, displayName })`: validate inputs + subdomain →
      `POST {acOrigin}/api/v1/auth/register` with `Content-Type: application/json`, body
      `{ email, password, displayName }` → parse → `RegisterResponse`. Errors via `parse.ts`
      → `AuthError.fromResponse`.
    - `login({ subdomain, email, password })`: validate → `POST {acOrigin}/api/v1/auth/user/token`
      body `{ email, password }` → `AuthTokenResponse`.
11. `http/MeetingApiClient.ts` — `MeetingApiClient`:
    - Constructed with `{ gcBaseUrl, fetchImpl? }`.
    - `joinMeeting(code, { userToken })`: validate meeting code →
      `GET {gcBaseUrl}/api/v1/meetings/{code}` with `Authorization: Bearer <userToken>` →
      `JoinMeetingResponse`. Errors via `MeetingError.fromResponse`.
    - `createMeeting(input, { userToken })`: build camelCase body (only defined fields) →
      `POST {gcBaseUrl}/api/v1/meetings` with bearer → 201 `CreateMeetingResponse`.

**Modified:**
12. `src/index.ts` (barrel) — add public exports: `AuthApiClient`, `MeetingApiClient`;
    `SdkError`, `SdkErrorCode`, `AuthError` (+ subtypes), `MeetingError` (+ subtypes),
    `NetworkError`; the `type` exports for request/response interfaces; the validation
    consts/regex helpers only if intended public (lean: export the error classes + clients +
    a few input/response types; keep validators internal unless a consumer needs them).
13. `package.json` — add `msw` to `devDependencies` (pin a 2.x version). Note: triggers a
    `pnpm-lock.yaml` regeneration (mechanical).

**Cross-boundary Rust edit:**
14. `crates/dt-guard/src/common/pii_vocabulary.rs` — append `"accessToken"` to
    `PII_TOKENS_CATEGORY_A` in the Wave-2 cohort block, with an inline comment tagging it as
    the camelCase counterpart of the existing `access_token` (task #11 sign-off `@security 2026-06-23`, recorded by
    security). It is a genuine secret identifier (not an allowlist entry like `token_type`).

### Error hierarchy shape (summary)

```
SdkError (base; code, status?, message; toJSON() → fixed non-secret key allowlist)
 ├─ AuthError                ← AC failures, fromResponse(status, body)
 │   ├─ AuthBadRequestError      (400)
 │   ├─ AuthUnauthorizedError    (401)
 │   ├─ AuthForbiddenError       (403)
 │   ├─ AuthConflictError        (409)
 │   └─ AuthRateLimitError       (429, retryAfterSeconds?)
 ├─ MeetingError             ← GC failures, fromResponse(status, body)
 │   ├─ MeetingBadRequestError   (400)
 │   ├─ MeetingUnauthorizedError (401)
 │   ├─ MeetingForbiddenError    (403)
 │   ├─ MeetingNotFoundError     (404)
 │   └─ MeetingConflictError     (409)
 └─ NetworkError             ← fetch reject / malformed envelope (no HTTP status)
```

Mapping is **by HTTP status**, sourced from the `{error:{code,message}}` envelope; the
server `code` string is preserved into `SdkError.code`-adjacent context (carried as the
`serverCode` on the typed error, included in `toJSON()` since it is non-secret), so callers
can branch on AC/GC's `INVALID_CREDENTIALS` etc. without it being a token leak.

### toJSON() redaction approach (R-23)

- `SdkError.toJSON()` returns exactly `{ name, code, status?, serverCode?, message }`.
- No token, password, Authorization header, or request body is ever stored as an enumerable
  property on any error instance — the clients build errors only from `(status, parsed
  error-envelope)`, which contains no secrets (AC/GC generic messages by design).
- A unit test asserts that `JSON.stringify(err)` for every subtype contains none of a set of
  sentinel secret values (a fake JWT, a password), even when the originating request carried
  them.

### Test list (~14 Vitest + MSW; `packages/sdk-core/src/http/__tests__/`)

MSW `setupServer` with per-test handlers; clients constructed with default `globalThis.fetch`
(MSW intercepts at the network layer) OR an injected `fetchImpl` — I'll use MSW node server so
request shape (URL, headers, body) is asserted from intercepted requests.

AuthApiClient (AC):
1. `register` 200 → posts to `https://demo.localhost:8443/api/v1/auth/register`, body exactly
   `{email,password,displayName}`, `Content-Type: application/json`; returns parsed
   `{userId,...,accessToken,...}`.
2. `register` 429 → throws `AuthRateLimitError`, `retryAfterSeconds` captured from header.
3. `login` 200 → posts to `/api/v1/auth/user/token`, body `{email,password}`; returns
   `{accessToken,tokenType,expiresIn}`.
4. `login` 401 (`INVALID_CREDENTIALS`) → throws `AuthUnauthorizedError`, `serverCode` preserved.
5. subdomain validation: invalid subdomain (`Demo!`, `-bad`, 64-char) → throws
   `AuthBadRequestError` **before any fetch** (assert MSW saw zero requests).
6. bounded input: `displayName` > 64 / `email` > 254 / `password` > 128 → throws pre-network.
7. correct subdomain origin interpolation for a valid subdomain (`acme` → `https://acme.localhost:8443`).

MeetingApiClient (GC):
8. `joinMeeting` 200 → `GET /api/v1/meetings/<code>` with `Authorization: Bearer <token>`;
   returns `{token,expiresIn,meetingId,meetingName,mcAssignment{mcId,grpcEndpoint,...}}`.
9. `joinMeeting` 200 with `webtransportEndpoint` absent → optional field handled (undefined).
10. `joinMeeting` 401 → `MeetingUnauthorizedError`.
11. `joinMeeting` 403 → `MeetingForbiddenError`.
12. `joinMeeting` 404 → `MeetingNotFoundError`.
13. meeting-code validation: bad code (11 chars / non-base62) → throws pre-network (zero requests).
14. `createMeeting` 201 → `POST /api/v1/meetings` with bearer + camelCase body
    (`displayName`, optional `scheduledStartTime`); returns `{meetingCode,...}`.

Redaction (R-23):
15. `toJSON()` / `JSON.stringify` of representative `AuthError` + `MeetingError` instances
    contains none of the sentinel secrets (fake JWT, password) and only the allowlisted keys.

(That's ~15 — within the R-42 10–15 band; I'll consolidate 8/9 or 5/6 if needed to land at 14.)

### Open question for security reviewer (Gate 1)

`accessToken` contains the substring `token`, which is ALREADY in `PII_TOKENS_CATEGORY_A`.
So for substring-based identifier matching the new entry is technically redundant. The task
nonetheless pre-classifies adding the explicit camelCase form as Mechanical (intent: make the
vocabulary self-documenting now that the camelCase string enters the codebase, and guard
against a future exact-match consumer). **Security: confirm (a) the entry is wanted given the
`token` substring already covers it, and (b) whether it stays Mechanical or upgrades to
Minor-judgment.** I will not touch `pii_vocabulary.rs` until this is resolved.

---

## Pre-Work

None.

---

## Implementation Summary

### HTTP API clients (R-11, R-12)
- `AuthApiClient.register/login` → AC `/api/v1/auth/register` + `/api/v1/auth/user/token`,
  uniform camelCase wire, subdomain-qualified origin via `resolveAcOrigin(template, subdomain)`,
  injectable `fetchImpl`, token via `Authorization: Bearer` only.
- `MeetingApiClient.joinMeeting/createMeeting` → GC `/api/v1/meetings/:code` (GET) +
  `/api/v1/meetings` (POST), bearer userToken, camelCase bodies (`displayName`/`scheduledStartTime`).
- `http/parse.ts` `safeFetch`/response handling; `http/origin.ts` origin construction.

### Typed error hierarchy (R-23)
- `SdkError` base (const-object `code` union; `toJSON()` fixed allowlist `{name,code,message,status?,serverCode?}`).
- `AuthError` {400/401/403/409/429+retryAfter}, `MeetingError` {400/401/403/404/409},
  `NetworkError` {FETCH_FAILED/MALFORMED_RESPONSE, static messages, non-enumerable cause},
  and domain-neutral `ValidationError` {code VALIDATION, field, status 400} (added at Gate 3 per code-reviewer F1).
- `fromResponse(status, envelope)` factories preserve server `code` as `serverCode`.

### Validation (R-31, R-11)
- `validation/limits.ts`: bounded validators (64/254/128), anchored `validateSubdomain`
  (pre-interpolation) + `validateMeetingCode` (12 base62), module-const regexes, Rust-SoT anchor comments.

### Cross-boundary (Minor-judgment)
- `crates/dt-guard/src/common/pii_vocabulary.rs`: `accessToken` → CATEGORY_A Wave-2 cohort + test.

### Tests (R-42, R-41)
- MSW-based unit + branch tests across http/errors/validation; 52 tests / 7 files;
  coverage 100% stmt/func/line, 95.76% branch (per-file ≥90%).

---

## Files Modified

```
crates/dt-guard/src/common/pii_vocabulary.rs       |   8 +-   (accessToken → CATEGORY_A + test)
packages/sdk-core/package.json                     |   2 +    (msw, @vitest/coverage-v8 devDeps)
packages/sdk-core/src/errors/SdkError.ts           | 139 +    (base + toJSON allowlist)
packages/sdk-core/src/errors/AuthError.ts          | 108 +
packages/sdk-core/src/errors/MeetingError.ts       |  92 +
packages/sdk-core/src/errors/NetworkError.ts       |  56 +    (static messages, non-enum cause)
packages/sdk-core/src/errors/ValidationError.ts    |  28 +    (domain-neutral, code VALIDATION)
packages/sdk-core/src/errors/index.ts              |  35 +    (internal re-export, not the barrel)
packages/sdk-core/src/http/AuthApiClient.ts        |  97 +
packages/sdk-core/src/http/MeetingApiClient.ts     | 114 +
packages/sdk-core/src/http/origin.ts               |  34 +
packages/sdk-core/src/http/parse.ts                |  83 +
packages/sdk-core/src/http/types.ts                | 113 +
packages/sdk-core/src/validation/limits.ts         | 106 +    (bounded + subdomain/code regex)
packages/sdk-core/src/index.ts                     |  48 +    (closed barrel: new public symbols)
packages/sdk-core/src/**/__tests__/*.test.ts       | 774 +    (5 files: 52 tests, MSW + branches)
pnpm-lock.yaml                                      | 419 +    (msw + coverage-v8 regen)
docs/user-stories/2026-05-02-browser-client-join.md|   2 +-   (#11 → Completed + output path)
docs/devloop-outputs/.../main.md                   | 401 +    (this file)
```

---

## Devloop Verification Steps

`./scripts/layer-all.sh` — **PASS** (PIPE_EXIT=0, no FAIL). Note: first run hit a transient
Layer 4 cold-compile race (86s, FAIL); clean re-run 163s OK. Reproduced green twice.

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | cargo (+dt-guard) build + nx typecheck |
| 2 Format | OK | cargo-fmt + nx-format |
| 3 Guards | OK | guards-passed; `ts-no-pii-in-logs` WARNING (non-blocking, exit 0) at `validation/limits.ts:44` on literal label `'Display name is required'` — flagged for security/obs confirmation at Gate 3 |
| 4 Test | OK | cargo-test + dt-guard pii_vocabulary 7/7 + TS unit 36 + test-utils 14 + component 3 |
| 5 Lint | OK | clippy + nx-lint |
| 6 Audit | OK | cargo-audit + pnpm-audit (msw tree clean) + buf-breaking |
| 7 Env-tests | N/A | `wave2-pending` — browser E2E harness not wired yet (later task); documented self-justifying gap |

Artifact-specific: no `.proto`/migration/k8s/Dockerfile in diff → none triggered.

---

## Code Review Results

### Gate 3 — Verdict Tracking

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | **RESOLVED-FIXED** | 1 finding (limits.ts WARNING) fixed; R-23/R-31 verified; **dt-guard Minor-judgment hunk Ownership-Lens re-confirmed** — trailer to land at commit |
| Test | **RESOLVED-FIXED** (re-confirmed post-ValidationError) | 2 findings fixed: `@vitest/coverage-v8@4.1.8` + 2 branch-test files; coverage held 100% stmt/func/line, 95.76% branch after the ValidationError refactor. 52 tests/7 files. |
| Observability | **RESOLVED-FIXED** | 1 finding (limits.ts pii-safe WARNING) fixed in-diff; no premature telemetry, discriminants label-ready, toJSON transitive-PII note present |
| Code Quality | **RESOLVED-FIXED** | F1 (validators threw `AuthError` + dead VALIDATION code) → new domain-neutral `ValidationError`; F3 (email throws unannotated) → pii-safe annotations. ADR + Ownership Lens clean; all Gate-1 relays landed. F2 doc-nit (stale line 41) reconciled by Lead. |
| DRY | **CLEAR** | idiom reused not cloned; allowlist redaction (no parallel TS secret list); anchor comments landed; no extraction opportunities |
| Operations | **CLEAR** | msw 2.7.0 exact devDep, lockfile committed, no deploy surface, origin config-driven |
| Semantic Guard | **CLEAR** | verified static-message seam (both NetworkError paths), non-enumerable cause excluded from toJSON, serverCode preserved; credential-leak/context-preservation/metrics all pass |

---

## Accepted Deferrals

- (none surfaced in this devloop) — all findings fixed in-diff; zero deferred, zero spun-out. Per-finding detail is in the verdict sections above.

---

## Rollback Procedure

1. Start commit: `7bc9aa4547308a621fa689fe55bc64ff3049b65e`
2. Review: `git diff 7bc9aa4..HEAD`
3. Soft reset: `git reset --soft 7bc9aa4`
4. Hard reset: `git reset --hard 7bc9aa4`

---

## Issues Encountered & Resolutions

### Issue 1: dt-guard classification dispute (Gate 1)
**Problem**: Task pre-classified the `pii_vocabulary.rs` edit as Mechanical; code-reviewer
ESCALATEd, arguing the append changes guard detection behavior (newly flags camelCase
`accessToken`) so it isn't value-neutral.
**Resolution**: Lead upheld upgrade to Minor-judgment; security (owner) concurred and
self-corrected. Owner sign-off + `Approved-Cross-Boundary` trailer satisfy the tier.

### Issue 2: Missing `target/release/dt-guard` binary
**Problem**: The classification-sanity guard (and Gate 2 `run-guards.sh`) failed with
`dt-guard-binary-missing` — the binary wasn't built in this checkout.
**Resolution**: `cargo build --release -p dt-guard`.

### Issue 3: Transient Layer 4 failure (Gate 2, first run)
**Problem**: First `layer-all.sh` run reported Layer 4 FAIL at 86s (cold-compile race).
**Resolution**: Layer 4 passed in isolation and on clean re-run (163s); reproduced green twice.

### Issue 4: Coverage never actually measured (Gate 3, Test F1)
**Problem**: `@vitest/coverage-v8` wasn't installed, so `--coverage` couldn't run — R-41
≥90% was unverifiable despite Gate 2 reporting test pass.
**Resolution**: Added the provider (pinned) + branch tests; coverage now 100/95.76.

### Issue 5: Validators raised auth-domain errors (Gate 3, Code Quality F1)
**Problem**: `validate*` helpers threw `AuthBadRequestError` even from the meeting path;
`SdkErrorCode.Validation` was dead.
**Resolution**: Introduced domain-neutral `ValidationError` (code VALIDATION); rerouted all
validators; resolved both the cross-domain leak and the dead code in one change.
