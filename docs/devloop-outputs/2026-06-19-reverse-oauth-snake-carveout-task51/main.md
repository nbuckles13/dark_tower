# Devloop Output: Reverse the OAuth snake_case carve-out — single camelCase wire rule

**Date**: 2026-06-19
**Task**: Flip AC OAuth-named response fields from snake_case to camelCase; single wire rule (no per-field carve-outs). Update lock tests, env-tests fixtures, runbooks/scripts, and reverse the R-11/R-53 prose in the browser-client-join story.
**Specialist**: auth-controller
**Mode**: Agent Teams (v2) — full, `--paired-with=security --paired-with=operations`
**Branch**: `feature/browser-client-join-task-51`
**Duration**: Extended — multi-round user scope ruling (A→B→A, settled on (A) full flip); 2 Gate-2 attempts + a single Gate-3 finding fixed.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `78b4d321c9681256d112fdb61baa4a2188380834` |
| Branch | `feature/browser-client-join-task-51` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (auth-controller) |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |
| Security | `security` (paired) |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` (paired) |
| Semantic Guard | `semantic-guard` |

---

## Task Overview

### Objective — FINAL DEFINITIVE RULING: option (A), FULL FLIP + GSA co-sign (user, "fix all", 2026-06-19)
Reverse the per-field OAuth snake_case carve-out across **ALL AC token endpoints** — user-flow
(register `/api/v1/auth/register` + login `/api/v1/auth/user/token`) AND the genuine OAuth
service-token endpoint (`/api/v1/auth/service/token`). All token-response wire fields
(`accessToken` / `tokenType` / `expiresIn` / `scope`) are camelCase under a SINGLE rule, NO per-field
carve-out. Because the service-token response is consumed by the shared GSA
`common::token_manager::OAuthTokenResponse` (all four services' s2s token path), that deserializer is
flipped IN LOCKSTEP under security co-sign — the three-member `TokenResponse` cluster moves atomically
(server `TokenResponse` + prod `OAuthTokenResponse` + env-tests `TokenResponse` mirror). Token semantic
contract is unchanged (same tokens, same claims, same TTL) — only wire field naming.

**Wire shape under the (A) ruling: ONE rule across ALL AC endpoints.** Every AC token-response wire key
is camelCase. The bounding note holds: any future strict-RFC-6749 third-party integration belongs behind
a separate `/oauth/token` endpoint, NOT by reintroducing per-field carve-outs.

**⚠️ RULING HISTORY (definitive resolution of a relay mix-up):** the user's FIRST answer → (A); an
ambiguous follow-up was mis-read as (B)+spinout (and a (B) plan was briefly approved); the user then
**definitively confirmed (A) — "final ruling is A (fix all), env-tests was just a note."** **(A) is the
FINAL operative ruling.** This devloop does the full flip incl. `/service/token` + the GSA
`OAuthTokenResponse` edit (security co-signed) — NO spinout. Any text below that still reads as "(B) /
two-rule / spun out" is stale and superseded by this header; the Cross-Boundary table HAS the
`token_manager.rs` GSA row.

### Scope
- **Service(s)**: ac-service (production DTOs + lock tests); env-tests fixtures; docs/scripts.
- **Schema**: No.
- **Cross-cutting**: Yes — runbook/scripts (operations), env-tests fixtures (test), story prose.

### Debate Decision
NOT NEEDED — this is a rule-change follow-up that surfaced during task #49's wire-shape
decision; the direction (single camelCase rule) is already settled. This devloop retrofits AC
+ the shared R-11/R-53 prose.

---

## Cross-Boundary Classification

<!-- Populated by implementer at planning; Lead validates at Gate 1 via
     validate-cross-boundary-classification.sh. FINAL RULING: option (A) — FULL FLIP + GSA co-sign.
     The AC structs in models/mod.rs + handlers/auth_handler.rs + token_service.rs are
     auth-controller's own domain (NOT GSA). crates/common/src/token_manager.rs IS a GSA path
     (ADR-0024 §6.4 auth/crypto primitive) and IS edited here — classified Domain-judgment / GSA
     with security co-sign, NOT Mechanical. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/ac-service/src/models/mod.rs` (`TokenResponse` + its lock test, renamed `_wire_shape`) | Mine | auth-controller |
| `crates/ac-service/src/handlers/auth_handler.rs` (`UserRegistrationResponse` + 2 lock tests + `assert_no_snake_keys` helper) | Mine | auth-controller |
| `crates/ac-service/src/services/token_service.rs` (`UserTokenResponse` struct) | Mine | auth-controller |
| `crates/common/src/token_manager.rs` (`OAuthTokenResponse` deserializer + wiremock bodies + reject-test) | **Not mine, Domain-judgment** (GSA — ADR-0024 §6.4 auth/crypto primitive) | auth-controller |
| `crates/ac-service/tests/integration/user_auth_tests.rs` (golden lock + user-flow reads) | Mine (AC test) | auth-controller |
| `crates/env-tests/src/fixtures/auth_client.rs` (`TokenResponse` + `UserRegistrationResponse` mirrors — BOTH flip) | Cross-boundary — test-domain fixture | @test (note as owner) |
| `docs/runbooks/ac-service-deployment.md` | Cross-boundary — ops surface | @operations (paired) |
| `docs/runbooks/ac-service-incident-response.md` | Cross-boundary — ops surface | @operations (paired) |
| `docs/runbooks/gc-deployment.md` (AC `.accessToken` parses — all endpoints) | Cross-boundary — ops surface | @operations (paired) |
| `docs/user-stories/2026-05-02-browser-client-join.md` (R-11, R-53, Q6, design bullet — single-rule) | Mine — my story prose | auth-controller |

**GSA commit trailer (final wording, supplied by @security):** the `token_manager.rs` edit ships with:
`Approved-Cross-Boundary: security OAuth wire-key rename (naming-only; token value/claims/TTL unchanged; SecretString extraction + [REDACTED] Debug preserved) per task #51 ruling-A; GSA auth-primitive co-sign per ADR-0024 §6.4`
A Domain-judgment/GSA edit (the table cell omits the bare word "Mechanical" to avoid the classification-guard parser false-positive; the "GSA forbids Mechanical" principle still holds). @security gives the in-loop hunk-ACK at Gate 2 after seeing the `rename_all` + the snake-reject deserialize test + the mock-site flips.

