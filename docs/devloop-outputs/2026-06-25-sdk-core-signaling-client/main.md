# Devloop Output: sdk-core SignalingClient (R-16, R-17, R-18, R-19)

**Date**: 2026-06-25
**Task**: WebTransport signaling client for the browser SDK — bidi to MC, JoinRequest, 4-byte BE framing on ClientMessage/ServerMessage, trace_parent/trace_state injection, JoinResponse parse + typed events, ErrorMessage→SignalingError mapping; ~15 unit tests with MockWebTransport.
**Specialist**: client
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task-13`
**Duration**: ~95m (team spawn → commit)

User story: `docs/user-stories/2026-05-02-browser-client-join.md` task #13 (deps #11, #12, #31 — all complete).

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5003d0802d8156d9b96baded514414b764b73e1a` |
| Branch | `feature/browser-client-join-task-13` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `client` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-FIXED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `RESOLVED-DEFERRED` |
| DRY | `RESOLVED-DEFERRED` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `CLEAR` |

**Gate 3 (Final Approval): all 7 verdicts in, no ESCALATED.** Security/Test/Observability/Operations RESOLVED-FIXED (4 review findings fixed in-diff: PII-example [L3], recordException server-text→telemetry, join-timeout silent-hang, success-path span-end assertion); Code Quality + DRY RESOLVED-DEFERRED (proto-gen glob spin-out to protocol/infra + DRY extract-on-second-use candidate, both tracked in docs/TODO.md); Semantic Guard CLEAR.

**Gate 2 (re-validation over review fixes): PASSED** 2026-06-25. `LAYER_ALL_EXIT=0` — L1–L5 OK (L4 = 129 tests incl. the 3 new review-fix tests), L6/L7 N/A (documented). Required two intermediate iterations cleared by Lead, both doc-hygiene (not code): the consolidated run flagged an `inline_debt_body` in THIS main.md's §Accepted Deferrals (multi-line bodies → converted to one-line `docs/TODO.md` pointers). Gate 3 fully satisfied: 7/7 verdicts (no ESCALATED) + clean pipeline. Proceeding to commit.

**Gate 1 (Plan Approval): PASSED** 2026-06-25 — all 7 reviewers confirmed; classification-sanity guard `STATUS=OK` (`dt-guard` debug+release binaries built locally). "Plan approved" sent to @implementer.

