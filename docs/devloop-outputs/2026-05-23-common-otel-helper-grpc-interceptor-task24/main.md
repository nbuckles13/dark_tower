# Devloop Output: Common OTel SDK Helper + gRPC Interceptor (Task #24, R-54 / R-56)

**Date**: 2026-05-23
**Task**: Common OTel init helper (`otel.rs`) with RAII `OtelGuard` + Tonic interceptor (`otel_grpc.rs`) for W3C trace-context inject/extract on gRPC boundaries. Adds `tracing-opentelemetry` to workspace deps.
**Specialist**: observability
**Mode**: Agent Teams (v2), Full mode
**Branch**: `feature/browser-client-join-task24`
**Duration**: in progress

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `e9c6dce806ecf286f8503883701f7a3ffb574587` |
| Branch | `feature/browser-client-join-task24` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-otel-task24` |
| Implementing Specialist | `observability` |
| Iteration | `1` |
| Security | `security@devloop-otel-task24` |
| Test | `test@devloop-otel-task24` |
| Observability | `(implementer)` |
| Code Quality | `code-reviewer@devloop-otel-task24` |
| DRY | `dry-reviewer@devloop-otel-task24` |
| Operations | `operations@devloop-otel-task24` |
| Semantic Guard | `semantic-guard@devloop-otel-task24` |

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |
| Semantic Guard | confirmed |

---

## Task Overview

### Objective
Ship `crates/common/src/observability/otel.rs` and `crates/common/src/observability/otel_grpc.rs` so AC/GC/MC/MH can each wire OTel SDK from a single common entry point (tasks #25, #26, #6, #27 depend on this). R-54 fail-hard at init, fail-soft at runtime per Clarification Q13.

### Scope
- **Service(s)**: `crates/common/` only (consumers wire next).
- **Schema**: No
- **Cross-cutting**: Yes — affects all four services as consumers (next devloops).

### Debate Decision
NOT NEEDED — R-54 + R-56 design already debated and resolved in user-story Clarification Q7 (full backend OTel rollout) and Q13 (fail-hard at init).

---

## Cross-Boundary Classification

<!-- crates/common/** outside the Guarded subset is not owned by a single specialist.
     Edits require DRY reviewer + code-reviewer approval (ADR-0024 §6.4 last paragraph).
     observability/ is not in the Guarded enumerated list.
     Cargo.toml workspace-dep addition is a non-trivial common edit but not Guarded. -->

| File | Classification | Owner | Notes |
|------|----------------|-------|-------|
| `crates/common/src/observability/otel.rs` (new) | Mine | observability | New module; `init_otel(...) -> Result<OtelInit, OtelInitError>` + `OtelGuard` (RAII). Not in GSA enumerated list. |
| `crates/common/src/observability/otel_grpc.rs` (new) | Mine | observability | New module; `client_interceptor()` + `server_interceptor()` + `BoundedTraceContextPropagator`. Not in GSA enumerated list. |
| `crates/common/src/observability/mod.rs` | Mine | observability | Add `pub mod otel;` and `pub mod otel_grpc;`. |
| `crates/common/Cargo.toml` | Mine | observability | Add `tracing-opentelemetry.workspace = true` + `opentelemetry.workspace` + `opentelemetry-otlp.workspace` + `tracing-subscriber.workspace` + `tonic.workspace`. |
| `Cargo.toml` (workspace) | Mine | observability | Add `tracing-opentelemetry = "0.25"` + `opentelemetry_sdk = { version = "0.24", features = ["rt-tokio", "trace"] }` to `[workspace.dependencies]`. Extend `opentelemetry-otlp` features to `["grpc-tonic", "trace"]`. Existing `opentelemetry`/`opentelemetry-otlp`/`tonic`/`tracing-subscriber` already live in `[workspace.dependencies]` — no promotion needed (my pre-plan assumption was wrong; verified at Cargo.toml:67-78). |

All rows are "Mine" — observability owns the OTel helper API surface in `crates/common/src/observability/`. No Guarded paths touched (none of these files appear in the ADR-0024 §6.4 enumerated GSA list). Per ADR-0024 §6.4 last paragraph, `crates/common/**` outside GSA requires DRY reviewer + code-reviewer approval — both will confirm on this devloop. No other specialist's domain code modified.

---

## Planning

### Files

See Cross-Boundary Classification table above.

### Design summary

**`otel.rs`** — `init_otel(service_name, service_version, environment, OtelConfig) -> Result<OtelInit, OtelInitError>`:
- `OtelConfig { endpoint: String, sample_rate: f64 }` — minimum surface; no `Default` impl (per security review @ item 6) so callers must supply explicit values.
- `OtelInit { guard: OtelGuard, layer: OpenTelemetryLayer<Registry, Tracer> }` — named-field return so it's discoverable and extensible. Services compose `init.layer` into their existing `tracing_subscriber::registry().with(...).init()` stack.
- `OtelGuard` — RAII; `Drop` calls `opentelemetry::global::shutdown_tracer_provider()` (returns `()` in 0.24 — no error path). Private fields, only constructable by `init_otel` (verified during implementation).
- `OtelInitError` — `thiserror`, `#[from]` for `opentelemetry::trace::TraceError`; variants `ConfigInvalid { reason }`, `CollectorUnreachable { endpoint, #[source] source }`, `Sdk(TraceError)`. Prefer concrete `#[source]` type if pinnable; document if `Box<dyn Error>` is needed.
- Builds OTLP-gRPC exporter pointed at `OtelConfig::endpoint`. **Eager probe** via shared `tonic::transport::Endpoint::from_shared(endpoint).connect_timeout(5s).connect().await` — the resulting `Channel` is handed to `opentelemetry_otlp::new_exporter().tonic().with_channel(channel)` so the production code path IS what the test exercises (no parallel probe; per test review @ item #1a). Documented fallback: emit one span + `force_flush` if `.with_channel` is unavailable in 0.17. Escalate to @team-lead before adding a parallel probe.
- Registers **`BoundedTraceContextPropagator`** (wrapper from the start, per security review @ item b) globally via `set_text_map_propagator`. Wrapper rejects on inbound: traceparent > 55 chars; traceparent version `ff` or non-`00`; non-hex chars in trace-id/parent-id; all-zero trace-id (W3C §3.2.2.3); all-zero parent-id (W3C §3.2.2.4); tracestate > 512 bytes (W3C §3.3.1.1); tracestate > 32 list members (W3C §3.3.1.2). Rejection = no parent attached (silent at propagator layer; gRPC request itself unaffected).
- Sampler: `Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(sample_rate)))`.
- Resource attrs: canonical OTel semantic-convention keys per ADR-0011 — `service.name`, `service.version`, `service.namespace=darktower`, `deployment.environment`. No `host.name`/`process.pid`/`k8s.pod.name` (PII-correlation risk per security review @ item 3).
- Returns `OpenTelemetryLayer<Registry, Tracer>` for service-side composition with existing `tracing_subscriber::registry()` stacks.

**`otel_grpc.rs`** — Tonic `Interceptor`:
- `client_interceptor() -> impl Interceptor + Clone + Send + Sync + 'static` — injects active OTel context into outbound metadata via global propagator. Only writes `traceparent`/`tracestate`. Contract anchor: `const PROPAGATED_HEADERS: &[&str] = &["traceparent", "tracestate"];`.
- `server_interceptor() -> impl Interceptor + Clone + Send + Sync + 'static` — extracts inbound context via global propagator; attaches to current OTel context.
- `MetadataInjector::set` uses `if let Ok(key) = MetadataKey::from_bytes(...)` form (no `unwrap`/`expect`/`panic`). Module-level `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]` locks it in per security review @ item 2.

**Module docstrings** call out: (a) fail-hard-at-init / fail-soft-at-runtime division per Clarification Q13, (b) the runtime `dt_otel_export_failures_total` counter + `OTelExportFailureRate` alert are task #28 not this devloop, (c) ADR-0011 resource-attribute convention, (d) bounded-propagator wrapper rationale.

### Tests (12 total, all `#[cfg(test)] mod tests`)

`otel.rs`:
1. `init_returns_err_on_unreachable_collector` — `127.0.0.1:1` → `Err(OtelInitError::CollectorUnreachable)`. Production code path exercised.
2. `propagator_round_trip_recovers_trace_id_and_span_id`.
3. `sampler_parent_sampled_child_sampled` + `sampler_parent_unsampled_child_dropped`.
4. `sampler_root_obeys_ratio` (0.0 and 1.0 branches with hard-coded trace_ids).
5. `build_resource_has_expected_attrs` + `init_otel_installs_resource_into_provider` — second test uses `init_otel_with_exporter` test seam + `InMemorySpanExporter` to assert the resource is wired into the provider, not just the builder (per test review @ item #2a).

`otel_grpc.rs`:
6. `client_interceptor_injects_traceparent_into_metadata`.
7. `server_interceptor_extracts_and_attaches_parent_context`.
8. **Security**: `interceptor_never_copies_authorization_to_span` + `server_interceptor_does_not_mutate_authorization_metadata` + `client_interceptor_does_not_propagate_authorization_metadata` + `client_interceptor_injects_only_traceparent_and_tracestate_keys`.
9. `bounded_propagator_rejects_overlong_traceparent` (W3C §3.2 length).
10. `bounded_propagator_rejects_overlength_tracestate` (W3C §3.3.1.1).
11. `bounded_propagator_rejects_too_many_tracestate_list_members` (W3C §3.3.1.2).
12. `bounded_propagator_rejects_malformed_traceparent_format` — W3C §3.2.2 covering version `ff`, non-hex chars, all-zero trace-id, all-zero parent-id (per test review @ item #c).

All `otel.rs` tests use `#[tokio::test]`; `otel_grpc.rs` tests `#[tokio::test]` for parity. Test-only `TestSpanCapture` `tracing_subscriber::Layer` for span-attribute inspection (mirrors `observability/testing.rs` Mutex-wrapped pattern).

### Failure model

Per Clarification Q13: **fail-hard at init, fail-soft at runtime.**
- `init_otel` returns `Err` if endpoint probe fails (`CollectorUnreachable`). Each service `main.rs` propagates → non-zero exit → K8s `Unready` → existing pod-health alerting.
- Runtime export failures: silent drop (default exporter behavior). Runtime counter + alert are task #28, NOT this devloop.

### Operational notes

- `OtelGuard::Drop` flushes pending spans on graceful shutdown (SIGTERM cleanup path).
- No new readiness endpoint — service's existing `/ready` handles it (init failure aborts before serve loop binds).
- Runbook entries about "collector upgrades in separate change window" live with per-service wiring tasks (#25/#26/#6/#27) or task #29, NOT this devloop.

---

## Pre-Work

None. Branch is clean at `e9c6dce`.

---

## Implementation Summary

### `crates/common/src/observability/otel.rs` (575 lines, new)
- `OtelConfig { endpoint, sample_rate }` (no `Default`; explicit caller-supply).
- `OtelInitError { ConfigInvalid, CollectorUnreachable { endpoint, #[source] tonic::transport::Error }, Sdk(#[from] TraceError) }`.
- `OtelGuard` — RAII; `Drop` calls `global::shutdown_tracer_provider()` wrapped in `catch_unwind` for panic safety.
- `OtelInit { guard: OtelGuard, layer: OpenTelemetryLayer<Registry, Tracer> }` — layer-return, services compose into existing `tracing_subscriber::registry()` stacks.
- `init_otel(...)` async — eager `Endpoint::from_shared(endpoint).connect_timeout(1s).connect().await` probe; resulting `Channel` handed to `opentelemetry_otlp::new_exporter().tonic().with_channel(channel)` so the production code path IS the test path (no parallel probe shape).
- Resource attrs (canonical OTel semantic conventions per ADR-0011): `service.name`, `service.version`, `service.namespace=darktower`, `deployment.environment`.
- Sampler: `Sampler::ParentBased(TraceIdRatioBased(sample_rate))`.

### `crates/common/src/observability/otel_grpc.rs` (764 lines, new)
- `BoundedTraceContextPropagator` — wraps SDK `TraceContextPropagator`; rejects (no parent attached): traceparent > 55 chars, version `ff`, non-hex trace-id/parent-id, all-zero trace-id, all-zero parent-id, tracestate > 512 bytes, tracestate > 32 list members. Per W3C §3.2 / §3.3.1.
- `client_interceptor()` / `server_interceptor()` — Tonic interceptors; inject/extract via global propagator. Only `traceparent`/`tracestate` propagate; `authorization` metadata is NEVER copied to span attrs (asserted by 4 distinct tests).
- `PROPAGATED_HEADERS: &[&str] = &["traceparent", "tracestate"]` const anchors the contract.
- Module-level `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]` on both files.

### `crates/common/src/observability/mod.rs`
- `pub mod otel;` + `pub mod otel_grpc;` (unconditional, production callers need them).
- Updated module docstring describing R-54 / R-56 boundaries.

### Workspace + crate `Cargo.toml`
- `[workspace.dependencies]` adds `opentelemetry_sdk = { version = "0.24", features = ["rt-tokio", "trace"] }` and `tracing-opentelemetry = "0.25"`; extends `opentelemetry-otlp` features to `["grpc-tonic", "trace"]`.
- `crates/common/Cargo.toml` consumes via `.workspace = true` plus `tonic`, `tracing-subscriber`.

### `docs/TODO.md`
- One entry under §Cross-Service Duplication (DRY) — `tracing_subscriber::registry()` extraction opportunity (deferred — divergent per-service EnvFilter defaults need a wider design pass).
- One entry under §Dependency Vulnerabilities — `async-std` unmaintained warning via `opentelemetry_sdk 0.24.1` transitive (P3, dev/build-time edge only, fix requires workspace-wide OTel-stack version bump).

---

## Files Modified

```
 Cargo.toml                                         |   4 +-
 crates/common/Cargo.toml                           |  14 +
 crates/common/src/observability/mod.rs             |  22 ++-
 crates/common/src/observability/otel.rs            | 575 +++ (new)
 crates/common/src/observability/otel_grpc.rs       | 764 +++ (new)
 docs/TODO.md                                       |   4 +
```

---

## Devloop Verification Steps

### Layer 1: cargo check — **PASS**
### Layer 2: cargo fmt — **PASS**
### Layer 3: Simple Guards — **PASS** (cross-boundary-classification-clean-1-files)
### Layer 4: Unit Tests — **PASS** (cargo test -p common: 53 observability:: tests pass, including all 22 new OTel/OTel-gRPC tests + the pre-existing `testing.rs` suite)
### Layer 5: Lint — **PASS** (`cargo clippy --workspace --lib --bins -- -D warnings` clean after code-reviewer's 8-finding sweep)
### Layer 6: Audit — **FAIL-pre-existing** (no regression introduced)
- `cargo audit`: `RUSTSEC-2023-0071` (rsa 0.9.10 Marvin) is pre-existing branch state, tracked in `docs/TODO.md:251`. Baseline-state `git stash` confirmed identical error.
- `cargo audit`: `RUSTSEC-2025-0052` (async-std unmaintained) is a WARNING (not error) introduced by `opentelemetry_sdk 0.24.1` transitive; doesn't grow the vulnerability count. Tracked in `docs/TODO.md` §Dependency Vulnerabilities as P3 — dev/build-time edge only (`cargo tree -p common -i async-std --edges normal` returns empty).
- `buf breaking`: `internal.proto`/`signaling.proto` deletion is from preceding tasks #30/#31 (intentional wire-breaking proto layout cleanup in this story), not this devloop.
### Layer 7: Env-tests — **N/A** (no service code touched; this devloop is the common helper landing only)

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | — | — | All 3 round-2 checklist items + all 6 plan-review items verified in code. |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | Test-driver `TestSpanCapture` needed `on_record` impl + client-side auth-leak symmetry; both fixed in-PR. 23 of 23 tests pass. |
| Code Quality | RESOLVED-FIXED | 8 | 8 | 0 | 8 clippy/fmt findings under `-D warnings`; all fixed plus bonus correctness improvements (uninlined_format_args, needless_pass_by_value, await_holding_lock). Final `clippy -D warnings` + `fmt --check` + 53 tests all clean. |
| DRY | RESOLVED-DEFERRED | 0 | — | 1 (TODO extraction) | Per ADR-0019 hybrid model: zero true duplication found; one extraction opportunity (`tracing_subscriber::registry()` init blocks across all four service main.rs files) logged to `docs/TODO.md` §Cross-Service Duplication. |
| Operations | RESOLVED-DEFERRED | 2 | 1 | 1 | Fix: `OtelGuard::Drop` panic-safety wrap restored. Defer: `async-std` unmaintained warning (RUSTSEC-2025-0052) tracked in `docs/TODO.md` §Dependency Vulnerabilities as P3. |
| Semantic Guard | CLEAR | 0 | — | — | All 4 checks (credential-leak / actor-blocking / error-context-preservation / metrics-path-completeness) pass on the diff. Native verdict SAFE. |

### Security Decisions (per ADR-0027 / Clarification Q13)

| Decision | Choice | Rationale | Reference |
|----------|--------|-----------|-----------|
| OTel init failure mode | Fail-hard (Err propagation) | Configuration bugs surface immediately at startup; collector-unreachable triggers K8s pod-Unready → existing pod-health alerting | Clarification Q13 |
| OTel runtime failure mode | Fail-soft (silent drop) | Covered by future `dt_otel_export_failures_total` counter + `OTelExportFailureRate` alert (task #28 scope) | Clarification Q13 |
| `traceparent` rejection on length/format mismatch | REJECT (no parent attached); request still dispatches | Prevent log/span exfil amplification per W3C §3.2 | User-story line 411 §1 |
| `tracestate` over-512-byte handling | REJECT (not truncate); request still dispatches | Truncation would produce malformed `k=v` semicolon-list per W3C §3.3.1.1 | User-story line 411 §1 |
| Authorization metadata in span attrs | NEVER propagated (positive test asserts subset = `{traceparent, tracestate}` only) | Bearer tokens / cookies must not leak into telemetry pipeline | User-story line 411 §2 |
| Resource attributes | Canonical OTel semantic conventions only; `host.name`/`process.pid`/`k8s.pod.name` REJECTED with defense-in-depth test | PII-correlation risk on long-retention spans | Security plan-review item 3 |

---

## Accepted Deferrals

- `docs/TODO.md` §Cross-Service Duplication (DRY) — `tracing_subscriber::registry()` init-block extraction across four services
- `docs/TODO.md` §Dependency Vulnerabilities — `async-std` unmaintained (RUSTSEC-2025-0052) via `opentelemetry_sdk 0.24.1`
- `docs/TODO.md` §Dependency Vulnerabilities — `rsa 0.9.10` Marvin (RUSTSEC-2023-0071) via `sqlx-mysql` (pre-existing, not introduced by this devloop)

---

## Rollback Procedure

1. Verify start commit: `e9c6dce806ecf286f8503883701f7a3ffb574587`
2. Review changes: `git diff e9c6dce806ecf286f8503883701f7a3ffb574587..HEAD`
3. Soft reset: `git reset --soft e9c6dce806ecf286f8503883701f7a3ffb574587`
4. Hard reset: `git reset --hard e9c6dce806ecf286f8503883701f7a3ffb574587`
5. No schema or infra changes — `git reset` is sufficient.

---

## Issues Encountered & Resolutions

TBD.

---

## Lessons Learned

TBD.

---

## Appendix: Verification Commands

```bash
./scripts/layer-all.sh
```
