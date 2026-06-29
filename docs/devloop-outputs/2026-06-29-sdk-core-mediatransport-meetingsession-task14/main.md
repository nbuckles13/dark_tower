# Devloop Output: sdk-core MediaTransport + MeetingSession facade (Task #14)

**Date**: 2026-06-29
**Task**: `MediaTransport.connectAll()` (active/active MH handshake, R-20/R-21), `MediaConnectionUpdate` send (R-60 SDK side), `MhClientMessage` envelope + trace injection (R-58 SDK side), `MeetingSession` join/disconnect facade (R-22), token redaction/cleanup (R-23), browser crypto pattern (R-32). ~15 unit tests (R-41).
**Specialist**: client
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-14`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f34f3a666fd32aa8d7f0d1b8db1923f1456104de` |
| Branch | `feature/browser-client-join-task-14` |
| User story | `docs/user-stories/2026-05-02-browser-client-join.md` (task #14) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session-508e0d81` |
| Implementing Specialist | `client` |
| Iteration | `1` |
| Security | `security@session-508e0d81` |
| Test | `test@session-508e0d81` |
| Observability | `observability@session-508e0d81` |
| Code Quality | `code-reviewer@session-508e0d81` |
| DRY | `dry-reviewer@session-508e0d81` |
| Operations | `operations@session-508e0d81` |
| Semantic Guard | `semantic-guard@session-508e0d81` |

---

## Task Overview

### Objective
Land the SDK media-transport layer and the public `MeetingSession` facade that composes the full browser join happy-path (auth → meeting token → MC signaling → active/active MH handshake → `MediaConnectionUpdate` report), with clean teardown and strict token hygiene.

### Scope
- **Service(s)**: client only — `packages/sdk-core/`, `packages/test-utils/` (MockWebTransport already exists; may need inspector additions).
- **Schema**: No.
- **Cross-cutting**: No new server/proto/infra changes. Proto types (`MhClientMessage`, `MhConnectRequest`, `MediaConnectionUpdate`, `MhConnectionStatus`, `ConnectionState`) already landed (tasks #2/#31) in `proto/dark_tower/signaling/v1/signaling.proto`; TS types are codegen'd via the `proto-gen` Nx `codegen` target into the gitignored `packages/sdk-core/src/proto/`.

### Debate Decision
NOT NEEDED — implementation within existing ADR-0028 boundaries; design (envelope shape, connectAll resolver semantics, MediaConnectionUpdate trigger) is fully specified in the user story §design and R-20/R-21/R-22/R-60.

---

## Requirements in scope
- **R-20**: `MediaTransport` opens a WT connection to **each** MH URL from `media_servers` in parallel; authenticates with meeting JWT as the first length-prefixed bidi message wrapped in an `MhClientMessage{connect_request:{join_token}}` envelope. Per-MH `onConnected(url)`; failure → typed `MediaConnectionError` with failing URL. **No frame I/O, no SFrame, no WebCodecs, no datagrams.**
- **R-21**: `connectAll()` resolves when ≥1 MH connects, rejects only if **all** fail. Per-MH state observable. Once settled (success/partial/all-fail), send ONE `MediaConnectionUpdate` `ClientMessage` listing per-MH terminal state for every URL.
- **R-58 (SDK side)**: populate `trace_parent`/`trace_state` on outbound `MhClientMessage` via the SAME `injectIntoClientMessage` helper used by `SignalingClient` — one shared injection path (browser→MC and browser→MH).
- **R-60 (SDK side)**: build the `MediaConnectionUpdate` (one `MhConnectionStatus` per URL; state ∈ CONNECTED/FAILED; optional `failure_reason`/`failure_code`; `observed_at` timestamp) and send via `SignalingClient`.
- **R-22**: `MeetingSession` composes `AuthApiClient` + `MeetingApiClient` + `SignalingClient` + `MediaTransport`; `join({orgSubdomain, meetingCode, credentials})` drives the happy path; emits `onJoined`/`onParticipantJoined`/`onParticipantLeft`/`onMediaConnected`/`onError`; `disconnect()` tears down all transports cleanly. Public SDK entry point.
- **R-23 (cleanup)**: tokens in memory only; never logged/URL/telemetry; SDK errors redact token fields; `disconnect()`/logout zeroes token references + cached `Bearer` header strings; overwrite buffers where the runtime allows.
- **R-32**: browser crypto — any randomness via `crypto.getRandomValues()` (no `Math.random()`); any `CryptoKey` uses `extractable: false`. (Likely "no new crypto introduced here" — confirm and document.)
- **R-41 (this slice)**: ~15 Vitest unit tests: `connectAll` happy/partial/all-fail; `MediaConnectionUpdate` send shape across all three; `MeetingSession` state machine (idle→fetching-token→connecting-mc→joining→joined→disconnecting); token redaction in errors. Coverage ≥ 90% for `sdk-core` must hold.

## Key integration points (read before planning)
- `SignalingClient` (`packages/sdk-core/src/signaling/SignalingClient.ts`) currently has NO public method to send a post-join `ClientMessage`. `#sendClientMessage`/`#runReadLoop` are private and were explicitly marked "liftable for task #14". The implementer must decide the cleanest seam to let `MediaTransport`/`MeetingSession` send a `MediaConnectionUpdate` over the existing MC bidi stream (e.g. a typed public `SignalingClient` method vs. a lifted shared send helper). Prefer extract-on-second-use per the DRY note in that file.
- Trace injection helper: `packages/sdk-core/src/telemetry/tracePropagation.ts::injectIntoClientMessage` is structural (`TraceCarrier`) and already serves both `ClientMessage` and `MhClientMessage`.
- Framing: reuse `packages/sdk-core/src/framing/length-prefix.ts` (`encodeFrame`/`FrameDecoder`, 64 KiB cap).
- Transport: `IWebTransport` + `connect` default factory (inject for tests). `MockWebTransport` in `packages/test-utils/` is the unit-test double.
- Public surface lands through `packages/sdk-core/src/index.ts` (closed barrel — export new public types there).

## Open scope question for Gate 1 (resolve with @observability + @test)
R-24/R-25/R-26 (the `dt_client_*` join metrics + bounded join logs) are mapped to **task #12** (scaffolding), but their natural and only emission site — `MeetingSession.join` — lands **here**. Decide at Gate 1 whether this devloop wires the metric/log emission (recommended, since this is the orchestration home and the scaffolding `getMeter()`/`getMetricsSink()`/`logJoinEvent` already exist) or explicitly defers it to a follow-up with a tracked TODO. Whatever is decided, record it under §Scope decisions (NOT §Accepted Deferrals unless an in-diff finding is left).

---

## Implementer Plan (client) — iteration 1

### Mechanism restatement (mechanism, not requirement-IDs)
The browser must end a join holding: (a) ONE WebTransport signaling connection to MC (already built — `SignalingClient`), and (b) N parallel WebTransport connections to every MH URL MC handed back in `JoinResponse.media_servers`. For each MH we ONLY perform the auth handshake: open the first bidi stream and write a single 4-byte-BE length-prefixed `MhClientMessage{connectRequest{joinToken}}` envelope (trace-injected). No media frames, no datagrams, no SFrame/WebCodecs. "Connected" = `transport.ready` resolved AND that one envelope wrote without throwing (no MH ack exists this story). Once every MH attempt has settled, we send exactly ONE `ClientMessage{mediaConnectionUpdate}` back over the EXISTING MC stream, carrying one `MhConnectionStatus` per URL (CONNECTED/FAILED + optional failure fields + `observedAt`). `MeetingSession` is the composition root that drives auth → GC token → MC join → MH `connectAll` → update-send, exposes an explicit state machine + high-level events, and on `disconnect()` tears down every transport and drops token references.

### Design decisions

**A. SignalingClient seam (the post-join send path).** Add TWO public methods:
- `sendMediaConnectionUpdate(reports: readonly MhConnectionStatusReport[]): Promise<void>` (the send seam). Maps plain `MhConnectionStatusReport[]` → proto `MhConnectionStatus[]` (cap `mhUrl`/`failureReason`/`failureCode` ≤256 UTF-8 bytes via shared `capMhUrl`, `state`→`ConnectionState`, `observedAt`=`timestampFromDate(new Date(observedAtMs))`), wraps in `ClientMessage{mediaConnectionUpdate}`, and sends it on the STORED join stream.
- **DRY BLOCKER RESOLUTION + @code-reviewer-A RECONCILIATION (cross-reviewer).** @dry-reviewer requires a standalone shared pipeline (MediaTransport's MH `MhClientMessage` send opens its OWN stream and physically CANNOT call SignalingClient's private `#sendClientMessage` → without sharing it copy-pastes the pipeline). @code-reviewer-A wants `#sendClientMessage` kept private + the single inject site cohesively owned. SYNTHESIS satisfying both: extract the mechanical pipeline to a leaf `sendFramedMessage<T extends TraceCarrier>(stream, schema: GenMessage<T>, message: T): Promise<void>` at `framing/sendFramed.ts` doing `injectIntoClientMessage(message)` (sync, under caller's active context) → `toBinary(schema, message)` → `encodeFrame` → `getWriter().write()` → `releaseLock()`. This is the ONE inject site for BOTH envelopes (MORE cohesive than two, not "scattered"). `#sendClientMessage` is KEPT as a private THIN WRAPPER `=> sendFramedMessage(stream, ClientMessageSchema, msg)`, so `sendMediaConnectionUpdate` + `join` still go through SignalingClient's own private method (the MC cohesion @code-reviewer-A asked for), each inside `context.with(setSpan(active, this.#span), () => …)`. `MediaTransport.#connectOne` calls the leaf directly for the MH envelope (the second use @dry-reviewer identified). `framing/` siting (@dry-reviewer #3-layering): foundational layer both `signaling/` and `media/` already import down from — no inversion. NEEDS both reviewers to bless this synthesis (raised to @code-reviewer + @dry-reviewer + @team-lead).
- `runInJoinContext<T>(fn: () => T): T` — runs `fn` SYNCHRONOUSLY inside `context.with(setSpan(active, this.#span), fn)` (R-58 fix; NO span-ownership change — span stays in SignalingClient as `dt_client.join`, NOT renamed; no signaling-client.test.ts span-assertion changes). @code-reviewer StackContextManager finding: callers (MediaTransport) MUST wrap the SYNCHRONOUS `injectIntoClientMessage(envelope)` call — NOT the whole async `connectAll` — in `runInJoinContext`, because the sync StackContextManager loses the active context across `await ready`. No-op wrapper when `#span` is undefined.
- Requires storing the join bidi stream on the instance (`#stream`), which `#connectAndJoin` currently only holds locally. Guard: throws `SignalingError(Transport)` if called before join settled / after teardown.
- This keeps generated `*_pb` types OFF the public surface — the method takes the plain `MhConnectionStatusReport`, not a proto.

**B. MediaTransport (`media/MediaTransport.ts`).** `connectAll(urls, jwt)`:
- ORDER PINNING (test G1): SEED the internal per-URL status map in input `urls` order (each → `connecting` placeholder) BEFORE any await, so Map insertion order == input order and `getStatusReports()` is byte-stable regardless of which MH's `ready` settles first.
- Launches `#connectOne` for ALL urls CONCURRENTLY (`urls.map(...)` → `Promise.allSettled`), never awaited in a for-loop (a slow/hanging MH can't delay the others or the ≥1 resolve). `#connectOne` never rejects: `connectFn(url)` → **register transport into the teardown set IMMEDIATELY, BEFORE `await ready`** (so `disconnect()` can always reach an in-flight/never-ready transport — no leak) → `await ready` → `createBidirectionalStream` → build `MhClientMessage{connectRequest{joinToken:jwt}}` → send it via the SHARED `runInContext(() => sendFramedMessage(stream, MhClientMessageSchema, envelope))` (= `signaling.runInJoinContext`), so the synchronous `injectIntoClientMessage` inside `sendFramedMessage` runs under the active `dt_client.join` span AFTER the `await` (StackContextManager-correct — see §A; one shared frame/inject path, R-58). Success records `observedAtMs=clock()`, sets per-URL state `connected`, fires `connected(url)`. Any failure CLASSIFIES the cause into a bounded `failureCode` + STATIC `failureReason` (never `cause.message`), records `observedAtMs=clock()`, sets state `failed`, builds a `MediaConnectionError` (carrying capped url + non-enumerable cause), fires `failed(err)`.
- **BOUNDED PER-MH CONNECT DEADLINE (@operations).** Each `#connectOne` arms a `setTimeout(connectTimeoutMs)` at start (mirrors SignalingClient's join-timer idiom: armed before awaiting `ready`, cleared on settle). On fire: mark that MH `failed` with a bounded `CONNECT_TIMEOUT` `failureCode` + static reason, tear down its transport, settle that `#connectOne`. REQUIRED because WebTransport `ready` can hang unbounded (no transport-level timeout knob) — without it the `allSettled` resolve (and thus the `MediaConnectionUpdate` to MC) could be delayed/never. `connectTimeoutMs` is configurable, default shorter than SignalingClient's 15s join (connect-only; proposing 10s).
- RESOLVE SEMANTICS: `allSettled`-style — wait for every `#connectOne` to SETTLE (each bounded by the deadline), then resolve with the reports if ≥1 `connected`, else reject `MediaConnectionError(AllFailed)` (fixed bounded message, no per-MH concat). The per-MH deadline guarantees bounded resolution and that the `MediaConnectionUpdate` carries COMPLETE per-MH states (timed-out MH appears as FAILED).
- `getStatusReports()` projects the seeded map → index-ordered `MhConnectionStatusReport[]` (so MeetingSession can send the update even on the all-fail rejection). `getState(url)` exposes `connecting|connected|failed`.
- `disconnect()` tears down EVERY stored transport via a SINGLE `#terminated`-guarded path (mirrors SignalingClient `#terminate`/`#teardown`): per-transport try/swallow (one throwing/slow `close()` can't abort the rest), clears all per-MH timers, idempotent — no dangling transports/writers/timers on partial failure or double-call.
- Injected: `connect` factory (default prod `connect`), `clock` (default `Date.now`), `connectTimeoutMs`, `metricsSink` (default `getMetricsSink()`), `runInContext` (default identity; MeetingSession passes `signaling.runInJoinContext` for R-58).

**C. MeetingSession (`session/MeetingSession.ts`)** — public entry point (PATH B: does NOT own the trace root span; the SINGLE `dt_client.join` span stays in SignalingClient covering signaling — so NO twin-span anti-pattern, @observability Q1). Explicit state machine via a const-object `MeetingSessionState` (`Idle → FetchingToken → ConnectingMc → Joining → Joined`, plus `Disconnecting`); `#setState` emits a `stateChange` event and updates the `state` getter. `#joinStartMs = clock()` anchors both histograms. After GC join it computes the JOIN LABEL SET (@observability §2 — the sink does NOT auto-attach implicit labels, so every emission must pass them): `client_version=__SDK_VERSION__`; `meeting_id_hash` = full SHA-256 over the `meetingId` UUID (NOT the low-entropy meetingCode) via `crypto.subtle.digest('SHA-256', …)`, hex-encoded, TRUNCATED TO 16 HEX CHARS (64 bits, no salt needed — UUID is 122-bit; R-32-clean); `org_id` = `orgSubdomain` PLAIN (a public org-level non-PII identifier; never hashed). The SAME `meeting_id_hash` is reused across all metrics. The label set + `metricsSink` are THREADED into `MediaTransport` (which emits `mh_connection_total`). `join({orgSubdomain, meetingCode, credentials})`:
1. `FetchingToken`: `AuthApiClient.login|register` (discriminated `credentials.mode`) → user token (`#userToken`); `MeetingApiClient.joinMeeting(meetingCode,{userToken})` → meeting token (`#meetingToken`) + `mcAssignment.webtransportEndpoint` + `meetingId`.
2. `ConnectingMc`: construct `SignalingClient`, bridge `participantJoined/participantLeft/error` → session events, `await signaling.join({...})` → `JoinedEvent` (`mediaServers`); record `dt_client_time_to_signaling_ready_ms`.
3. `Joining`: construct `MediaTransport` (passing `runInContext: signaling.runInJoinContext` for R-58), bridge `connected`→`mediaConnected` (first one records `dt_client_time_to_first_mh_connected_ms`), run `connectAll(mediaServers,#meetingToken)` in `try/catch` (catch = all-fail); ALWAYS `signaling.sendMediaConnectionUpdate(media.getStatusReports())` afterwards (R-21 "once settled… send ONE update").
4. ≥1 MH connected → `Joined`, emit `joined`. All MH failed → emit `error(allFailed)`, `disconnect()`.
- METRICS (R-24/R-25, all five in-scope; emit sites per @observability final): (a) `MeetingSession` emits FOUR via the threaded `metricsSink`: a single `#settleJoin(status, failureStage?)` helper (reached from BOTH the success path and every catch) emits `dt_client_join_attempts_total{status,failure_stage}` exactly once (`status`∈`success|failure`, NOT `outcome`; `failure_stage`∈`none,signup,gc_create_token,gc_join,mc_signaling_connect,mc_join_response,mh_connect,internal`); the two histograms `dt_client_time_to_signaling_ready_ms` + `dt_client_time_to_first_mh_connected_ms` off `#joinStartMs`; and **`dt_client_signaling_connection_total{status,close_reason}` FACADE-DERIVED** from `signaling.join` resolve/reject (@observability ACCEPTED facade-derivation — no telemetry plumbing into shipped SignalingClient). success → `close_reason=normal` (sentinel); failure → mapped from the `SignalingError` — close_reason via a `SignalingErrorCode`→`CloseReason` map (Unauthorized/Forbidden→`auth_failed`, Timeout→`timeout`, InternalError→`server_error`, else→`unknown`), since the join-reject `SignalingError` often has NO numeric `closeCode` (timeout/ErrorMessage paths) so `normalizeCloseReason(closeCode)` alone would collapse to `unknown` (@observability condition 2 — real mapping, not all-unknown). (b) `MediaTransport` emits `dt_client_mh_connection_total{status,mh_index_bucket=0|1|2+}` per MH settle (threaded labels). Every emission carries the full join label set (`client_version`/`meeting_id_hash`/`org_id`) and routes through the injected `metricsSink` (default `getMetricsSink()`; no-op when unconfigured) — never the raw `meter`. PII: `userId`/`email` from `RegisterResponse` NEVER reach a label (label set is exactly the three). **R-26 bounded LOGS DEFERRED** (PATH B) — NO `logJoinEvent` call left in `join`; tracked in `docs/TODO.md` §Observability Debt + §Scope decisions.
- `disconnect()`: `Disconnecting` → `media?.disconnect()` + `signaling?.close()` → drop `#userToken`/`#meetingToken` references (JS strings are immutable — we null the refs; there are no long-lived `Bearer` buffers to overwrite, the `Authorization` strings are per-call transients inside `MeetingApiClient`; documented).
- Static `MeetingSession.configure(config)` delegates to `configureTelemetry` (the contract telemetryConfig.ts already documents).
- Errors bubble through `onError`; any failure during `join` routes to `disconnect()`.

**D. MediaConnectionError (`errors/MediaConnectionError.ts`)** extends `SdkError` (inherits the one `toJSON` redaction allowlist). `mediaCode` is a CONST-OBJECT UNION (@code-reviewer C, mirroring `SignalingErrorCode`/`SdkErrorCode` per ADR-0011): `export const MediaConnectionErrorCode = { Transport: 'TRANSPORT', AllFailed: 'ALL_FAILED', ConnectTimeout: 'CONNECT_TIMEOUT' } as const` + the derived type — so observability maps each cause to a bounded label without coupling to message strings. Plus capped `mhUrl`; `toJSON` emits only `mediaCode`+`mhUrl` (both non-secret) on top of the base allowlist. Cause kept on non-enumerable `Error.cause` only — mirrors `SignalingError`. The JWT is NEVER passed to or stored on this error. Boilerplate (@code-reviewer F): sets `this.name='MediaConnectionError'` + `Object.setPrototypeOf(this, MediaConnectionError.prototype)`, and OMITS optional fields (`mhUrl`/`failureReason`/`failureCode`) rather than assigning `undefined` (exactOptionalPropertyTypes).

**E. capMhUrl util — sited in `validation/limits.ts` (@dry-reviewer #3).** Add `MAX_MH_URL_BYTES = 256` + a `capUtf8Bytes(s, max)` (used as `capMhUrl`) to the existing bounds module (which already houses MC-anchored limits — `MAX_MESSAGE_SIZE`, subdomain, meeting-code — under the same "match the server's accepted shape" convention; the 256 bound is anchored to MC's `floor_char_boundary`, proto:309). UTF-8 CHAR-BOUNDARY-SAFE truncation: measure BYTES via `TextEncoder` (NOT `.length`), stop before a codepoint would exceed the cap — never split a multibyte sequence. Shared by `MediaTransport`/`MediaConnectionError` AND `SignalingClient`'s `MhConnectionStatus` mapping; applied to `mh_url` AND `failure_reason`/`failure_code` (all client-controlled per the proto comment). Siting it in `validation/` (a foundational layer both signaling and media import DOWN from) avoids the signaling→media import inversion a `media/mhUrl.ts` home would force.

**E'. Bounded failure fields (security).** Per-MH `failureCode` is a small SDK enum-as-string (e.g. `TRANSPORT_ERROR`, mirroring `SignalingErrorCode`); `failureReason` is a STATIC SDK message. The raw transport-reject cause is NEVER stringified into either — it lives only on the non-enumerable `Error.cause` of `MediaConnectionError`. The all-fail aggregate error carries a static message + `mhUrl=''`, no per-MH string concatenation, no token. Connect targets come STRICTLY from the authenticated `JoinResponse.media_servers` (one-directional: JoinResponse → connect → report); the reported-back `mhUrl` is never fed into a connect target.

**F. R-32:** the ONE crypto touch is `meeting_id_hash` = `crypto.subtle.digest('SHA-256', …)` over the meeting id (a digest, NOT randomness; NEVER `Math.random`; no `CryptoKey`/`extractable` surface). Satisfied via SubtleCrypto. The task #9 `no-secrets`/lint rules still apply.

**G. Shared typed event-emitter (@dry-reviewer — extract now).** The on/off/`#emit` listener-registry idiom is now used by THREE classes (`SignalingClient` existing + `MediaTransport` + `MeetingSession` new) → extract a small generic `TypedEventEmitter<EventMap>` (e.g. `events/TypedEventEmitter.ts`) providing `on`/`off`/protected `emit`, copy-on-emit semantics (a listener unsubscribing mid-emit can't mutate iteration). `MediaTransport`/`MeetingSession` extend it; `SignalingClient` is refactored onto it (keeps its public `on`/`off` signatures unchanged). One emitter implementation, three typed event maps.

### SCOPE DECISION — PATH B (@team-lead, locked at Gate 1 via the reconciliation handshake; single source of truth)
NO span rename/relocation — the `dt_client.join` span STAYS in SignalingClient (task #13), unchanged; no signaling-client.test.ts span-assertion changes. R-58 fixed via `SignalingClient.runInJoinContext(fn)` wrapping the SYNCHRONOUS inject (StackContextManager-correct). All FIVE metrics in-scope NOW; R-26 logs DEFERRED-with-tracking (TODO entry KEPT).

**R-25 — ALL FIVE metrics:**
- `dt_client_join_attempts_total{status=success|failure, failure_stage}` — label `status` (NOT `outcome`); failure_stage ∈ `none,signup,gc_create_token,gc_join,mc_signaling_connect,mc_join_response,mh_connect,internal`.
- `dt_client_time_to_signaling_ready_ms` (histogram) — join start → MC JoinResponse; timed in `MeetingSession`.
- `dt_client_time_to_first_mh_connected_ms` (histogram) — join start → first MH connected; timed in `MeetingSession` (first `mediaConnected`).
- `dt_client_signaling_connection_total{status, close_reason}` — FACADE-DERIVED, emitted from `MeetingSession` off `signaling.join` resolve/reject (@observability ACCEPTED — no telemetry plumbing into shipped SignalingClient). success → `close_reason=normal` sentinel; failure → `SignalingErrorCode`→`CloseReason` map (Unauthorized/Forbidden→`auth_failed`, Timeout→`timeout`, InternalError→`server_error`, else→`unknown`) — real mapping, not all-`unknown`, since the reject error usually lacks a numeric closeCode.
- `dt_client_mh_connection_total{status, mh_index_bucket=0|1|2+}` — emitted from `MediaTransport.connectAll`; bucket the raw index.
- Implicit label set on ALL: `client_version=__SDK_VERSION__`, `meeting_id_hash` = SHA-256 over the meetingId UUID, hex, truncated to **16 hex chars (64-bit, no salt)**, `org_id` = `orgSubdomain` plain. Conventions documented in `client.md` (client originates them — Q2/Q3).
PII hard-fail: no user_id/email/ip/user_agent/raw-meeting-id label. All metrics route through `getMetricsSink()` (no direct `meter`). VERIFY-FLAGS (@observability, not blockers): (1) the GC telemetry-proxy 12-key allowlist may expect `dt.`-prefixed attribute forms — confirm bare metric label keys (`status`/`failure_stage`/`close_reason`/`mh_index_bucket`) pass the METRICS-signal allowlist end-to-end, else flag the proxy owner (infrastructure); (2) `meeting_id_hash` as a metric label is one series per meeting (unbounded over time) — tension with ADR-0011 ≤1000-combo; it's task #12's catalog decision, confirm the proxy/backend aggregates-or-drops it for METRIC series.

**R-26 bounded LOGS — DEFERRED with tracking (PATH B).** Root cause: the whole-join `trace_id` the `JoinLogRecord` needs would require relocating the `dt_client.join` root span from `SignalingClient` to `MeetingSession.join` (task #13 span semantics) — its own planning. Tracked via the `docs/TODO.md` §Observability Debt entry + the §Scope decisions pointer (both KEPT). NO `logJoinEvent` call left in `join` (no half-wired/uncovered lines). Forward-looking scope split, not an in-diff finding.

### Test plan (~19-21 Vitest, Node; `MockWebTransport`; runtime-built protobuf, no fixtures; coverage ≥90% is the gate, not the count)
`media/__tests__/media-transport.test.ts`: (1) connectAll all-connect → all `connected` fired + resolves, index-ordered reports; (2) partial → resolves, mixed per-URL state, `failed` fires `MediaConnectionError`; reports index-stable — urls `[A,B,C]` with B failing → `statuses == [A:CONNECTED, B:FAILED, C:CONNECTED]` in EXACT order (G1); (3) all-fail → rejects `MediaConnectionError(AllFailed)`, `getStatusReports()` all FAILED; (4) `MhClientMessage` envelope decoded off MH stream0 = `{connectRequest{joinToken}}`, trace fields EMPTY when telemetry unconfigured; (5) trace fields POPULATED through the REAL path — `connectAll` with `runInContext = signaling.runInJoinContext` and a real active `dt_client.join` span: assert the decoded MH envelope's `traceParent` trace-id EQUALS the signaling join span's trace-id (StackContextManager-correct, NOT a hand-set span); (6) redaction depth (G3) — `MediaConnectionError` from a REAL failed handshake carrying the real jwt: jwt substring absent from `toJSON()`, `JSON.stringify`, `.message`, AND `.stack`; PLUS wire-level (G3 addendum) — decode the outbound `MediaConnectionUpdate` off the MC stream, assert the jwt is in NO `MhConnectionStatus.failureReason`/`failureCode`; (7) `getState` observable; (8) `disconnect()` idempotent (double-call), and tears down an IN-FLIGHT never-ready transport (registered before `ready`) (G4/@operations); (8b) per-MH CONNECT DEADLINE — a never-ready MH (fake timers) → that MH settles FAILED with `CONNECT_TIMEOUT`, `connectAll` still resolves bounded if another connected, and the timed-out MH appears FAILED in `getStatusReports()` (@operations); (M1) `dt_client_mh_connection_total{status,mh_index_bucket}` emitted per MH into an injected `InMemoryMetricsSink`.
`validation/__tests__/limits.test.ts` (new): (9) `capMhUrl`/`capUtf8Bytes` truncates >256-BYTE input at a CODEPOINT boundary (multibyte input, no split; `TextEncoder`-measured) (G2).
`signaling/__tests__/signaling-client.test.ts` (extend): `sendMediaConnectionUpdate` — (10) all-connected shape (decode MC stream → one `MhConnectionStatus` per URL, CONNECTED, `observedAt` set); (11) partial shape (FAILED + bounded failureReason/failureCode); (12) all-failed shape; (13) trace injected on the update; (14) `mhUrl` AND `failureReason` >256 bytes capped (G2); (15) throws `SignalingError(Transport)` when called before join settles / after teardown (G4); (15b) `runInJoinContext(fn)` runs `fn` SYNCHRONOUSLY under the active `dt_client.join` span (PATH B — span name UNCHANGED; no rename, no signaling span-assertion changes).
`session/__tests__/meeting-session.test.ts`: (16) state sequence — success `idle→fetching-token→connecting-mc→joining→joined` AND failure `…→Disconnecting` via the `stateChange` event, asserting the FULL captured ordered array (Q1); (17) emits `joined`+`mediaConnected`, bridges `participantJoined`; (18) `disconnect()` closes media+signaling transports (spies), lands in `Disconnecting`, idempotent (G4); (M2) join-metric emission across success / partial / all-fail into an injected `InMemoryMetricsSink` — asserts `dt_client_join_attempts_total{status,failure_stage}` on BOTH outcomes, both histograms recorded, `dt_client_signaling_connection_total{status,close_reason}`, and implicit labels (`client_version`, `meeting_id_hash`, `org_id`) present + no PII labels. (PATH B: R-26 logs DEFERRED — no `logJoinEvent` emission/test in this slice; a negative check that `join` does NOT call the logger guards against half-wiring.)

## Cross-Boundary Classification

| File (planned) | Classification | Owner | Notes |
|----------------|----------------|-------|-------|
| `packages/sdk-core/src/media/MediaTransport.ts` (new) | Mine | client | active/active MH handshake |
| `packages/sdk-core/src/media/events.ts` (new) | Mine | client | plain public types: `MediaConnectionState`, `MediaTransportEventMap`, options |
| `packages/sdk-core/src/signaling/events.ts` (edit) | Mine | client | add plain `MhConnectionStatusReport` (@dry-reviewer #4: lives in the foundational signaling layer beside `SignalingJoinParams`; media→signaling edge, never signaling→media) |
| `packages/sdk-core/src/validation/limits.ts` (edit) | Mine | client | add `MAX_MH_URL_BYTES=256` + `capUtf8Bytes`/`capMhUrl` (@dry-reviewer #3: foundational bounds module, MC-anchored) |
| `packages/sdk-core/src/session/MeetingSession.ts` (new) | Mine | client | composition root + state machine |
| `packages/sdk-core/src/session/events.ts` (new) | Mine | client | `JoinOptions`/credentials, `MeetingSessionState`, event map |
| `packages/sdk-core/src/errors/MediaConnectionError.ts` (new) | Mine | client | typed error, redacting `toJSON`; const-object `MediaConnectionErrorCode` |
| `packages/sdk-core/src/errors/SdkError.ts` (edit) | Mine | client | add `SdkErrorCode.Media` discriminant (base code for MediaConnectionError) — small in-domain addition not in the original list |
| `packages/sdk-core/src/framing/sendFramed.ts` (new) | Mine | client | shared `sendFramedMessage(stream, schema, message)` — inject+toBinary+encodeFrame+write (MC+MH reuse, @dry-reviewer #1; foundational layer #3) |
| `packages/sdk-core/src/events/TypedEventEmitter.ts` (new) | Mine | client | shared generic on/off/emit registry (@dry-reviewer; 3rd use) |
| `packages/sdk-core/src/signaling/SignalingClient.ts` (edit) | Mine | client | add `#stream` + public `sendMediaConnectionUpdate` + public `runInJoinContext`; `#sendClientMessage` KEPT as thin private wrapper over shared `sendFramedMessage`; refactor onto `TypedEventEmitter`. PATH B: NO span rename, NO metric emission (signaling_connection_total is facade-derived in MeetingSession) |
| `docs/observability/metrics/client.md` (edit) | Not mine, Minor-judgment | observability | observability-domain catalog, NOT a GSA (no wire-format/auth/forensics/schema → no trailer); bounded content @observability requested (has "client.md catalog updates" in its Gate-3 acceptance checklist = owner confirmation): document `meeting_id_hash`=SHA-256(meetingId UUID)→16 hex; `org_id`=subdomain (not server UUID); `signaling_connection_total` success⇒`close_reason=normal` sentinel |
| `docs/TODO.md` (edit) | Not mine, Minor-judgment | (shared hygiene doc) | append-only: R-26 logs deferral entry under §Observability Debt + the doc-hygiene non-blockers; not a GSA (no wire-format/auth/forensics/schema → no trailer) |
| `packages/sdk-core/src/index.ts` (edit) | Mine | client | export new public types (no `*_pb` leakage) |
| `packages/sdk-core/src/errors/index.ts` (edit) | Mine | client | internal re-export of `MediaConnectionError` |
| `packages/sdk-core/src/media/__tests__/media-transport.test.ts` (new) | Mine | client | tests |
| `packages/sdk-core/src/media/__tests__/helpers.ts` (new) | Mine | client | test-local decode helpers (`decodeMhClientMessages`/`decodeClientMessages`) + `makeConnect` url→mock factory + `waitFor` |
| `packages/sdk-core/src/events/__tests__/TypedEventEmitter.test.ts` (new) | Mine | client | shared-emitter unit tests |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` (new) | Mine | client | tests |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` (edit) | Mine | client | add `sendMediaConnectionUpdate` + `runInJoinContext` cases (PATH B: span name UNCHANGED, no span-assertion edits) |
| `packages/sdk-core/src/validation/__tests__/limits.test.ts` (new) | Mine | client | `capMhUrl`/`capUtf8Bytes` UTF-8 byte-cap test (G2) |

No Guarded Shared Areas. No proto edits (proto already landed). No `packages/test-utils/**` change needed — `MockWebTransport` + `InMemoryMetricsSink` already cover the MH handshake + metric doubles.

### Implementation notes (iteration 1 — built + validated)
- **VALIDATION (local):** `tsc --noEmit` + `eslint` + `prettier --check` all clean. `vitest run --coverage`: 171 tests pass; coverage stmts 97.73% / branches 90.55% / funcs 97.59% / lines 98.80% — all ≥90% (Gate-2). Component tier (prod `vite build` + `bundle-content`) passes. Public `dist/index.d.ts` verified to contain ZERO generated `*_pb`/proto-path references (plain shapes only).
- **DEVIATION 1 (flag for @semantic-guard/@code-reviewer):** added `SdkErrorCode.Media = 'MEDIA'` to `errors/SdkError.ts` as the base discriminant for `MediaConnectionError` (mirrors how `SignalingError` uses `SdkErrorCode.Signaling`). Small in-domain enum addition; was not in the originally-approved file list.
- **DEVIATION 2 (flag for @security — strictly STRONGER R-23):** rather than store `#userToken`/`#meetingToken` as instance fields and null them in `disconnect()` (the planned wording), `MeetingSession.join` holds both tokens ONLY as `join()`-scoped locals — they are NEVER assigned to instance fields, so no token-bearing reference outlives the call (GC-eligible the moment `join()` settles). `disconnect()` is therefore transport-only with no token state to zero. This satisfies "no persistent token holder outlives disconnect" more strongly than nulling would; the eslint `no-unused-private-class-members` rule (write-only privates) also pushed this direction.

---

## Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed on PATH B (GOVERNING RULE = B, A-token superseded, re-frozen to B; 5 metrics in-scope; R-58 via runInJoinContext + sync-inject; R-26 logs DEFERRED-with-TODO → RESOLVED-DEFERRED; signaling span keeps `dt_client.join`, no rename) |
| Code Quality | confirmed (StackContextManager fix verified; seam synthesis blessed; C/D1/D2/E/F adopted; ADR-0028/0011/0019/0024 ✓; Ownership Lens: all-Mine, no GSA) — 2 Gate-3 verify-at-review notes: mediaCode vs failureCode enum naming; keep sendFramed proto-typed signature off public barrel |
| DRY | confirmed (single `sendFramedMessage` leaf in `framing/sendFramed.ts` reused by all 3 send sites; `#sendClientMessage` KEPT as thin private wrapper over it per the code-reviewer synthesis; shared `TypedEventEmitter` → rides as code-reviewer finding E; no TODO extraction-opportunities) |
| Operations | confirmed (per-MH connect deadline, idempotent teardown, all-fail update-send all in plan) |
| Semantic Guard | confirmed |

---

## Gate 2 — Validation

**Verdict: PASS on task #14's own diff.** Pipeline: `./scripts/layer-all.sh` (start commit f34f3a6; diff = `packages/sdk-core/**` + `docs/observability/metrics/client.md` + `docs/TODO.md`).

| Layer | Result | Notes |
|-------|--------|-------|
| 1 Compile/typecheck | OK | sdk-core + test-utils tsc clean |
| 2 Format | OK | nx format + buf format clean |
| 3 Guards | 31/33 PASS | scope-drift (ours) FIXED — 2 test rows added; 2 remaining are PRE-EXISTING (knowledge-index infra/INDEX.md §6.7; todo-tracking #56 main.md:315), operations-ruled non-blocking, tracked in docs/TODO.md §Documentation Hygiene |
| 4 Test | OK | Rust workspace 389×2 + 171 TS tests; sdk-core coverage 97.73/90.55/97.59/98.80 (≥90%) |
| 5 Lint | OK | nx lint clean |
| 6 Audit | OK | cargo-audit, pnpm-audit, buf-breaking |
| 7 Env-tests | PRECONDITION (no cluster) | no `kind` binary + dead KUBECONFIG; operator lane; zero task-#14 coverage (browser-E2E is task #40/#44; Rust env-tests unchanged R-46); operations-aware, non-blocking |

Operations ruling (pipeline-health): the 2 pre-existing Layer-3 guard failures + Layer-7 no-cluster precondition do NOT block task #14's Gate 2 — they're branch/infra health failing on prior content task #14 never touched. Verified HEAD==f34f3a6 (all task-#14 changes uncommitted). Branch-health fixes tracked for separate repo-hygiene commits (owners: infrastructure, #56-doc).

Implementer deviations (recorded for Gate-3 eyeball): (1) `SdkErrorCode.Media` added to errors/SdkError.ts (in-domain enum); (2) R-23 STRICTLY STRONGER — MeetingSession holds tokens only as `join()`-scoped locals, never instance fields.

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | — | — | R-23 verified stronger (join-scoped locals); R-32 SubtleCrypto-only; caps on all 3 client fields. 1 cosmetic nit routed to code-reviewer (timeout msg) |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | R-41 lens all clean; F-T1 FIXED: end-to-end on-wire redaction guard (decodes real MediaConnectionUpdate, asserts failureReason/failureCode JWT-clean) — stronger than proposed. 177 tests, coverage 97.75/90.18/97.60/98.82 (≥90%), 3× no flake |
| Observability | RESOLVED-DEFERRED (final, clear to commit) | 3 | 2 | 1 (R-26 logs) | F1 (failure_stage 8/8, server-vs-local split) + F2 (meeting_id_hash='none' doc) VERIFIED FIXED; R-58 sync-inject test-proven; 5 metrics PII-clean. Co-signed client.md `Approved-Cross-Boundary: observability`. Accepted deferral = R-26 logs → docs/TODO.md §Observability Debt |
| Code Quality | RESOLVED-FIXED | 3 | 3 | 0 | F1 (timeout msg=mediaCode +test), F3 (signalingFailureStage local-vs-server split → mc_join_response/mc_signaling_connect, wired +tests; gc_create_token catalog-honesty), F2 (client.md+TODO.md rows added) all fixed. ADR-0028/0011/0019/0024 ✓. COMMIT-TIME: `Approved-Cross-Boundary: observability` trailer required (client.md). Zero deferrals/escalations |
| DRY | CLEAR | 0 | — | — | send pipeline single-sourced (1 leaf, 3 sites); 1 shared TypedEventEmitter; import inversions gone; no extraction debt |
| Operations | RESOLVED-FIXED | 1 | 1 | 0 | disconnect() teardown moved to `finally` (throwing stateChange listener can't strand transports) + test; verified per-MH deadline, idempotent teardown, all-fail-send content-asserted. Pre-existing SignalingClient.#terminate same-shape → tracked in docs/TODO.md (follow-up, not in-diff) |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | error-context FIXED: transport.ready rejection cause captured → non-enumerable Error.cause + leak-free test; credential-leak/async-blocking/metrics-path-completeness CLEAR |

---

## Scope decisions
<!-- Devloop-local scope notes (e.g., "X belongs to task Y"). NOT deferrals. -->
- **PATH B — locked at Gate 1 (@team-lead reconciliation handshake; single source of truth).** NO span relocation/rename: the `dt_client.join` span STAYS in `SignalingClient` (task #13), unchanged. R-58 fixed via `SignalingClient.runInJoinContext(fn)` wrapping the SYNCHRONOUS inject at the `#connectOne` site (StackContextManager-correct — NOT a coarse `context.with` around async `connectAll`).
- **R-24/R-25 — all five join METRICS WIRED IN-SCOPE** (`dt_client_join_attempts_total` + both histograms from MeetingSession; `dt_client_signaling_connection_total` from SignalingClient; `dt_client_mh_connection_total` from MediaTransport; join label set `client_version`/`meeting_id_hash`/`org_id` threaded to all sites — @observability §2). None need `trace_id`. Names/labels in §SCOPE DECISION.
- **R-26 — bounded join LOGS DEFERRED with tracking** — see `docs/TODO.md` §Observability Debt (KEPT). Root cause: the whole-join `trace_id` requires relocating the `dt_client.join` root span from `SignalingClient` to `MeetingSession.join` (task #13 span semantics), its own planning. No `logJoinEvent` call left in `join`. Forward-looking scope split, NOT an in-diff finding.

## Accepted Deferrals
<!-- One pointer per docs/TODO.md entry. -->
No in-diff reviewer findings were left unresolved — all Gate-3 findings (Security nit, Test F-T1, Code Quality F1/F2/F3, Operations disconnect, Semantic Guard error-context, Observability F1/F2) were FIXED in this PR. The accepted deferral + spun-out follow-ups (forward-looking, not in-diff findings) tracked in `docs/TODO.md`:

- **R-26 bounded join LOGS** (the accepted deferral driving @observability's RESOLVED-DEFERRED verdict; a Gate-1 scope split, not an in-diff finding — see §Scope decisions) → `docs/TODO.md` §Observability Debt. Root cause: whole-join `trace_id` needs the `dt_client.join` root span relocated from SignalingClient to MeetingSession.join (task #13 semantics, own planning).
- **SignalingClient.#terminate emit-before-teardown hardening** (operations spun-out follow-up; pre-existing — the diff only renamed `#emit`→`emit`) → `docs/TODO.md` §Client Architecture. Symmetric ~2-line try/finally, same shape as the task-#14 MeetingSession.disconnect fix. Owner: client.
- **Pre-existing branch-health (operations-ruled non-blocking, NOT task #14)** → `docs/TODO.md` §Documentation Hygiene: infra/INDEX.md `§6.7`-inside-backticks (knowledge-index); #56 fill-layer7 main.md §Accepted-Deferrals numbered-paragraphs (todo-tracking). Fix as separate repo-hygiene commits (owners: infrastructure, #56-doc).
