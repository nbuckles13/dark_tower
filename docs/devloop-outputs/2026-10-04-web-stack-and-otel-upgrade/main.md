# Devloop Output: Web stack (axum 0.8 / tonic / prost) + OpenTelemetry lockstep upgrade

**Date**: 2026-10-04
**Task**: Upgrade axum 0.7→0.8 (+tower/tower-http as needed), tonic/prost/tonic-build, and the OpenTelemetry crate family as one compatible set. Supersedes Dependabot #67 and #66. Step 2 of 3 on the consolidated dependency-upgrade branch.
**Specialist**: global-controller (paired: protocol)
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/2-web-stack-and-otel`
**Duration**: ~22h wall-clock (2026-10-04 03:56 → 2026-10-05 01:55 UTC, including a ~16h session pause)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `c7de08b0707238854f46b9d39c864a4a955bd6e6` |
| Branch | `feature/2-web-stack-and-otel` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `global-controller` |
| Paired | `paired-protocol` (proto-gen/build.rs GSA; regenerated code review); `paired-auth-controller` (added at Gate 1 on operator instruction: newly-live AC admin client endpoints) |
| Tier | `full` |
| Iteration | `2` (Gate-3 fix round + Gate-2 L7 retry) |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Database | `database` (added at Gate 1: audit migration, operator decision 6) |
| Client | `client` (added at Gate 1: registration 401→400 is client-visible) |
| Semantic Guard | `semantic-guard` (included: extractor impls on auth paths + route-syntax docs/comments are plausible check surfaces) |

---

## Task Overview

### Objective
One coherent dependency move so Cargo.lock ends with a single axum, tonic and prost version; otel crates at the latest mutually compatible set. Served paths, wire contracts (buf breaking), spans/propagation/resource attributes and metric names/labels unchanged.

Latest releases at setup (cargo search, 2026-10-04): axum 0.8.9, axum-extra 0.12.6, tower-http 0.7.1, tonic/tonic-build/tonic-prost/tonic-prost-build 0.14.6, prost 0.14.4, opentelemetry/_sdk/-otlp/-proto 0.33.0, tracing-opentelemetry 0.34.0.

### Scope
- **Service(s)**: all (ac, gc, mc, mh, common, proto-gen)
- **Schema**: Yes (one forward migration widening `auth_events.valid_event_type`; §10)
- **Cross-cutting**: Yes

### Debate Decision
NOT NEEDED — dependency upgrade with behaviour-preservation constraints; no design change.

### Lead setup note
Teammate prompts direct each teammate to read its own `docs/specialist-knowledge/{name}/INDEX.md` and `.claude/skills/devloop/review-protocol.md` as its first action, rather than the Lead pasting the contents inline (same content, avoids ~330KB of duplicated prompt text).

---

## Cross-Boundary Classification

One row per changed path or glob (the scope guard expands globs against the diff). Owner-routing semantics are unchanged from the plan rounds; GSA rows name the owner, with security reviewing per the intersection rule.

| File / glob | Category | Owner | Why |
|-------------|----------|-------|-----|
| `Cargo.toml` | GSA-adjacent → Domain-judgment | protocol | workspace pins incl. tonic/prost/tonic-prost(-build) codegen toolchain; otel 0.33 set; async-trait hoisted; security reviews |
| `Cargo.lock` | Mechanical | global-controller | minimal re-resolution (one axum/tonic/prost/opentelemetry*) |
| `crates/proto-gen/Cargo.toml` | GSA → Domain-judgment | protocol | tonic-prost runtime + tonic-prost-build build-dep; security reviews (GSA intersection rule) |
| `crates/proto-gen/build.rs` | GSA → Domain-judgment | protocol | tonic_prost_build::configure(); 5 skip_debug entries verbatim; security reviews |
| `crates/proto-gen/src/lib.rs` | GSA → Domain-judgment | protocol | "three"→"five" comment; stale result_large_err allow removed (tonic 0.14 Status is boxed); security reviews |
| `proto/buf.yaml` | GSA → Domain-judgment | protocol | buf breaking carve-out removed (@paired-protocol); security reviews |
| `docs/protocol/CONVENTIONS.md` | Domain-judgment | protocol | dated carve-out-removed note (@paired-protocol) |
| `scripts/lang/proto/breaking.sh` | Minor-judgment | infrastructure | comment tense only (@paired-protocol) |
| `migrations/20261004000001_auth_events_admin_event_types.sql` | GSA → Domain-judgment | database | NOT VALID swap of valid_event_type + event_has_subject, lock_timeout; security reviews |
| `migrations/20261004000002_auth_events_validate_event_types.sql` | GSA → Domain-judgment | database | VALIDATE both constraints, lock_timeout; security reviews |
| `crates/ac-service/**` | Domain-judgment | auth-controller | newly-live admin endpoints + handler fixes, audit, registration statuses/throttle, no-store, request span/extraction; security reviews |
| `crates/ac-test-utils/**` | Domain-judgment | auth-controller | harness helper rename (R5) |
| `crates/common/Cargo.toml` | Domain-judgment | observability | tower-http (shared request-span builders); test dev-deps |
| `crates/common/src/observability/otel.rs` | Domain-judgment | observability | 0.33 provider/resource/shutdown, configured_layer, set_remote_parent |
| `crates/common/src/observability/otel_grpc.rs` | Domain-judgment | observability | set_remote_parent, grpc_make_span, 0.33 citation, flags-03 test |
| `crates/common/src/observability/otel_http.rs` | Domain-judgment | observability | set_remote_parent, PROBE_ENDPOINTS, http_request_span |
| `crates/common/src/observability/testing.rs` | Domain-judgment | observability | `pub mod otel` (test-utils) |
| `crates/common/src/observability/testing/otel.rs` | Domain-judgment | observability | shared deployed-filter span capture + trace fixtures (DRY F2, Gate 3) |
| `crates/gc-service/**` | Mine | — | axum 0.8 routes, gRPC wiring, telemetry filter, tests |
| `crates/mc-service/**` | Minor-judgment | meeting-controller | BoxBody→Body, interceptor layer before auth, INFO grpc span, reparent via helper; observability reviews |
| `crates/mh-service/**` | Minor-judgment | media-handler | BoxBody→Body, interceptor layer before auth, reparent via helper; observability reviews |
| `crates/env-tests/**` | Domain-judgment | observability | collector span + cross-service trace_id env-tests, collector-log secrets scan, otel-proto fixture fields |
| `crates/dt-guard/src/gsa_sync.rs` | Domain-judgment | infrastructure | CANON db/migrations/** → migrations/**; content owner database, security reviews |
| `scripts/guards/simple/cross-boundary-ownership.yaml` | Domain-judgment | database | GSA path correction; infrastructure + security review |
| `docs/decisions/adr-0024-agent-teams-workflow.md` | Domain-judgment | database | §6.4 GSA path correction |
| `.claude/skills/devloop/SKILL.md` | Domain-judgment | database | GSA path correction mirror |
| `.claude/skills/devloop/review-protocol.md` | Domain-judgment | database | GSA path correction mirror |
| `docs/decisions/adr-0003-service-authentication.md` | Minor-judgment | auth-controller | admin:services sentence (R7) |
| `docs/decisions/adr-0004-api-versioning.md` | Mine | — | axum sample `/meetings/{id}` |
| `docs/BUILD_REQUIREMENTS.md` | Minor-judgment | protocol | prost-build 0.14 via tonic-prost-build; OUT_DIR |
| `docs/TODO.md` | Minor-judgment | observability | closes trace-continuity + R-55 entries, async-std entry, buf carve-out section; AC oracle/SNAT entries (auth-controller) |
| `audit-suppressions.toml` | Minor-judgment | security | RUSTSEC-2025-0052 removed (async-std gone) |
| `.cargo/audit.toml` | Minor-judgment | security | regenerated from audit-suppressions.toml |
| `docs/observability/metrics/ac-service.md` | Domain-judgment | observability | validation category, audit labels |
| `docs/observability/metrics/gc-service.md` | Minor-judgment | observability | `/ready` endpoint label |
| `docs/observability/label-taxonomy.md` | Minor-judgment | observability | pointer to ErrorCategory enum |
| `docs/observability/slos.md` | Minor-judgment | observability | open item extended to AC |
| `docs/observability/alerts.md` | Domain-judgment | auth-controller | ACAuditLogWriteFailures inventory; observability reviews |
| `docs/runbooks/ac-service-incident-response.md` | Domain-judgment | auth-controller | #audit-log-write-failures section; operations reviews |
| `infra/docker/prometheus/rules/ac-alerts.yaml` | Domain-judgment | auth-controller | ACAuditLogWriteFailures (ADR-0031); observability + operations review |
| `infra/docker/prometheus/kustomization.yaml` | Minor-judgment | infrastructure | files: entry for ac-alerts.yaml |
| `infra/grafana/dashboards/errors-overview.json` | Minor-judgment | observability | ac_errors_total grouped by error_category (pre-existing label bug) |
| `infra/docker/prometheus/rules/gc-alerts.yaml` | Minor-judgment | observability | stale `/other` telemetry note in GCTelemetryProxyHighRejectionRate description corrected (post-approval; ops approved) |
| `docs/runbooks/gc-incident-response.md` | Minor-judgment | operations | Scenario 10/11/13 telemetry queries `endpoint="/other"` → telemetry endpoint regex; `/other` scope note (post-approval; ops approved) |
| `infra/services/otel-collector/collector.yaml` | Minor-judgment | infrastructure | comment only (security R-COL) |
| `packages/sdk-core/**` | Domain-judgment | client | stale 400/409 comments, register() JSDoc, register 400/409 + signup tests |
| `packages/web-app/**` | Domain-judgment | client | sign-up text pin; e2e registration-budget model |

---

## Planning

### 1. Target set (verified by resolving a scratch crate against crates.io, not assumed)

| Crate | From | To | Evidence |
|-------|------|----|----------|
| axum | 0.7 | 0.8.9 | axum-core 0.5.6 |
| tower | 0.5 | 0.5 (0.5.3, already latest) | |
| tower-http | 0.5 | 0.7.1 | depends on http 1 / tower 0.5 only |
| tonic | 0.12 | 0.14.6 | otlp/proto 0.33 require `tonic ^0.14.1` |
| tonic-build | 0.12 | **replaced** by `tonic-prost-build` 0.14.6 (+ runtime `tonic-prost` 0.14.6) | 0.14 split prost codegen out |
| prost / prost-types | 0.13 | 0.14.4 | otlp/proto 0.33 require `prost ^0.14` |
| opentelemetry / _sdk / -otlp / -proto | 0.24 / 0.24 / 0.17 / 0.7 | 0.33.0 (all four) | all pin `^0.33` to each other |
| tracing-opentelemetry | 0.25 | 0.34.0 | requires `opentelemetry 0.33.0` (no longer depends on the SDK) |
| axum-extra | — | not added | no `Host`/`TypedHeader`/`#[async_trait]` extractor/`Option<Extractor>` usage anywhere in the tree (grepped) |

Scratch resolution result: exactly one `axum` (0.8.9), one `tonic` (0.14.6), one `prost` (0.14.4), one `hyper` (1.x). **Remaining duplicate: `tower-http` 0.6.x** — pulled only by `reqwest` (0.12 *and* 0.13 both require `tower-http ^0.6.8`), not by anything in this stack. Today's lock already carries 0.5.2 + 0.6.7 for the same reason, so the count is unchanged. Options sent to Lead (§6).

Feature changes needed to avoid new duplicates/behaviour:
- `opentelemetry-otlp`: `default-features = false, features = ["grpc-tonic", "trace"]`. 0.33 defaults are `http-proto` + `reqwest-blocking-client` + `internal-logs` + metrics/logs, which would pull **reqwest 0.13** (a second reqwest) and an HTTP exporter we don't use. Today we only use grpc-tonic.
- `opentelemetry` / `opentelemetry_sdk`: `internal-logs` is a new default (absent in 0.24). Proposal: `default-features = false` with the 0.24 default set (`trace`, `metrics`, `logs`) so the SDK does not start emitting its own diagnostics into our `tracing` subscriber — keeps the documented "runtime export failures drop silently; dt_otel_export_failures_total is task #28" contract in `otel.rs`. **Question for @observability.**
- `opentelemetry_sdk` `rt-tokio`: dropped (only feeds the experimental async-runtime processors, see §3). `testing` stays in dev-deps (InMemorySpanExporter moved to `opentelemetry_sdk::trace::InMemorySpanExporter`).

### 2. axum 0.8 / tower-http 0.7
- Route syntax: GC `routes/mod.rs` `/:code/guest-token`, `/:code`, `/:id/settings` → `{code}`/`{id}`. Grep of all `.route(` sites in every crate (incl. test routers): those three are the only colon captures; no `*wildcard` routes. No `without_v07_checks`.
- Served paths identical — add/extend GC route tests to hit the three templated URLs through `build_routes()` (the 15 #67 panics already prove they're exercised; I'll confirm each is covered by a non-404 assertion).
- `TimeoutLayer::new` is deprecated since tower-http 0.6.7 (warning → clippy fail) → `TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, ..)`, which is exactly the old default (408), so behaviour is unchanged.
- tower-http 0.7 trace change: `TraceLayer::new_for_grpc()` failure events now include the gRPC error message; Default* events explicitly parent to the request span. Log content only; no metric/span-name change. Flagged for @observability/@security (gRPC messages in logs — our `Status` messages are already generic by convention; I'll grep the GC/MC/MH `Status::*` constructors for anything sensitive).
- Remaining axum 0.8 breaks (`Sync` handlers, `serve` generic, `Path` tuple arity) are compile-driven.

### 3. OpenTelemetry 0.24 → 0.33 (common + test harnesses)
- `TracerProvider` → `SdkTracerProvider`; `Tracer` → `SdkTracer`; `Config` removed → `.with_sampler()` / `.with_resource()` on the builder; `TraceError` → exporter `ExporterBuildError` (the `OtelInitError::Sdk` variant's source type changes; variant/message kept).
- Exporter: `SpanExporter::builder().with_tonic().with_endpoint(..).with_channel(channel).build()` — the eager probe still hands the *same* `Channel` to the exporter (`with_channel` exists in 0.33).
- **Resource unchanged**: `Resource::builder_empty().with_attributes([service.name, service.version, service.namespace, deployment.environment]).build()`. `Resource::builder()` would add `telemetry.sdk.*` + env-detector attributes; `builder_empty` keeps exactly today's four. Builder's first `with_resource` replaces (merge only applies on a second call), verified in sdk 0.33 source.
- **Shutdown**: `global::shutdown_tracer_provider()` is removed (0.28). `OtelGuard` will hold a clone of the `SdkTracerProvider` and call `provider.shutdown()` in `Drop` (still wrapped in `catch_unwind`, result ignored as before — Drop can't propagate). Still only constructible by `init_otel`.
- **Batch processor model**: 0.28+ `with_batch_exporter(exporter)` runs on a dedicated background thread instead of a Tokio task (the `runtime::Tokio` variant survives only behind `experimental_trace_batch_span_processor_with_async_runtime`). Proposal: use the default (non-experimental) thread-based processor; all four services use multi-thread `#[tokio::main]`, so the tonic channel's I/O keeps being driven by the runtime while the BSP thread/shutdown blocks. Upstream README documents exactly this tonic + batch pattern. Alternative if @observability prefers zero runtime-model change: enable the experimental feature and keep `runtime::Tokio`. **Question for @observability/@operations.**
- OTLP gRPC exporter now retries by default (0.33). Noted as a runtime behaviour change; I don't plan to disable it. **@operations/@observability FYI.**
- **tracing-opentelemetry 0.32+ span-shape changes** (observability constraint "spans unchanged"):
  - *Context activation* (#202) is on by default: entering a span starts its OTel context, after which `set_parent` returns `Err(AlreadyStarted)`. Every production reparent site calls `Span::current().set_parent(..)` on an already-entered span (common `otel_http`/`otel_grpc`, MC `webtransport/trace.rs`, MH `webtransport/connection.rs`), so with defaults **trace continuity would silently break**. Plan: `OpenTelemetryLayer::new(tracer).with_context_activation(false)` (0.25 semantics). Test harnesses (`gc/mc/mh tests/common/otel_*`, common tests) build their layers the same way — I'll route them through one shared constructor/setting so harness and prod can't drift.
  - `target` span attribute is new and on by default → `.with_target(false)` to keep the attribute set.
  - Location attributes are **renamed upstream with no opt-out**: `code.filepath`/`code.lineno`/`code.namespace` → `code.file.path`/`code.line.number`/`code.module.name` (semconv alignment). Grepped infra/docs/dashboards/collector config: nothing keys on the old names. Options: accept the rename (recommended), or `with_location(false)` (drops them entirely — worse). **Decision for @observability.**
  - `set_parent` now returns `Result<(), SetParentError>` (`#[must_use]` via `Result`). Plan: one helper in `common::observability` (e.g. `set_remote_parent(&Span, Context)`) used by the 4 production sites: `LayerNotFound` (OTel disabled, `OTEL_ENABLED=false`) and `SpanDisabled` (`Span::none()`, the documented GC no-op) are the expected silent cases exactly as today; `AlreadyStarted` is a wiring bug that activation-off precludes — `debug_assert!` + `tracing::warn!` rather than swallow. Tests assert `.is_ok()` on their direct `set_parent` calls (stronger than today). **@observability / @dry-reviewer: OK?**
- Metrics: our exported metrics go through the `metrics` crate + Prometheus exporter, not OTel — untouched. GC telemetry proxy: `opentelemetry_proto::tonic::*` module paths are unchanged in 0.33; any message-field churn is compile-driven and must keep the allowlist/filter semantics identical (proxy tests are the check).

### 4. tonic 0.12 → 0.14 / prost 0.14 (with @paired-protocol)
- `proto-gen`: `tonic_prost_build::configure().skip_debug([...same 5 paths...]).compile_protos(..)`; add `tonic-prost` runtime dep. Generated code stays in OUT_DIR. `.proto` files untouched. **Correction:** both protos are currently under `buf.yaml` `breaking.ignore`, so Layer 6 green proves nothing until @paired-protocol's carve-out removal lands; the real evidence (unsuppressed run, 0 findings, positive control) is in "### Protocol (paired)" below; I'll also diff the generated `.rs` (old vs new OUT_DIR) for @paired-protocol to confirm the `skip_debug` set and field tags/names are identical.
- `tonic::body::BoxBody` → `tonic::body::Body` in GC `grpc/auth_layer.rs`, MC/MH `grpc/auth_interceptor.rs`, MC `tests/auth_layer_integration.rs`, and main.rs server wiring. Interceptor/`InterceptedService` paths unchanged in 0.14; `NamedService` moved to `tonic::server` if referenced.
- Wire behaviour note: tonic 0.14.6 maps "no trailers" to `Unknown` instead of `Ok` (upstream fix #2543) — only affects malformed peers.

### 5. Escalation — AC admin routes are not served today → **Lead decision: SERVE, with conditions (see §9)**
`crates/ac-service/src/routes/mod.rs` already declares `/api/v1/admin/clients/{id}` and `/{id}/rotate-secret` (since `99cfc6f5`). Under axum 0.7 (matchit 0.7) braces are **literal**, so those routes match only the literal string `{id}`; `GET /api/v1/admin/clients/<uuid>` is a 404 today (handlers are only exercised by calling them directly in tests). Under axum 0.8 they become real captures and the four admin endpoints (get/update/delete/rotate-secret) start serving, behind the existing `require_admin_scope` layer. This is the one place "served paths must be identical" cannot hold without deliberately breaking the routes. My recommendation: let them serve (it's the evident intent, and the metric normalizer at `ac-service/src/observability/metrics.rs:330` already expects them), add a router-level test proving each is reachable and admin-scope-gated, and pull **auth-controller** + **security** into review for it. Sent to @team-lead.

### 6. Escalation — leftover `tower-http` 0.6 duplicate → **Lead decision: option (a)**. tower-http 0.7.1. The reqwest-owned 0.6.x copy stays as an **operator-accepted transitive duplicate** (reqwest 0.12/0.13 require `^0.6.8`); `cargo tree -i tower-http@0.6.x` evidence is recorded here at implementation. axum/tonic/prost/otel must still end single-version.
Options: (a) take tower-http 0.7.1 (latest; duplicate 0.6 remains via reqwest only, same count as today) — recommended; (b) pin tower-http 0.6 to dedupe against reqwest (holds back a release — not my call); (c) neither reqwest 0.12 nor 0.13 can dedupe it. Sent to @team-lead.

### 7. Mechanism restatement
Instance: "bump axum/tonic/otel". Mechanism: "every *default* that changed across these majors must be either re-pinned to today's value or consciously accepted". Same-owner siblings this surfaced beyond the named list: route-syntax in AC (§5), deprecated `TimeoutLayer::new`, otel `internal-logs`/`target`/context-activation defaults, otlp default transport features, otlp retry default, tonic no-trailers status. Each is listed above with a keep-or-accept decision.

Also considered: adding `axum`/`tonic`/`prost` to dt-guard `SINGLE_VERSION_CRATES` so the one-version rule is enforced, not one-off. Not proposed: that list's documented criterion is process-global state (recorder/dispatcher/provider), which these don't share — membership is guard *policy content*, so I'm raising it rather than doing it.

### 8. Gate 1 reviewer input: resolutions (supersede §1-§4 where they differ)

**OTel (observability B1/B2, R3-R7; operations 1-4; security S4/S6)**
- **R-OBS-B1 (set_parent).** The layer gets `.with_context_activation(false)`. One pub constructor, `common::observability::otel::configured_layer(tracer)`, also sets `.with_target(false)` (`with_level` stays at its false default). All 7 `OpenTelemetryLayer::new` sites (prod `init_otel`, common otel_http/otel_grpc tests, gc `tests/common/otel_support.rs`, mc/mh `tests/common/otel_capture.rs`) go through it, so harness and prod cannot drift. The continuity tests are the proof, unmodified.
  - **Result handling** goes through one helper, `set_remote_parent(&Span, Context)`, used at the 4 prod sites. No `let _ =`.
    - `AlreadyStarted` → `warn!`.
    - `SpanDisabled` → silent (filtered span / `Span::none()`).
    - `LayerNotFound`: counter-proposal to @observability, because it is the *normal* case when `OTEL_ENABLED=false`. No layer is composed then, yet the GC HTTP middleware and the gRPC interceptors still run, so an unconditional warn would fire on every request in every OTel-disabled deployment. Plan: `init_otel` sets a process-global `OnceLock` "OTel layer installed". `LayerNotFound` then warns iff installed (a real wiring bug, e.g. the layer was composed under a `reload::Layer`), and stays silent iff OTel is disabled by config.
  - A positive-control test asserts that a valid inbound traceparent DOES parent the span through the helper. It sits next to the reject tests (security S9 note).
- **R-OBS-B2 / OPS-1 (shutdown flush).** `OtelGuard` holds an `SdkTracerProvider` clone. `Drop` calls `provider.shutdown_with_timeout(OTEL_SHUTDOWN_TIMEOUT)`, a named const of 3s, chosen against the 30s drain inside the 35s grace period. An `Err` is logged at `warn!`; the `catch_unwind` stays. Guard doc: it must be dropped in `main()` (block_on thread), never moved into a spawned task. The BSP is a dedicated std thread that `block_on`s the tonic export, which needs a free runtime worker to drive the Channel, and GC/MC/MH run 1 worker under their CPU limits. All four mains already hold it to end of `main`. Module doc and main.rs comments are updated.
- **BSP model.** The default thread-based processor; no experimental async-runtime feature. The dependency is documented at the guard (above).
- **R-OBS3 (resource).** `Resource::builder_empty().with_attributes([4])`. The unit test tightens to assert the EXACT key set. `deployment.environment` is unchanged.
- **R-OBS4 (span attrs).** `with_target(false)`. The `code.filepath/lineno/namespace` → `code.file.path/line.number/module.name` rename is **accepted by @observability** (no opt-out upstream; no consumers found). `otel.status_message` has no uses.
- **OPS-3 (exporter).** `.with_channel(channel)` is kept (env endpoint ignored, same as today; the eager probe covers that exact channel). Export timeout stays at the 10s default. **Retry:** otlp 0.33 enables retries by default (3 retries, exponential backoff; changelog §Retry). 0.17 had none, and retries would stretch the shutdown flush, so I set `.with_retry_policy(RetryPolicy::disabled())` to preserve today's behaviour. No `experimental-grpc-retry`.
- **Default features.**
  - `opentelemetry-otlp`: `default-features = false, ["grpc-tonic", "trace"]` (S4). No HTTP exporter, no reqwest 0.13, no extra TLS stack, no `tls-*` features.
  - `opentelemetry`/`_sdk`: drop the new `internal-logs` default by listing the 0.24 default set. Still open with @observability (asked in my first message, not yet answered).
- **S6 propagator.** The bound checks and adverse tests stay unmodified. I re-verify the delegated format/zero/version checks against sdk 0.33 source and update the citation in `otel_grpc.rs:~93`. One upstream change, per @observability: the 0.33 inner `TraceContextPropagator` accepts unknown traceparent flags and zeroes them (W3C-correct). If a test asserts rejection of `flags=02`, I change it to assert accept-with-flags-masked, with a justification line here. That is the only assertion edit I expect; any other one gets listed here.
- **Stale version prose (DRY 4).** Re-verified against the new sources, not just renumbered: root `Cargo.toml` opentelemetry-proto comment, `otel_grpc.rs:95`, `otel.rs:166`, MC `gc_client.rs:84`.
- **Single-version check (OPS-4, code-reviewer 5/7, test 6).** main.md gets `cargo tree -d` evidence that axum, tonic, prost, opentelemetry, opentelemetry_sdk and opentelemetry-proto each appear exactly once (the registry also holds sdk 0.32.0; it must not be in the lock).

**axum / tower-http (security S1/S2/S7, test 1-3, obs R6/R7, ops 5)**
- Only the 3 GC captures change (`/{code}/guest-token`, `/{code}`, `/{id}/settings`). Swept every `.route`/`.nest` in all crates incl. test routers, test-utils and the MC/MH health/metrics routers: no other `:` or `*` segments, and `/health`, `/ready`, `/metrics` are static and unchanged. **No test request URL is edited.** `route_layer` auth placement is unchanged, verifiable by a before/after diff of both routes files.
- No `Option<Extractor>`, no custom `FromRequestParts`/`OptionalFromRequestParts`, no `Option<Extension<Claims>>` introduced. If the compiler pushes toward one, I stop and flag it to @security. AC keeps `into_make_service_with_connect_info`.
- Metric `endpoint` labels stay raw-URI → `normalize_endpoint`/`normalize_path`. **No switch to `MatchedPath`.** The normalizer tests are untouched.
- `TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, 30s)`, so timeouts stay 408. DefaultBodyLimit, the MH `max_decoding_message_size` and CORS config (fail-closed, `allow_credentials(false)`, `max_age`) are unchanged.

**tonic / prost (code-reviewer 2/3, DRY 1/3, security S3/S8, protocol)**
- Taking @paired-protocol's verified patch (`scratchpad/proto-gen-0.14.patch`) for the proto-gen part, applied by me: `tonic_prost_build::configure()`, one `.skip_debug([..])` per entry (repeated calls extend the set, verified in 0.14.6 source), all 5 entries and their justification comments kept, header comment renamed to `tonic-prost-build`, lib.rs "three" → "five". **Verification of skip_debug**: it is fail-closed by construction. The hand-written redacting `Debug` impls in `proto-gen/src/lib.rs` would collide (E0119) if any skip were lost. The OUT_DIR diff (protocol) shows the 5 `#[prost(skip_debug)]`, all `#[prost(..)]` tags identical, the 11 gRPC route strings byte-identical, and no new Debug-deriving wrapper types.
- `tonic-prost`, `tonic-prost-build` and `async-trait` go into `[workspace.dependencies]`, and members use `{ workspace = true }`. `async-trait` stays on `McClientTrait` (it is used as `dyn`).
- **`#[tonic::async_trait]`**: tonic 0.14.6 generated server traits still use `#[async_trait]` (`tonic-build-0.14.6/src/server.rs:227`), so the attribute stays at all 11 sites. Unchanged pattern, no mix, no shim.
- **buf breaking:** Layer 6 buf breaking proves nothing for either proto while `proto/buf.yaml` `breaking.ignore` exists. The evidence is protocol's unsuppressed run + positive control (§Protocol (paired)). Once the buf.yaml restore lands, Layer 6 is a real gate.
- `BoxBody` → `tonic::body::Body` is migrated identically in GC `auth_layer.rs` and in MC/MH `auth_interceptor.rs` + tests (same type, same bounds).
- **S8 path routing proof**: the generated paths are byte-identical (protocol diff). The existing tests that prove it are GC `grpc/auth_layer.rs` unit tests + `tests/auth_tests.rs`, MC `grpc/auth_interceptor.rs` tests + `tests/auth_layer_integration.rs`, MH `grpc/auth_interceptor.rs` tests.
- **S5 TLS drift**: before/after `cargo tree -e features -i rustls` and the lock name list go into main.md. Must hold: no openssl/openssl-sys/native-tls, a single reqwest (default-features off, rustls-tls), no new tonic `tls-*` features, no new `ClientConfig::builder()` call path.

**GC OTLP ingest (observability R5, security S9)**
Prost drops unknown fields on decode, so fields that 0.7 didn't know about were stripped before re-encode. Under 0.33 they would be forwarded, and the filter is private-by-default. Enumerated by diffing every field, oneof variant and enum variant in the 0.7.0 vs 0.33.0 generated `common`, `resource`, `metrics`, `trace`, `collector.metrics` and `collector.trace` modules (registry source):

| Field (0.33) | New? | Handling |
|---|---|---|
| `common.AnyValue.value::StringValueStrindex(i32)` | new | **drop**: non-scalar arm of `value_is_scalar`, + test beside `bytes_value_on_allowlisted_key_is_dropped` |
| `common.KeyValue.key_strindex` | new | **dropped by allowlist** (a dictionary-keyed KV has `key == ""`, not allowlisted), + one-line test |
| `resource.Resource.entity_refs: Vec<EntityRef>` (`schema_url`, `type`, `id_keys`, `description_keys`) | new | **clear** via the shared `filter_resource()`, called from both `filter_metrics` and `filter_traces` (§8b), + test through both request types |
| `metrics.Metric.metadata: Vec<KeyValue>` | pre-existing gap (in 0.7 too) | **clear**, counted in the dropped tally, + test that a non-allowlisted metadata key does not survive (partial-invariant fix, in-loop) |
| everything else in the metrics/trace/collector request trees | unchanged 0.7 → 0.33 | existing handling |

**Rollback / verification (operations 6/7, observability)**
- **Rollback**: superseded by the "Rollback (updated)" line in §8b. Wire formats, traceparent and resource attributes are unchanged, so mixed old/new pods during a rolling update interoperate.
- **Kind end-to-end**: superseded. See the corrected item 5 in the Validation plan section (no Tempo; collector receiver metrics + GC rollout-restart flush).

**Test hygiene (test 4/5).** No new `#[ignore]`, cfg-gating, deleted tests or loosened assertions. Test-helper API renames are listed here at implementation time (provider/exporter construction in `otel_support.rs`, `otel_capture.rs`, the otel unit tests); assertions are unchanged except as justified above.

**async-std (DRY 5).** sdk ≥ 0.29 drops async-std. If `cargo tree -i async-std` (all edges incl. dev) is empty, I remove RUSTSEC-2025-0052 from every place that carries it, in-loop: `audit-suppressions.toml` (entry + comment block), `.cargo/audit.toml` (ignore list + comment), the `docs/TODO.md` 'Polyglot Pipeline Follow-ups' async-std entry, and its rows in TODO.md's suppression table (~2344/2396). `scripts/audit-suppressions-check.sh` then verifies consistency.


#### 8b. Second-round Gate 1 resolutions (supersede §8 where they differ)

- **`set_remote_parent` without `debug_assert!`** (code-reviewer, ADR-0002: deployed images may be built with `debug_assertions` on). `AlreadyStarted` gets `warn!` only, and logs the error variant only: never the `Context`, carrier or headers (semantic-guard). `SpanDisabled` is silent. `LayerNotFound` warns iff `init_otel` installed the layer (OnceLock flag), and is silent when OTel is disabled by config. The branch decision is a pure function (`SetParentError` + installed flag → disposition), so @test's per-branch tests need no panic: success (child trace_id/parent span_id equal the remote), LayerNotFound-uninstalled silent, LayerNotFound-installed warn, `Span::none()` silent, AlreadyStarted warn (forced with a layer built with activation on). **The helper is the only prod call path**: at implementation I grep for non-test `set_parent(` and record the result here.
- **`internal-logs` stays ON** (observability decision; my proposal withdrawn). `opentelemetry`/`_sdk` keep their default features. `opentelemetry-otlp` stays `default-features = false` (no reqwest 0.13, no HTTP exporter) with explicit `["grpc-tonic", "trace", "internal-logs"]`. All flags are set in the ROOT `[workspace.dependencies]` (dt-guard R5). `rt-tokio` is dropped from both the root entry and `crates/common/Cargo.toml` dev-dep. The `otel.rs` failure-model doc is rewritten: runtime export failures surface as SDK internal-logs at warn/error through our subscriber; the metric and alert remain task #28; the OTLP endpoint is logged verbatim, so it must never carry userinfo credentials.
- **Shared layer constructor** `configured_layer<S>(tracer)` lives in non-gated `otel.rs` and is generic over the subscriber `S`. Prod `init_otel` calls it, and so do all 7 sites (common `otel.rs` prod, `otel_grpc.rs` test helper, `otel_http.rs` test helper, gc `otel_support.rs` ×2, mc/mh `otel_capture.rs`). It sets `with_context_activation(false)` and `with_target(false)`, and wraps the layer in a **per-layer target filter** that excludes, on crate boundaries (`t == p || t.starts_with(p + "::")`, with `opentelemetry` also matching `opentelemetry_*`/`opentelemetry-*` crate names): `opentelemetry*`, `h2`, `hyper`, `tonic`, `tower::buffer`. Never bare `tower`. The fmt/JSON layer is unfiltered, so SDK warnings stay visible in logs. Test through the shared constructor: `h2` and `opentelemetry_sdk` spans are NOT exported, while real `TraceLayer::new_for_http()` and `new_for_grpc()` spans (`tower_http::trace::*`) and one of our layer spans (MH `SpanLayer`, `mh_service::*`) ARE exported, all in the same run. Optional SpanCapture hoist (TODO.md:869): not taken in this loop. This loop changes the layer construction those harnesses share, not the capture fixtures, so the entry stays open.
- **Activation positive control** (test #3). I flip `with_context_activation(true)` locally, record which GC/MC/MH continuity tests go red, revert, and log the result here.
- **init_otel rejects current_thread** (observability). `Handle::current().runtime_flavor() == CurrentThread` → `ConfigInvalid` with a distinct reason ("init_otel requires a multi-thread Tokio runtime: …"), checked before the probe. The 3 existing `init_otel` tests move to `#[tokio::test(flavor = "multi_thread")]`, each keeping its own specific error assertion.
- **Shutdown tests** (test #6 revised).
  - (a) multi_thread + fake OTLP collector (a tonic `TraceService` from `opentelemetry-proto` gen-tonic, as a dev-dep): end a span, drop the guard, assert the collector received it.
  - (b) multi_thread + a collector that accepts the connection and never responds: drop returns within `OTEL_SHUTDOWN_TIMEOUT` + slack, referencing the const.
  - (c) current_thread → asserts the distinct reason string.
  - Guard: `shutdown_with_timeout(OTEL_SHUTDOWN_TIMEOUT = 3s)`, `Err` → `warn!` inside `catch_unwind`.
- **Retry**: `RetryPolicy::disabled()` (0.17 parity; operations confirmed).
- **GC route tests** (test #1): for each of the 3 templated URLs, assert something only the matched route produces (the route's auth-layer 401, or the handler's JSON error `code`), not a bare status. No existing request URL is edited.
- **Span attribute shape** (test #5): one capture test asserts no `target` attribute. Any test asserting `code.filepath/lineno/namespace` moves to the new names, with a justification line here (accepted rename). The resource test asserts exactly 4 keys with their values.
- **Traceparent** (joint observability + security): **zero edits to existing assertions.** The 7 reject tests in `otel_grpc.rs` and the 1 in `otel_http.rs` stay byte-identical. If a delegated reject ever fails, I tighten `within_bounds`. **One new test**: traceparent flags `03` through `BoundedTraceContextPropagator` and the real `server_interceptor` → `set_remote_parent` path is accepted, with the same trace_id/span_id, `is_sampled()`, and flags == SAMPLED. That is also the positive control for activation-off. The `otel_grpc.rs:~93` citation is re-verified and bumped to 0.33. (My earlier "flags=02 rejection test" premise was wrong: no such test exists.)
- **gRPC TraceLayer Status grep** (security 3, observability). Done. GC/MC/MH server `Status::*` constructors built with `format!` only carry field names (`mc_service.rs:151-164`, `mh_service.rs:98-114` "{field} is required/too long/must be a valid URL"), integer-conversion errors (`mc_service.rs:219/222`) and counts (MH `mh_service.rs:244` held/cap). There are no sqlx/reqwest/JWT error strings and no echoed client values; auth arms are generic ("Invalid token"/"Access denied"). Clean, so no sanitization is needed, and I'll re-check any constructor the diff touches. tower-http 0.7 `DefaultOnFailure` (ERROR level) on the grpc TraceLayer now includes the status message.
- **AC (pending Lead ruling on §5).** If the routes serve:
  - Router-level matrix through `build_routes` for get/update/delete/rotate-secret: no token → 401; valid token without `admin:services` → 403; admin token + unknown UUID → handler-specific response (not router 404); non-UUID id → 400 `Path<Uuid>` rejection.
  - One metrics assertion that `/api/v1/admin/clients/<uuid>` normalizes to `/api/v1/admin/clients/{id}`.
  - `Cache-Control: no-store`: IN. See §9 item 3.
  - The commit message states that the upgrade turns these 4 endpoints on. API_CONTRACTS.md inclusion is flagged to security/AC.
- **Round-3 amendments.**
  - **"Installed" flag** (observability amendment + test isolation + DRY single-writer). The OnceLock is written in exactly one place, inside `configured_layer()` (the SSoT constructor), so harnesses and prod share the same installed state; `init_otel` gets it by calling `configured_layer()`. The branch logic is `parent_disposition(err, installed: bool)`, and `set_remote_parent` is a thin wrapper that reads the flag. Per-branch tests call the inner fn with an injected `installed` value, so test order cannot make them flaky. Warn and silent cases are asserted on captured log output (a scoped test subscriber), not just "no panic". Extra test: no layer + `set_remote_parent` gives no warn. "Installed + non-otel subscriber gives a warn" is covered by the predicate unit test plus a scoped-subscriber test, if practical; if not, I'll say so.
  - **GC OTLP filter, DRY.** One `filter_resource(&mut Resource, ..)` (attribute allowlist + `entity_refs` clear) is called from both `filter_metrics` and `filter_traces`. The entity_refs test runs through both request types. `StringValueStrindex` / `key_strindex` handling stays in the shared `value_is_scalar` / key-allowlist path.
  - **Traceparent.** The flags-03 test and the activation-off positive control are ONE test, going through `set_remote_parent`.
- **Rollback (updated; Lead ruling: single commit, option b).** The **safe-revert unit** is: `git revert <sha>` **and**, in the same revert commit, `git checkout <sha> -- migrations/20261004000001_auth_events_admin_event_types.sql migrations/20261004000002_auth_events_validate_event_types.sql`. Reason: the db-migrate Job runs plain `sqlx migrate run` (no `--ignore-missing`, `infra/docker/db-migrate/Dockerfile`), which FAILS on an applied-but-missing version and would block every redeploy (`deploy.sh:run_migration_job`). The superset CHECK is safe for the old binary (app-only rollback). Then rebuild images and redeploy; no ConfigMap/manifest change. A revert also:
  - returns the AC admin `{id}` endpoints to 404;
  - restores the buf `breaking.ignore` carve-out;
  - restores the RUSTSEC-2025-0052 suppression (if async-std was removed);
  - removes no-store, BadRequest/`validation`, the scope restriction and the honest audit types (old binary writes old types);
  - removes `ACAuditLogWriteFailures` and the INFO request spans (continuity returns to broken).
  All harmless as a unit; the schema stays widened.

### 9. Intended served-path changes (Lead-ruled behaviour change)

**Sweep** (Lead condition 3): every axum `Router` in the tree, in src, tests, test-utils, env-tests fixtures and dev servers. Files: ac `routes/mod.rs`, `middleware/http_metrics.rs`, `tests/http_metrics_integration.rs`; gc `routes/mod.rs`, `main.rs`, `middleware/{http_metrics,otel,cors_observer}.rs`, `tests/{cors_integration,http_metrics_integration,otel_http_grpc_bridge}.rs`; mc `main.rs`, `observability/health.rs`; mh `main.rs`, `observability/health.rs`. I grepped every path literal containing `{` in all of them, and checked for non-literal `.route(` arguments and `&str` path consts (none). **The only pre-0.8 brace paths are AC's two templates (4 endpoints).** The GC/MC/MH routers have none. (`gc tests/http_metrics_integration.rs:106` `"/api/v1/meetings/{code}"` is a metric *label* assertion, not a route.) No newly-live path beyond the AC four, so nothing extra to escalate.

| Method + path | Handler | Live before | Live after |
|---|---|---|---|
| `GET /api/v1/admin/clients/{id}` | `handle_get_client` | no (literal `{id}` only) | yes, behind `require_admin_scope` (`admin:services`) |
| `PUT /api/v1/admin/clients/{id}` | `handle_update_client` | no | yes, same |
| `DELETE /api/v1/admin/clients/{id}` | `handle_delete_client` | no | yes, same |
| `POST /api/v1/admin/clients/{id}/rotate-secret` | `handle_rotate_client_secret` (returns the new plaintext secret once) | no | yes, same |

The GC captures (`/:code` → `/{code}` etc.) are syntax-only: served URLs are identical.

**Sweep command + result** (recorded per @test):
```
for f in $(grep -rln --include=*.rs -E 'Router::new\(\)|axum::Router|Router<' crates | grep -v target); do grep -n -E '"/[^"]*\{' $f; done
grep -rn --include=*.rs -E '\.(route|nest|route_service|nest_service)\(\s*[A-Za-z&]' crates   # non-literal paths: none
grep -rn --include=*.rs -E 'const [A-Z_]+: &str = "/[^"]*[{:]' crates                     # path consts: none
```
→ `ac-service/src/routes/mod.rs:42` (`/api/v1/admin/clients/{id}`), `:48` (`.../{id}/rotate-secret`). The only other hit is `gc tests/http_metrics_integration.rs:106`, a metric label. No `format!`/const/`nest()` routes.

**Conditions (Lead) + security A1-A5 (all IN this loop).**
1. **Handler review** by @security + @paired-auth-controller. Defects are fixed in-loop:
   - **A1 (BLOCKER, privilege escalation).** `handle_update_client` currently writes any charset-valid scope list, and stored scopes cap token issuance (`token_service.rs:112-124`). Fix: reject unless `new_scopes ⊆ ServiceType::from_str(credential.service_type)?.default_scopes()`. `models/mod.rs` `default_scopes()` is the SSoT (no duplicated list). An unparseable stored service_type fails closed. **The exact rule is @paired-auth-controller's ruling**; I implement what they rule. Real-router tests:
     - PUT adding `admin:services` → rejected, stored scopes unchanged (re-GET);
     - PUT adding `admin.force-rotate-keys.ac` (and `service.rotate-keys.ac`) → rejected;
     - PUT adding `internal:meeting-token` to a non-GC credential → rejected;
     - cross-type (GC credential + `service.write.mh`) → rejected;
     - valid subset → 200;
     - end-to-end: after a rejected PUT, a service-token request for the escalated scope is still refused.
   - **A2 / R2 (validation → 400).** Add `AcError::BadRequest(&'static str)` → 400, code `INVALID_REQUEST`; the fixed string is the body message (see §10 "Error messages"). It is used for the A1 rejection and at **every** validation site currently built as `AcError::Database`: admin_handler `register_service` (~l.55), `create_client` (l.696), `update_client` (l.846/867/895), and `registration_service.rs:32`. `repositories/users.rs:171` is left alone (role comes from code). **Label** (observability ruling): new `ErrorCategory::Validation`, `as_str() == "validation"`, with an explicit `From<&AcError>` arm (no catch-all) and `test_error_category_mapping`/`_as_str` cases. Docs and dashboards:
     - catalog `ac-service.md`: `ac_errors_total` list corrected to what `From` can produce, plus `validation`; `ac_token_validations_total` entries (52/317) bumped 6→7 values / 14 series, per its "bounded by the enum" framing;
     - `label-taxonomy.md:81`: value list replaced by a pointer to the `ErrorCategory` enum (it had already drifted; clock_skew was missing);
     - `metrics.rs:13` doc: points at the enum;
     - `errors-overview.json` 284/286/302/443/444: `error_type` → `error_category` (pre-existing empty-legend bug);
     - `slos.md` open-items row "GC availability numerator counts client-attributable 4xx" extended to "GC and AC" (AC numerator = `ac_errors_total` across all categories). The numerator itself is NOT changed in this loop.
     - Test: invalid create, invalid update, and the registration path each yield 400 and record `ac_errors_total{operation=..., error_category="validation", status_code="400"}` via MetricAssertion.
   - **Expected metric shift (recorded per @observability).** AC admin and registration validation failures move from `{error_category="internal", status_code="500"}` to `{error_category="validation", status_code="400"}`. AC 5xx-based views drop accordingly. SLO burn is unchanged: those errors were already counted, and the numerator is all categories. There are no AC alert rules (no ac-alerts.yaml); I grepped for 5xx-ratio expressions at implementation and record the result here.
   - **A3 / R1 (scope tier, PUT policy).** `admin:services` stays as the scope for all 4 routes (no ADR defines a finer one). PUT policy CONFIRMED by auth-controller: `new_scopes ⊆ ServiceType::from_str(credential.service_type)?.default_scopes()`; unparseable → fail closed; empty list allowed. PUT is narrowing-only. The existing char/len checks stay as defence-in-depth.
   - **A4 (no secret material).** Verified by security and auth-controller. Test: GET and PUT JSON carry no `client_secret` or any `*hash*` key. Rotate returns only client_id + the one-time secret.
   - **A5.** Unknown UUID → 404 `NOT_FOUND` JSON. Non-UUID → `Path<Uuid>` 400 text/plain (envelope accepted by auth-controller; admin-only caller). Auth runs before extraction, so no token + non-UUID → 401 (no pre-auth parsing oracle).
   - **R3 (DELETE was broken: hard delete 500s).** Every API-created client has a `service_registered` `auth_events` row, and the FK has no ON DELETE, so a hard delete violates it. SET NULL breaks the `event_has_subject` CHECK, and CASCADE would destroy the audit log. **DELETE becomes revoke**: `service_credentials::deactivate` (`is_active=false`; token issuance already rejects inactive credentials, `token_service.rs:85`; ADR-0003's disable mechanism). Response keeps the `{"deleted": true}` shape, 200. Idempotent: a second DELETE → 200; unknown id → 404; never 500. The now-uncalled `service_credentials::delete` is removed, the "Hard delete" doc comment updated, and the 3 existing delete unit tests adjusted. Security conditions:
     - (a) a client_credentials token request after DELETE is refused;
     - (b) a subsequent PUT leaves `is_active=false` (PUT cannot reactivate);
     - (c) a repeat DELETE is never 500.
     - The DELETE tests seed via `handle_create_client` / the API so an `auth_events` row exists (the existing direct-repo seeding dodged the FK).
   - **R5 (admin routes and real user tokens).** Real AC user, meeting and guest tokens are rejected only by claims shape (`ServiceClaims.scope` is required; the user-side claims have no `scope` → deserialization fails → 401).
     - `admin_auth_tests.rs:267` `test_admin_endpoint_rejects_user_token` is misleading: its "user token" is `server_harness.rs:226 create_user_token`, which signs a ServiceClaims with `service_type: None`, and the test asserts 200. Rename it to `..._accepts_scope_bearing_token_without_service_type`, fix the comment, and rename/doc the harness helper.
     - Add a real negative test: a token signed via `crypto::sign_user_jwt` with real `UserClaims` (roles incl. "admin"), same key → 401 on an admin route. This pins the shape-based guard.
   - **R7 docs.** One sentence in ADR-0003 Comp. 2: "`admin:services` is an AC-internal HTTP scope gating `/api/v1/admin/*`; it is not in any `default_scopes()` set and does not follow the `service.*` pattern." Rotate and delete doc comments state that tokens already issued remain valid until exp (ADR-0007, accepted residual).
2. **Real-router matrix** (in `crates/ac-service/tests/integration/admin_auth_tests.rs`, via `TestAuthServer` → real `build_routes`; per-test DB isolation), for each of GET/PUT/DELETE `/clients/{id}` and POST `/clients/{id}/rotate-secret`:
   - **Admin + existing client UUID → 200, proving the handler acted on that id** (the positive control that would have been 404 under axum 0.7, stated in the test doc):
     - GET: `client_id` equals the seeded one, no secret fields;
     - PUT: a follow-up GET shows the narrowed scopes;
     - DELETE: `{"deleted": true}`, then a follow-up GET returns 200 with `is_active: false`, plus the R3 (a)/(b)/(c) checks;
     - rotate: the new secret differs, authenticates at `/api/v1/auth/service/token`, and the old one fails.
   - No token → 401 + `WWW-Authenticate`.
   - Token without `admin:services` → 403 `INSUFFICIENT_SCOPE`.
   - Admin + random UUID → 404, body code `NOT_FOUND`.
   - Admin + `not-a-uuid` → 400.
   - No token + `not-a-uuid` → 401.
   - Plus: the A1 escalation tests, the R5 real-UserClaims → 401 test, and an http_metrics assertion that `/api/v1/admin/clients/<uuid>` normalizes to `/api/v1/admin/clients/{id}`.
3. **`Cache-Control: no-store`: IN** (Lead; auth-controller CONFIRMED, R4). Router-wide `SetResponseHeaderLayer::if_not_present(CACHE_CONTROL, "no-store")` on AC (tower-http `set-header`). Test: no-store on service-token, user-token, create-client and rotate-secret responses; JWKS still `max-age=3600`.
4. The commit message states that this upgrade turns the 4 endpoints on, makes DELETE a revoke, restricts PUT scopes, and adds no-store and the 400 variant. The Rollback line covers the revert. **API_CONTRACTS.md: OUT** (R6; it documents none of the AC admin surface, so adding 4 would leave it partial).

**Committed collector env-test** (Lead ruling; replaces the Validation 5(a) manual check). A new test in `crates/env-tests/tests/30_observability.rs`:
- drives real GC→MC traffic (a join that reaches MC over gRPC);
- precondition: the `otelcol_receiver_accepted_spans` baseline series must EXIST, and a missing series fails with a reason token distinct from "delta not observed";
- polls the rise with `poll_until_any_instance_above` / `instances_exceeding_baseline` (`fixtures/metrics.rs`), no sleeps;
- asserts `otelcol_receiver_refused_spans` does not rise.

It runs in the Lead's Layer 7. `docs/TODO.md` gets the per-service attribution residual on the ~:870 entry (a collector `count` connector keyed on `service.name`; owner observability + infrastructure). The ~:870 entry itself stays open, and so does the ~:871 MC inbound-reparent entry (the env-test proves export arrival, not reparenting). The GC rollout-restart / no-shutdown-warn check is the Lead's, by hand, after Gate 2.

### 10. Durable admin audit + deactivated-client semantics (FINAL rulings; database + security in planning/review)

**Names (Lead FINAL, not to be reopened):** `service_scopes_updated`, `service_deactivated`, `service_secret_rotated`, `user_registered`. **One string per event** across the DB CHECK, `AuthEventType::as_str()` and the `ac_audit_log_failures_total{event_type}` label. The label `scopes_updated` is renamed → `service_scopes_updated`; observability approved, and the only emitter is the currently-dead `registration_service.rs:124`, with no dashboard/alert consumer. Rename sites: `metrics.rs:~458` `AUDIT_LOG_FAILURE_PAIRS`, `tests/audit_log_failures_integration.rs` (44/60/349/367; absorbed by the `ALL` derivation below), `docs/observability/metrics/ac-service.md:171`, `registration_service.rs:124`. `docs/TODO.md:911` is a closed historical entry and stays as is (dry-reviewer). `service_secret_rotated` is added to the same places. **Plus `user_registration_failed`** (residual (b); variant `UserRegistrationFailed`, in `ALL`, `AUDIT_LOG_FAILURE_PAIRS` and the ac-service.md label list).

**Vocabulary SSoT (DRY a-e, code-reviewer 2, database D3, observability).**
- `AuthEventType` gets variants `ServiceScopesUpdated`, `ServiceDeactivated`, `ServiceSecretRotated`, `UserRegistered`, plus `AuthEventType::ALL`, generated by `string_enum!` from the same list as the enum (Gate 3: a no-wildcard match test cannot force list membership). A drift-rule comment goes at the enum and at the migration: a new variant needs a migration.
- `record_audit_log_failure(event_type: AuthEventType, reason: &str)` is now typed, and every emit site passes a variant (no literals). The unit-test fixtures at `metrics.rs:~846-848` switch to real variants.
- `AUDIT_LOG_FAILURE_PAIRS` stays hand-written as the prod-emittable subset, plus a unit test that every pair's event_type is some variant's `as_str()` and that every variant with an audit-write site is present.
- `ALL_EVENT_TYPES` in the test is derived from `AuthEventType::ALL`.
- The handler tracing fields `event = "client_secret_rotated"/client_updated/client_deleted` are handler-operation log names, a separate axis like `CREDENTIAL_OPERATIONS`. Left alone.
- `#[allow(dead_code)]` on `AuthEventType` variants → `#[expect(dead_code, reason = ..)]`, and removed wherever the new code makes a variant live (code-reviewer 4).

**Migration (database final shape + operations lock ask).**
- `migrations/20261004000001_auth_events_admin_event_types.sql`:
  ```sql
  SET LOCAL lock_timeout = '5s';
  ALTER TABLE auth_events
      DROP CONSTRAINT valid_event_type,
      ADD CONSTRAINT valid_event_type CHECK (event_type IN (<10 existing verbatim>, 'service_scopes_updated', 'service_deactivated', 'service_secret_rotated', 'user_registered')) NOT VALID;
  ```
  **The same ALTER also swaps `event_has_subject`** (database ruling: same NOT VALID treatment, because Postgres doesn't know a CHECK is a relaxation and would otherwise scan under ACCESS EXCLUSIVE): `DROP CONSTRAINT event_has_subject, ADD CONSTRAINT event_has_subject CHECK (user_id IS NOT NULL OR credential_id IS NOT NULL OR event_type IN ('key_generated','key_rotated','key_expired','user_registration_failed')) NOT VALID`. The `valid_event_type` list also includes `'user_registration_failed'`. The DOWN block additionally notes that the old `event_has_subject` fails on subjectless `user_registration_failed` rows (and the old `valid_event_type` on any new-type row), so rollback is a forward fix.
  NOT VALID still enforces the CHECK on new rows, so there is no enforcement gap. It ends with a `-- DOWN migration (manual rollback):` block: (a) SQL restoring the original 10-value CHECK; (b) (a) fails while any new-type row exists, so rollback is a forward fix and audit rows are never deleted; (c) app-only rollback is safe, because the old binary writes only the original values.
- `migrations/20261004000002_auth_events_validate_event_types.sql`: `SET LOCAL lock_timeout = '5s'; ALTER TABLE auth_events VALIDATE CONSTRAINT valid_event_type; ALTER TABLE auth_events VALIDATE CONSTRAINT event_has_subject;`. The scans run under SHARE UPDATE EXCLUSIVE, so inserts continue. DOWN block: `-- no-op to reverse (validation state only)`.
- `SET LOCAL` because sqlx wraps each file in a transaction. A lock_timeout hit fails the db-migrate Job loudly, and a rerun retries; no retry logic is added. `20250122000001` is never edited. `event_has_subject` gains only the `user_registration_failed` exemption; credential events pass `credential_id = Some(..)`, `user_registered` passes `user_id`. **No new index:** `idx_auth_events_type_time (event_type, created_at DESC)` serves the limiter's count (range scan over the window's registration rows, then IP filter), and CREATE INDEX in a transactional migration would block inserts. AC uses runtime `query_as`, so there is no `.sqlx` refresh.
- **Deploy ordering** (operations). `deploy.sh` runs the db-migrate Job before the new AC pods roll. If the migration somehow lagged, the new binary's new event types would fail the old CHECK and fall through the fail-soft path (`warn!` + `ac_audit_log_failures_total{reason="db_write_failed"}`), which now fires `ACAuditLogWriteFailures` (below). Requests are unaffected; the remediation is to apply the migration (runbook step 3).

**Drift / migration tests (database 4, test D, code-reviewer 2, DRY e).** One `#[sqlx::test]` (all migrations applied):
- inserts one row per `AuthEventType::ALL` variant, with a seeded credential_id/user_id where `event_has_subject` needs it; every insert succeeds;
- `"not_an_event"` fails with a check violation on `valid_event_type` specifically;
- reads the CHECK value set back from `pg_constraint` and asserts it equals `ALL`'s strings in BOTH directions;
- asserts `convalidated = true` for BOTH `valid_event_type` and `event_has_subject` (proves file 2 ran);
- the `ALL` loop inserts `user_registration_failed` in its real write shape (no user_id, no credential_id), like `key_*`, and that succeeds;
- negative control: a subjectless `user_login_failed` fails on `event_has_subject` specifically (the exemption didn't widen).
Positive control: drop a value from the migration locally and watch it go red (recorded here).

**Writers: one per operation (auth-controller ruling, code-reviewer 1, database D4).** The service layer owns state change + audit row + `record_audit_log_failure`; handlers do HTTP mapping only.
- `registration_service::update_service_scopes` and `deactivate_service` are fixed (today they write `service_token_issued`/`service_token_failed`), **re-keyed to `credential_id`**, take the actor, and become the live path for PUT/DELETE. A sibling `rotate_service_secret` is added for rotate. The `#[allow(dead_code)]` markers go. No code remains writing `service_token_*` for a non-token event.
- `user_service` registration (`~:242`): `user_login` → `user_registered`. **Side effect:** `register_user` now emits `user_registered` + `user_login` (auto-login) instead of 2× `user_login`. At implementation I list every count-based test/doc this touches (each assertion edit gets a justification line), and record a grep of `user_login` in env-tests/dashboards.
- **Registration rate limit: OPTION A (auth-controller ruling; security cc).** `count_registrations_from_ip` (`user_service.rs:206-229`) counted `user_login` rows per IP. Today each registration writes two (the misrecorded row + the auto-login), so with the default max of 5 the limiter trips on the 4th registration, and logins also consume the budget. The config unit is *registrations*, so:
  - the query binds event types via `AuthEventType::*.as_str()` (SSoT, no literal), keyed on `ip_address = $1`. **Final form per residual (b) below:** `event_type IN (user_registered, user_registration_failed)`, any `success`. `log_registration_event` already passes `ip_address` (verified at implementation);
  - the wrong doc comment ("service_registered") and the stale "for simplicity, user_login" comment are fixed, and a comment at both the query and `log_registration_event` names the coupling (the limiter depends on this event type);
  - **sibling fail-open defect, fixed in-loop**: `result.map(..).unwrap_or(0)` turned a DB error into "0 registrations", silently disabling the limiter. It now returns `Result<i64, AcError>`, propagated with `?`, so registration fails (500) when the limiter can't be evaluated;
  - **behaviour change** (main.md + commit message): the effective per-IP registration threshold goes from ~3 to the configured 5, and logins no longer count;
  - tests:
    - **existing tests tightened in place (deliberate assertion strengthening, test's ruling).** `test_register_user_rate_limiting` (`user_service.rs:385`, unit tier) only asserted "limited at some point", and `test_register_rate_limit` (`user_auth_tests.rs:585`, HTTP tier) asserted `hit_rate_limit || success_count <= 6` ("may vary"). Both passed through the trip-on-the-4th bug and would pass for any threshold from 1 to 6. Both now assert exactly `DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS` registrations succeed and the next is `TooManyRequests`/429 (loop bound MAX+1, N from the const, no literal). Their stale doc comments ("service_registered", "based on user_login", "may vary") are fixed. Justification: they pin the corrected behaviour (4th → MAX+1); no third duplicate test.
    - **new: decoupling.** N successful logins from the same IP don't consume registration quota; a full quota of registrations still succeeds afterwards.
    - **new: fail-closed.** Break only the count query's read path (the `auth_events` seam; `users` untouched). Assert (a) registration returns **500 `DATABASE_ERROR`** (generic body; `AcError::Database` propagated from the count query), and (b) **no user row exists for that email**, which proves the limiter fails before insert.
  - **Accepted residual (a)** (security + auth-controller), stated in the `count_registrations_from_ip` doc comment and here: the limiter counts best-effort audit rows (ADR-0032), so a failed `user_registered` insert leaves that registration uncounted; `ACAuditLogWriteFailures` is the detector. Residual (b) (failed attempts are unthrottled, plus the email-existence oracle) is pre-existing; auth-controller writes its TODO at verdict time, and nothing changes for it here.
  - **Residual (b) IN (auth-controller ruling; supersedes "TODO"; security conditions 1-6 + database review).** Registration leaks email existence (`user_service.rs:116-118`, and a successful registration auto-logs-in). Removing the oracle needs a flow change (verify-by-email), which is design and auth-controller's TODO at verdict. **Throttling it is in this loop:**
    - schema: per the migration bullets above (`user_registration_failed` in both constraint swaps, VALIDATE in file 2);
    - `AuthEventType::UserRegistrationFailed`, in `ALL` / `AUDIT_LOG_FAILURE_PAIRS` / `ALL_EVENT_TYPES` / the ac-service.md label list, emitted via the typed `record_audit_log_failure`;
    - **IP is structural** (security 3): `register_user`'s `ip_address: Option<&str>` becomes `&str`. The handler always has it from `ConnectInfo<SocketAddr>` (`auth_handler.rs:155/163`), so the `if let Some(ip)` limiter skip disappears (it can't be bypassed by a missing IP) and every row carries `ip_address`;
    - **order:** the limiter runs first, before any validation or DB lookup. **A 429 writes NO row**, so a blocked client can't grow `auth_events` at request rate or extend its window. Every validation failure (incl. `email_exists`) writes exactly one row: `success=false`, `user_id`/`credential_id` NULL, `ip_address` set, user_agent as given, `failure_reason` ∈ {`invalid_email`, `weak_password`, `empty_display_name`, `email_exists`}, **metadata NONE**. Best-effort;
    - limiter: `event_type IN ($a,$b)` bound to `AuthEventType::{UserRegistered,UserRegistrationFailed}.as_str()`, any `success`, per IP and window, **fail-closed** (`Result` + `?`);
    - doc-comment residuals: (a) best-effort audit rows can leave attempts uncounted (ACAuditLogWriteFailures detects it); (c) count-then-insert is non-atomic, so a parallel burst can exceed MAX by up to the concurrency. (The former residual (b), "failed attempts unthrottled", is superseded by this ruling.)
    - **Tests (test T1-T7), N = `DEFAULT_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS`, all through the real router:**
      - T1: N failed attempts → the N+1th is 429. A mix (2 successes + N-2 failures) → the next is 429, while N-1 of the mix → the next is still allowed (both row types are counted, each once).
      - T2: the refused request's total `auth_events` delta is 0, and hammering past N keeps the row count flat.
      - T3: exactly one row per failed attempt (total delta 1): `user_registration_failed`, `success=false`, ip set, user_id NULL, metadata exactly NULL. The submitted email, password and display name appear nowhere in the serialized row (all columns).
      - T4: covered by the `ALL` insert in real write shape (above).
      - T5: see statuses below.
      - T6: attempts from IP-B don't count against IP-A, and back-dated rows outside the window don't count (inserted directly, no sleeping).
      - T7: logins-don't-count also covers failed logins (`success=false` `user_login`), and fail-closed covers the combined query.
      - **Limiter before validation:** at the limit, an *invalid* request (e.g. a weak password) gets 429, not 400, and writes no row.
      - **T6 window edge:** a back-dated row just *inside* the window DOES count, alongside the one just outside that doesn't.
      - If any metric test asserts register failures under 401/`authentication`, it's edited and listed here.
  - **Registration error statuses (FINAL: Lead Operator Decisions + auth-controller).** The four `register_user` validation returns stop using `AcError::InvalidToken` → 401. Invalid email (~96), short password (~101), empty display name (~109) → `AcError::BadRequest(&'static str)` (400 `INVALID_REQUEST`, message verbatim). Existing email (~117) → **new `AcError::Conflict(&'static str)` → 409 `CONFLICT`**, message text verbatim until the oracle TODO lands (security's condition). The Conflict `error_category` is `validation` (observability ruled; see Row contents). Conflict also serves rotate-on-deactivated (above).
    - T5: one router test per case: status + pinned code, body message unchanged, plus exactly one `user_registration_failed` row with the matching token.
    - Assertion edits, justified: `tests/integration/user_auth_tests.rs:443-444` (asserted 401 "using InvalidToken error for 'already exists'"), `user_service.rs:602` (matched `InvalidToken`), plus any others found at implementation.
    - **Metric shift (record; observability):** on `ac_errors_total`, registration validation failures move from `authentication`/401 to `validation`/400 or 409, next to the admin `internal`/500 → `validation`/400 shift. AC SLO burn magnitude is unchanged (all categories counted). AC panels split by error_category show authentication dropping and validation rising for registration. Those 401s no longer pollute the login brute-force / auth-failure signals.
    - **Client (@client's revised spec, in this loop).** Verified: `AuthError.fromResponse` maps 400 → `AuthBadRequestError`, 409 → `AuthConflictError`, 429 → `AuthRateLimitError` (Retry-After). Nothing in sdk-core/sdk-svelte/web-app keys on a register 401. The sign-up UI renders "AUTH: <server message>", so the four body messages above keep the visible text identical. Edits:
      1. `packages/sdk-core/src/errors/AuthError.ts`: header l.7-12 ("AC today emits only 401/403/404/429/500 …") rewritten (AC now emits 400 `INVALID_REQUEST` for registration/admin validation and 409 for duplicate registration); l.58 drops "forward-compat; AC has no live 400 today"; l.86 becomes "409 — conflict, e.g. registering an email that already exists in the org."
      2. `packages/sdk-core/src/http/AuthApiClient.ts` `register()` @throws names `AuthBadRequestError` (400 format), `AuthConflictError` (409 email exists) and `AuthRateLimitError` (429 per-IP throttle, now counting failed attempts).
      3. `packages/sdk-core/src/http/__tests__/auth-api-client.test.ts`: register `it.each` with 400 `{code:'INVALID_REQUEST', message:'Invalid email format'}` → `AuthBadRequestError`, and 409 `{code:'CONFLICT', message:'An account with this email already exists'}` → `AuthConflictError`. Each asserts not `AuthUnauthorizedError`, the exact status, the serverCode and the message verbatim.
      4. `packages/web-app/src/__tests__/authViews.test.ts`: sign-up with a stubbed 409 → `last-error` reads exactly "AUTH: An account with this email already exists".
      5. `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` ~l.534: register-mode + 409 → `failure_stage: 'signup'`.
      6. **e2e budget model** (the limiter no longer counts `user_login`): `packages/web-app/e2e/cohort.ts` header ("WHAT THE AC REGISTRATION LIMIT ACTUALLY COUNTS") and `cohortWindowSpend`/`assertCohortFitsAuthWindow` (sign-ins no longer spend; spend = N+1 registrations + failed registrations from the host; error text updated); `packages/web-app/tests/cohort.test.ts:119-131` to match (justified assertion edit: `{registrations:4, firstTestSignIns:4, total:8}` encoded the old `user_login` accounting; the new expectation counts registrations only, citing the `AuthEventType` SSoT; a failing test's worker restart re-registers and still counts); `packages/web-app/e2e/README.md` §Budgets (~l.330-370), noting that failed registrations from the same host (e.g. Rust env-tests' negative register cases) now spend the bucket (Kind config is 100/1min, so no live risk).
      - No e2e spec asserts a `/auth/register` status (auth-rejection's 401s are GC join + AC login), so no e2e expectation edits beyond item 6.
- **D5.** Keep `service_credentials::update_scopes`; delete the byte-identical `update_metadata` (its doc is false) and move its tests. Remove `delete()` (R3).

**Row contents (security M3-M5, auth-controller).**
- Every row sets `credential_id`, `success` and `metadata.actor_sub` (from a non-optional `Extension<crypto::Claims>` on the PUT/DELETE/rotate handlers).
- `service_scopes_updated` adds `old_scopes`/`new_scopes`.
- `service_secret_rotated` carries credential_id/client_id only.
- An A1 rejection writes `service_scopes_updated`, `success=false`, `failure_reason="scope_not_permitted"`, with the requested scopes.
- `MAX_SCOPES_PER_CLIENT = 32` (named const) → 400 above it.
- Charset/length-invalid input → `failure_reason="invalid_scope_format"` only, never the raw string.
- Rotate on a deactivated client → `service_secret_rotated`, `success=false`, `failure_reason="client_deactivated"`.
- `service_deactivated` is written only on an actual active→inactive transition.
- Unknown id → no row (the tracing audit event stays).
- **Error messages (auth-controller ruling on @client's finding; supersedes "generic body").** `AcError::BadRequest(&'static str)` → 400 `INVALID_REQUEST` and `AcError::Conflict(&'static str)` → 409 `CONFLICT`. The fixed string IS the response `message`, passed through verbatim. `&'static str` makes it structurally impossible for a runtime string (DB error, echoed input) to reach the body.
  - Registration messages stay byte-identical: "Invalid email format", "Display name cannot be empty", "An account with this email already exists", "Password must be at least 8 characters". The last is `const WEAK_PASSWORD_MESSAGE: &str` beside `MIN_PASSWORD_LENGTH`, with a unit test that it contains `MIN_PASSWORD_LENGTH.to_string()` (drift guard).
  - Admin sites that echoed input via `format!` become fixed text: "Invalid service_type. Must be one of: global-controller, meeting-controller, media-handler" (with a test that every `ServiceType::as_str()` appears in it), "Scope cannot be empty", "Scope exceeds maximum length of 100 characters", "Scope contains invalid characters", "Too many scopes", "Scope not permitted for this client's service type", and (Conflict) "client is deactivated". If a log needs the offending value, it is logged at the call site (the caller's own input, never secrets).
  - Tests: an exact-body-message router assertion (status + `code` + `message`) for all four registration cases and for each admin message.
- **No PII in registration logs** (security, adopted by auth-controller; ADR-0011). `register_user` adds no tracing field or message carrying `request.email`, the display name or the password; it logs only the fixed `failure_reason` token. The audit-write-failure warn carries only the event_type. Admin scope strings may be logged (charset-bounded, ≤100 chars). (Gate 3: auth-controller greps the diff for `email`/`display_name` in tracing macros.)
- **`error_category` for Conflict (observability, ruled):** `AcError::Conflict` → existing `ErrorCategory::Validation` ("validation") via an explicit `From` arm, with a mapping-test case; `status_code` separates 409 from 400. The rotate-on-deactivated test adds a MetricAssertion `ac_errors_total{error_category="validation", status_code="409"}`.

**Atomicity.** Fail-soft (security ruling, ADR-0032): the mutation commits; a failed audit insert → `warn!` + metric. Not transactional.

**Deactivated-client semantics (auth-controller FINAL).** 404 only for an unknown UUID.
| Operation on a deactivated client | Behaviour |
|---|---|
| GET `/{id}` | 200, `is_active: false` |
| list | included, `is_active: false` (no filter added) |
| PUT | narrowing → 200, persists, `is_active` stays false, audit row. Widening → 400 `scope_not_permitted` (same A1 rule; tested) |
| rotate-secret | **409 `Conflict` "client is deactivated"**, no secret generated, **stored hash unchanged**, no secret field in the body; `success=false` audit row |
| DELETE again | 200 `{"deleted": true}`, **no** new audit row |
| token grant (previously valid secret) | 401 `INVALID_CREDENTIALS`, body **identical** to a bad-secret response (asserted in the same test) |

**Why 409** (auth-controller re-decision, code-reviewer 3): the request is well-formed and refused only because of the resource's state, which is Conflict semantics. The earlier 400 rested only on "no Conflict variant exists"; with `AcError::Conflict` added for duplicate-email registration (below), 409 is correct.

**Tests (test A-E).**
- **A.** One real-router test per table row, each starting from a client deactivated through the real DELETE route. "DELETE again" asserts a total `auth_events` row-count delta of 0.
- **MAX_SCOPES_PER_CLIENT** boundary through the router, computed from the const: MAX → 200, MAX+1 → 400. An `invalid_scope_format` case asserts the raw rejected string appears nowhere in the audit row.
- **B.** "Exactly one row" means the total `auth_events` row-count delta across the request is 1, plus a match on event_type, credential_id and success.
  - Metadata is asserted as the exact JSON (key set + values).
  - The rotated plaintext secret (from the response) and the stored hash appear nowhere in the serialized row.
  - Non-success paths (401, 403, unknown-UUID 404, non-UUID 400) have delta 0. Rotate-on-deactivated 409 and A1 rejection have delta 1 with `success=false` (pinned).
- **C.** Audit-failure path through the existing deterministic `break_auth_events_inserts` seam (per-test schema). The request still succeeds, the state change persists, `ac_audit_log_failures_total{event_type, reason="db_write_failed"}` delta is 1, and siblings are `assert_delta(0)`. One per writer fn: PUT, DELETE, rotate, registration.
- **D.** The migration test above.
- **E.** Registration test: exactly one `user_registered` row and one `user_login` row (auto-login), and no second `user_login`. Same pattern for the re-keyed helpers. Existing tests asserting the old misrecorded types are listed with justifications.

**Alert: `ACAuditLogWriteFailures` (observability; authored per ADR-0031 by auth-controller's spec, reviewed by observability).**
- New `infra/docker/prometheus/rules/ac-alerts.yaml` from `_template-service-alerts.yaml`, group `ac-service-warning`: `expr: sum by (event_type) (increase(ac_audit_log_failures_total{job="ac-service"}[10m])) > 0`, `for: 1m`, `severity: warning`. The summary/description name only `{{ $labels.event_type }}`; `runbook_url` → `docs/runbooks/ac-service-incident-response.md#audit-log-write-failures`.
- Added to `infra/docker/prometheus/kustomization.yaml` `files:` (`dt-guard alert-rules-policy` enforces set equality).
- New runbook section with the auth-controller's 5 triage steps: (1) `event_type` identifies the write site; (2) correlate with AC DB error/pool metrics, and if they spiked → DB runbook; (3) DB healthy → grep AC warn logs for "Failed to log auth event" with a CHECK error, meaning code deployed ahead of its migration, so apply the migration and don't roll back the code; (4) recover lost rows from the `target:"audit"` log events for the window and export them before retention expires; (5) a gap in admin events means admin-action forensics loss, so identify the `admin:services` token holders active in the window.
- An inventory row in `docs/observability/alerts.md`. I'll check whether `33_alert_rules_loaded.rs` enumerates rule files or names and extend it if so. **The repo has no `promtool test rules` harness** (stated at `gc-alerts.yaml:338`), so `dt-guard alert-rules-policy` + `33_alert_rules_loaded` are the only checks. Recorded, not added (a new harness is new mechanism).

**GSA path correction (Lead: IN).** `db/migrations/**` → `migrations/**` (the real path) in all five mirrors: ADR-0024 §6.4 (`adr-0024-agent-teams-workflow.md:419`), `.claude/skills/devloop/SKILL.md:142`, `.claude/skills/devloop/review-protocol.md:30`, `scripts/guards/simple/cross-boundary-ownership.yaml:44`, and the CANON array, which lives in `crates/dt-guard/src/gsa_sync.rs:40` (the shell wrapper only dispatches). `crates/dt-guard/src/common/manifest_match.rs:153-154` is a synthetic glob-matching fixture and stays. Then `validate-gsa-sync.sh` plus the classification guard, which should now recognise the migration rows as GSA.

### 11. Inbound trace continuity is broken in the deployed config (Lead: IN; observability (a)-(d) with security conditions)

**Mechanism** (confirmed on the live cluster by observability: 2h, GC 7019 / MC 552 / AC 30 spans, zero shared trace_ids). `tower_http` `TraceLayer` request spans are DEBUG, and the deployed `RUST_LOG=info,<svc>=debug` disables them, so `set_parent` hits `Span::none()` (`SpanDisabled` under 0.34).

**(a) INFO request spans. Security R-URI is a blocker condition.**
- **HTTP TraceLayers (GC `routes/mod.rs:257`, AC `routes/mod.rs:134`).** A custom `MakeSpan` at INFO, span name `request`, recording only `method` and `endpoint` = that service's existing `normalize_endpoint()` / `normalize_path()` (the metric-label SSoT). No raw uri, query, headers or version: the meeting code in `/api/v1/meetings/{code}` and the public `/guest-token` is a guest-access capability.
  - **AC inbound extraction: IN** (observability/Lead; the missing receive half of GC→AC, since GC's `AcClient` already injects at `gc-service/src/services/mod.rs:32`). AC's router gets `extract_trace_context` middleware **inside** its TraceLayer (same placement and reasoning as GC `routes/mod.rs:229-251`), reparenting via `set_remote_parent`, on the same custom MakeSpan. To avoid duplicating GC's `middleware/otel.rs` wrapper, the axum middleware fn moves into `common::observability::otel_http` if it fits as-is (DRY check at review); otherwise AC gets a thin wrapper over the common `extract_trace_context`.
  - **Probe/scrape spans stay DEBUG** (Lead ruling; operations condition). The shared HTTP MakeSpan picks the level **from the same normalized `endpoint` value it records**: `/health`, `/ready`, `/metrics` → DEBUG, everything else → INFO, on both GC and AC. No second path list.
  - **Parent only when there is one** (observability condition, to keep (b) honest). The extraction path calls `set_remote_parent` only when the extracted context carries a VALID remote span context, i.e. a traceparent was present and accepted. Probes and scrapes send none, so there's no call and no warn. A traceparent-bearing request to a DEBUG endpoint DOES warn once (someone sent context we dropped).
  - **One probe-path constant** (the normalized `/health`, `/ready`, `/metrics` values), used by both the MakeSpan level decision and the tests, never re-listed.
  - Tests, through the real GC and AC routers under the manifest-derived filter:
    - (i) a probe path without traceparent → no exported span and no warn;
    - (ii) an API path → an INFO span is exported (positive control, same test);
    - (iii) two probe requests with traceparent → exactly one warn (injected latch).
- **gRPC TraceLayers (GC `main.rs:315`, MC `main.rs:458`).** `DefaultMakeSpan::new().level(Level::INFO)`: the gRPC uri is the bounded `/pkg.Service/Method` (observability + security OK).
- One shared helper per transport in `common::observability`, so levels and fields can't drift.
- AC `routes/mod.rs:136` also uses the deprecated `TimeoutLayer::new` → `with_status_code(StatusCode::REQUEST_TIMEOUT, ..)` (408 unchanged), same as GC.
- **Test** (security, test 6): for BOTH capability-bearing routes, `/api/v1/meetings/{code}` and `/{code}/guest-token`, a request with a known code yields an exported span whose name, attributes and events contain no trace of the code, while `endpoint="/api/v1/meetings/{code}"` IS present (positive control). The JSON log line emitted inside that request also does not contain the code.
- **Impact** (for operations/observability): no new log lines (On/Response events stay DEBUG). Every INFO+ line inside a request gains `method`/`endpoint` (bounded) via the current-span context, and those spans are now exported, subject to sampling.

**(b)** `set_remote_parent`: `SpanDisabled` with OTel installed → **warn-once**; with OTel not installed → silent. Added to the pure `parent_disposition` table, with tests per branch. **Isolation** (test 4): plain `cargo test` shares a process per test binary, so the once-latch is **injectable**. The inner fn takes a `&AtomicBool` latch, and the thin public wrapper supplies the process-global one. Warn-once tests (incl. the probe-with-traceparent case) pass a fresh latch, so they're order-independent.

**(c)** A component test with the subscriber built from the **deployed filter string**, read from the manifests (`infra/services/gc-service/deployment.yaml`, `mc-service/mc-0-deployment.yaml`, `ac-service/statefulset.yaml` `RUST_LOG`; fails loudly if not found, so no literal copy can drift) through `configured_layer`. A known traceparent goes through the real GC HTTP router, GC gRPC server, MC gRPC server and **AC HTTP router**, and the test asserts the span reparents. Positive control: reverting to DEBUG goes red (recorded here).

**(d) Real cross-service trace_id env-test (Lead: IN; security: cleared, keep `verbosity: normal`).** Extends `30_observability.rs`:
- the collector accepted/refused counter check from §9 stays as the precondition;
- then a fresh traceparent is driven through a GC join, and the test asserts its trace_id appears under `service.name=global-controller`, `service.name=meeting-controller` **and `service.name=auth-controller`**. The GC join path calls AC deterministically: `join_meeting` always calls `ac_client.request_meeting_token` on success, `handlers/meetings.rs:471`. All three are asserted in `kubectl logs -n dark-tower deploy/otel-collector` (env-tests already shell out to kubectl in `00_cluster_health.rs`);
- MH is not asserted (no OTel overlay in Kind; tracked separately).
- Robustness (test 8): a random trace_id is minted per run; polling goes through `crates/env-tests/src/eventual.rs` (bounded, no fixed sleep) over `kubectl logs --since=<window>`. Precondition with a distinct failure message: at least one GC span in the window, separating "collector saw nothing" from "continuity broken".
- This closes `docs/TODO.md` ~:870 (its "does not log per-span trace-ids" premise sentence is corrected) and ~:872 ("R-55 collector data/export env-test"), citing the new tests.

**Security R-COL.**
- Rewrite the `infra/services/otel-collector/collector.yaml:402-407` comment to reality: `normal` DOES print span attributes and trace IDs; ADR-0011 source discipline is the primary control and the collector log a secondary sink; dev collector only.
- Extend `00_cluster_health.rs` `test_secrets_not_in_logs` to scan the otel-collector pod log after traffic has flowed, with the existing patterns plus `eyJhbGci` and a non-empty positive control.

**Expected span-attribute baseline for review** (observability, pre-upgrade cluster): spans carry `code.filepath/code.namespace/code.lineno thread.id thread.name busy_ns idle_ns`, no `target`, and a resource of exactly 4 attrs. After the upgrade: the `code.file.path/code.line.number/code.module.name` names, still no `target`, and the same 4 resource attrs.

**Post-approval implementation additions (observability ruled; Lead informed):**
- **Mechanism (tracing-opentelemetry ≥0.32 child-start):** creating any child span starts the parent's OTel context (`layer.rs` `parent_context` → `with_started_cx`, explicit and contextual parents, even with activation off). After that, `set_parent` on the parent fails `AlreadyStarted` (warned by `set_remote_parent`). The rule: **no span may be created under a request/connection span before the reparent.**
  - *gRPC:* the auth layers' JWT-validation spans ran before `server_interceptor`, which broke MH inbound continuity. In GC, MC and MH (and their test rigs) extraction is now `tonic::service::InterceptorLayer` directly inside `TraceLayer`/`SpanLayer`, before auth. Each server builder carries the invariant comment "the extraction layer must stay between TraceLayer and any span-creating layer (auth)".
    - **Security-visible:** trace context is now extracted from unauthenticated gRPC requests, so a rejected request's span may sit inside a caller-chosen trace. Bounds still apply via `BoundedTraceContextPropagator`. @security acked via @observability; the GC `auth_layer.rs` and MC/MH `auth_interceptor.rs` module docs now say auth-rejection forensics must key on the layer's own log fields/metrics, never on trace membership. The resulting sampling-lever design question is recorded in `docs/TODO.md` §Observability Debt ("Public-ingress sampling lever").
    - Auth-layer tests (GC `grpc/auth_layer.rs` unit tests, MC/MH `grpc/auth_interceptor.rs` unit tests, MC `tests/auth_layer_integration.rs`) pass with assertions unchanged. The only edit there is the `BoxBody`→`Body` type rename.
  - *WebTransport:* `configured_layer` excludes `quinn`, `quinn_proto`, `quinn_udp`, `wtransport` and `wtransport_proto` (there is no `h3` crate in Cargo.lock). The MC `webtransport/connection.rs` and MH `webtransport/connection.rs` reparent sites carry the invariant comment. MC and MH `otel_webtransport_integration` gain `test_wt_valid_traceparent_reparents_under_deployed_filter`, which runs the manifest `RUST_LOG` (enables `<svc>=debug`) as a tripwire for future debug spans; the all-levels tests stay as the stricter tripwire.
- **GC `/ready` label (observability approved):** `normalize_endpoint` maps `/ready` to itself, with a test case. The `gc-service.md` cardinality now counts the 12 true normalizer outputs. The stale "telemetry reports as `/other`" content is corrected in `gc-alerts.yaml` (GCTelemetryProxyHighRejectionRate description), `gc-incident-response.md` (Scenario 10/11/13 queries now use `endpoint=~"/api/v1/telemetry/v1/(metrics|traces)"`, with the note that `/other` holds only unrecognized paths), `alerts.md` and the `gc-service.md` telemetry honesty block (operations approved the wording).

### Validation plan
Single list, incl. for the Lead's Layer 7 run:
1. **Lock.** `cargo tree -d` plus a Cargo.lock grep, showing exactly one each of axum, tonic, prost, opentelemetry, opentelemetry_sdk, opentelemetry-proto (sdk 0.32 absent). The only remaining duplicates are listed with their owner (tower-http 0.6 via reqwest, §6).
2. **TLS drift (S5).** Before/after `cargo tree -e features -i rustls` and the Cargo.lock name list. No openssl/openssl-sys/native-tls, a single reqwest, no tonic `tls-*` features.
3. **async-std.** `cargo tree -i async-std` empty → RUSTSEC-2025-0052 removed everywhere (§8).
3b. **Layer-7 registration budget (security heads-up).** In Kind all registrations share one SNAT'd peer IP, and the limiter now also counts failed registrations. Before "Ready for review" I count the per-minute registration attempts (successful + failed) that env-tests (incl. negative register cases) and the web-app e2e cohort make against `AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS=100` / `WINDOW_MINUTES=1` (`infra/services/ac-service/config.env:5-6`), and record the worst-case window here. If it can exceed the budget, I raise it with the Lead rather than changing config.
   - **Count (test reviewer; whole-run upper bound, so it also bounds any 1-min window).** env-tests, all successful (no deliberate invalid registrations): 21_cross_service_flows 2, 23_meeting_creation 4, 24_join_flow 5, 26_mh_quic 16, 27_mc_slot_placement 6, 28_mh_egress_admission ≤ `MAX_S9_PARTICIPANTS` = 40, 31_gc_telemetry 2, 34_mc_kek_rotation 2, 35_mc_server_mute_teardown 8, so ≤ 85. Playwright green run: shared user V 1 + userB 1 + auth-rejection throwaway 1 + cohort 4 = 7, +1 per failing test (worker restart re-registers V). auth-rejection's invalid `/register` is route-fulfilled and never reaches AC.
   - **Worst case 93 + (failing e2e tests) vs 100/min** (re-verified at implementation: the new 30_observability tests add 1 shared user). Fits even if the whole run landed in one minute**, and spend is strictly lower than under the old limiter (2 per registration + every sign-in). I re-verify this count at implementation.
   - Residual: raising `MAX_S9_PARTICIPANTS`, or 28 hitting its cap during a burst of e2e failures, shrinks the margin. The fix is `config.env`, not the limiter.
4. **Local gates.** `./scripts/layer-fast.sh` green. All new tests from §8/§8b, the activation flip positive control recorded, and the non-test `set_parent(` grep recorded.
5. **Kind (Layer 7, the Lead's layer-all run; nothing attempted from the container).** Corrected per @observability: there is **no Tempo**. The collector traces pipeline is `debug` at `verbosity: normal` (collector.yaml), which prints no resource attributes, so per-service attribution and a cross-service trace_id are NOT observable in-cluster (residual: TODO.md:870).
   - (a) **Committed env-test (§9)**: after driving GC→MC traffic, `otelcol_receiver_accepted_spans` rises above baseline and `otelcol_receiver_refused_spans` stays flat (Prometheus job `otel-collector-telemetry`). The baseline series must exist first; a missing series is a precondition failure, not a pass.
   - (b) **Lead, by hand, after Gate 2**: `kubectl rollout restart` of one GC pod: accepted-spans rises across the restart, and the pod's final logs carry no shutdown warn.
   - Resource attribute exactness is proven by the exact-set unit test, not in-cluster.
   - Cross-service continuity is proven at the component tier: the continuity tests through `configured_layer()` plus the activation-flip positive control. No manual query exists (no trace backend; normal verbosity logs no trace IDs). The open residual is TODO.md:870.
   - The shutdown flush is proven by the fake-collector tests (§8b a/b); in-cluster we only observe the accepted-spans increment plus the absence of a shutdown warn.


### Protocol (paired): codegen migration, wire identity, buf breaking

**Codegen move (verified in a scratch worktree, `cargo test -p proto-gen` green, 51 tests).** `tonic-build` 0.12 → `tonic-prost-build` 0.14; `proto-gen` gains runtime dep `tonic-prost` (generated code calls `tonic_prost::ProstCodec`). `skip_debug` now takes an `IntoIterator` — `.skip_debug(["…"])`; in 0.14.6 repeated calls extend a `HashSet`, so the per-entry form with its justification comments is kept. Fail-closed both ways: a dropped skip collides with the hand-written redacting `Debug` in `lib.rs` (E0119), an over-applied one breaks `ServerMessage`'s derive. WKTs still map to `::prost_types` by default.

**Wire identity (OUT_DIR diff, baseline built at `c7de08b0` vs 0.14).** Every `#[prost(...)]` attribute (tags, scalar/bytes types, oneofs, enum values), field and type name: identical in `signaling.v1`; identical in `internal.v1` modulo module order (tonic 0.14 emits each client beside its server; sorted sets identical). All 11 gRPC route strings identical; 5 `#[prost(skip_debug)]` present. Non-wire deltas only: prost 0.14 auto-derives `Eq, Hash` where possible; `tonic::body::BoxBody` → `tonic::body::Body`; codec path moved to `tonic_prost`; rustdoc list reflow.

**buf breaking.** `scripts/lang/proto/breaking.sh` → `STATUS=OK REASON=buf-breaking-passed`, but preceded by `SUPPRESSED=dark_tower/signaling/v1/signaling.proto,dark_tower/internal/v1/internal.proto` — vacuous for both files. Ignore-free evidence (inline `--config '{"version":"v2","modules":[{"path":"."}],"breaking":{"use":["FILE"]}}'`, against-trees `git archive`d to scratch, run via `pnpm exec buf` 1.72.0):

| Run | Result |
|-----|--------|
| `/work` `proto/` vs loop start `c7de08b0`, no ignore | rc=0, 0 findings |
| `/work` `proto/` vs pipeline base (merge-base `c1903d2d`, ancestor of `origin/main`), no ignore | rc=0, 0 findings |
| **Positive control**: scratch copy with `SubscriberSlot.sender_id` tag 1→99 (internal) and a `MediaKind` value 2→98 (signaling), vs `c7de08b0`, no ignore | rc=100; `FIELD_NO_DELETE` in `internal.proto:179`, `ENUM_VALUE_NO_DELETE` in `signaling.proto:19` — both files are evaluated |

The second row meets the `proto/buf.yaml` carve-out's RESTORE condition (main is post-reshape). Lead folded the restore into this loop: delete the `breaking.ignore` key + comment block, close the three `docs/TODO.md` bullets under §"Restore buf breaking enforcement after ADR-0036 story 1", and correct `docs/protocol/CONVENTIONS.md`. Acceptance: `breaking.sh` emits no `SUPPRESSED=` line and still returns OK.

**Restore applied (post "Plan approved").** `proto/buf.yaml`: `breaking.ignore` key + its 46-line comment block deleted (now `breaking.use: [FILE]` only). `docs/TODO.md`: §"Restore buf breaking enforcement after ADR-0036 story 1" deleted (3 bullets; the ci-client duplicated-base-ref point survives independently at the existing VERIFY-then-remove entry near `TODO.md:57`). `docs/protocol/CONVENTIONS.md`: dated "Correction (2026-10-04, carve-out removed)" appended to the existing correction chain. `scripts/lang/proto/breaking.sh`: COVERAGE LIMIT comment reworded from "the carve-out applies" to "any carve-out applies … while one exists"; the `SUPPRESSED=` detector is unchanged (standing control for any future entry).

| Check | Result |
|-------|--------|
| `bash scripts/lang/proto/breaking.sh` (Layer-6 wrapper, base `c1903d2d`) | `STATUS=OK REASON=buf-breaking-passed`, rc=0, **no `SUPPRESSED=` line** |
| Positive control under the **committed** `buf.yaml` (scratch copy of `proto/` with the same two injected tag changes, vs `c7de08b0`) | rc=100; findings in both `internal.proto:179` and `signaling.proto:19` — the real config now enforces both files |
| Detector still live: wrapper's awk over `HEAD:proto/buf.yaml` vs new file | old → `dark_tower/signaling/v1/signaling.proto,dark_tower/internal/v1/internal.proto`; new → empty |

---

## Operator Decisions (Gate 1)

Recorded by the Lead, 2026-10-04.

1. **AC admin client routes go live** (implementer escalation 1). `/api/v1/admin/clients/{id}` GET/PUT/DELETE and `/api/v1/admin/clients/{id}/rotate-secret` POST were declared in brace syntax under axum 0.7 (commit 99cfc6f5), so braces were literal and real IDs got 404; under 0.8 they serve. This is an intended exception to "served paths identical". Operator conditions:
   - Treat as newly live, sensitive endpoints. Security + auth-controller (added as `paired-auth-controller`) review the handlers: scope for delete/rotate (is `admin:services` right?), no secret material in responses except rotate's one-time return, unknown/malformed-ID behaviour.
   - Real-router tests per route: admin token → success; no token → 401; non-admin token → 403; real UUID reaches the handler.
   - Sweep GC, MC, MH (and every other axum Router) for pre-0.8 `{…}` paths; each gets the same treatment and is listed under an "Intended served-path changes" subsection in §Planning.
2. **tower-http 0.7.1 accepted with reqwest's 0.6.x duplicate** (implementer escalation 2). reqwest 0.12/0.13 require `tower-http ^0.6.8`; the 0.6 copy is reqwest-owned (today's lock already carries 0.5.2 + 0.6.7). axum, tonic, prost and the otel crates must still end single-version.
3. **Lead decision: buf.yaml `breaking.ignore` carve-out removed in this loop** (paired-protocol finding). Restore condition met (0 findings unsuppressed vs merge-base); re-enables Layer-6 enforcement for both shipped protos.

4. **Lead decision: AC router-wide `Cache-Control: no-store`** (security finding, subject to auth-controller owner confirmation). Same class as the newly live rotate-secret response; RFC 6749 §5.1 MUST on token responses; `if_not_present` keeps JWKS `max-age`.
5. **Lead decision: end-to-end export proof is a committed env-test** (collector accepted/refused span counters after GC→MC traffic; baseline series must exist). Per-service attribution needs a new collector connector (infrastructure-owned) → `docs/TODO.md` trace-continuity entry. GC rollout-restart / no-shutdown-warn check run by the Lead after Gate 2.

6. **Operator decision: AC admin audit added in this loop.** rotate-secret, update and delete wrote no `auth_events` row; a new forward migration extends `valid_event_type` and the handlers write audit rows. Schema evolution is a GSA → `database` added as reviewer (with security). Accompanying auth-controller rulings accepted: A1 PUT scopes limited to `ServiceType::default_scopes()`; A2 DELETE → deactivate (hard delete failed with 500 for every real client on the `auth_events.credential_id` FK); A3 `AcError::BadRequest` 400 for validation errors; `docs/API_CONTRACTS.md` unchanged (documents no AC admin endpoints); one-sentence scope note in ADR-0003 Component 2.

7. **Lead decisions after Gate-1 round 3** (2026-10-04 ~20:45):
   - GSA path drift `db/migrations/**` → `migrations/**` corrected in all five mirrors in this loop (database finding; a correction, not a list extension).
   - Inbound trace continuity is broken in the deployed config (tower-http request span at DEBUG is filtered by `info,<svc>=debug`); fixed in this loop: INFO-level request spans at every extraction site, warn-once on a disabled span with OTel installed, component test under the deployed filter. Security R-URI: custom MakeSpan records method + normalized endpoint only (meeting codes are guest-access capabilities).
   - Security R-COL: collector `verbosity: normal` accepted for the dev collector; collector.yaml comment corrected; collector pod log added to `test_secrets_not_in_logs`. End-to-end env-test asserts a fresh trace_id under both GC and MC in the collector log.
   - Event names (owner's final call): `service_scopes_updated`, `service_deactivated`, `service_secret_rotated`, `user_registered`; audit writes best-effort (security, ADR-0032); rotate on a deactivated client is rejected with no secret minted.
   - Migration rollback: single commit (Gate-2 hook attests one tree); the Rollback section requires a revert to keep (restore) both new migration files.
   - Migration shape (database, accepted by operations): two files — CHECK swap `NOT VALID`, then `VALIDATE CONSTRAINT` — each with `SET LOCAL lock_timeout = '5s'`.
   - AC audit-write-failure alert `ACAuditLogWriteFailures` added in this loop (observability ruling; auth-controller authors, observability reviews).
   - AC inbound trace extraction added (GC already injects; observability: partial-invariant rule).
   - Request spans: `/health`, `/ready`, `/metrics` stay DEBUG on GC and AC; all other request spans INFO (operations condition upheld over observability's no-exclusions position — probe/scrape noise at sample rate 1.0 would blur the accepted-spans env-test).
   - GSA path correction: the CANON list lives in `crates/dt-guard/src/gsa_sync.rs:40` (not the shell script). Editing its membership is guard policy content (database + security), not machinery, per CLAUDE.md §Guard-crate ownership.
   - Registration rate limiter (auth-controller, security approved): counts `user_registered` + new `user_registration_failed` per IP; fails closed on a DB error. Effective threshold moves from ≈3 to the configured 5 (state in commit message). The failed-attempt event needs a subject-less allowance in `event_has_subject` (database + security review).
   - Registration validation failures stop returning 401: bad email, short password and empty name → 400 BadRequest; existing email → 409 via a new `AcError::Conflict` (message text unchanged). sdk-core already maps 400/409 (`AuthError.ts:41-50`); `client` reviewer verifies, stale SDK comments updated. AC 401 counts drop accordingly. Rotate on a deactivated client is re-decided by auth-controller now that `Conflict` exists.
   - Failed-registration throttle: `user_registration_failed` added to `valid_event_type` and exempted in `event_has_subject` (same two-file NOT VALID/VALIDATE shape); rows carry non-null `ip_address` and only fixed `failure_reason` codes (no email/name/password); limiter check runs before validation; a 429 writes no row; every failure writes exactly one row. No new index (existing `(event_type, created_at)` serves the limiter).
   - Email-existence oracle on registration needs a design change (e.g. verify-by-email): `docs/TODO.md` entry owned by auth-controller; not in this loop.
   - Rotate on a deactivated client: re-decided to **409 Conflict** (auth-controller; the earlier 400 rested only on the absence of a Conflict variant).
   - Accepted trade-off (Lead): the registration limiter keys on the ConnectInfo peer IP. Behind SNAT/ingress all clients share one budget, so counting failed attempts lets one client exhaust registration for everyone behind that IP. Accepted because there is no production deployment and the throttle closes an unlimited email-enumeration oracle; the real fix (trusted-proxy / forwarded-IP config) is design + infra topology → `docs/TODO.md` entry owned by auth-controller alongside the oracle item.
   - **This section is authoritative.** Where a teammate message conflicts with it, this section wins.

## Implementation Notes and Evidence

### Single-version lock evidence
Cargo.lock was re-resolved minimally (no wholesale `generate-lockfile`):
- axum 0.8.9; tonic 0.14.6; prost 0.14.4; hyper 1.8.1; reqwest 0.12.24 (single each).
- opentelemetry / opentelemetry_sdk / opentelemetry-otlp / opentelemetry-proto: 0.33.0 each; tracing-opentelemetry 0.34.0. No `opentelemetry_sdk` 0.32 in the lock.
- `cargo tree -d -e normal,build` lists no axum/tonic/prost/opentelemetry* duplicate.
- **Operator-accepted transitive duplicate:** `tower-http` 0.7.1 (ours) + 0.6.7, which `cargo tree -i tower-http@0.6.7 -e normal` shows is pulled only by `reqwest 0.12.24` (reqwest 0.12/0.13 require `^0.6.8`).
- `async-std`: `cargo tree -i async-std -e all` matches no package, so RUSTSEC-2025-0052 was removed (`audit-suppressions.toml`, regenerated `.cargo/audit.toml`; `audit-suppressions-check.sh` → OK; TODO entries closed).

### TLS drift (S5)
`cargo tree -e features -i rustls` (rustls features) is identical before (worktree at `c7de08b0`) and after: `aws-lc-rs`, `aws_lc_rs`, `ring`, `std`, `tls12`, rustls 0.23.45. Cargo.lock has no openssl/openssl-sys/native-tls before or after. tonic features: channel, codegen, default, router, server, transport, with no `tls-*`. opentelemetry-otlp runs with `default-features = false` (no reqwest 0.13, no HTTP exporter).

### `set_remote_parent` is the only production reparent path
`grep -rn 'set_parent(' crates/*/src`: the only non-test call is `crates/common/src/observability/otel.rs` inside `set_remote_parent`. Every other hit is in a `#[cfg(test)]` module and asserts `.is_ok()`.

