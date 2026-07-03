# Devloop Output: GC OTel Wiring (Task #26)

**Date**: 2026-07-03
**Task**: Wire OpenTelemetry into global-controller — `main.rs` `init_otel`, Config knobs surfaced in configmap, R-56 gRPC interceptor on outbound AC/MC clients + inbound HTTP trace-context extraction, integration tests for trace continuity.
**Specialist**: global-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task-26`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1fe546be10747860e74715d83f81d225336ad83b` |
| Branch | `feature/browser-client-join-task-26` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `global-controller (self)` |
| Implementing Specialist | `global-controller` |
| Paired Overlay | `--paired-with=observability` (otel_http.rs co-implementation; added at Gate 1 to satisfy Domain-judgment) |
| Iteration | `1` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `RESOLVED-DEFERRED` (1 finding fixed; 1 extraction-opportunity breadcrumb → TODO.md) |
| Operations | `CLEAR` |
| Semantic Guard | `CLEAR` (native SAFE) |

**Gate 1 (Plan Approval): PASSED** — all 7 reviewers confirmed; classification-sanity guard `STATUS=OK`; one ESCALATE (otel_http.rs classification) adjudicated → Domain-judgment via `--paired-with=observability`. "Plan approved" issued 2026-07-03.

**Gate 2 (Validation): PASSED for task-26 scope** (L1-6 attempt 1, findings fixed within the attempt).
- The validator's base ref resolved to `merge-base origin/main HEAD` (58fc6bd) → 263-file diff, swamping task-26's ~27 files with 249 prior committed story files (no `DEVLOOP_BASE_SHA`-style override for the general base ref exists; proto-only). Results were disentangled manually.
- **Task-26 layers/guards: all green** — Rust compile/fmt/test/clippy/audit pass (common 220/220, gc-service 340/340 + integration binaries incl. the 4 new otel tests); Layer 3 guards `validate-cross-boundary-scope`, `api-version-check`, `validate-cross-boundary-classification` all `STATUS=OK` after fixes; Layers 2/4/5/6/7 no FAIL (env-tests passed).
- **Two task-26 findings found & fixed in-attempt**: (1) scope-drift — classification table was missing root `Cargo.toml` + `tests/common/mod.rs` + `tests/common/otel_support.rs`, and the `docs/TODO.md` cell's long parenthetical broke the matcher (shortened to `docs/TODO.md (update)`); (2) api-version-check flagged 3 test-harness `route("/")` in `middleware/otel.rs` — `api_version.rs` has no `guard:ignore` support, so the test-only dummy routes were renamed to a versioned literal `/api/v1/otel-test-target`.
- **Pre-existing branch reds — root-caused and FIXED (not task-26 code, but blocking every backend devloop)**: On investigation (not "incomplete client work" as first suspected — corrected), the Layer 1 `nx-typecheck-failed` was an **nx wiring bug**: the inferred `typecheck` target lacked the `proto-gen:codegen` dependency its sibling targets (build/test:unit/test:component/lint) all declare, so `nx affected -t typecheck` ran `tsc` against ungenerated (git-ignored) `*_pb.ts` artifacts → `TS2307`. Fixed workspace-wide in `nx.json` `targetDefaults` (`typecheck` dependsOn `^codegen`) as a standalone direct commit `6e75c6a`. The `validate-todo-tracking` red (inlined body in task #14's §Accepted Deferrals) fixed in `a3396c8`. **After both fixes the whole-branch pipeline is fully green** (`GATE2=PASS`, all layers OK, Layer 4 = 3109 tests / 0 failed, Layer 7 env-tests OK), so task #26 committed on a real PASS verdict with **no `--no-verify` bypass**. Commit: `5913e13`.

**Gate 3 (Final Approval): PASSED** — all 7 verdicts collected, zero ESCALATED. Security CLEAR, Test CLEAR, Observability CLEAR, Code Quality CLEAR, Operations CLEAR, Semantic Guard CLEAR (SAFE), DRY RESOLVED-DEFERRED (its one real finding — `trace_headers()` duplication — was **fixed** in-review by hoisting to `services/mod.rs`; the deferred label is solely the ADR-0019 forward extraction-opportunity breadcrumb logged to `docs/TODO.md`, not unfixed code). Each reviewer independently verified (built/ran tests, not read-only): observability re-verified post-hoist; security traced GSA-empty + auth-non-leak; code-reviewer source-verified the tonic-vs-axum ordering + api_version guard-gap. Pre-commit re-verify after the DRY hoist: `validate-cross-boundary-scope`, `validate-cross-boundary-classification`, `api-version-check` all `STATUS=OK`; `cargo fmt --check` + `cargo check -p gc-service -p common --all-targets` clean.

---

## Task Overview

### Objective
Wire OTel into GC (R-55 partial, R-56 partial): `init_otel()` in `main.rs`, three Config knobs
(`otel_enabled`/`otel_endpoint`/`otel_sample_rate`) surfaced in `infra/services/gc-service/configmap.yaml`,
R-56 gRPC propagation interceptor on GC's outbound `AcAuthClient` (GC→AC) and any MC client, plus an
inbound HTTP trace-context extraction layer on the Axum router (feeds the same global propagator that the
`/api/v1/telemetry` proxy relies on). Integration tests: GC→AC trace continuity, HTTP→gRPC bridge,
telemetry-proxy preserves trace context.

### Scope
- **Service(s)**: global-controller (+ config surfacing in `infra/services/gc-service/`)
- **Schema**: No
- **Cross-cutting**: Consumes `crates/common/src/observability/otel.rs` + `otel_grpc.rs` (from task #24, Completed). Mirrors AC wiring (task #25, Completed).

### Debate Decision
NOT NEEDED — implementation follows the R-54/R-55/R-56 design already ratified and the task #25 (AC) exemplar.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/gc-service/src/config.rs` | Mine, Domain-judgment | — |
| `crates/gc-service/src/main.rs` | Mine, Domain-judgment | — |
| `crates/gc-service/src/routes/mod.rs` | Mine, Domain-judgment | — |
| `crates/gc-service/src/services/ac_client.rs` | Mine, Domain-judgment | — |
| `crates/gc-service/src/services/mc_client.rs` | Mine, Mechanical | — |
| `crates/gc-service/src/services/telemetry_forwarder.rs` | Mine, Mechanical | — |
| `crates/gc-service/src/services/mod.rs` (hoisted shared `trace_headers()` helper, per @dry-reviewer's fix-it finding: deduped the identical inline "build HeaderMap + inject" pattern out of `ac_client.rs` and `telemetry_forwarder.rs`) | Mine, Mechanical | — |
| `crates/gc-service/src/main.rs` (inbound gRPC server-interceptor wiring — same file as main.rs row above, called out separately since it's a distinct hunk) | Mine, Domain-judgment | — |
| `crates/gc-service/src/middleware/otel.rs` (NEW — thin axum wrapper calling `common::observability::otel_http::extract_trace_context`) | Mine, Domain-judgment | — |
| `crates/gc-service/src/middleware/mod.rs` (module decl + re-export) | Mine, Mechanical | — |
| `crates/gc-service/Cargo.toml` | Mine, Mechanical | — |
| `crates/common/src/observability/otel_http.rs` (NEW) | Not mine, Domain-judgment — `--paired-with=observability` (resolved, see Notes) | observability |
| `crates/common/src/observability/mod.rs` (module decl for new file) | Not mine, Minor-judgment | observability |
| `crates/common/Cargo.toml` (`http = { workspace = true }` as an explicit direct dep — currently only transitive via reqwest/tonic/axum) | Not mine, Mechanical | observability |
| `crates/gc-service/tests/otel_*.rs` (NEW, 4 files per task deliverable — see Test plan) | Mine, Domain-judgment | test (coordinate) |
| `crates/gc-service/tests/common/mod.rs` (module decl for new `otel_support` test-support module) | Mine, Mechanical | — |
| `crates/gc-service/tests/common/otel_support.rs` (NEW — test-only OTel harness: propagator install, subscriber install, fixtures) | Mine, Domain-judgment | test (coordinate) |
| `Cargo.toml` (workspace root — pins `opentelemetry`/`opentelemetry_sdk`/`tracing-opentelemetry` dep versions consumed by gc-service's dev-deps) | Mine, Mechanical | — |
| `docs/TODO.md` (update) | Mine, Mechanical | — |
| `docs/user-stories/2026-05-02-browser-client-join.md` (task #26 status) | Mine, Mechanical | — |
| `docs/runbooks/gc-deployment.md` (OTel section: GC now calls `init_otel`) | Mine, Mechanical | — |
| `infra/services/gc-service/configmap.yaml` | Not mine, Minor-judgment | operations |
| `infra/services/gc-service/deployment.yaml` | Not mine, Minor-judgment | operations |
| `infra/services/gc-service/network-policy.yaml` | Not mine, Minor-judgment | operations |
| `infra/kubernetes/overlays/kind/services/gc-service/kustomization.yaml` | Not mine, Minor-judgment | operations |
| `infra/kubernetes/overlays/kind/services/gc-service/configmap-otel-patch.yaml` (NEW) | Not mine, Minor-judgment | operations |

Notes:
- GC's `Config` is built exclusively via `Config::from_vars(&HashMap)` in tests (`gc-test-utils/src/server_harness.rs`) — no literal `Config { .. }` struct construction outside `config.rs` itself. Unlike AC's task #25 (which had to touch `routes/mod.rs` test fixtures + `ac-test-utils`), adding new `Config` fields here has **zero test-fixture blast radius**: new fields get their defaults automatically through `from_vars`. `routes/mod.rs` is still touched, but only for the new extraction-layer wiring in `build_routes`, not for Config literals.
- **`config.rs`/`main.rs` reclassified `Mine, Mechanical` → `Mine, Domain-judgment`** per @code-reviewer's Gate-1 finding: adding 4 new config fields + validation + `otel_config()` + reordering `main.rs`'s subscriber composition is new code, not a value-neutral find/replace, and there's no guard covering this change-pattern. AC's task #25 classified the identical scope as `Mine, Domain-judgment` — this was a mislabel, not a substantive disagreement (both rows are "Mine", so the relabel doesn't change any owner-confirmation gating, just audit-trail accuracy).
- **`otel_http.rs` reclassified `Not mine, Minor-judgment` → `Not mine, Domain-judgment`** per @code-reviewer's Gate-1 classification challenge (per ADR-0024 §6.2 monotonicity, reviewers may only upgrade, and I'm accepting the upgrade rather than contesting it — the precedent argument is sound): the direct precedent, task #24's `otel.rs`/`otel_grpc.rs` (same shape — new file, new security-sensitive public API, first-of-kind), was Implementing-Specialist-authored by observability, not authored by a downstream consumer under hunk-ACK. **RESOLVED**: @observability agreed with the upgrade and elected `--paired-with=observability` (not a side-devloop) — the design is already fully closed (structure, security contract, test shape all settled in our exchange), so spinning up a separate devloop's planning/gate lifecycle for what's now transcription of an agreed spec would cost more than it saves. Working agreement: I write `otel_http.rs` to the agreed spec and loop @observability in on a first rough draft (not a polished one) before considering it done — they review/adjust the file directly, not just a Gate-2 hunk-ACK.
- **`ac_client.rs` (Domain-judgment) vs. `telemetry_forwarder.rs` (Mechanical) tier distinction** (per @code-reviewer's question): both call the same `otel_http::inject_trace_context` helper at one outbound reqwest call site each, but `ac_client.rs`'s `HeaderMap` already carries a live `Authorization: Bearer <token>` header on the same request (`ac_client.rs:94-97`/`:142-145`) — merging the injected trace headers into that map without clobbering or reordering the existing auth header is the judgment call. `telemetry_forwarder.rs`'s outbound request carries no other sensitive header, so applying the helper there is a straight mechanical application of the same helper — no analogous care needed.
- **`otel_http.rs` deny-lints (per @code-reviewer item D, confirmed)**: yes — `otel_http.rs` carries the same module-level `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]` as `otel_grpc.rs` (already stated in §Security contract above).
- **`Config`'s hand-rolled `Debug` impl (per @code-reviewer item E, confirmed)**: GC's `Config` has a hand-rolled `impl fmt::Debug` (`config.rs:105-131`, redacts `database_url`/`gc_client_secret`) — NOT `#[derive(Debug)]`. The 4 new OTel fields (`otel_enabled`, `otel_endpoint`, `otel_sample_rate`, `environment`) are non-secret (mirrors AC, whose equivalent fields are plain `.field(...)` entries, not `[REDACTED]`) and will be added as plain `.field(...)` entries to the existing `Debug` impl — called out explicitly here since it's easy to forget on a hand-rolled impl (AC's own task #25 devloop had to touch its equivalent).
- **Pre-existing doc bug, non-blocking** (per @code-reviewer side note): `routes/mod.rs:148-150`'s existing layer-order comment has `TimeoutLayer`/`TraceLayer` backwards (says `TimeoutLayer` is innermost; per the confirmed `Router::layer` semantics `TraceLayer`, added first at line 156, is actually innermost). Not introduced by this PR, but since I'm adding a new comment right next to it explaining the new extraction layer's placement, I'll fix the pre-existing comment in the same hunk while I'm there.
- `crates/common/src/observability/` is NOT a Guarded Shared Area (per ADR-0024 §6.4 list) — the Domain-judgment classification above is a real choice under §6.2, not GSA-forced. (Superseded: my original plan reasoned this only needed Minor-judgment; see the reclassification note above — @code-reviewer's precedent argument, that task #24's sibling files were owner-authored, was correct and I'm not contesting it.)
- **Gate-1 rulings received and locked in** (superseding the "TBD"/questions framing below, which is left in place as the audit trail):
  - @observability + @dry-reviewer: `otel_http.rs` lives in `crates/common/src/observability/otel_http.rs`. Structure is framework-agnostic — `HeaderInjector`/`HeaderExtractor` adapters over `&http::HeaderMap` plus plain `inject_trace_context`/`extract_trace_context` functions, built on the existing global `BoundedTraceContextPropagator`. **No `axum` dependency added to `common`** (caught by @dry-reviewer, corrected by @observability) — `http::HeaderMap` is already transitive via reqwest/tonic/axum. gc-service owns the thin axum `middleware::from_fn` wrapper (new `middleware/otel.rs`, sibling to the existing `middleware/http_metrics.rs`) and the reqwest call-site usage in `ac_client.rs`/`telemetry_forwarder.rs`.
  - @observability: inbound gRPC (d) is **IN SCOPE**, not a follow-up. GC (unlike AC) has a real, high-traffic inbound Tonic server (MC/MH registration+heartbeat) — leaving it out defeats the point of the devloop. Wire `otel_grpc::server_interceptor()` alongside the existing `grpc_auth_layer` on `main.rs`'s `TonicServer::builder()`. Needs its own integration test (MC/MH→GC continuity is a distinct boundary from the GC→AC/GC→MC tests already planned) — added as test #4 below.
  - @dry-reviewer: reuse `otel_grpc::PROPAGATED_HEADERS` from `otel_http.rs` rather than defining a second copy of the two-string list — one constant anchors the security contract across both transports.
  - @dry-reviewer also flagged (not a Gate-1 blocker, informational): GC's new 4-knob `Config`/`main.rs` shape will closely mirror AC's task #25 shape — a legitimate 2nd-sibling DRY extraction-opportunity candidate. Per ADR-0019 this is DRY's call to log in `docs/TODO.md` §Cross-Service Duplication at verdict time, not a fix-or-defer finding for this PR; not actioned here.
  - @operations: confirmed the network-policy diff must be a **new additional** :4317 egress rule (not a port edit on the existing :4318 telemetry-proxy rule), confirmed per-key `configMapKeyRef` (no `envFrom`), confirmed the 2-file Kind-overlay strategic-merge form, confirmed base `OTEL_ENABLED: "false"`.

---

## Planning

### Summary

Wires the common `init_otel` (task #24) SDK helper into GC's `main.rs`, adds the four frozen Config knobs (mirroring AC/task #25 exactly), and wires all four R-56 trace-propagation surfaces GC touches. Two scope corrections vs. the task title (confirmed by investigation, see teammate brief): GC→AC is HTTP (reqwest), not gRPC — so R-56 there means a new W3C header-inject/extract pair, not `otel_grpc`'s Tonic interceptors. GC→MC IS gRPC and reuses `otel_grpc::client_interceptor()` directly.

### The four propagation surfaces

**(a) Inbound HTTP extract** (`routes/mod.rs::build_routes`)
New gc-service axum middleware `crate::middleware::otel::extract_trace_context_middleware` (in new `middleware/otel.rs`, sibling to `middleware/http_metrics.rs`), thin wrapper calling `common::observability::otel_http::extract_trace_context(request.headers())` — added via `.layer(middleware::from_fn(extract_trace_context_middleware))`. Mechanism mirrors `otel_grpc::server_interceptor()` exactly: extract W3C headers via the global propagator, then `tracing::Span::current().set_parent(parent_cx)`.

**Placement is load-bearing and easy to get backwards.** `axum::Router::layer` semantics: the *last*-added `.layer()` call becomes the *outermost* wrapper and runs first on the request path (documented on `Router::layer`: for `.layer(one).layer(two).layer(three)`, execution is `three → two → one → handler`). Today's stack is:
```rust
.layer(TraceLayer::new_for_http())      // added 1st → innermost, runs LAST before the handler
.layer(TimeoutLayer::new(...))          // added 2nd
.layer(middleware::from_fn(http_metrics_middleware))  // added 3rd → outermost, runs FIRST
```
`TraceLayer` is currently the innermost layer — it creates and enters its request span immediately before the handler runs. For `tracing::Span::current().set_parent(...)` to attach the parent to *that* span (so every span the handler creates, including the MC-client's `#[instrument]` span, inherits it), my extraction middleware must run **after** `TraceLayer` has entered its span but **before** the handler — i.e. it must be *more inner* than `TraceLayer`. Per the "earlier-added = more-inner" rule, that means adding `.layer(extract_trace_context)` **as a new first `.layer()` call, physically above `.layer(TraceLayer::new_for_http())` in the chain**:
```rust
.layer(middleware::from_fn(extract_trace_context))  // NEW — added even earlier → even more inner than TraceLayer
.layer(TraceLayer::new_for_http())
.layer(TimeoutLayer::new(...))
.layer(middleware::from_fn(http_metrics_middleware))
```
This reading reconciles with the teammate brief's "place ABOVE TraceLayer" instruction (source-code-position "above" = added earlier = more inner = runs after `TraceLayer` creates its span = correct timing). I'm flagging the mechanism explicitly for @observability + @code-reviewer confirmation at Gate 1 given how easy this is to get backwards with no compile-time signal — the task's own "HTTP→gRPC bridge" integration test is the thing that would actually catch a wrong-order regression (trace_id mismatch), so I want the reasoning checked before, not after, writing it.

**(b) Outbound HTTP inject (GC→AC)** (`services/ac_client.rs`)
New helper `common::observability::otel_http::inject_trace_context(headers: &mut http::HeaderMap)` (mirrors `client_interceptor()`'s inject side). Both `request_meeting_token` and `request_guest_token` build a `http::HeaderMap`, call the helper, then attach via `reqwest::RequestBuilder::headers(injected)`. `reqwest::header::HeaderMap` is a re-export of `http::HeaderMap`, so the **same** `HeaderInjector`/`HeaderExtractor` adapter pair serves both (a) and (b)/(c) — one adapter, not two. `otel_http.rs` imports `otel_grpc::PROPAGATED_HEADERS` directly (`use super::otel_grpc::PROPAGATED_HEADERS;`) rather than declaring a second copy of the two-string allowlist (per @dry-reviewer — confirmed).

**Injector/Extractor design (confirmed with @semantic-guard)**: `HeaderInjector` mirrors `otel_grpc.rs`'s `MetadataInjector` exactly — implements `Injector::set()` only (insert-by-key into the target `HeaderMap`), with **no method that reads or iterates** the map it's writing into. This makes an accidental read-and-forward of `Authorization`/`Cookie` (both live in the same `HeaderMap` `ac_client.rs` builds) structurally impossible, not just something the test matrix happens to catch. `HeaderExtractor` mirrors `MetadataExtractor`: `get()` is a single keyed lookup, `keys()` returns names only (never values) — no path exists that could copy a header value into a span attribute or log line.

**(c) Outbound gRPC inject (GC→MC)** (`services/mc_client.rs`)
`MeetingControllerServiceClient::new(channel)` → `MeetingControllerServiceClient::with_interceptor(channel, otel_grpc::client_interceptor())` at the existing per-call construction site (`mc_client.rs:194`). Direct reuse of task #24's helper — Mechanical.

**(d) Inbound gRPC extract** (GC's Tonic server, `main.rs`) — **CONFIRMED IN SCOPE** (@observability ruling: GC's inbound Tonic server, unlike AC's absent one, is real and high-traffic — MC/MH registration+heartbeat — so R-56 "every boundary" applies).

**Critical correction from @observability's Gate-1 trace-through (not caught in my original plan): `.with_interceptor(mc_service, otel_grpc::server_interceptor())` alone is a SILENT NO-OP.** `server_interceptor()`'s contract requires an already-active tracing span at the point it runs (its own unit test pre-enters a span before calling it) — `.set_parent()` mutates *that* span's stored OTel parent; on `Span::none()` it's a no-op. Traced the actual call chain: `grpc/auth_layer.rs`'s `GrpcAuthService::call` only emits `debug!`/`warn!` events, never creates/enters a span; `main.rs:256-260`'s grpc_server builder has exactly one `.layer(grpc_auth_layer)` and nothing span-creating; interceptors registered via `.with_interceptor()` run at the per-service dispatch point, which is BEFORE the generated method's own `#[instrument]` span (e.g. `mc_service.rs:198`'s `gc.grpc.register_mc` span). So without a span-creating layer, the interceptor extracts the parent, finds no active span, drops it silently — compiles clean, existing `otel_grpc.rs` unit tests still pass (they don't reflect this call chain), looks wired in review, but MC/MH→GC traces never actually link.

**IMPLEMENTED — required fix landed as one unit with the interceptor wiring**: added a span-creating layer to the grpc_server builder — `tower_http::trace::TraceLayer::new_for_grpc()` (zero new deps: already used via `new_for_http()` in `routes/mod.rs`). Composition uses `GlobalControllerServiceServer::with_interceptor(mc_service, otel_grpc::server_interceptor())` / `MediaHandlerRegistryServiceServer::with_interceptor(...)` at `.add_service(...)`.

**Correction discovered during implementation (relative order DOES matter, opposite of the original plan text above)**: `tonic::transport::Server::builder().layer(...)` composes via `tower::ServiceBuilder` semantics — the FIRST `.layer()` added is OUTERMOST (runs first) — the OPPOSITE convention from `axum::Router::layer` (last-added = outermost, confirmed for surface (a) above). Putting `.layer(grpc_auth_layer)` before `.layer(TraceLayer::new_for_grpc())` (grpc_auth_layer outermost) fails to compile: `GrpcAuthService`'s hand-rolled `Service` impl (`grpc/auth_layer.rs`) requires its wrapped inner service's response body type to be exactly tonic's `BoxBody`, but `TraceLayer` changes the response body type when wrapping `Routes` — so `GrpcAuthService<Trace<Routes,...>>` doesn't type-check (E0271). **Fix: `TraceLayer::new_for_grpc()` must be the FIRST `.layer()` call (outermost)**, `grpc_auth_layer` second:
```rust
.layer(tower_http::trace::TraceLayer::new_for_grpc())
.layer(grpc_auth_layer)
```
This is also functionally correct for the same reason as the original plan (TraceLayer's span must exist before `grpc_auth_layer`/the interceptor run inside it) — just reached via the opposite-of-axum ordering rule for this specific builder type. Documented explicitly in the `main.rs` comment, cross-referencing the axum-side ordering comment in `routes/mod.rs` so the two don't get flipped by false analogy. @observability notified and agreed.

Needs a dedicated integration test proving MC/MH→GC parent continuity through the live Tonic stack (test #4 below) — this is the one test that would have caught the no-op gap, since none of the other three planned tests exercise GC-as-server.

**Telemetry forwarder** (`services/telemetry_forwarder.rs`) reuses helper (b) at its one outbound `POST` call site — same shape as `ac_client.rs`.

### Security contract (mirrors `otel_grpc.rs`, non-negotiable)
`otel_http`'s inject/extract touch ONLY `traceparent`/`tracestate`. Never read/copy `authorization` or any bearer header. Never put header values into span attributes. Test matrix ports directly from `otel_grpc.rs`'s existing rejection/no-auth-leak tests (bounds enforcement is already handled by the globally-registered `BoundedTraceContextPropagator` — `otel_http` doesn't re-implement bounds-checking, just adapts the transport). `otel_http.rs` carries forward `otel_grpc.rs`'s module-level `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]` (per @security).

Two additions to the test matrix beyond the direct `otel_grpc.rs` port, per @security (HTTP's header surface is browser-reachable, unlike gRPC metadata which only ever originates from MC/MH):
1. **Middleware-level no-leak test**, not just adapter-unit-level: build a minimal router with the real `extract_trace_context_middleware` wired in, send a request carrying `Authorization: Bearer <secret>` + `Cookie: session=...` + a valid `traceparent`, assert the handler receives the auth/cookie headers untouched AND zero span fields contain those key names or values. Closes the gap between "the bare extractor never reads those keys" and "the wired middleware never accidentally logs/forwards the full HeaderMap somewhere."
2. **Non-UTF8 header value → `None`, not panic**: unlike `tonic::MetadataValue` (ASCII-only by construction), `http::HeaderValue` can hold opaque non-UTF8 bytes (`HeaderValue::from_bytes`) — new attacker-reachable surface that can't occur on the gRPC side. `Extractor::get` impl uses `.and_then(|v| v.to_str().ok())` (mirrors the gRPC adapter, already handles it) — add an explicit test asserting a non-UTF8 `traceparent` value is treated as absent, not a panic.

### Config knobs (mirrors AC/task #25 exactly — frozen decisions, not re-litigated)
`otel_enabled: bool` (env `OTEL_ENABLED`, default `false`, explicit-boolean gating — NOT presence-of-endpoint, regression test locks this) / `otel_endpoint: String` (env `OTLP_ENDPOINT`, default `""`, distinct from the existing `otel_collector_endpoint` :4318 telemetry-proxy field) / `otel_sample_rate: f64` (env `OTEL_SAMPLE_RATE`, default `1.0`, PARSE-ONLY — range owned solely by `init_otel`) / `environment: String` (env `DEPLOYMENT_ENVIRONMENT`, default `"development"`) / `otel_config(&self) -> Option<OtelConfig>` gate method / fail-fast on `otel_enabled && otel_endpoint.trim().is_empty()`.

### main.rs wiring
Move `Config::from_env()` before the `tracing_subscriber` init (currently after — GC differs from AC's already-fixed ordering here). Gate `init_otel` on `config.otel_config()`. Hold `_otel_guard` (named binding) to end of `main`. Compose `.with(otel_layer)` first onto the bare `registry()`, then `EnvFilter`, then `fmt::layer().json()`.

**Test-boundary notes (AC-parity, per @test — mirrors `docs/devloop-outputs/2026-06-25-ac-otel-wiring-task25/main.md` §Tests):**
1. **Default-off coverage is de-facto, not new test surface.** GC's existing integration suites (`meeting_tests.rs`, `mc_assignment_rpc_tests.rs`, `telemetry_proxy_tests.rs`, and the rest of `crates/gc-service/tests/`) build `Config` via `Config::from_vars` without setting `OTEL_ENABLED` — so they already exercise the `otel_enabled=false` / `init_otel` never called / zero-OTel-layer-composed path by construction, same as AC's task #25. This task adds no new default-off test; those suites staying green post-wiring **is** the default-off coverage, verified at review.
2. **`OtelGuard::drop`/flush-on-drop is `common`'s to own and test, not GC's to reimplement.** GC's obligation is purely structural: `_otel_guard` from `init_otel`'s `OtelInit` is a **named binding held to the end of `main`** (never `let _ = ...`, which would immediate-drop and flush nothing). GC does NOT add a `#[cfg(test)]` module in `main.rs` attempting to exercise the drop/flush behavior itself — that's `crates/common/src/observability/otel.rs`'s existing test coverage (the `OtelGuard` `Drop` impl + its `catch_unwind` belt), and duplicating/mocking it here would test `common`, not GC.

### Cargo.toml
**Resolved (per @security, checked AC's landed task #25 code)**: AC has **no direct `opentelemetry`/`opentelemetry_sdk`/`tracing-opentelemetry` deps** in its Cargo.toml — `init_otel` returns an opaque layer type and `.with(otel_layer)` type-infers fine without naming the type in `main.rs`. Following that precedent: no new direct deps added to `gc-service/Cargo.toml` unless something GC-specific forces it during implementation (fewer direct deps = smaller re-audit surface). `common/Cargo.toml` gets `http = { workspace = true }` promoted from transitive to an explicit direct dep (for `otel_http.rs`'s `HeaderMap`/`HeaderName`/`HeaderValue` types) — that's the one real dependency change in this task.

### Infra (operations owns; proposing exact diffs for Gate-1 confirmation)
- `configmap.yaml`: add `OTEL_ENABLED: "false"`, `OTLP_ENDPOINT: "http://otel-collector.dark-tower:4317"`, `OTEL_SAMPLE_RATE: "1.0"`, `DEPLOYMENT_ENVIRONMENT: "development"` (carrying forward AC's tracked prod-mistagging caveat into `docs/TODO.md`).
- `deployment.yaml`: GC uses **per-key `configMapKeyRef`** (confirmed — not `envFrom`), so each of the 4 new keys needs its own `env:` entry or `validate-env-config` will flag an orphan.
- `network-policy.yaml`: GC already has an egress rule to `otel-collector` on **:4318** (HTTP, for the telemetry proxy forwarder — pre-existing, task #10). This task needs a **second, new** egress rule to the same pod selector on **:4317** (OTLP-gRPC, for `init_otel`'s own span export) — distinct port, distinct purpose, do not conflate/merge.
- Kind overlay: new `infra/kubernetes/overlays/kind/services/gc-service/configmap-otel-patch.yaml` (2-file strategic-merge, mirrors AC) flipping `OTEL_ENABLED` to `"true"` for dev; `kustomization.yaml` gets a `patches:` entry.
- Runbook: `docs/runbooks/gc-deployment.md:1286-1288` currently says GC/MC/MH "do not yet call init_otel" as one 3-service bullet. **@operations supplied the exact GC-specific replacement text below (Gate-1 confirmed) — use it verbatim, do NOT copy AC's bullet text.** GC's Deployment/PDB shape differs materially from AC's StatefulSet (rollout-stall vs. outage behavior), so this is not a mechanical AC-mirror despite the rest of this task following AC's pattern closely:

  > - **GC (R-55 landed, task #26):** OTel init is gated by the explicit `OTEL_ENABLED` flag. Break-glass = set `OTEL_ENABLED=false` in GC's ConfigMap (or drop the Kind overlay's `OTEL_ENABLED=true` patch) and restart GC; it boots with the OTel layer off and no collector dependency. GC is a 2-replica `Deployment` (`maxUnavailable: 0`, PDB `minAvailable: 1`) — unlike AC's StatefulSet, a bad `OTEL_ENABLED=true` rollout does **not** cause an outage: new pods CrashLoop before ever binding their HTTP/gRPC ports, so they never pass readiness and the rollout stalls with the old ReplicaSet still serving all traffic. It also does **not** self-heal — `kubectl rollout status` hangs indefinitely until an operator intervenes (`kubectl rollout undo`, or flip the break-glass lever above).
  > - **MC / MH:** still pending (#6 / #27), unchanged.

  Also confirmed by @operations: `infra/services/otel-collector/network-policy.yaml`'s ingress rule for `:4317` **already allow-lists `gc-service`** (pre-provisioned fleet-wide for R-55) — the GC-side egress addition below is the only missing half of the zero-trust pair; nothing needed on the collector side.

### Test plan (confirmed with @test)
Four new integration test files under `crates/gc-service/tests/`, following `meeting_tests.rs`'s wiremock+JWKS+`MockMcClient`+`#[sqlx::test]` pattern:
1. `otel_ac_trace_continuity.rs` — active span → `AcClient` HTTP call carries valid `traceparent`, trace-id matches.
2. `otel_http_grpc_bridge.rs` — inbound HTTP `traceparent` → GC extracts → outbound `assign_meeting` gRPC call carries the SAME trace-id in metadata. **Confirmed approach (per @test): a real minimal Tonic server, NOT an extended `MockMcClient`.** `MockMcClient` implements `McClientTrait` *post*-interceptor with plain-arg methods (no `tonic::Request`/`MetadataMap` anywhere in that boundary) — it structurally cannot exercise `otel_grpc::client_interceptor()`, since the interceptor only runs inside the real `McClient::assign_meeting` that `MockMcClient` never calls into. Mirror `crates/mh-service/tests/common/mock_mc.rs`'s existing pattern exactly: `tonic::transport::Server::builder()` on `127.0.0.1:0` implementing `MeetingControllerService::assign_meeting_with_mh`, a `MockMcHandle` RAII wrapper (cancels+aborts the spawned task on `Drop` so a panicking assertion doesn't leak the port), and a `with_connected_tx(mpsc::Sender<...>)` channel to push the received `request.metadata().clone()` for assertion. Test harness constructs `AppState.mc_client = Arc::new(McClient::new(token_receiver))` (the REAL client, not the mock) and registers the MC's endpoint in the DB (`MeetingControllersRepository::register_mc(...)`, mirrors `mc_assignment_rpc_tests.rs::setup_mcs`) pointing at the mock server's actual bound `http://127.0.0.1:{port}` address — otherwise the real interceptor-bearing channel never gets built.
3. `otel_telemetry_proxy_trace_context.rs` — POST to `/api/v1/telemetry/...` with `traceparent` → forwarder's outbound collector request preserves it (mock collector via wiremock, mirrors `telemetry_forwarder.rs`'s own test style).
4. `otel_grpc_inbound_continuity.rs` (NEW per @observability's (d) in-scope ruling) — a real gRPC client (bare `MeetingControllerServiceClient`/registry client, no interceptor needed on the test side) calls GC's live `register_mc` RPC with a `traceparent` metadata value set → asserts the resulting `gc.grpc.register_mc` span (via the same span-capture pattern `otel_grpc.rs`'s own tests use, or a log-line assertion) carries the SAME trace-id. **This is the specific test that would catch the `TraceLayer::new_for_grpc()` gap above** — without that layer, this test fails (interceptor runs, finds no active span, drops the parent silently); with it, the span exists when the interceptor runs and `.set_parent()` takes effect. Distinct boundary from test #2 (inbound vs. outbound, MC/MH-as-caller vs. GC-as-caller) — needs GC's actual running Tonic server (`GlobalControllerServiceServer`), not a mock. The real-Tonic-server *harness pattern* from test #2 (spin up a server, connect a real client, assert on what crosses the wire) is reusable here even though the two tests exercise opposite directions and different servers — not literally shared server infrastructure, just the same test-construction approach.
5. **Middleware-level no-leak test** (per @security, item 1 above) — not a distinct file necessarily, but a distinct test case: minimal router wired with the real `extract_trace_context_middleware`, asserts `Authorization`/`Cookie` pass through untouched and never appear in span fields.

Plus unit tests in `otel_http.rs` porting `otel_grpc.rs`'s security-contract test matrix (round-trip, no-auth-leak both directions, injected-keyset-is-subset-of-`PROPAGATED_HEADERS`) + the non-UTF8-header-value test (@security item 2) and `config.rs`'s OTel test block (mirrors AC's 11 tests verbatim, service name/defaults swapped).

**Test-reliability principle** (@test, citing ADR-0034 §Implementation Notes): prefer exercising the real code path over mocking the implementation detail under test — this is why #2/#4 use real Tonic servers rather than mock-trait extensions. Noted as a documented review bar, not just a stylistic preference.

### Gate-1 status (all questions resolved; see §Cross-Boundary Classification Notes above for the locked decisions)
1. ~~`otel_http.rs` location~~ — **RESOLVED**: `crates/common/src/observability/otel_http.rs`, framework-agnostic (no `axum` dep on `common`). @observability + @dry-reviewer confirmed. Two sibling files (`otel_grpc.rs` + `otel_http.rs`), not a shared trait — @dry-reviewer confirmed a trait at N=2 transports is premature generalization given the two adapters' underlying maps differ meaningfully (`tonic::MetadataMap` vs. `http::HeaderName`/`HeaderValue`).
2. ~~(d) inbound-gRPC scope~~ — **RESOLVED**: in scope. @observability confirmed.
3. ~~Layer-placement mechanism for (a)~~ — **RESOLVED**: @observability explicitly confirmed correct, verified against `tower_http::trace::Trace`'s actual `ResponseFuture::poll` semantics (span guard re-entered on every poll, so `middleware::from_fn`'s lazy-future construction doesn't break the ordering argument). Ship as planned.
4. ~~Mocking approach for tests #2/#4~~ — **RESOLVED**: real minimal Tonic server, mirroring `mh-service/tests/common/mock_mc.rs`. @test confirmed with precedent.
5. ~~Infra diffs~~ — **RESOLVED**: all 5 rows confirmed by @operations at Gate 1, including verbatim runbook text (now in §Infra above) and collector-side ingress-already-provisioned confirmation.
6. ~~HTTP header-surface test coverage~~ — **RESOLVED**: @security's two additions (middleware-level no-leak test, non-UTF8-header-value test) folded into §Test plan and §Security contract above.
7. ~~Injector/Extractor read/write-shape~~ — **RESOLVED**: @semantic-guard confirmed the write-only-Injector / keyed-lookup-only-Extractor design (mirrors `otel_grpc.rs` exactly) — folded into §Planning "(b)" above.
8. ~~`PROPAGATED_HEADERS` duplication~~ — **RESOLVED**: `otel_http.rs` imports `otel_grpc::PROPAGATED_HEADERS` directly, no second constant. @dry-reviewer confirmed.

9. **NEW finding from @observability's Gate-1 trace-through of (d)** — **RESOLVED (fix identified, not yet implemented)**: `.with_interceptor(mc_service, otel_grpc::server_interceptor())` alone would be a silent no-op (no active span exists at GC's inbound gRPC dispatch point for `.set_parent()` to mutate). Fix: add `.layer(tower_http::trace::TraceLayer::new_for_grpc())` to the grpc_server builder alongside `.layer(grpc_auth_layer)`, landed as one unit with the interceptor wiring. Folded into §Planning "(d)" above, with test #4 rewritten to specifically exercise `register_mc` and catch a regression of this gap.

**Still open**: @code-reviewer has not yet responded (FYI-only message sent, no blocking questions asked of them) — not blocking, since no question was raised of them. All reviewer-raised questions across all 7 reviewers are now resolved. Operations re-confirmed Gate 1 (already sent "Plan confirmed" to @team-lead) with one wording correction (GC-specific runbook text, not an AC copy) — now fixed in §Infra above. DRY reviewer confirmed and already sent "Plan confirmed" to @team-lead.

Out of scope (per teammate brief, not touching): CORS/`CorsLayer` (task #10, GC has none today), `dt_client_*`/`gc_telemetry_*` new metrics, AC/MC/MH service code, proto changes.

---

## Implementation Summary

All items in the plan are implemented, self-checked, and green. Two corrections surfaced during implementation (both already folded into §Planning above and communicated to reviewers):

1. **Surface (d) `TraceLayer::new_for_grpc()` ordering is the OPPOSITE of surface (a)'s.** `tonic::transport::Server::builder().layer(...)` composes via `tower::ServiceBuilder` semantics (first-added = outermost) — the opposite of `axum::Router::layer` (last-added = outermost). `TraceLayer::new_for_grpc()` must be the FIRST `.layer()` call on the grpc_server builder (not "either order," as originally written) — confirmed by a real compile error (`GrpcAuthService`'s hand-rolled `Service` impl requires an exact `BoxBody` response type; `TraceLayer` wrapped inside it breaks that). Documented explicitly in `main.rs`'s comment, cross-referenced from `routes/mod.rs`'s comment so the two orderings aren't flipped by false analogy.
2. **Test #4 needed a real positive assertion, not just RPC-success.** The original test sketch would have proven the call didn't crash but NOT that the trace_id actually propagated (i.e., it would NOT have caught the surface-(d) no-op regression it was written to catch). Fixed by wiring an `InMemorySpanExporter` into the test's `OpenTelemetryLayer` and asserting the finished `gc.grpc.register_mc` span's `trace_id` literally equals the injected `traceparent`'s trace-id. **Verified the test actually catches the regression**: temporarily removed `TraceLayer::new_for_grpc()` from the test harness and confirmed the test fails with a trace-id mismatch (`left: <random-root-trace-id>`, `right: <injected-trace-id>`), then restored.

### Files changed

**`crates/common/`** (authored `--paired-with=observability`, per Gate-1 classification upgrade — first draft shared with @observability, awaiting their direct review/adjustment before this is considered final):
- `src/observability/otel_http.rs` (NEW) — `HeaderInjector`/`HeaderExtractor` over `&http::HeaderMap`, `inject_trace_context`/`extract_trace_context`, reuses `otel_grpc::PROPAGATED_HEADERS`. 9 unit tests (round-trip, bounds-rejection delegation, non-UTF8 handling, no-auth-leak both directions, keyset-subset).
- `src/observability/mod.rs` — module declaration.
- `Cargo.toml` — `http` promoted to a direct dependency (was transitive only).

**`crates/gc-service/`**:
- `src/config.rs` — 4 new fields (`otel_enabled`/`otel_endpoint`/`otel_sample_rate`/`environment`) + `otel_config()` + `ConfigError::InvalidOtelConfig` + Debug impl entries + `DEFAULT_OTEL_COLLECTOR_ENDPOINT` namespace fix (unrelated pre-existing TODO item, landed here per its own deferral note) + 12 new tests.
- `src/main.rs` — config-load-before-subscriber reorder, `init_otel` wiring with named `_otel_guard`, inbound gRPC `TraceLayer::new_for_grpc()` + `.with_interceptor(..., otel_grpc::server_interceptor())` on both `GlobalControllerServiceServer` and `MediaHandlerRegistryServiceServer`.
- `src/middleware/otel.rs` (NEW) — `extract_trace_context_middleware`, thin wrapper over `otel_http::extract_trace_context`. 3 tests including the two @security-mandated additions (middleware-level no-leak, non-panic pass-through).
- `src/middleware/mod.rs` — module declaration + re-export.
- `src/routes/mod.rs` — extraction middleware wired as the new first `.layer()` call (above `TraceLayer::new_for_http()`); fixed the pre-existing `TimeoutLayer`/`TraceLayer` innermost-ordering comment bug found by @code-reviewer.
- `src/services/ac_client.rs` — `Self::trace_headers()` helper, injected at both `request_meeting_token`/`request_guest_token` call sites via `.headers(...)` (merges, doesn't clobber the existing `Authorization` header).
- `src/services/mc_client.rs` — `MeetingControllerServiceClient::with_interceptor(channel, otel_grpc::client_interceptor())`.
- `src/services/telemetry_forwarder.rs` — same inject pattern as `ac_client.rs`, applied at the one outbound collector-forward call site.
- `Cargo.toml` — dev-only additions: `opentelemetry`/`opentelemetry_sdk` (with `testing` feature for `InMemorySpanExporter`)/`tracing-opentelemetry`/`tokio-stream`, all needed only by the new integration tests, not production code.
- `tests/common/mod.rs` + `tests/common/otel_support.rs` (NEW) — shared test-environment helpers (global propagator install, scoped `OpenTelemetryLayer` subscriber guard, known-`traceparent` fixtures).
- `tests/otel_ac_trace_continuity.rs` (NEW) — 2 tests, surface (b).
- `tests/otel_http_grpc_bridge.rs` (NEW) — 1 test, surfaces (a)+(c) bridge, real mock Tonic MC server.
- `tests/otel_grpc_inbound_continuity.rs` (NEW) — 1 test, surface (d), real GC gRPC server + `InMemorySpanExporter`, verified to catch the no-op regression.
- `tests/otel_telemetry_proxy_trace_context.rs` (NEW) — 1 test, full HTTP router → forwarder trace preservation.

**Infra** (proposed; operations owns final confirm at Gate 3):
- `infra/services/gc-service/configmap.yaml` — 4 new keys, `OTEL_ENABLED: "false"` base default.
- `infra/services/gc-service/deployment.yaml` — 4 new per-key `configMapKeyRef` entries.
- `infra/services/gc-service/network-policy.yaml` — new additive `:4317` egress rule (distinct from the existing `:4318` telemetry-proxy rule).
- `infra/kubernetes/overlays/kind/services/gc-service/configmap-otel-patch.yaml` (NEW) + `kustomization.yaml` — 2-file strategic-merge dev-enable, mirrors AC. **Validated**: `kubectl kustomize` renders the overlay correctly, all 4 env vars resolve with no orphans, confirmed the otel-collector's existing ingress rule already allow-lists `gc-service` on `:4317` (operations' claim, verified directly).

**Docs**:
- `docs/runbooks/gc-deployment.md` — GC-specific break-glass bullet (operations' verbatim text, not an AC copy).
- `docs/TODO.md` — 3 edits: updated the R-56/AC entry (shared layer + producer now landed, AC-side consumption remains open), extended the `DEPLOYMENT_ENVIRONMENT` prod-mistagging caveat to cover GC, marked the `DEFAULT_OTEL_COLLECTOR_ENDPOINT` namespace fix resolved.
- `docs/user-stories/2026-05-02-browser-client-join.md` — **NOT updated yet**; task-status flip to Completed is deferred to end-of-devloop per team convention (see e.g. commit `1fe546b`), not done mid-implementation.

### Self-check results (all green)
- `cargo fmt --all -- --check` — clean.
- `cargo check --workspace --all-targets` — clean.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, zero findings.
- `cargo test -p common -p gc-service --all-targets` — all green: common 220/220, gc-service lib 340/340 (×2, unit+doc harness), all 20 integration test binaries 100% pass including the 5 new/otel-specific ones.
- Regression-catching verified by hand for test #4 (temporarily removed the fix, confirmed the test fails with the exact mismatch it's designed to catch, restored).

### Outstanding before Gate 2/3
- @observability's direct review of `otel_http.rs`'s first draft (sent, awaiting response — per our `--paired-with` agreement, they review/adjust the actual file, not just hunk-ACK).
- @operations' Gate-3 re-confirm of the 4 infra files + runbook text (Gate-1 already confirmed the plan; this is the final diff check).
- Commit will need an `Approved-Cross-Boundary: observability <reason>` trailer per @team-lead's instruction, reflecting the owner-paired design on `otel_http.rs`.

---

## Code Review Results (Gate 3)

| Reviewer | Verdict | Findings | Fixed | Deferred |
|----------|---------|----------|-------|----------|
| Security | CLEAR | 0 | 0 | 0 |
| Test | CLEAR | 0 | 0 | 0 |
| Observability | CLEAR | 0 | 0 | 0 |
| Code Quality | CLEAR | 0 | 0 | 0 |
| DRY | RESOLVED-DEFERRED | 1 | 1 | 0 (1 extraction-opportunity breadcrumb → TODO.md) |
| Operations | CLEAR | 0 | 0 | 0 |
| Semantic Guard | CLEAR (SAFE) | 0 | 0 | 0 |

All reviewers verified independently (built/ran tests, source-checked the tricky mechanisms), not read-only. DRY's single real finding (`trace_headers()` duplicated across `ac_client.rs`/`telemetry_forwarder.rs`) was **fixed** in-review by hoisting to `crates/gc-service/src/services/mod.rs`; observability re-verified the post-hoist tree.

---

## Accepted Deferrals

No findings were deferred — DRY's one real finding was fixed in the diff. The RESOLVED-DEFERRED verdict reflects only a forward cross-service extraction-opportunity breadcrumb (ADR-0019 DRY-exception, not fix-or-defer):

- `docs/TODO.md` §Cross-Service Duplication — Per-service R-55 OTel `main.rs` init + Config four-knob scaffold now has 2 identical siblings (AC #25 + GC #26); hoist at MC #6 / MH #27.