**Gate 2 (Validation): PASSED** 2026-06-25 (attempt 2). `LAYER_ALL_EXIT=0` — L1 OK, L2 OK, L3 OK, L4 OK (126 tests), L5 OK, L6 N/A (audit always-run subset passed: cargo-audit/pnpm-audit/buf-breaking OK), L7 N/A (env-test wiring is a registered intentional-gap placeholder, `wave2-pending`; browser-E2E hookup is task #18/#19 scope). Attempt 1 had 3 Layer-3 guard findings: (1) `no-pii-in-logs-ts` JSDoc-example logging a participant name → fixed by @implementer (opaque `participantId`); (2) `validate-cross-boundary-scope` planned-but-untouched `src/proto/**` row → moved to a note; (3) `validate-todo-tracking` pre-existing `inline_debt_body` in task #12 main.md → fixed standalone in commit `f7b2980` (not task #13's changeset). "Start Review" dispatched to all 7 reviewers.

---

## Task Overview

### Objective
Implement `SignalingClient` in `packages/sdk-core/src/signaling/` — the WebTransport signaling layer that connects the browser SDK to the Meeting Controller (MC):

- **R-16**: Open a WebTransport bidi stream to MC's `webtransport_endpoint`; send `JoinRequest` (meeting JWT + capabilities + empty `correlation_id`/`binding_token` for first join) wrapped in a `ClientMessage` envelope; apply 4-byte big-endian length-prefix framing (reuse `framing/length-prefix.ts`; max frame ≤ 64 KiB = MC `MAX_MESSAGE_SIZE`).
- **R-17**: Parse `JoinResponse` → typed `onJoined` event (`participant_id`, `user_id` as BigInt, `existing_participants`, `media_servers`); store `correlation_id`/`binding_token` (storage only). Parse subsequent `ServerMessage` variants → typed `ParticipantJoined`/`ParticipantLeft` events; other variants decoded + debug-logged only.
- **R-18**: `ErrorMessage` → typed `SignalingError` event with proto `ErrorCode` mapped to an SDK error code; auth-class errors close the connection with a typed reason. Frame parser enforces max-frame and handles partial reads buffered across chunks (reuse `FrameDecoder`).
- **R-19**: Populate `trace_parent`/`trace_state` on every outbound `ClientMessage` via `telemetry/tracePropagation.ts::injectIntoClientMessage`, under a single root span `dt_client.join` per join call (`getTracer()` from task #12).

### Scope
- **Service(s)**: `packages/sdk-core` (client SDK) only.
- **Schema**: No.
- **Cross-cutting**: No backend changes. Consumes (does not define) the existing `proto/dark_tower/signaling/v1/signaling.proto` wire contract via generated protobuf-es types.

### Debate Decision
NOT NEEDED — implements an already-decided design (ADR-0028 §3, story requirements R-16–R-19). Building blocks (framing codec, trace helper, transport interface, mock) all landed in tasks #9/#12.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/sdk-core/src/signaling/**` | Mine | — |
| `packages/sdk-core/src/errors/SignalingError.ts` (new) | Mine | — |
| `packages/sdk-core/src/errors/SdkError.ts` (add `Signaling` discriminant) | Mine (flag #1) | — |
| `packages/sdk-core/src/index.ts` | Mine | — |
| `packages/sdk-core/package.json` (add `@bufbuild/protobuf` runtime dep) | Mine | — |
| `packages/sdk-core/project.json` (dependsOn proto-gen:codegen + implicitDependencies) | Mine | — |
| `pnpm-lock.yaml` (from `pnpm install`) | Mine | — |
| `packages/sdk-core/vitest.config.ts` (coverage exclude `src/proto/**`) | Mine | — |
| `packages/sdk-core/.prettierignore` (new — exclude generated `src/proto/**` from package-local prettier) | Mine | — |
| `docs/TODO.md` (Step-9 append target) | Mine | — |
| `packages/proto-gen/project.json` (`codegen.outputs` glob `*_pb.ts` → `**/*_pb.ts`) | Not mine, Minor-judgment | protocol/infrastructure |

> `packages/sdk-core/src/proto/**` is intentionally **NOT** in the touched-files table: generated,
> gitignored (`.gitignore:13`), never appears in the git diff (produced fresh by `proto-gen:codegen`).
> It is consumed, not committed.
>
> `packages/proto-gen/project.json` WAS originally listed here as "will not be edited" (Gate-1 Flag 1
> deferral). **That deferral was reversed post-completion** — see §Accepted Deferrals — so it now
> appears in the table as a Minor-judgment cross-boundary edit (the one-line glob fix).

### Gate-1 Decisions (Lead)

- **Flag 1 — proto-gen `codegen.outputs` glob bug** (`{workspaceRoot}/packages/sdk-core/src/proto/*_pb.ts` should be `**/*_pb.ts`; flat `*` doesn't cross `/`, so the nested actual outputs `dark_tower/<pkg>/v1/*_pb.ts` are declared as ZERO cached outputs): **owner-implements, do NOT edit in this devloop.** `proto-gen/**` is owned by **protocol** (ownership manifest `scripts/guards/simple/cross-boundary-ownership.yaml:31`); the defect was introduced by task #7 (commit `501ece2`).
  - **Severity — verified, corrected from initial review framing (re: @test challenge):** this is a **local-dev footgun, NOT a CI flake.** ci-client.yml's only `actions/cache@v4` step caches Playwright browsers (`~/.cache/ms-playwright`); it does **not** persist the Nx cache (`.nx/cache`), and no Nx Cloud/remote cache is configured. Every CI job therefore starts with an empty Nx cache → codegen always cache-**MISS** → buf runs → `_pb.ts` present. The "fresh CI runner + cache-HIT" co-occurrence the deferral challenge assumed cannot happen here. The bug bites only a **local** dev with a populated `.nx/cache` who has also removed the gitignored generated files (e.g. `git clean`).
  - **Why deferral's cost-math holds:** the "fix-later cost" cited in the challenge assumed latent CI flakiness — which doesn't exist here — so that side of the inequality is much smaller than argued. The fix also cannot be a client edit regardless: `proto-gen/**` is owner-implements (no protocol reviewer on this devloop), and `dependsOn` (ordering, `Mine`) is orthogonal to the glob (cache-output-correctness, protocol-owned) — `dependsOn` cannot fix a codegen *self* cache-hit. `--paired-with=protocol` was considered and rejected as disproportionate for a non-CI-blocking, pre-existing one-liner.
  - **Action:** implementer files a concrete follow-up in `docs/TODO.md` routed to **protocol/infrastructure** with the exact fix (`*_pb.ts` → `**/*_pb.ts`) and the local-footgun rationale; surface it in the completion report. Implementer does **not** touch `packages/proto-gen/`.
- **Flag 2 — add `Signaling` discriminant to `SdkErrorCode` (`src/errors/SdkError.ts`):** **approved as `Mine`.** The entire `src/errors/` hierarchy is client-owned; `AuthError`/`MeetingError`/`NetworkError`/`ValidationError` each already add their own discriminant — `Signaling` follows the established pattern. No cross-boundary concern.

Notes:
- The generated TS proto types under `packages/sdk-core/src/proto/` are gitignored (`.gitignore:13`) and produced by `nx run proto-gen:codegen`. They are NOT part of the diff. `proto/**` (the GSA wire-format surface) is **not** edited — task #13 consumes the existing contract.
- `@bufbuild/protobuf@2.12.0` is added as a direct runtime dependency of `sdk-core` (the generated `_pb.ts` files import it). Dependency-manifest change → full mode (already full). Owned by client. Security/DRY reviewers scrutinize at Gate 1/3.
- `packages/test-utils/MockWebTransport.ts` (owned by `test`) should **not** need edits — its R-15 control points + inspectors already cover task #13's needs. Any test-side framing/decode helper belongs in sdk-core test files, not in test-utils. If the implementer believes a test-utils edit is needed, surface it as a cross-boundary row before proceeding.

---

## Planning

### Mechanism

`SignalingClient` is an event-driven bidi-stream pump over `IWebTransport`:
`join()` → connect (injected factory, default prod `connect`) → await `ready` →
open FIRST bidi stream → build `ClientMessage{joinRequest}` → inject W3C trace →
`toBinary` → `encodeFrame` → write. Then a read loop pulls chunks, feeds
`FrameDecoder` (partial-read + oversize bounded), `fromBinary(ServerMessageSchema)`,
and dispatches on `sm.message.case`. `join()` resolves with the typed `JoinedEvent`
on `joinResponse`, or rejects on auth/framing/error-before-join. A single
`dt_client.join` root span wraps the join; the JoinRequest is built+injected inside
`context.with(trace.setSpan(active, span))` so the registered StackContextManager
(installed by `configureTelemetry`'s `WebTracerProvider.register`) makes the span
active at inject time. `getTracer()===undefined` (telemetry unconfigured) ⇒ no span,
and `injectIntoClientMessage` is a safe no-op.

### File layout

- `src/signaling/SignalingClient.ts` — the class (connect, join/span lifecycle, dispatch proto-case → events).
  The send path (`injectIntoClientMessage` → `toBinary` → `encodeFrame` → `writer.write`) and the read loop
  (`FrameDecoder.push` → `fromBinary` → dispatch + `FramingError` handling) are kept as TWO cohesive,
  liftable private methods (`#sendClientMessage`, `#runReadLoop`) — NOT scattered across methods.
  **DRY decision (per @dry-reviewer, final): extract-on-second-use, NOT now.** A separate generic
  `framedChannel` module would be speculative before a second consumer exists; task #14
  (`MhClientMessage`/MediaTransport) needs the identical shape and will lift these blocks then.
  @dry-reviewer logs this as an extract-on-second-use candidate in `docs/TODO.md` §Cross-Service Duplication
  at review time; no separate module this task.
- `src/signaling/events.ts` — public plain-typed surfaces: `JoinedEvent` (`participantId:string`,
  `userId:bigint`, `existingParticipants:RosterParticipant[]`, `mediaServers:string[]`),
  `ParticipantJoinedEvent`, `ParticipantLeftEvent`, `RosterParticipant`, `SignalingJoinParams`,
  `SignalingCapabilities`, event-map type. **No generated proto types leak into the public barrel.**
- `src/signaling/errorCodeMap.ts` — proto `ErrorCode` → `SignalingErrorCode` pure map.
- `src/errors/SignalingError.ts` — **siting decision: under `errors/`** alongside Auth/Meeting
  (extends `SdkError`, inherits the `toJSON` redaction allowlist). Exports `SignalingErrorCode`.
- `src/signaling/__tests__/signaling-client.test.ts` + `__tests__/helpers.ts` (test-local framing/encode).

### Event/emitter design

Minimal typed emitter inside `SignalingClient` (no new dep): `on(type, listener)` /
`off(type, listener)` over a `Map<type, Set<listener>>`. Event types:
`'joined' | 'participantJoined' | 'participantLeft' | 'error'` (the spec's
`onJoined`/`onParticipantJoined`/`onParticipantLeft`/SignalingError surfaces). R-22
`MeetingSession` will re-emit these as its high-level `onJoined`/`onError`.

### Read-loop lifecycle & teardown (per @code-reviewer)

Join settlement and the read loop are DECOUPLED. `join()` resolves/rejects via an internal
one-shot deferred; the read loop is long-lived and independent.

- **Join deferred is one-shot.** `JoinResponse` resolves it (→ `onJoined` + span OK/end); a terminal
  error BEFORE it settles rejects it (→ span ERROR/end). A `#joinSettled` flag guards it so it's
  settled exactly once.
- **(1) Loop keeps running after join resolves** to dispatch subsequent `participantJoined`/
  `participantLeft`/`error`. Resolving the join deferred does NOT stop the pump.
- **(2) Post-join errors go to the `'error'` event, never a dangling promise.** Any read/`fromBinary`/
  framing error routes through `#fail(err)`: if `!#joinSettled` → reject `join()`; else → emit `'error'`
  with the `SignalingError`. So an error after join cannot become an unhandled rejection on a promise
  nobody awaits.
- **(3) Background loop can never produce an unhandled rejection.** It's launched as
  `void this.#runReadLoop()`; `#runReadLoop` has an internal `try/catch/finally` — `catch` routes to
  `#fail`+teardown, `finally` releases the reader lock. Holds regardless of whether `join()` has resolved,
  rejected, or the transport closed first.
- **(4) Clean teardown on `transport.closed` and `close()`.** I subscribe to `transport.closed`
  (`.then(info → #onClosed(info), err → #onClosed(undefined, err))`): if join pending → reject with
  `SignalingError(Transport, { closeCode })`; else emit `'error'` if it was an error close. `close()` and
  `#onClosed` are idempotent (a `#closed` flag) and call `reader.cancel()` on the active reader → the
  pending `read()` resolves `done` → loop breaks → `finally` `releaseLock()` (no leaked locked stream).
  The JoinRequest writer is acquired, used for the single write, then `releaseLock()`-ed immediately (the
  long-lived connection holds no writable lock).

### ErrorCode → SDK mapping (R-18)

| proto `ErrorCode` | `SignalingErrorCode` | Auth-class? close conn |
|---|---|---|
| UNKNOWN(0) | `Unknown` | no |
| INVALID_REQUEST(1) | `InvalidRequest` | no |
| UNAUTHORIZED(2) | `Unauthorized` | **YES** → close(3401,'auth') |
| FORBIDDEN(3) | `Forbidden` | **YES** → close(3401,'auth') |
| NOT_FOUND(4) | `NotFound` | no |
| CONFLICT(5) | `Conflict` | no |
| INTERNAL_ERROR(6) | `InternalError` | no |
| CAPACITY_EXCEEDED(7) | `CapacityExceeded` | no |
| STREAM_ERROR(8) | `StreamError` | no |
| _(local framing throw)_ | `Framing` | no (join rejects) |
| _(transport closed/failed pre-join)_ | `Transport` | no (join rejects) |

**Numeric close code preserved (per @observability).** On a transport close / terminal
settle, `SignalingError` carries an OPTIONAL numeric `closeCode?: number` sourced from
`WebTransportCloseInfo.closeCode` — NOT a pre-stringified reason. The raw free-form
`reason` string is never used as a label and is NOT carried on the error. `SignalingClient`
also exposes the transport's structured `closed` info (numeric code) so task #14 / R-22 can
call `normalizeCloseReason(closeCode)` itself to derive the bounded `close_reason` label.
`closeCode` is added to the `toJSON` allowlist (numeric protocol code, non-PII); `reason` is not.
SignalingClient does NOT call `normalizeCloseReason` here (no metric emitted this task).

Close code `3401` chosen so `normalizeCloseReason(3401) → CloseReason.AuthFailed`
(consistent with `closeReason.ts`). `SignalingError extends SdkError` with
`code=SdkErrorCode.Signaling`, `signalingCode`, and the proto enum NAME in `serverCode`.
`details` map from `ErrorMessage` is **NOT** serialized (server-controlled keys; PII).
Token fields never placed on the error (R-23); `toJSON` stays the fixed allowlist + `signalingCode`.

**Security invariants (confirmed w/ @security).** (A) The read loop NEVER calls
`propagation.extract` / continues a span from inbound `ServerMessage.trace_parent`/`trace_state`
(server-controlled, untrusted) — inbound trace fields are ignored entirely; injection is
OUTBOUND-only. (B) The server-authored `ErrorMessage.message` surfaces ONLY on the
`SignalingError` handed to the caller via the `'error'` event; it is NEVER `console.*`-logged
anywhere in the SDK. Any internal/debug logging on the error path keys off the bounded
`signalingCode`, never the raw `message`. Known variants are not logged; unknown variants log the
`message.case` discriminant only. `ErrorMessage.details` dropped entirely (no bounded subset).

**Semantic-guard watch-points (folded in).** (1) *error-context-preservation:* framing/transport
wraps pass the underlying cause through as `new SignalingError(..., { cause: err })` — the cause is
RETAINED on the live error object for debugging but is NOT in the `toJSON` allowlist (redacted on the
client surface). (2) *span lifecycle on all paths:* `dt_client.join` is `span.end()`-ed in a `finally`
so it ends on every exit (resolve, auth/framing/transport reject, any throw); error paths call
`recordException`/`setStatus(ERROR)`. **Token-safety caution:** the span is active while the
JoinRequest (carrying the meeting JWT) is built, so I NEVER feed the outbound
ClientMessage/JoinRequest/token into `recordException` — only decode/transport/framing error objects
(which carry no token) are recorded. (3) The unknown-variant `console.debug` logs the `message.case`
name only, never the decoded message object.

### Codegen build wiring + dep (integration note #1/#2)

- `packages/sdk-core/package.json`: add `"@bufbuild/protobuf": "2.12.0"` (runtime dep; generated `_pb.ts` imports it). `pnpm install`.
- `packages/sdk-core/project.json`: add `"implicitDependencies": ["proto-gen"]` + per-target
  `"dependsOn": [{ "projects": ["proto-gen"], "target": "codegen" }]` on ALL FOUR source-compiling targets:
  `build`, `test:unit`, **`test:component`**, `lint`. (`test:component` per @operations: its
  `bundle-content.test.ts` runs a real `vite build` in `beforeAll` that compiles the barrel → imports the
  generated proto module; the component CI job runs cold with no prior build/lint, and `implicitDependencies`
  only affects the project graph / affected-detection — NOT target sequencing — so only the explicit
  per-target `dependsOn` makes codegen run first.)
- `src/errors/SdkError.ts`: add `Signaling: 'SIGNALING'` to `SdkErrorCode` (shared base — flagged below).
- `src/index.ts`: export `SignalingClient`, `SignalingError`, `SignalingErrorCode`, event types.

### Wider-than-task flags (for reviewers)

1. **`SdkErrorCode` gains `Signaling`** (`errors/SdkError.ts`) — extends the shared discriminant union;
   client-domain but touches the error base. → @code-reviewer / @dry-reviewer.
2. **`proto-gen/project.json` `codegen.outputs` glob looks wrong**: declared
   `packages/sdk-core/src/proto/*_pb.ts` but actual output is nested at
   `.../proto/dark_tower/signaling/v1/signaling_pb.ts`. On a cache-restore (no re-run) the file may
   not be restored → downstream build break. Owned by protocol/infra (task #6); I will NOT edit it.
   My `dependsOn` guarantees ORDERING (fresh checkout always runs codegen). → flag to @team-lead for a
   follow-up; tell me if you want me to fix it as a cross-boundary edit.
3. **`SignalingError` drops the proto `details` map from serialization** (PII). → @security to confirm.

### Test list (~16, Vitest/Node, MockWebTransport; helpers build framed bytes via create/toBinary/encodeFrame)

1. connect→JoinRequest outbound shape (decode captured frame → `ClientMessage{joinRequest}`, empty correlation/binding).
2. capabilities populated in JoinRequest.
3. framing round-trip (outbound frame re-decodes via `FrameDecoder`).
4. JoinResponse→`onJoined`: BigInt `userId`, roster, `mediaServers` URLs from `mediaHandlerUrl`.
5. correlationId/bindingToken stored + readable via getters.
6. ParticipantJoined typed event. 7. ParticipantLeft typed event (reason).
8. unknown variant (`streamPublished`) decoded + debug-logged, no throw, no event.
9. ErrorMessage→SignalingError mapping — **table-driven, EXHAUSTIVE over all 9 `ErrorCode`s**, asserting
   `err.signalingCode === expected` per a fixed mapping table (not "some error fired"); a new `ErrorCode`
   forces a test update.
10. UNAUTHORIZED closes connection with the **typed reason** — assert close info `{closeCode:3401, reason:'auth'}`
    and `normalizeCloseReason(3401) === CloseReason.AuthFailed` (not merely that `close()` was called).
11. FORBIDDEN closes connection with the same typed reason.
12. trace_parent populated on outbound when telemetry configured — assert `traceParent` matches
    `^00-[0-9a-f]{32}-[0-9a-f]{16}-0[01]$` AND carries the active `dt_client.join` span's trace/span IDs.
    **Does NOT require non-empty `traceState`** (fresh root span ⇒ empty tracestate ⇒ propagator omits the field).
12b. (separate, explicit) seed vendor `tracestate` into the active context before join → assert it propagates
    onto the outbound `traceState`.
13. no-telemetry: outbound `traceParent`/`traceState` empty, no throw.
14. partial-read buffering (JoinResponse split across two chunks → one `onJoined`).
14b. **multiple frames in ONE chunk** (JoinResponse + ParticipantJoined in a single `simulateServerMessage`)
    → BOTH events fire (proves the read loop drains every frame from `FrameDecoder.push`, not just `frames[0]`).
15. oversize frame → SignalingError(`Framing`), join rejects.
16. join() resolves JoinedEvent on success / rejects on auth error.
17. join awaits `transport.ready` before opening the first bidi stream (no stream opened until ready resolves).
18. read-loop terminal error via `simulateError`/`closed` rejection → pending join rejects with `SignalingError(Transport)`,
    numeric `closeCode` preserved.
19. stream write-failure path — inject a stub `IWebTransport` whose bidi `writable` rejects → join rejects (typed).

Coverage target ≥ 90% (R-41 portion). Hard-to-hit branches called out: no-telemetry no-op (13),
span error-path end (10/11/15/18), transport-terminal settle (18), write-failure (19) — each has a dedicated test.

---

## Pre-Work

None.

---

## Implementation Summary

Implemented `SignalingClient` (R-16–R-19) as planned. Mechanism: `join()` → injected
transport factory (default prod `connect`) → await `ready` → open the FIRST bidi stream →
build `ClientMessage{joinRequest}` (empty correlation/binding) → `injectIntoClientMessage`
(R-19, single call site) under `context.with(trace.setSpan(active, dt_client.join))` →
`toBinary` → reused `encodeFrame` → write (writer released immediately). A long-lived read
loop pumps chunks through the reused `FrameDecoder` → `fromBinary(ServerMessageSchema)` →
dispatch on `message.case`: `joinResponse` → store correlation/binding + emit `joined` +
resolve `join()` (span OK/end); `participantJoined`/`participantLeft` → typed events;
`error` → `SignalingError` (auth-class closes with `close(3401,'auth')`); unknown → debug-log
the case name only. Send-path + read-loop kept as two cohesive, liftable private methods
(extract-on-second-use per DRY). Single terminal `#terminate` path: rejects pending `join()`
or emits `'error'` post-join, ends the span exactly once (`recordException` only the
SignalingError — never the token-bearing JoinRequest), cancels the reader + closes the
transport (idempotent). `SignalingError` sited under `errors/` (inherits `toJSON` redaction;
adds `signalingCode` + numeric `closeCode`; cause on non-enumerable `Error.cause`; `details`
dropped). Public surface is plain-typed — NO generated proto types leak into the barrel.

**Verification (local):**
- `tsc --noEmit`: clean. `eslint src/`: clean. `prettier --check`: clean.
- Unit (`vitest run --coverage`): **126 passed**; coverage **stmts 97.57 / branch 91.01 /
  funcs 98.21 / lines 98.63** (≥90 gate met). Generated `src/proto/**` excluded from coverage.
- Component (`vitest --config vitest.component.config.ts`): vite build + `bundle-content` →
  **3 passed**; `dist/index.{mjs,cjs}` + `.d.ts` build clean (no proto types in the public `.d.ts`).
- Clean-room codegen ordering — proven for ALL FOUR targets (per @operations): for each of
  `build` / `test:unit` / `test:component` / `lint`, `rm -rf packages/sdk-core/src/proto` then
  `pnpm exec nx run sdk-core:<target> --skip-nx-cache` ran `proto-gen:codegen` (`buf generate`)
  FIRST, regenerated the nested `signaling_pb.ts`, then the target succeeded (exit 0) — each
  reporting "Successfully ran target <target> for project sdk-core and 1 task it depends on".
  Proves the explicit `dependsOn: proto-gen:codegen` edge holds on a cold-cache fresh checkout
  for every source-compiling target (not just test:unit).

Tests cover (R-41 portion): JoinRequest outbound shape (decoded frame, empty correlation/binding),
capabilities, 4-byte framing round-trip, JoinResponse→onJoined (BigInt userId + roster +
mediaServers), correlation/binding getters, ParticipantJoined/Left, unset-participant +
unknown-variant (debug-log case name only, no payload), ErrorMessage→SignalingError exhaustive
over all 9 ErrorCodes (signalingCode + serverCode), toJSON redaction (no token), UNAUTHORIZED/
FORBIDDEN typed-close (3401→AuthFailed), trace_parent W3C-format + span linkage, seeded
tracestate propagation, no-telemetry no-op, partial-read split, multiple-frames-in-one-chunk,
oversize→Framing(+cause, not in toJSON), ready-gating, transport-close-before-join (closeCode
preserved), transport-error, post-join error→`'error'` event, write-failure, off()/idempotent close.

Follow-up filed: `docs/TODO.md` §Polyglot Pipeline Follow-ups — proto-gen `codegen.outputs` glob
`*_pb.ts`→`**/*_pb.ts` (local-dev footgun; protocol/infrastructure-owned; NOT edited here).

---

## Files Modified

New (all `Mine`, client domain):
- `packages/sdk-core/src/signaling/SignalingClient.ts` — the client (join/span lifecycle, send, read loop, dispatch, teardown).
- `packages/sdk-core/src/signaling/events.ts` — public plain-typed surfaces + `mapLeaveReason`/`ParticipantLeaveReason`.
- `packages/sdk-core/src/signaling/errorCodeMap.ts` — proto `ErrorCode`→`SignalingErrorCode` table + `isAuthClass`/`staticMessageFor`.
- `packages/sdk-core/src/errors/SignalingError.ts` — `SignalingError`/`SignalingErrorCode` (extends `SdkError`).
- `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` — ~30 behavior tests.
- `packages/sdk-core/src/signaling/__tests__/maps.test.ts` — direct map-helper coverage.
- `packages/sdk-core/src/signaling/__tests__/helpers.ts` — test-local framing/encode (no hex fixtures).
- `packages/sdk-core/.prettierignore` — excludes generated `src/proto/**` from the package-local prettier gate.

Edited (all `Mine`):
- `packages/sdk-core/src/errors/SdkError.ts` — `+ Signaling: 'SIGNALING'` discriminant (Gate-1 approved).
- `packages/sdk-core/src/index.ts` — barrel exports for the signaling layer + `SignalingError`.
- `packages/sdk-core/package.json` — `+ "@bufbuild/protobuf": "2.12.0"` (runtime dep, exact pin).
- `packages/sdk-core/project.json` — `implicitDependencies: ["proto-gen"]` + per-target `dependsOn: proto-gen:codegen` on build/test:unit/test:component/lint.
- `packages/sdk-core/vitest.config.ts` — coverage `exclude` `+ src/proto/**` (generated code not coverage-gated).
- `pnpm-lock.yaml` — records the new `@bufbuild/protobuf` edge for `sdk-core` (from `pnpm install`).
- `docs/TODO.md` — proto-gen glob follow-up (routed to protocol/infrastructure).

NOT edited (consume-only / not mine): `proto/**`, `packages/proto-gen/**`, `packages/test-utils/**`.
Generated `packages/sdk-core/src/proto/**` is gitignored — produced by codegen, NOT committed.

---

## Code Review Results

### Gate 2 (validation pipeline) — attempt 1

- **L3 `no-pii-in-logs-ts`** (`SignalingClient.ts:145`): the JSDoc `@example` modeled
  `console.log(e.participant.name)` — a participant display name is PII (R-23/R-26), and an
  example is copy-paste guidance. **Fixed**: example now logs the opaque `participantId` with an
  inline "never log display names" warning; dropped the `'Ada'` literal. Guard re-run:
  `STATUS=OK` (35 files clean). (Lead cleared two non-mine findings: gitignored `src/proto/**`
  scope-row → moved to a note; pre-existing task-#12 `inline_debt_body` → fixed in `f7b2980`.)

### Security review

- **`recordException` shipped untrusted server text to telemetry** (`#settleJoinFailure`): for an
  `ErrorMessage`-derived failure, `span.recordException(error)` serialized the server-authored
  free-form `error.message` (and `error.stack`'s first line) into the `dt_client.join` span's
  `exception.message`, which the OTLP exporter ships to the collector — the one untrusted-server-
  text→telemetry path. **Fixed** (security's preferred bounded-record option): now
  `span.recordException({ name: error.name, message: error.signalingCode })` — bounded SDK enum
  only, no server text, no stack. Keeps `recordException` present on the error path
  (reconciles @semantic-guard's metrics-path-completeness ask) while removing the trust dependency
  for traces entirely. Regression test added: `records a BOUNDED exception on the join span —
  server message text never reaches telemetry` (spies the join span's `recordException`, feeds a
  sentinel server reason, asserts only `signalingCode` is recorded and the sentinel is absent).
  Unit suite now **127 passed**, coverage 97.57/91.01/98.21/98.63 (≥90), lint clean.

### Operations review

- **No join timeout → silent-hang failure mode** (the Gate-1 operations concern): if MC accepts the
  bidi stream + JoinRequest but never sends a `JoinResponse` and never closes (stalled/wedged MC,
  half-open path), the read loop blocks forever in `reader.read()`, `join()` never settles, and the
  transport/reader leak — no typed error, no teardown. **Fixed**: bounded join deadline. `join()`
  arms a `setTimeout(#joinTimeoutMs)` (default `DEFAULT_JOIN_TIMEOUT_MS` = 15000; override via
  `SignalingClientOptions.joinTimeoutMs`; `<= 0`/non-finite disables) that, if `!#joinSettled`, calls
  `#terminate(new SignalingError(SignalingErrorCode.Timeout, …))` → rejects `join()` + tears down
  (cancel reader, close transport). New dedicated `SignalingErrorCode.Timeout` (cleaner than reusing
  `Transport` for observability). Timer cleared on settle (`#clearJoinTimer` in both
  `#settleJoinSuccess`/`#settleJoinFailure`) so it never outlives or post-fires the join. NOT
  reconnection logic (out of scope per Gate 1). Two tests added (fake timers): a stalled-MC transport
  → `join()` rejects with `Timeout` within the deadline + transport closed (proves stream opened +
  JoinRequest sent first); and `joinTimeoutMs: 0` opt-out → no deadline fires. Unit suite now
  **129 passed**, coverage 97.62/90.97/98.24/98.66 (≥90), lint clean, `no-pii-in-logs-ts` OK.
  - **`Timeout` vs `Transport` code**: @operations offered either; chose dedicated
    `SignalingErrorCode.Timeout` (accepted RESOLVED-FIXED by @operations). Rationale: it serves R-25's
    `failure_stage` mapping in #14 BETTER than reusing `Transport` — a join-deadline expiry =
    `mc_join_response` stage, distinct from a connect/close failure (`Transport` → `mc_signaling_connect`).
    Reusing `Transport` would conflate the two stages. No disagreement → no escalation.

### Observability review

- Same finding as @security (recordException exporting server free-form text — and, since V8 embeds
  `error.message` in `error.stack`'s first line, the `exception.stacktrace` attribute too). **Resolved by
  the same fix**: `span.recordException({ name: error.name, message: error.signalingCode })` is a plain
  **object literal** (NOT an `Error`), so no `.stack`/stacktrace echo, no server text. Removes the
  trust dependency on MC keeping `ErrorMessage.message` generic for the trace channel.

### Test review

- **Weak/mislabeled success-path span assertion** (`signaling-client.test.ts`): the only telemetry-
  configured SUCCESS-path test asserted only `participantId`, never that the span ended — leaving
  `#settleJoinSuccess`'s `setStatus(OK)` + `end()` executed-but-unasserted (coverage-only). **Fixed**:
  retitled "ends the dt_client.join span with OK status exactly once on a successful join" and now spies
  the join span — asserts `end()` called once, `setStatus(OK)` set, `recordException` NOT called, and
  `end()` still == 1 after `close()` (settle-once / no-double-end invariant). Mirrors the error-path
  bounded-exception test's seam. Unit suite **129 passed**, coverage 97.62/90.97/98.24/98.66 (≥90), lint clean.

### Code Quality review

- RESOLVED-DEFERRED — zero defects in the task #13 changeset; the single deferred item is the
  pre-existing, protocol/infra-owned proto-gen `codegen.outputs` glob (recorded in `docs/TODO.md`).

---

## Accepted Deferrals

- `docs/TODO.md` §Cross-Service Duplication (DRY) — extract framed send-path + read-loop on #14 second-use

---

## Post-Completion Correction — proto-gen glob deferral reversed

The proto-gen `codegen.outputs` glob fix (`*_pb.ts` → `**/*_pb.ts`) was originally **deferred** as a
protocol/infra spin-out (Gate-1 Flag 1; §Accepted Deferrals pointer). **That deferral was reversed**
per user feedback after completion:

- It's a one-line, value-neutral, structure-preserving Mechanical edit. The Nx `outputs` array is a
  cache declaration with **no wire-format / runtime coupling**, so it is not a GSA *by criterion* —
  a Mechanical edit there is "proceed with review," not owner-implements.
- The cost-math the review protocol's own suspicious-deferral check encodes favors fixing now: a
  sub-5-LoC fix vs. a ~15-line `docs/TODO.md` tracking entry plus recurring find-it / re-establish-
  context cost. I over-weighted the ownership formalism over that obvious asymmetry.

**Action taken:** fixed `packages/proto-gen/project.json` (classified Minor-judgment cross-boundary,
Owner protocol/infrastructure — see the Cross-Boundary Classification table), removed the now-obsolete
`docs/TODO.md` §Polyglot Pipeline Follow-ups entry, and folded both into the task #13 commit. Verified
the fix: with a warm `.nx/cache`, `rm -rf packages/sdk-core/src/proto` followed by
`nx run proto-gen:codegen` now **restores** the nested `_pb.ts` from cache (previously restored
nothing). Lesson: ownership formalism does not override an obvious cost asymmetry on a non-GSA
Mechanical fix.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `5003d0802d8156d9b96baded514414b764b73e1a`
2. Review changes: `git diff 5003d0802d8156d9b96baded514414b764b73e1a..HEAD`
3. Soft reset: `git reset --soft 5003d0802d8156d9b96baded514414b764b73e1a`
4. Hard reset: `git reset --hard 5003d0802d8156d9b96baded514414b764b73e1a`

---

## Issues Encountered & Resolutions

### Issue 1: First sdk-core consumer of generated protobuf-es types
**Problem**: The generated TS proto types (`packages/sdk-core/src/proto/**`) are gitignored and were not wired into any sdk-core Nx target; task #13 is the first consumer, so a fresh checkout would fail to compile.
**Resolution**: Added per-target `dependsOn: ["proto-gen:codegen"]` to all four sdk-core targets (build, test:unit, test:component, lint) + `implicitDependencies: ["proto-gen"]`. Verified with a clean-room proof (`rm -rf src/proto && nx run sdk-core:test:unit`). The orthogonal cache-output-correctness bug in proto-gen's `codegen.outputs` glob was ruled owner-implements (protocol/infra) and tracked in docs/TODO.md.

### Issue 2: Gate 2 doc-hygiene guard failures (2 iterations, no code involved)
**Problem**: The always-run `validate-todo-tracking` / `validate-cross-boundary-scope` guards flagged (a) a planned-but-untouched gitignored `src/proto/**` row, (b) a pre-existing `inline_debt_body` in task #12's main.md, and (c) inline debt bodies in THIS main.md's §Accepted Deferrals.
**Resolution**: (a) moved the gitignored row to a note; (b) fixed task #12's file as a standalone doc commit `f7b2980` (content-preserving relocation, matching precedent 5003d08); (c) converted §Accepted Deferrals to one-line `docs/TODO.md` pointers. No code change in any of these.

### Issue 3: recordException telemetry-trust-boundary leak (caught in review)
**Problem**: `span.recordException(error)` would serialize the server-authored free-form `ErrorMessage.message` (and its V8 stack echo) into the exported `dt_client.join` span — untrusted-server-text → telemetry.
**Resolution**: security + observability jointly flagged; fixed by recording a bounded plain object `{ name, message: signalingCode }` (no Error → no stack); sentinel regression test added.

---

## Lessons Learned

1. **A devloop's own main.md is in the always-run guard surface.** `validate-todo-tracking`/scope-drift scan all `docs/devloop-outputs/**/main.md`, so §Accepted Deferrals must be pointer-only (`- \`docs/TODO.md\` §X — hook`) and the Cross-Boundary table must not list gitignored/never-committed paths (move them to notes). Two Gate-2 iterations here were pure doc-hygiene, zero code.
2. **`recordException(error)` is a distinct telemetry trust boundary from `toJSON`.** `setStatus`/`toJSON` redaction does not cover `recordException`, which captures `error.message` + `error.stack`. Passing a bounded plain object (not the Error) closes both channels — a reusable pattern for any client-side span error path carrying server text.
3. **Owner-implements + verified severity beats a reflexive now-fix.** The proto-gen glob looked like a CI risk; verifying ci-client.yml caches only Playwright (no Nx cache) reclassified it to a local-dev footgun, justifying the routed follow-up over pulling protocol into this devloop.
4. **Cohesive-but-not-yet-extracted (extract-on-second-use) is the right DRY call for an N=1 pattern** whose second consumer (#14) will define the correct generic shape; pre-extraction is speculative generality.