### Context-activation positive control (test reviewer #3)
With `.with_context_activation(true)` flipped locally in `configured_layer` (restored afterwards), these went red:
- GC: `otel_grpc_inbound_continuity::inbound_register_mc_call_with_traceparent_produces_matching_span`, `otel_http_grpc_bridge::inbound_http_traceparent_bridges_to_outbound_mc_grpc_call`, `otel_telemetry_proxy_trace_context::telemetry_proxy_forward_preserves_inbound_traceparent`, `otel_request_span_tests::{http_request_span_reparents_under_deployed_filter, probe_paths_export_no_span_and_api_paths_export_info_span, grpc_request_span_reparents_under_deployed_filter}`.
- MC: `otel_grpc_inbound_continuity::{inbound_call_does_not_leak_authorization_into_span, inbound_call_with_traceparent_produces_matching_span, request_span_reparents_under_deployed_filter}`, `otel_webtransport_integration::{test_wt_no_auth_leak_in_connection_span_attributes, test_wt_valid_traceparent_reparents_connection_span}`.
- MH: `otel_grpc_integration::test_inbound_register_meeting_reparents_handler_span_to_injected_trace`, `otel_webtransport_integration::test_wt_valid_traceparent_reparents_connection_span`.
- These stayed green, which is expected because they cover outbound/inject paths: GC `otel_ac_trace_continuity` (2), MC `otel_grpc_outbound_integration` (3), MH `gc_integration` (10).