**@security confirmations (recorded):**
- **Ownership: auth-controller + security ONLY — no GC/MC/MH co-sign.** @security verified all three services consume the single shared `spawn_token_manager` / one `OAuthTokenResponse` struct (not per-service copies), so they're consumers of a shared type, not co-owners. §6.4 intersection rule does NOT fire (single GSA surface, single owner-pair).
- **Redaction preserved (do NOT change):** `OAuthTokenResponse`'s redacting Debug `.field("access_token", &"[REDACTED]")` uses the Rust field IDENT (stays snake after the wire flip) and the SecretString extraction at :689 are UNTOUCHED — only the struct-level serde derive changed. Confirmed in the diff.
- **`pii_vocabulary.rs` NOT touched (correct):** its matcher targets Rust LOG/INSTRUMENT identifiers (word-boundary), not serde wire keys; every Rust ident stays `access_token`, so the guard keeps protecting it unchanged and no `accessToken` identifier is introduced by this diff. Adding `accessToken` here would be a guard edit with no triggering surface. Filed instead as a docs/TODO.md follow-up (add `accessToken` + camel siblings to CATEGORY_A when the TS SDK lands the camel identifier — task #11 area); @security records it as an accepted spin-out at Gate 3.

**§6.4 intersection-rule check (confirmed — SINGLE co-sign suffices):** the intersection rule fires only when an edit spans TWO GSAs (canonical case: auth-routing identity fields in `proto/internal.proto`, which span wire-format AND auth-routing-policy → protocol + auth + security). This change touches ONLY `crates/common/src/token_manager.rs`, an auth/crypto primitive. The OAuth `/service/token` contract is HTTP/JSON, NOT a `proto/*` protobuf path — so NO wire-format/protocol GSA is crossed. Therefore @security's single co-sign satisfies §6.4; no protocol co-sign needed.

**NOT in scope (security-accepted spin-out, see docs/TODO.md):** `crates/dt-guard/src/common/pii_vocabulary.rs` — adding camel `accessToken` to the secret-detection vocab is guard-owned and deferred to TS-SDK time (task #11); task #51 deliberately does NOT edit it.

**Mechanism restatement**: "Every AC token-carrying HTTP response struct serializes camelCase with no per-field rename." Sweeping that wider mechanism surfaces exactly three structs (`TokenResponse`, `UserRegistrationResponse`, `UserTokenResponse`). No hidden logout/introspect/revoke sibling exists (AC has no such endpoints). Admin responses (`RotateKeysResponse`, `CreateClientResponse`, `RotateSecretResponse`) carry no OAuth token fields and are untouched.

**Deliberately OUT of scope (token semantic contract — UNCHANGED):**
- `InternalTokenResponse` (`models/mod.rs:23`, wire `{token, expires_in}`) — internal GC↔AC contract, not an OAuth token-carrying browser-facing response; `expires_in` here is not an OAuth carve-out.
- `MeetingTokenClaims` / `GuestTokenClaims` `token_type` (`internal_tokens.rs`) — JWT *claims* inside the signed token, part of the unchanged semantic contract, not wire response keys.
- `user_service::RegistrationResponse` (`user_service.rs:25-33`) — DOES derive `serde::Serialize`, but is **never serialized to an HTTP wire response**: `auth_handler.rs:197-204` maps it field-by-field into the in-scope wire struct `UserRegistrationResponse` (the `Json`-wrapped one, getting the camel flip). Exclude on **"not wire-serialized / converted in handler,"** NOT on "no Serialize" (@semantic-guard correction — the false "no Serialize" note was a credential-leak trap: it carries `access_token`, so a future direct `Json`-wrap relying on that note would ship a snake_case `access_token` wire response). Left as-is, but the reason is the conversion boundary, not the absence of a derive.

**Scripts verified, NO change needed** (@operations independently swept + confirmed): `scripts/register-service.sh` parses only `.client_id`/`.client_secret` (RegisterServiceResponse — not an OAuth carve-out field). `scripts/test-oauth-integration.sh` only greps kubectl logs for the literal "token" string and never parses JSON token keys. The task named these as candidates; sweep shows neither consumes `access_token`/`token_type`/`expires_in`/`scope`. No no-op edits.

**What the carve-out reversal IS (scope precision, per @operations):** the change reverses the **OAuth token-response serde `rename` on the user/service token wire DTOs** (`TokenResponse`, `UserRegistrationResponse`, `UserTokenResponse`). It is distinct from `internal_tokens.rs`, where `token_type: "meeting"|"guest"` is a serialized VALUE (not a renamed key) and `expires_in` belongs to the separate internal meeting-token JSON struct — the diff must NOT touch the internal meeting-token wire.

**Out-of-scope Prometheus labels (do NOT flip — would break PromQL):** `mh-incident-response.md:174-175`, `mc-incident-response.md:1040,1212` use `token_type` as a metric label (`sum by(token_type)`, `mh_jwt_validations_total{token_type=...}`) — a different namespace from the OAuth JSON wire field. Left untouched.

**Pre-existing staleness — do NOT propagate, do NOT fix here:** `gc-deployment.md:881,956` POST to `/api/v1/auth/login`, but AC's actual routes are `/auth/user/token`, `/auth/register`, `/auth/service/token` (no `/auth/login`). Flagged by @operations as a separate follow-up; this task only flips the `.access_token`→`.accessToken` jq path on those lines and leaves the stale endpoint path as-is.

---

## Planning

### ⚖️ SCOPE RULING (user, relayed by @team-lead): option (A) — FULL FLIP + GSA co-sign [FINAL, 2026-06-19]
The ruling went through a relay mix-up (A → mis-read as B → user re-confirmed); the **DEFINITIVE final ruling is (A): "fix all" — flip ALL AC token endpoints incl. `/service/token`, with the GSA `OAuthTokenResponse` consumer flipped in lockstep under @security co-sign. NO spinout** (only the dt-guard `pii_vocabulary.rs` camel-`accessToken` addition is a security-accepted spin-out). The user's "env-tests" remark was just a note (verify coverage), NOT a scope reduction. The `/service/token` cluster (`TokenResponse` + GSA `OAuthTokenResponse` + env-tests `TokenResponse` mirror + all wiremock bodies) is **ACTIVE this devloop**.

⚠️ **STALE-(B) WARNING for the body below:** several Planning/plan-item blocks that follow (the env-tests "user-flow only", the "two-rule" story-prose item, the grep-audit "deferred /service/token" exclusion) were drafted during the brief (B) detour and read "(B) / two-rule / spun out". They are SUPERSEDED by this (A) ruling and by the §Implementation Summary / §Cross-Boundary table / §Files Modified, which are the authoritative (A) record. Where a body block says "(B)", read it as the historical (B) option; the implemented scope is (A).

### Approach (single rule: camelCase wire everywhere on AC)

**(1) Production DTOs.**
- `TokenResponse` (`models/mod.rs`): add clean `#[serde(rename_all = "camelCase")]` → wire `accessToken`/`tokenType`/`expiresIn`/`scope`. Rust field idents stay snake (they're not the wire).
- `UserRegistrationResponse` (`auth_handler.rs`): drop the three per-field `#[serde(rename = "...")]` overrides; the existing struct-level `rename_all = "camelCase"` then yields `accessToken`/`tokenType`/`expiresIn`. Update the snake-justifying comment.
- `UserTokenResponse` (`token_service.rs:187`): add clean `#[serde(rename_all = "camelCase")]` (currently a bare `Serialize` with no derive → fully snake today).

**(2) Lock tests flip to all-camel (stay load-bearing, pin the new shape).**
- `models/mod.rs::test_service_token_response_wire_shape_stays_snake` → RENAME to `test_service_token_response_wire_shape`; golden key-set → `{accessToken, tokenType, expiresIn, scope}`; rewrite the TRIPWIRE doc-comment to describe the single-rule camel invariant (delete the "MUST stay snake / RFC 6749 §5.1 load-bearing" prose).
- `auth_handler.rs::test_user_registration_response_wire_shape` → golden set `{userId, email, displayName, accessToken, tokenType, expiresIn}`; value spot-checks move to camel keys; comment → single-rule.
- `auth_handler.rs::test_user_token_response_login_wire_shape` → golden set `{accessToken, tokenType, expiresIn}`; comment → single-rule.
- `auth_handler.rs::test_user_registration_request_wire_shape` — REQUEST side, already camel (`displayName`); unaffected, but I'll re-read its comment for stale "OAuth carve-out" prose.
- `user_auth_tests.rs::test_register_wire_shape_golden_lock` → `golden_response_keys` flips the three OAuth keys to camel; the invariant pair INVERTS — assert camel forms PRESENT, snake forms ABSENT (the exact opposite of #46). Update the "mixed scheme is deliberate" prose to "single rule: all camelCase."

**(3) env-tests fixtures** (`fixtures/auth_client.rs`): mirror (1) — `TokenResponse` gets `rename_all = "camelCase"`; `UserRegistrationResponse` drops the three per-field renames. Keeps round-trip deserialization aligned with the new camel wire. (@test owns; coordinating.)

**(4) Runbooks** (@operations, paired) — ENDPOINT-CLASSIFIED A-vs-B split (@operations swept + classified every site by endpoint):

**User-flow sites — FLIP REGARDLESS of A-vs-B (`/register`, `/login`):**
- `gc-deployment.md` L884 (`/auth/login`), L952 (`/auth/register`), L959 (`/auth/login` retry inside register flow) — `jq -r '.access_token'` → `.accessToken`. **Flip ONLY the `.access_token` jq path; leave adjacent `.meetingId`/`.mcAssignment.mcId`/`.token` GC join-response parses untouched (separate already-camel contract); do NOT propagate the pre-existing `/auth/login` endpoint-path staleness on L881/L956 — flip the response key only.** Verified L884/952/959 endpoints by grep.

**/service/token sites — HELD under (B) (stay snake if B wins; flip only under A):**
- `ac-service-incident-response.md` L219/L420/L817 — ALL `/auth/service/token` → under (B) the ENTIRE FILE stays untouched.
- `ac-service-deployment.md` whole block — body L976-978, prose L988-990, comment L996, jq L999 — ALL `/auth/service/token` → under (B) the ENTIRE FILE stays untouched (and L988-990's "access_token, token_type, expires_in, scope" prose stays accurate for the still-snake /service/token, so no change needed — @operations confirmed).
- `gc-deployment.md` L849 — `/auth/service/token` → held snake under (B).

**@operations operability recommendation (my call, adopting it):** under (B), `gc-deployment.md` is legitimately MIXED on the same wire — L849 service-token stays `.access_token` while L884/952/959 user-flow become `.accessToken` (correct: different endpoint responses). Add a one-line `# Note:` near L849 stating the service-token response is snake_case by design (mirrors `common::token_manager`), so no operator "fixes" it later. Prevents a future incorrect sweep.

- **Scripts: no change** — @operations independently confirmed both are no-op (`register-service.sh` = `.client_id`/`.client_secret` only; `test-oauth-integration.sh` = `kubectl logs | grep` on log substrings incl. a `grep "service/token"` that matches a log line not a response key). Adding edits would be no-ops.

**(5) Story prose** (`browser-client-join.md`): R-11 (L70), R-53 (L211), design bullet (L263) → single rule "all wire fields camelCase, no per-field carve-outs." Clarification Question 6 (L473) disposition note updated for the post-#46 reversal. Add the deferred-`/oauth/token` paragraph.

### ⚠️ GATE-1 BLOCKER — `TokenResponse` flip is a live runtime break (consumer-compat finding)

@code-reviewer flagged that the task's change-(1) lists `TokenResponse` (the `/service/token` `client_credentials` endpoint) in the flip set, but the task's OWN rationale + R-11 scope §5.1 as "normatively load-bearing ONLY for /service/token." I ran the consumer-compat check:

**`crates/common/src/token_manager.rs:424-433` — `OAuthTokenResponse`** is a plain `#[derive(Deserialize)]` with NO `rename_all`; it deserializes AC's `/service/token` response using literal snake keys `access_token`/`token_type`/`expires_in` and reads `.access_token` at L689. This is the SHARED TokenManager used by all four internal services (GC/MC/MH + self) for the OAuth client_credentials flow.

**Consequence**: flipping server `TokenResponse` → camel WITHOUT flipping `OAuthTokenResponse` in lockstep = every service's token acquisition fails (serde missing-field on `access_token`). This is a runtime break, NOT a cosmetic flip. `crates/common` is NOT auth-controller's crate (shared/protocol domain) — flipping it is a cross-crate coordinated change beyond this task's single-crate scope, and it widens blast radius to all four services' auth path.

**Two readings (escalated to @team-lead for the call):**
- **(A)** Flip all three incl. `TokenResponse` — REQUIRES a lockstep `crates/common::OAuthTokenResponse` flip (+ its mock-server test JSON at L844+) and is a wire-runtime change across all services → NOT Mechanical; needs the cross-crate owner + security trailer.
- **(B)** Flip only the two USER-flow structs (`UserRegistrationResponse` + `UserTokenResponse`); leave `TokenResponse` + its `_stays_snake` lock untouched. This matches the task's OWN rationale ("§5.1 load-bearing only for /service/token") and R-11's existing carve-out for /service/token, and confines the change to AC's user-flow wire (the actual browser-SDK-facing surface the task cites as the motivation).

**PLAN POSITION: reading (B).** (@code-reviewer independently ran the consumer-compat grep and made this a hard finding, not a question; I concur.) The task's stated motivation is browser-SDK consumer confusion over the USER-flow mixed scheme; `/service/token` is service-to-service only (no browser SDK, no third-party) and its §5.1 conformance IS conceded load-bearing by the task's own words. (A) trades a live multi-service runtime break for cosmetic consistency on an endpoint no browser touches. Two independent reasons (A) is wrong, per @code-reviewer:
1. **Runtime break** — flipping `TokenResponse` makes AC emit `accessToken`; `OAuthTokenResponse` (required, non-default `access_token`/`expires_in`) fails deserialize → token acquisition errors at startup for GC/MC/MH. Matrix-(e) ("no snake `access_token` in production code outside REJECT-asserting tests") would itself be VIOLATED by this live deserializer — its field names ARE the contract, not a stale ref. You cannot satisfy (e) and keep services working without also flipping `OAuthTokenResponse`.
2. **GSA** — `crates/common/src/token_manager.rs` is in ADR-0024 §6.4 Guarded Shared Areas (auth/crypto primitives). Editing `OAuthTokenResponse` → NOT Mechanical (surface precedence, review-protocol §3); needs auth + security co-sign + arguably the four service owners (wire-runtime coupling across every service token path). Per ADR-0024 §6.2 a Mechanical→GSA upgrade AUTO-ROUTES to ESCALATE — not negotiable in-thread.

**Honest framing (@code-reviewer) — THIS IS THE LIVE (B) FRAMING:** under the FINAL (B) ruling the AC wire is a TWO-rule wire — user-flow camelCase (register/login) + the genuine OAuth `/service/token` endpoint staying snake_case per §5.1 (its consumer is the shared s2s `common::token_manager`, not the browser SDK). This is the correct, deliberate contract; the prose must NOT overstate it as "single rule, no carve-outs." The `/service/token` flip (+ the GSA `OAuthTokenResponse` edit) is SPUN OUT — see §Accepted Deferrals.

**Paired-reviewer divergence on A-vs-B (reinforces this is a user/Lead scope call):** @code-reviewer recommends **(B)** (keep `/service/token` snake; the flip exceeds Mechanical/single-crate scope). @security, as paired GSA reviewer, leans **(a)/(A)** — include the `token_manager.rs::OAuthTokenResponse` flip classified GSA / intersection-rule with the security co-sign trailer (`Approved-Cross-Boundary: auth-controller …` AND `Approved-Cross-Boundary: security …`), honoring the task's literal "single rule everywhere" intent. Both agree the two sides are ONE wire contract that must move together and that token_manager.rs is GSA / NOT Mechanical. The divergence is purely on scope (does #51 take on the GSA edit + four-service blast radius, or defer it). That's the user/Lead's call — escalated. If (A): @security can give the GSA hunk-ACK in-loop (paired), so it's executable within this loop IF re-scoped, but still needs @team-lead to authorize the expanded GSA scope + the four service owners' awareness of the coordinated wire move.

**STATUS: formal A-vs-B call HELD by @team-lead — escalated to the user** (narrow vs full-with-GSA-cosign). My plan takes (B); if the user rules (A), @team-lead re-scopes (it's a new GSA task), I do NOT ride it as Mechanical. @team-lead confirmed `token_manager.rs` is GSA. Until the ruling: DO NOT touch `TokenResponse`, its `_stays_snake` lock, the env-tests `TokenResponse` mirror, or `token_manager.rs`. My Cross-Boundary table contains NO `token_manager.rs` row (the tell that (A) crept in) — confirmed absent, BUT if (A) is ruled I will ADD that row classified **GSA / intersection-rule** (NOT Mechanical) with the security co-sign. The two user-flow structs below are unblocked (consumers = browser SDK + env-tests fixtures, no GSA) and ready regardless.

**STATUS: HELD by @team-lead — escalated to the user for the scope call (narrow vs full-with-GSA-cosign).** @team-lead additionally confirmed `crates/common/src/token_manager.rs` is a GSA path (auth/crypto primitive) → option (A) needs security + owner co-sign and exceeds #51's stated single-crate scope. Until the ruling: DO NOT touch `TokenResponse`, its `_stays_snake` lock, the env-tests `TokenResponse` mirror, or `token_manager.rs`. The two user-flow structs below are unaffected by the blocker (consumers = browser SDK + env-tests fixtures, no GSA) and are ready to execute regardless of the decision.

### The `TokenResponse` cluster — ATOMIC, A/B-gated (per @dry-reviewer; resolves the token_manager.rs scope-completeness hold)

`token_manager.rs::OAuthTokenResponse` is NOT a separate concern from the `TokenResponse` A/B decision — it is the THIRD member of the SAME wire cluster, gated by the SAME ruling. Verified production path: token_manager.rs:643 hits `/api/v1/auth/service/token`, :655 sends `grant_type: client_credentials`, :673 deserializes the response into `OAuthTokenResponse`. That IS AC's `TokenResponse` wire, consumed by GC/MC/MH. The cluster:

  **(1) AC `TokenResponse`** (serialize, `models/mod.rs`)
   ↔ **(2) `common::token_manager::OAuthTokenResponse`** (DESERIALIZE, `token_manager.rs:425` — production runtime path, GSA)
   ↔ **(3) env-tests `TokenResponse` fixture** (deserialize, `auth_client.rs:62`)

**REQUIRED SAFETY WORDING (@security + @dry-reviewer, verbatim — this clause is the safety mechanism, not doc hygiene; it stops a future hurried clone from treating the deserializer flip as optional under (A)):**
> `token_manager.rs::OAuthTokenResponse` moves atomically with AC `TokenResponse` + the env-tests fixture under the A/B ruling. It is the ONE cluster member invisible to the entire Gate-2 matrix — server side is caught by lock tests, fixture side by env-tests, but the common deserializer is exercised by neither until live-cluster bring-up. Omission under (A) = a GREEN Gate 2 followed by a silent fail-closed fleet auth outage at deploy, NOT a caught regression.

**All three move TOGETHER on the A/B ruling — atomic, never one-sided:**
- **Under (B):** all three STAY snake — (1) no `rename_all`, (2) no `rename_all` (deliberately UNCHANGED-because-B, named here so the next reader sees the cluster was kept whole on purpose, not silently omitted), (3) fixture stays snake. Three-way aligned, ZERO change to any member, ZERO fleet risk. The scope-completeness concern dissolves under (B).
- **Under (A):** all three flip camel together. (2) `OAuthTokenResponse` specifically REQUIRES: `rename_all="camelCase"` + a Cross-Boundary Classification ROW + GSA owner trailers (`Approved-Cross-Boundary: auth-controller …` AND `Approved-Cross-Boundary: security …`) + a deserialize wire-shape test (parses camel, rejects snake). If (2) is left behind it fails the fleet CLOSED at live-cluster bring-up — and it's the one member that slips the entire Gate-2 matrix (matrix a-d never exercise token_manager against a real AC response; @security's cross-crate-blind-spot). @code-reviewer's masking-risk: flipping (3) the fixture without (2) makes env-tests green while prod breaks — so (1)+(2)+(3) flip together or none do.

This callout makes the cluster atomic in the plan; `OAuthTokenResponse` is now a NAMED held member (conditional-on-A, no-op-under-B), closing the omission that let it drop the first time.

### Implementation spec — UNBLOCKED user-flow scope (executes regardless of A/B ruling)

**Production (2 structs):**
- `UserRegistrationResponse` (`auth_handler.rs:46-59`): delete the three per-field `#[serde(rename = "access_token"|"token_type"|"expires_in")]` overrides + the snake-justifying comment at :52. Struct-level `rename_all = "camelCase"` (already present) then yields `accessToken`/`tokenType`/`expiresIn`. Rust idents unchanged.
- `UserTokenResponse` (`token_service.rs:187-191`): ADD `#[serde(rename_all = "camelCase")]` (nothing to remove — plain snake today).

**Lock tests (3, user-flow only):**
- `auth_handler.rs::test_user_registration_response_wire_shape` → golden `{userId, email, displayName, accessToken, tokenType, expiresIn}`; camel value spot-checks; rewrite doc-comment rationale from "§5.1 carve-out / STAY snake" to single-rule-camel + task #51/R-11 pointer.
- `auth_handler.rs::test_user_token_response_login_wire_shape` → golden `{accessToken, tokenType, expiresIn}`; rewrite doc-comment (drop "do NOT fix to camelCase").
- `user_auth_tests.rs::test_register_wire_shape_golden_lock` → `golden_response_keys` flips the 3 OAuth keys to camel; invariant pair INVERTS (camel PRESENT, snake ABSENT); credential-echo guards untouched; rewrite "mixed scheme is deliberate" prose to single-rule.
- (`test_user_registration_request_wire_shape` — request side, already camel; only stale-comment sweep.)

**Doc-comment rewrites must NAME THE INVARIANT, not just delete old prose (@test, applies to ALL flipped locks incl. the (A) `models/mod.rs` tripwire):** each rewritten tripwire states WHY the shape is deliberate — "single-rule camelCase, no per-field exceptions, R-11 as amended by task #51; a future snake reintroduction trips this" — so the next sweeper sees intent, not a bare assertion. Do NOT just strip the RFC-6749 prose and leave a naked golden set.

**Bidirectional locks (per @test, matching task #49 GC pattern):** port the `assert_no_snake_keys(value, ctx)` helper from `gc-service/src/models/mod.rs:987` into the AC lock tests so each flipped user-flow lock explicitly asserts BOTH (a) camel key-set equality AND (b) no serialized key contains `_`. The current AC tests have only `wire_keys()` BTreeSet-equality (implicit absence) — adding the explicit reject avoids a one-sided assertion. **@security refinement:** BTreeSet set-equality already gives "snake absent" for free, but the assertion MESSAGE must call out that the snake-rejection is INTENTIONAL not incidental — so a future reader sees it's a deliberate contract guard. Matches the register-REQUEST lock discipline.

**env-tests sweep CONFIRMED complete (@test):** the fixture structs `TokenResponse` + `UserRegistrationResponse` are the ONLY env-tests surface. All other consumers read typed fixture fields (`token_response.access_token` etc. in 20/21/25*.rs) — Rust idents stay snake post-flip, so zero change there. Explicitly LEAVE: `25_auth_security.rs:60` `claims["scope"]` (JWT-claim forgery mutation, not a wire key) and `gc_client.rs:141` Debug `expires_in` (GC fixture, out of scope).

**Integration runtime reads — AUTHORITATIVE inventory (per @test + @code-reviewer; cross-file sweep run, see below). These break at RUNTIME (JWT-extraction `unwrap`/`expect` panics on missing key), not just key-set:**
COMPLETE user-flow flip set in `user_auth_tests.rs` (all register `/api/v1/auth/register` or login `/api/v1/auth/user/token`):
- `:61` `body.get("access_token")`, `:66` `body["token_type"]`, `:67` `body["expires_in"]` (register asserts) → camel.
- `:170` register golden-lock `for snake in ["access_token","token_type","expires_in"]` — INVERTS under (B): assert these ABSENT + camel forms PRESENT (the test #88 `test_register_wire_shape_golden_lock` inversion; call out :170 specifically so the loop isn't missed).
- `:214`, `:269` register JWT extraction `body["access_token"]` (`test_register_token_has_user_claims`, `test_register_assigns_default_user_role`) → `body["accessToken"]`. LOAD-BEARING — panics if missed.
- `:655` `body.get("access_token")`, `:658` `body["token_type"]`, `:659` `body["expires_in"]` (login asserts) → camel.
- `:690` login JWT extraction (`test_login_token_has_user_claims`) → camel.
- `:1017` `/auth/user/token` extraction (`test_org_extraction_valid_subdomain`) → camel.
- The `.expect("Should have access_token")` MESSAGE strings (:216/:271/:692/etc.) are cosmetic — flip for consistency, but non-load-bearing.

**Cross-file sweep across `crates/ac-service/tests/**` — NEGATIVE CONFIRMATION (@code-reviewer asked for it explicitly):** grepped `["access_token"]`/`.get("access_token")`/`["token_type"]`/`["expires_in"]`. ZERO user-flow hits outside `user_auth_tests.rs`. The only other file with hits is `internal_token_tests.rs` — ALL of which are out-of-scope and STAY snake: `body["expires_in"]` at :398/:408/:449/:658/:668/:705/:969/:1013 (InternalTokenResponse `{token, expires_in}` wire) and `claims["token_type"]` at :1080/:1156 (JWT claims inside the signed token). None are register/login user-flow. So `user_auth_tests.rs` is the complete user-flow integration surface.

**env-tests fixture (user-flow only):** `auth_client.rs:UserRegistrationResponse` (L252-265) drops its 3 per-field renames in lockstep; update the mirror doc-comment (L249-251, L258) that says "mixed scheme / preserved snake_case" → single-rule. **@dry-reviewer apply-time guards (non-blocking, recorded):** (i) the fixture's hand-rolled `Debug` impl (L267-278) and redaction test (L313) reference `access_token`/`token_type` as RUST FIELD IDENTS, not wire keys — dropping the serde renames must NOT touch them; no global rename sweep over this file. (ii) flip the mirror-attribution doc-comment prose to single-rule or it will lie about the new shape. No `TODO.md` edit needed — its §Cross-Service Duplication 2026-05-23 entry describes the mirror RELATIONSHIP, not the scheme. **Under (B) the env-tests row is `UserRegistrationResponse` mirror ONLY — env-tests `TokenResponse` (L63-68) is UNTOUCHED** (it's a second live snake-key deserializer of `/service/token`; it tracks the server's real shape, which stays snake under (B)). (@code-reviewer verified this is a 2nd snake consumer — see the (A) masking-risk warning below.)

**Login-lock naming confirmed:** the actual test is `test_user_token_response_login_wire_shape` (the task brief's `test_user_token_response_wire_shape` is shorthand) — that's the one I'm flipping.

**Story prose — FINAL (B) ruling, honest TWO-rule framing (this is the LIVE instruction):** R-11 (L70) / R-53 (L211) / design-bullet (L263) / Q6 (L473) amend to: **"All USER-FLOW wire fields camelCase (`/register` + `/login`); the genuine OAuth `/service/token` client_credentials endpoint STAYS snake_case per RFC 6749 §5.1 (its consumer is the shared s2s `common::token_manager`, not the browser SDK) — the one documented, consumer-justified boundary."** The reversal removes the USER-FLOW per-field mixed scheme ONLY; the `/service/token` snake stays (its single-rule flip is SPUN OUT). Deferred-`/oauth/token` note RETAINED: any future strict-RFC-6749 third-party integration goes behind a separate `/oauth/token` endpoint, never by reintroducing per-field carve-outs. (Done in the actual story file — verified two-rule.)

### `/service/token` deltas — ⛔ SPUN OUT (NOT this devloop; ready-made spec for the deferred GSA task)
**These deltas are NOT executed here.** The FINAL ruling is (B): `/service/token` stays snake; this cluster is deferred to its own GSA-co-signed task (docs/TODO.md + §Accepted Deferrals). The spec below is retained as the ready-to-execute plan for that task — it captures the atomic-cluster mechanism, the GSA classification, the security co-sign, the cross-crate blind-spot guard, and the verified wiremock span. DO NOT touch any of these files in THIS devloop.

**Correct mechanism for a wire-NAME flip = "every serde participant in this wire contract, serializer AND deserializer" (@security).** The `/service/token` contract has exactly ONE deserializer: `common::token_manager::OAuthTokenResponse`. It must move in lockstep or the contract breaks. The two sides are inseparable — the deferred flip is "flip the whole `/service/token` contract," never just the server struct.

- `TokenResponse` (`models/mod.rs`): add `rename_all = "camelCase"`.
- `models/mod.rs::test_service_token_response_wire_shape_stays_snake` → rename + flip golden to camel.
- **`crates/common/src/token_manager.rs:424-433` `OAuthTokenResponse` — GSA (ADR-0024 §6.4 enumerated: jwt.rs/meeting_token.rs/token_manager.rs/secret.rs), NOT Mechanical, intersection-rule co-sign (`Approved-Cross-Boundary: auth-controller …` AND `Approved-Cross-Boundary: security …`; @security gives the hunk-ACK in-loop, paired):** add `rename_all = "camelCase"`. Redacting Debug at :435 keeps `access_token` field IDENT (redaction unaffected — wire derive only). Required non-`Option` fields `access_token`/`expires_in` are why a one-sided flip fails closed → `TokenError::InvalidResponse` → empty refresh loop → GC/MC/MH cannot acquire service tokens (fleet-wide s2s auth outage). Add a deserialize round-trip test: parses camel, REJECTS snake (negative-assertion lock, mirrors the others). **Flip ALL wiremock JSON keys across the FULL span L844–L1756 (44 key occurrences total: 16× `"access_token"`, plus the paired `"token_type"`/`"expires_in"` in each mock body) → camelCase.** ⚠️ RANGE CORRECTION: @team-lead's note approximated "~L837–1075" but the actual wiremock bodies extend to **L1756** — the L1075 cap would MISS ~half the mock bodies (e.g. :1242/:1296/:1337/:1599/:1650/:1699/:1754), and a missed mock body fails `OAuthTokenResponse` deserialization → token_manager test failure. Use the verified full span. Authoritative grep: `grep -n '"access_token":\|"token_type":\|"expires_in":' crates/common/src/token_manager.rs` (44 hits, L844–L1756). No `"scope"` key in any mock body (it's `#[serde(default)]`, absence is fine). The `.field("access_token", …)` Debug labels at :438-441 are Rust idents — leave (or tidy, per @team-lead), NOT wire keys.
- **Cross-crate-blind-spot WARNING (@security, why the verification matrix misses (A)'s break):** matrix (a) `cargo test -p ac-service`, (b) lock tests, (c) env-tests round-trip, (d) AC integration ALL exercise the AC side + the env-tests mirror — NONE exercise `common::token_manager` against a real AC response. Under (A) the break is invisible until a live cluster brings up GC/MC/MH — the cross-crate variant of the task #46 DB-gated false-CLEAR. The `OAuthTokenResponse` reject-test above is the guard that closes this blind spot; it MUST be part of (A).
- `auth_client.rs:TokenResponse` mirror (L63-68, a SECOND live snake deserializer): add `rename_all = "camelCase"`. **MASKING-RISK WARNING (@code-reviewer):** this mirror MUST flip in the SAME lockstep as the production `OAuthTokenResponse`, never ahead of it. If the env-tests mirror flips to camel while `OAuthTokenResponse` stays snake, env-tests pass GREEN while production breaks — the green test actively MASKS the fleet-wide auth outage. The mirror must track the server's REAL shape, not a hypothetical one. So under (A): server `TokenResponse` + `OAuthTokenResponse` (prod) + `auth_client.rs:TokenResponse` (env-tests) all flip together, or none do.
- R-11/R-53 prose: drop the `/service/token` snake carve-out entirely (true single rule, no exceptions).
- **Service-token runbook hunks (move ONLY under A — @operations complete site list):** `ac-service-deployment.md` L976-980 (response block) + L988-990 (success-criteria prose) + **L996 (`# Extract access_token from response` comment — flip wording too, same hunk as L999)** + L999 (jq). `ac-service-incident-response.md` ALL THREE sites: L219, L420, **L817** (the far-down one easy to miss — security's heads-up listed only two). `gc-deployment.md` L849 (service-token jq).

### Grep-audit (e) caveat for Gate 2 — under the FINAL (B) ruling
The audit target is **OAuth token-carrying HTTP wire response fields on the USER-FLOW endpoints** (register + login). Under (B), residual snake `access_token`/`token_type`/`expires_in`/`scope` identifiers REMAIN — correctly — in these deliberately-excluded / deferred places:
- **The `/service/token` pair (`TokenResponse` in models/mod.rs + its GSA consumer `OAuthTokenResponse` in token_manager.rs) — DEFERRED (spun out), STAYS snake.** Its `test_service_token_response_wire_shape_stays_snake` lock KEEPS its name and snake golden. These snake fields are the LIVE §5.1 contract for the service-token endpoint — matrix-(e) must read as "outside the deliberately-snake /service/token surface too," else (e) self-contradicts (you cannot keep the s2s fleet working AND remove these without the spun-out GSA flip).
- `internal_tokens.rs` (JWT claims, inside the signed token).
- `InternalTokenResponse` (internal GC↔AC `{token, expires_in}` contract — NOT an OAuth token-response).
- `user_service::RegistrationResponse` (derives `Serialize` but is converted field-by-field in `auth_handler.rs:197-204` and never wire-serialized — excluded on the conversion boundary, NOT on "no Serialize").

Documenting this so Gate 2 doesn't false-positive on the deliberately-snake `/service/token` surface or the three internal surfaces.

**IDENT-vs-WIRE clarification (@security, applies regardless):** matrix-(e) greps source for snake `access_token`/`token_type`/`expires_in`. But `rename_all = "camelCase"` flips the WIRE keys, NOT the Rust field IDENTS — idiomatically the idents stay snake (the `userId`/`displayName` precedent; confirmed idiomatic by @code-reviewer). So EVERY flipped struct STILL contains snake Rust idents in source after the flip: `UserRegistrationResponse`/`UserTokenResponse` fields, the `auth_handler.rs:197-204` field-mapping, and the `RegistrationResponse` conversion. Matrix-(e)'s "no snake `access_token` in production code" must be read as **"no snake `access_token` as a WIRE KEY (serde `rename`/serialized output), not as a Rust field identifier."** Gate-2's grep WILL hit the idents; that is EXPECTED and correct — the audit passes when no snake survives as a USER-FLOW wire key (serde attr or emitted JSON), which the lock tests positively assert.

---

## Pre-Work

None.

---

## Implementation Summary

Scope: (A) FULL FLIP + GSA co-sign, NO spinout. Removed the per-field OAuth snake_case carve-out across
ALL AC token endpoints (user-flow AND `/service/token`); every token-response wire key is now camelCase.
The GSA consumer `OAuthTokenResponse` flipped in lockstep. Token semantic contract unchanged — naming-only.

- **Production DTOs:** `UserRegistrationResponse` (`auth_handler.rs`) dropped its 3 per-field
  `#[serde(rename = …)]` overrides; `UserTokenResponse` (`token_service.rs`) + `TokenResponse`
  (`models/mod.rs`) gained clean `#[serde(rename_all = "camelCase")]`. Rust field idents stay snake.
- **GSA consumer (`crates/common/src/token_manager.rs`):** `OAuthTokenResponse` gained
  `#[serde(rename_all = "camelCase")]` so it parses the camel wire; redacting Debug + consuming code
  unchanged (Rust idents). Flipped ALL wiremock JSON bodies to camel (L844–L1756 span). The two negative
  tests handled with semantic care: `test_missing_oauth_fields` keeps the required key ABSENT but the
  present keys camel (isolates "missing required field" not "all-shape-wrong"); `test_empty_token_in_response`
  → `accessToken: ""`. Added `test_oauth_token_response_wire_shape_camel` — the deserialize-side
  reject-test (parses camel, rejects snake) that closes the cross-crate blind spot. Ships with the
  `Approved-Cross-Boundary: security …` trailer (GSA, NOT Mechanical).
- **Lock tests (auth_handler.rs):** ported `assert_no_snake_keys`; flipped both user-flow response
  goldens to camel + bidirectional reject + rewrote doc-comments. **(models/mod.rs):** RENAMED
  `test_service_token_response_wire_shape_stays_snake` → `_wire_shape`, golden → camel, §5.1 "stays snake"
  prose REPLACED with the single-rule invariant + no-snake reject.
- **Integration (user_auth_tests.rs):** flipped register/login asserts + 4 JWT-extraction reads; inverted
  the golden-lock invariant pair (camel PRESENT, snake ABSENT) + golden set + doc-comment; credential-echo
  guards untouched byte-for-byte.
- **env-tests (auth_client.rs):** BOTH mirrors flip — `UserRegistrationResponse` (drop 3 renames) AND
  `TokenResponse` (add `rename_all`). Debug/redaction tests untouched (Rust idents). Added the DB-free
  `UserRegistrationResponse` round-trip lock (parses camel, rejects snake) per the user's env-tests note.
- **Runbooks:** FULL set flips to camel — `gc-deployment.md` (L849 service-token + L884/952/959 user-flow;
  removed the now-wrong snake-by-design note), `ac-service-deployment.md` (response block + success-criteria
  prose + extraction jq), `ac-service-incident-response.md` (3 jq sites). Scripts: no-op.
- **Story prose:** R-11 / R-53 / design-bullet / Q6 → genuine SINGLE rule (all AC wire camelCase, no
  carve-out) + deferred-`/oauth/token` note retained.
- **Spin-out (security-accepted):** docs/TODO.md entry for the dt-guard `pii_vocabulary.rs` camel
  `accessToken` addition (guard-owned, deferred to TS-SDK/task #11). `pii_vocabulary.rs` NOT edited.

---

## Files Modified

- `crates/ac-service/src/models/mod.rs` — `TokenResponse` add `rename_all`; lock test renamed `_wire_shape` + golden→camel + reject.
- `crates/ac-service/src/handlers/auth_handler.rs` — `UserRegistrationResponse` (drop 3 renames) + `assert_no_snake_keys` helper + 2 lock tests flipped.
- `crates/ac-service/src/services/token_service.rs` — `UserTokenResponse` add `rename_all`.
- `crates/common/src/token_manager.rs` — **GSA**: `OAuthTokenResponse` add `rename_all` + all wiremock bodies camel + 2 negative tests semantic-care + new deserialize reject-test.
- `crates/ac-service/tests/integration/user_auth_tests.rs` — golden lock flip + invariant inversion + register/login asserts + 4 JWT-extraction reads.
- `crates/env-tests/src/fixtures/auth_client.rs` — BOTH `TokenResponse` + `UserRegistrationResponse` mirrors flip + doc-comments + round-trip lock test.
- `docs/runbooks/gc-deployment.md` — all OAuth jq sites → camel (snake-by-design note removed).
- `docs/runbooks/ac-service-deployment.md` — service-token response block + prose + jq → camel.
- `docs/runbooks/ac-service-incident-response.md` — 3 jq sites → camel.
- `docs/user-stories/2026-05-02-browser-client-join.md` — R-11 / R-53 / design-bullet / Q6 single-rule.
- `docs/TODO.md` — dt-guard `pii_vocabulary.rs` camel-`accessToken` follow-up (replaces the deferred /service/token entry, now done).
- `docs/devloop-outputs/2026-06-19-reverse-oauth-snake-carveout-task51/main.md` — this file.

**Deliberately NOT modified:** `crates/dt-guard/src/common/pii_vocabulary.rs` (guard-owned, security-accepted spin-out to TS-SDK time). Scripts (`register-service.sh`, `test-oauth-integration.sh`) — no OAuth token-key parsing (no-op).

---

## Devloop Verification Steps

Gate 2 — `./scripts/layer-all.sh` (attempt 2, after fmt/clippy/secret-scanner fixes). Substantively GREEN:

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile | OK | cargo build + dt-guard + nx typecheck |
| 2 Format | OK | (attempt-1 fmt fail at token_manager.rs fixed) |
| 3 Guards | OK | all 33 guards (attempt-1 fails fixed: classification owner-cell + todo-tracking inline-body in main.md by Lead; no-hardcoded-secrets via `placeholder_jwt` indirection) |
| 4 Test | OK* | all 378 ac-service + 49 common/token_manager real tests pass. *Lone pipeline failure `test_issue_user_token_timing_attack_prevention` = timing-variance flake (32.5% under parallel load); re-ran 3/3 green isolated; `token_service.rs` timing logic untouched by diff |
| 5 Lint | OK | (attempt-1 clippy doc_markdown at token_manager.rs:423/427 fixed) |
| 6 Audit | cargo-audit OK | `pnpm-audit` FAIL = pre-existing accepted standing deferral (ADR-0033 §10); zero JS deps changed |
| 7 Env-tests | N/A | `wave2-pending` — live env-test not wired in this env (intentional, self-justifying) |

Task verification matrix (all pass, per @test + @code-reviewer independent runs):
- (a) `cargo test -p ac-service` — lib 378/378, integration register/login 13/13 (real DB) ✓
- (b) wire-shape locks all-camel + reject snake bidirectionally (`assert_no_snake_keys` on user-flow locks; inline reject on service-token lock; golden invariant inverted; new `OAuthTokenResponse` deserialize reject-test camel-OK/snake-Err) ✓
- (c) env-tests round-trip — both fixtures flipped in lockstep + new round-trip lock + redaction green ✓
- (d) AC integration tests pass ✓
- (e) grep audit — zero stray OAuth wire-key survivors in production; remaining snake = legit excluded surfaces (reject-test fixtures, InternalTokenResponse `{token,expires_in}`, JWT claim values, Prometheus `token_type` labels, PII-vocab) ✓

---

## Code Review Results

| Reviewer | Verdict | Findings | Notes |
|----------|---------|----------|-------|
| Security | RESOLVED-DEFERRED | 1 fixed, 1 spin-out | GSA hunk-ACK on `token_manager.rs` GRANTED (6-point verify); `Approved-Cross-Boundary: security` trailer on commit; §6.4 single co-sign sufficient (no proto crossing). Finding (3 stale (B)-residue comments) fixed. 1 accepted spin-out: pii_vocabulary.rs `accessToken` → #11 |
| Test | RESOLVED-FIXED | 1 fixed | Verification matrix (a)-(e) all pass; finding (3 stale comments) fixed in-changeset; timing-flake concurred |
| Observability | CLEAR | 0 | Metrics/log/trace-neutral; `OAuthTokenResponse` Debug redaction preserved |
| Code Quality | RESOLVED-FIXED | 1 fixed | ADR-0003/0020/0024 compliant; Ownership Lens records GSA hunk + co-sign; lockstep genuine; matrix-e grep clean; finding fixed |
| DRY | RESOLVED-DEFERRED | 0 / 1 spin-out | Cluster flipped atomically (no one-sided flip); no true duplication; wire-key-set helper TODO refreshed; 1 accepted spin-out: pii_vocabulary.rs → #11 |
| Operations | RESOLVED-FIXED | 0 | Full (A) runbook flip correct + complete; zero snake residue; scripts no-op; no L849 note (uniform camel) |
| Semantic Guard | CLEAR (native SAFE) | 0 | Credential-leak/actor/error-context/metrics all clear; secret-fixture fix non-vacuous (strictly stronger) |

**No ESCALATED verdicts.** Two RESOLVED-DEFERRED (Security, DRY) for the same single accepted spin-out: the `pii_vocabulary.rs` camel-`accessToken` vocab addition, deferred to TS-SDK time (#11), guard/security-owned — tracked in `docs/TODO.md` §Cross-Service Duplication. GSA co-sign for `token_manager.rs` carried as the commit trailer.

---

## Accepted Deferrals

- `docs/TODO.md` §Cross-Service Duplication (DRY) — pii_vocabulary CATEGORY_A: add camel `accessToken` at TS-SDK time (#11)

---

## Rollback Procedure

1. Start commit: `78b4d321c9681256d112fdb61baa4a2188380834`
2. Review: `git diff 78b4d32..HEAD`
3. Soft reset: `git reset --soft 78b4d32`
4. Hard reset: `git reset --hard 78b4d32`
No schema/infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

### Issue 1: TokenResponse flip has a GSA consumer the task omitted
**Problem**: Flipping `TokenResponse` (`/service/token`) to camel breaks `common::token_manager::OAuthTokenResponse` — a required-snake-key deserializer that is the service-to-service token path for all four services, and an ADR-0024 §6.4 GSA (auth/crypto primitive). Task #51 named `TokenResponse` to flip but never mentioned this consumer and scoped out client wire changes.
**Resolution**: Lead verified the consumer, escalated the scope contradiction to the user. User ruled (A) full flip — flip `OAuthTokenResponse` in lockstep as a security-co-signed GSA edit (no spinout). The new `test_oauth_token_response_wire_shape_camel` deserialize reject-test closes the cross-crate blind spot that would otherwise pass Gate-2 green and break only at live-cluster bring-up.

### Issue 2: A→B→A scope-ruling churn + stale relay echoes
**Problem**: An ambiguous AskUserQuestion answer was mis-read as (B)+spinout (briefly approved); the user then definitively confirmed (A) "fix all". The intermediate (B) relay propagated through reviewers out of order, generating repeated "revert to (B)" relays at the implementer.
**Resolution**: Implementer correctly refused to revert the security-co-signed GSA edit on second-hand relays and asked for direct confirmation each time. Lead anchored (A) directly and re-synced reviewers; all self-corrected. Zero rework — the held line meant the (A) code was never destabilized.

### Issue 3: Gate-2 attempt-1 failures (all mechanical)
**Problem**: Layer 2 fmt (token_manager.rs:788), Layer 3 guards (classification owner-cell wording, todo-tracking inline-body in main.md, no-hardcoded-secrets on a test fixture), Layer 5 clippy doc_markdown (token_manager.rs:423/427).
**Resolution**: Lead fixed the two main.md doc-guard issues (canonical single-owner cell; pointer-bullet deferral). Implementer fixed the three token_manager.rs items (cargo fmt; backticks; `placeholder_jwt` variable-indirection per the models/mod.rs precedent + a strengthened non-vacuous redaction assertion).

### Issue 4: Layer-4 timing-test flake
**Problem**: `test_issue_user_token_timing_attack_prevention` failed once (32.5% bcrypt-timing variance under full-pipeline parallel load).
**Resolution**: Re-ran 3/3 green in isolation; the test is in `token_service.rs` timing logic untouched by the diff. Confirmed flake, not a regression.

### Issue 5: Stale (B)-residue comments (Gate-3 finding)
**Problem**: Three doc-comments (`auth_handler.rs:744`, `:798`, `user_auth_tests.rs:90`) still claimed `/service/token` "stays snake / deferred to its own task," contradicting the (A) flip in the same diff.
**Resolution**: Rewritten to the (A) truth (flipped, no carve-out remains). Re-verified by security + test; finding cleared.

---

## Lessons Learned

1. **A wire-shape flip must move its consumer deserializer in lockstep.** The serialize side (server `TokenResponse`) and the deserialize side (`OAuthTokenResponse`) are one contract; flipping one without the other is a fleet-wide auth break invisible to per-crate tests. The deserialize-side reject-test is the load-bearing guard against the masking risk.
2. **A task that names a struct to flip may omit its cross-crate/GSA consumers** — restate the change in mechanism-language ("every producer AND consumer of this wire") and sweep for deserializers, not just the named producers.
3. **Never revert a security-co-signed GSA edit on a second-hand relay.** The implementer's discipline — holding and requiring direct authority — protected the work through the messaging churn. Only direct Lead authority changes a ruling.
4. **The cross-boundary classification guard parses the Owner cell as a single canonical manifest name** — verbose "owner + co-sign (trailer)" text trips `owner_not_in_manifest`. Keep the co-sign in prose + the commit trailer, not the table cell.
5. **Timing-attack tests are flaky under parallel pipeline load** — retry-to-confirm in isolation before treating a Layer-4 failure as a regression.
6. **An AskUserQuestion non-selection with a side note is not a reversal of a prior explicit selection** — re-confirm rather than infer a flip.