### Request-span level positive control (§11 (c))
With `http_request_span`/`grpc_make_span` reverted to DEBUG locally (restored afterwards), these went red: GC `otel_request_span_tests::{http_request_span_reparents_under_deployed_filter, probe_paths_export_no_span_and_api_paths_export_info_span, grpc_request_span_reparents_under_deployed_filter}`, AC `otel_request_span_tests::{http_request_span_reparents_under_deployed_filter, probe_paths_export_no_span_and_api_paths_export_info_span}`, MC `otel_grpc_inbound_continuity::request_span_reparents_under_deployed_filter`. The deployed filter is read from the manifests (`RUST_LOG`). The GC meeting-code tests run with filter `debug`, so they stay green either way.

### Enum ↔ CHECK drift positive control (§10)
With `user_registered` removed from migration file 1 locally (restored), `auth_events_check_drift` went red (2 tests).

### Implementation findings beyond the plan (reviewers informed)
1. **tracing-opentelemetry 0.34: a child span starts its parent's OTel context** (`layer.rs` `parent_context` → `with_started_cx`, even with activation off). Once any child span exists, `set_parent` on the parent returns `AlreadyStarted`.
   - (a) **gRPC (production impact):** each auth layer's JWT `validate`/`get_key` INFO spans were created under the request span BEFORE `server_interceptor` ran. MH inbound continuity was broken by this. Fix in GC, MC, MH and their test rigs: `server_interceptor` now runs as `tonic::service::InterceptorLayer` directly inside `TraceLayer`/`SpanLayer` and before auth; the per-service `with_interceptor` wrapping is removed. HTTP was already safe (extraction is the innermost global layer).
   - (b) **WebTransport:** quinn's per-packet spans are created under the MC/MH connection span before the JoinRequest arrives. `configured_layer`'s exclusion list now also covers `quinn`, `quinn_proto`, `quinn_udp`, `wtransport` and `wtransport_proto`, so the OTel layer never sees them. In prod they're disabled by `RUST_LOG` anyway; this makes it robust to `quinn=trace` while debugging.
2. **GC `/ready` metric label.** GC's `normalize_endpoint` had no `/ready` arm (it fell into `/other`), so the probe-level rule couldn't match it. Added `"/ready" => "/ready"` (AC already had it). This adds one `gc_http_requests_total{endpoint}` value; the catalog cardinality note is updated. Pending @observability ack.
3. **tonic 0.14 `Status` is boxed.** Every `#[expect(clippy::result_large_err)]` became unfulfilled and was removed, as were the stale `#[allow(...)]` lines whose comments claimed Status was ~176 bytes (proto-gen lib.rs, MC media_coordination.rs, MH doc line).
4. **`OtelInitError::Sdk`** now wraps `opentelemetry_otlp::ExporterBuildError` (`#[from]`, source preserved).
5. **Layer-7 registration count** re-verified: env-tests ≤ 86 (the new 30_observability tests add 1 shared user). Whole-run worst case is 93 + failing e2e tests vs 100/min (the 00_cluster_health collector scan authenticates with client credentials, not a registration, after the Gate-2 fix).

### Assertion edits (justified)
- **GC/MC/MH/common:** raw `span.set_parent(..)` calls in tests now assert `.is_ok()` (stronger; the API now returns `Result`). `otel.rs` `init_rejects_malformed_endpoint` now asserts the URL-parse reason, so the new current-thread check can't satisfy it vacuously; the 3 init tests moved to `multi_thread`. The resource test asserts the exact 4-key set (stronger). No traceparent rejection test was edited.
- **AC (from the AC stream; each tied to a Lead/owner ruling):**
  - admin_handler unit tests: delete-then-get expects 200 `is_active:false` (R3); repeat delete expects 200 (idempotent); the update payload is a permitted subset (A1); `Database(..)` validation matches became `BadRequest(<fixed message>)`, and invalid service_type no longer echoes input.
  - registration_service tests: re-keyed to credential_id; not-found cases assert `NotFound`.
  - user_service tests:
    - the four validation tests moved `InvalidToken` → `BadRequest`/`Conflict` with exact messages, and the invalid-email loop uses a distinct IP per attempt;
    - `test_register_user_rate_limiting` is tightened to exact MAX (from the const);
    - `test_register_user_without_ip_address` is removed, because the no-IP bypass no longer exists (`ip_address: &str` is compiler-enforced).
  - user_auth_tests: four HTTP tests moved 401 → 400/409 with code + message + one failure row; `test_register_rate_limit` tightened from `<= 6` to exact MAX; R5 rename `test_admin_endpoint_rejects_user_token` (asserted 200) → `..._accepts_scope_bearing_token_without_service_type`, and the harness helper is renamed `create_scope_token_without_service_type`.
  - service_credentials tests: `update_metadata` tests moved to `update_scopes`; `delete` tests removed with `delete()`.
  - audit_log_failures_integration: the user_registered seam switched from DROP to insert-block (the limiter now reads `auth_events` first and fails closed); label `scopes_updated` → `service_scopes_updated`.
  - credential_ops_metrics_integration: rotate not-found records `rotate_secret/error` like update/delete; the delete test seeds via the create handler.
  - rate_limit_metrics_integration: the fixture stamps `user_registered` rows.
- **Client:** `cohort.test.ts` expectation `{registrations:4, firstTestSignIns:4, total:8}` → `{registrations:4, total:4}`; the prod-default case now asserts that a limit below N+1 fails (the 4-person cohort fits under 5 now that sign-ins don't count).

### Rollback
Single commit. Safe-revert unit: `git revert <sha>` **plus**, in the same revert commit, `git checkout <sha> -- migrations/20261004000001_auth_events_admin_event_types.sql migrations/20261004000002_auth_events_validate_event_types.sql` (the db-migrate Job's plain `sqlx migrate run` fails on an applied-but-missing version). Then rebuild and redeploy. The only manifest change is the Prometheus rules ConfigMap (+`ac-alerts.yaml`); a revert removes that rule on the next deploy. No service ConfigMap/env change. During the rollout and again after a revert, the per-IP registration limiter undercounts for up to one `registration_rate_limit_window_minutes` (old binary counts `user_login`, new counts `user_registered` + `user_registration_failed`; each ignores the other's rows), as noted in the AC runbook. The revert also: returns the AC admin `{id}` endpoints to 404, restores the buf `breaking.ignore` carve-out and the RUSTSEC-2025-0052 suppression, removes no-store/BadRequest/Conflict/scope restriction/honest audit types/ACAuditLogWriteFailures, and returns inbound continuity to broken. The schema stays widened, which is safe for the old binary.

### Kind verification (Lead)
Layer 7 runs `30_observability.rs` (collector accepted/refused counters; GC+MC+AC shared trace_id) and `00_cluster_health.rs` (collector-log secrets scan). The GC rollout-restart flush check is the Lead's, by hand, after Gate 2.

### Gate 3 review fixes
- **gRPC layer order is now enforced by code.** `common::observability::otel_grpc::inbound_layers(span_layer, auth_layer)` → `InboundLayers`, with per-service `grpc::server_layers(auth)` in GC, MC and MH, used by both `main.rs` and every gRPC test rig. The rigs run the REAL auth layer, and GC/MC deployed-filter tests send valid service tokens. Positive control:
  - Swapping auth outside extraction does not compile (E0271: each auth layer accepts only tonic `Body`; 6 sites).
  - The misorder that compiles (extraction outside the span layer) turns red: GC `otel_grpc_inbound_continuity::inbound_register_mc_call_with_traceparent_produces_matching_span`, GC `otel_request_span_tests::grpc_request_span_reparents_under_deployed_filter`, MC `otel_grpc_inbound_continuity::{inbound_call_does_not_leak_authorization_into_span, inbound_call_with_traceparent_produces_matching_span, request_span_reparents_under_deployed_filter}`, MH `otel_grpc_integration::test_inbound_register_meeting_reparents_handler_span_to_injected_trace`.
- **Deployed-filter capture hoisted** to `common::observability::testing::otel` (test-utils; adds `opentelemetry_sdk/testing`). GC/AC/MC/MH test copies are removed, and the AC otel tests build their router via `make_app_state`.
- **init_otel rejects userinfo endpoints** (fixed reason, no echo; semantic-guard). The filter test uses the real `http_request_span` / `grpc_make_span` builders. The hung-collector test also asserts a lower bound.
- **AC:** conditional `deactivate`/`rotate_secret` (`AND is_active`, Option), atomic `old_scopes`, limiter count moved to `repositories/auth_events.rs` with `record_db_query`, `event_has_subject` both-directions pin, `AuthEventType` and `ServiceType` generated from ONE `Variant => "str"` list by a `string_enum!` macro (enum + `ALL` + `as_str`; `ServiceType::from_str` iterates `ALL`), so `ALL` cannot omit a variant (the earlier index-match tests were vacuous: a forced match arm does not force list membership; replaced by a distinctness test plus a serde/round-trip test), `get_service` single not-found path, `log_admin_event` for register_service, fail-closed limiter test, deactivated-client list/widen/narrow coverage, delta-0 on 401/403/400, audit-failure tests assert success and persisted state, logins test asserts statuses + row count.
  - Assertion edits: repo not-found tests now expect `None`, not a `Database` error, because the conditional UPDATE reports no-row as `None`; `test_update_scopes` asserts `old_scopes`. The other edits strengthen existing assertions as listed.
- **env-tests:** `--tail=-1` on every selector `kubectl logs`; shared `fixtures/collector.rs` (`OTEL_COLLECTOR_SELECTOR`, `collector_log`, `assert_collector_log_has_no_secrets`); JWT-shape regex reused from `gc_client.rs` with a real-token positive control (replaces the vacuous `eyJhbGci`); the scan runs after authenticated traffic with the fresh trace_id as in-window control (00 and 30).
- **DRY F7-F9 (re-review):** all trace-id fixture copies (MH/GC tests, GC middleware unit tests, common in-module tests) now import `common::observability::testing::otel`; the reject tests' deliberately-malformed traceparents stay byte-identical. A unit test pins the hex fixtures to the numeric ids. `otel_grpc::GrpcTraceLayer` type alias is used by `grpc_trace_layer` and the GC/MC `server_layers`. TODO.md:870 is closed.
- **init_otel userinfo check** runs on the raw endpoint before the URL parse, so a parse error cannot echo credentials (semantic-guard residual).
- **Docs/comments:** proto-gen build.rs `tonic-prost-build` and lib.rs "five"; AC runbook limiter rollout note; TODO S16 6-of-14 and the dead-glob entries updated; client README/cohort/fixtures comments.

### Gate 2 (attempt 1) fixes
Layer 7 failed in two places, both in the new leak-scan code; the GC→MC→AC trace-continuity assertion passed.
- **`00_cluster_health` scan sent a user token to GC `/api/v1/me`, which validates SERVICE tokens (401).** It now authenticates the dev client with client credentials (`DEV_TEST_CLIENT_ID`/`DEV_TEST_CLIENT_SECRET`, new consts in `fixtures/auth_client.rs`). That AC-minted service token goes to `/api/v1/me` with a fresh traceparent and is the JWT-regex positive control. This also drives AC's secret-verifying path into the scanned window.
- **The `client_secret` detector was a false positive on span names.** AC's `#[instrument(skip_all)]` spans `hash_client_secret` / `verify_client_secret` appear in the log by name only, with no value. Final spec (Lead ruling after security + observability; spans not renamed), one shared helper `fixtures::collector::assert_log_has_no_secret_fields` used by BOTH the AC service-log scan and the collector scan:
  - secret-material keys, prefix/suffix-permissive with a separator and a non-empty value: `(?i)[A-Za-z0-9_]*(?:client_secret|password|secret|api_key)[A-Za-z0-9_]*["']?\s*[:=]\s*["']?[^\s"',}]+`, so `client_secret_hash=…`, `password_hash=…`, `join_token_secret=…` fail;
  - `token` / `authorization` as exact whole keys: `(?i)(?:^|[^A-Za-z0-9_])(?:token|authorization)["']?\s*[:=]\s*["']?[^\s"',}]+`, so `token_type=Bearer` passes;
  - literal check: the run's secret values (the dev client secret) are absent from both logs; an empty list fails;
  - the JWT / `Bearer ey` / trace-id controls are unchanged.
  - 00 now drives the traffic FIRST, so the AC scan (`--tail=-1 --since=5m`) covers it too.
  - Lib unit tests both ways: real span-name lines, `token_type`, and valueless keys pass; observability's and security's positive table fails, including the real dev secret.
  - Validated against the live Kind logs (AC + 14k collector lines): zero hits from either pattern.
- **Test re-check follow-ups:**
  - `DEV_TEST_CLIENT_ID`/`DEV_TEST_CLIENT_SECRET` replace every dev-client literal across env-tests (10/20/21/23/24/25/31); the only literal left is sample log text in `collector.rs` unit tests.
  - 30's literal-secret check uses the OTLP user's registration password, a secret that file actually sends to AC inside the window, instead of the never-sent dev client secret.
  - **AC-log scan in-window positive control (security + test).** The live AC log showed AC emits NO line for a successful client-credentials grant (only startup lines), so the AC scan was scanning nothing in its window. AC now logs `SERVICE_TOKEN_ISSUED_MESSAGE` ("Service token issued", INFO, no identifiers; `token_service.rs`) once per issuance, inside the request span whose `endpoint` is `/api/v1/auth/service/token`. Before scanning, 00 asserts the AC window contains a line with both, with a distinct failure message. env-tests doesn't depend on ac-service, so the string is restated there; a rename fails loudly.
  - Additional helper tests: an empty secret list fails `assert_log_has_no_secret_fields`; a secret under an unknown key (`x=<secret>`) fails via the literal layer alone; a JWT-shaped value fails the collector scan.
  - Kind-free lib tests pin the scan helper's controls: missing trace_id, a known secret value present, and an empty secret list each panic; a clean log with the trace_id and an AC span-name line passes.

## Gate 1

| Reviewer | Plan Status |
|----------|-------------|
| Paired Protocol | confirmed |
| Paired Auth Controller | confirmed |
| Database | confirmed |
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |
| Client | confirmed |

---

## Gate 3 Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Paired Protocol | RESOLVED-FIXED | 2 | 2 | 0 | wire-identical regen; buf unsuppressed |
| Paired Auth Controller | RESOLVED-FIXED | 2 | 2 | 0 | TOCTOU + string_enum! drift guard |
| Database | RESOLVED-FIXED | 6 | 6 | 0 | |
| Client | RESOLVED-FIXED | 3 | 3 | 0 | |
| Security | RESOLVED-FIXED | 3 | 3 | 0 | |
| Test | RESOLVED-FIXED | 10 | 10 | 0 | |
| Observability | RESOLVED-FIXED | 4 | 4 | 0 | |
| Code Quality | RESOLVED-FIXED | 6 | 6 | 0 | |
| DRY | RESOLVED-FIXED | 9 | 9 | 0 | annotated existing TODO §Cross-Service Duplication entry (string_enum! as hoist candidate) — not a deferral of a diff finding |
| Operations | RESOLVED-FIXED | 3 | 3 | 0 | |
| Semantic Guard | RESOLVED-FIXED | 1 | 1 | 0 | endpoint userinfo rejected before parse |

---

## Gate 2 Log

### Attempt 1 (L1-6 attempt 1; L7 attempt 1 of 2)
- Layers 1-6: OK (L4/L6 aggregate N/A from proto intentional-gap placeholders; cargo test, nx test, audits, buf breaking all passed).
- Layer 7: FAIL. All other env-tests passed; the new GC→MC→AC trace-continuity assertion **passed live**. Browser e2e not run (skipped after env-test failure, per layer7.sh).
  - `00_cluster_health.rs:268` — leak-scan setup sent a user token to GC `/api/v1/me`, which accepts client-credentials tokens → 401. Test defect.
  - `fixtures/collector.rs:80` — `!log.contains("client_secret")` matched span names `hash_client_secret` / `verify_client_secret` (`#[instrument(skip_all)]`); no values present (Lead checked all 190 hits in the live collector log). Detector false positive; to be tightened to value-bearing shapes plus a literal-secret check, not weakened.

### Attempt 1 fixes — reviewer confirmations
- Leak-scan detector rebuilt to the Lead's final spec (shared helper for AC + collector scans; secret-class keys prefix/suffix-permissive with a value; token/authorization exact; literal-secret check, empty list fails). security, observability, test: confirmed.
- `00_cluster_health` uses the client-credentials token GC `/api/v1/me` accepts; traffic runs before both scans.
- AC previously logged nothing on a successful service-token grant, so the AC scan read only startup lines. Added INFO `SERVICE_TOKEN_ISSUED_MESSAGE` (no identifiers, `token_service.rs:182`) as the AC in-window positive control. auth-controller, observability, security: approved (≈ single-digit lines/hour/cluster).
- Test's follow-ups: dev-client constants used everywhere, 30's literal check uses a secret it sends, fail-closed unit tests for the scan helper (9/9).

### Attempt 2 (L7 attempt 2 of 2) — PASS
- `layer-all.sh`: L1 OK, L2 OK (no FMT_APPLIED), L3 OK, L4 N/A (cargo-test + nx-test passed; proto intentional-gap placeholder), L5 OK, L6 N/A (cargo-audit, pnpm-audit, buf-breaking passed; no proto suppression), **L7 OK** (`env-tests-passed`, `browser-e2e-passed`). TOTAL_RESULT=N/A (self-justifying wrapper REASONs only), exit 0.
- New live checks green: GC→MC→AC shared trace_id in the collector log; collector + AC leak scans with in-window positive controls.

### Manual GC rollout-restart check (Lead, after Gate 2)
- `kubectl rollout restart deployment/gc-service`: both old pods drained 30s and logged "Global Controller shutdown complete"; **no** "OpenTelemetry span flush at shutdown failed" warning; no OTel WARN/ERROR in either old pod's log.
- Collector `otelcol_receiver_accepted_spans` 35472 → 36022 across the restart; `otelcol_receiver_refused_spans` 0. New pods 2/2 Running.

## Accepted Deferrals

- `docs/TODO.md` §Observability Debt — per-service collector span counts need a `count` connector
- `docs/observability/slos.md` open items — AC SLO numerator still counts client 4xx (joined to GC item)
