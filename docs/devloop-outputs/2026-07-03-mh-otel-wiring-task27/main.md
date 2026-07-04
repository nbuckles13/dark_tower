# Devloop Output: MH OTel Wiring + MhClientMessage Trace-Field Reads (Task #27)

**Date**: 2026-07-03
**Task**: MH OTel init wiring (R-55 partial), gRPC trace-propagation interceptor (R-56 partial), and MhClientMessage envelope trace-field reads (R-58 MH side)
**Specialist**: media-handler
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-27`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `1fe546be10747860e74715d83f81d225336ad83b` |
| Branch | `feature/browser-client-join-task-27` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (media-handler) |
| Implementing Specialist | `media-handler` |
| Iteration | `1` |
| Security | `security` (CLEAR) |
| Test | `test` (CLEAR) |
| Observability | `observability` (CLEAR) |
| Code Quality | `code-reviewer` (CLEAR) |
| DRY | `dry-reviewer` (CLEAR) |
| Operations | `operations` (CLEAR) |
| Semantic Guard | `semantic-guard` (CLEAR) |

---

## Task Overview

### Objective
Complete the MH side of the backend OpenTelemetry rollout (story `2026-05-02-browser-client-join`, task #27):

1. **R-55 partial (MH main.rs + Config)**: MH `main.rs` calls `common::observability::otel::init_otel(...)` with `service.name=media-handler` before the runtime boots, composing the returned OTel layer onto the `tracing_subscriber` registry and holding the `OtelGuard` in `main`. MH `Config` adds `otel_enabled: bool` (default `false`), `otel_endpoint: String`, `otel_sample_rate: f64` (default `1.0`) + an `otel_config() -> Option<OtelConfig>` mapper. Env vars (`OTEL_ENABLED` / `OTLP_ENDPOINT` / `OTEL_SAMPLE_RATE`) surfaced in `infra/services/mh-service/configmap.yaml`.
2. **R-56 partial (MH gRPC interceptors)**: `client_interceptor()` on MH's **outbound** gRPC clients — `GcClient` (`MediaHandlerRegistryServiceClient`, MH→GC) in `grpc/gc_client.rs` and `McClient` (`MediaCoordinationServiceClient`, MH→MC) in `grpc/mc_client.rs`. `server_interceptor()` on MH's **inbound** MC-facing gRPC server (`MediaHandlerServiceServer`) built in `main.rs`.
3. **R-58 MH side (envelope reads)**: at `webtransport/connection.rs:handle_connection()`, after decoding the `MhClientMessage` envelope, extract `trace_parent`/`trace_state` via the global W3C propagator and attach the extracted context as parent of the existing `mh.webtransport.connection` span so existing spans become child spans.

### Scope
- **Service(s)**: media-handler (mh-service) + its own K8s configmap
- **Schema**: No
- **Cross-cutting**: Consumes `crates/common/observability/{otel,otel_grpc}` (task #24, Completed); reads proto trace fields added by task #2 (Completed). No proto or common edits.

### Debate Decision
NOT NEEDED — implementation of an already-decided design (R-54/R-55/R-56/R-58 per story + ADR-0011). AC (task #25) and the common helper (task #24) are the landed exemplars.

### Dependencies (all Completed)
- Task #2 — proto `MhClientMessage.trace_parent`/`trace_state` fields (tags 20/21). ✓
- Task #24 — `common::observability::otel::init_otel` + `otel_grpc::{client_interceptor, server_interceptor, BoundedTraceContextPropagator}`. ✓
- Task #31 — proto STANDARD-lint rename sweep. ✓
- **Exemplar**: Task #25 AC wiring → `docs/devloop-outputs/2026-06-25-ac-otel-wiring-task25/main.md` (main.rs registry composition + Config `otel_config()` pattern to mirror).

---

## Cross-Boundary Classification

<!-- Filled by implementer at planning; reviewers confirm/upgrade at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `crates/mh-service/src/main.rs` | Mine | — |
| `crates/mh-service/src/config.rs` | Mine | — |
| `crates/mh-service/Cargo.toml` | Mine | — |
| `crates/mh-service/src/grpc/gc_client.rs` | Mine | — |
| `crates/mh-service/src/grpc/mc_client.rs` | Mine | — |
| `crates/mh-service/src/grpc/mod.rs` (module decl for span_layer) | Mine | — |
| `crates/mh-service/src/grpc/span_layer.rs` (new — see §Scope Addition) | Mine | — |
| `crates/mh-service/src/webtransport/connection.rs` | Mine | — |
| `crates/mh-service/tests/common/grpc_rig.rs` | Mine | — |
| `crates/mh-service/tests/common/wt_client.rs` | Mine | — |
| `crates/mh-service/tests/common/mock_mc.rs` (traceparent capture) | Mine | — |
| `crates/mh-service/tests/common/mod.rs` (module decl for otel_capture) | Mine | — |
| `crates/mh-service/tests/common/otel_capture.rs` (new — shared InMemorySpanExporter helper) | Mine | — |
| `crates/mh-service/tests/gc_integration.rs` (2 GC outbound-inject tests) | Mine | — |
| `crates/mh-service/tests/otel_grpc_integration.rs` (new) | Mine | — |
| `crates/mh-service/tests/otel_webtransport_integration.rs` (new) | Mine | — |
| `docs/TODO.md` (deferred overlay+netpol bullet + DRY entry updates) | Mine | — |
| `infra/services/mh-service/configmap.yaml` | Not mine, Minor-judgment | operations |

**Layer-A scope-drift check (manual, guard pending per ADR-0024 §6.8):** reconciled the actual `git status` diff against this table — every changed/new file has a row; all `crates/mh-service/**` + `docs/TODO.md` are Mine, `configmap.yaml` is the sole non-Mine (Minor-judgment/operations). No GSA paths. PASS.

### Scope Addition (beyond the 7 Gate-1 directives): `grpc/span_layer.rs`
While writing the inbound-extraction test, the implementer found empirically that `MediaHandlerServiceServer::with_interceptor(server_interceptor())` **alone does not reparent the handler span** — tonic's `Interceptor::call()` runs synchronously/eagerly *before* the handler future (and its `#[instrument]` span) is polled, so `Span::current()` is `Span::none()` and `set_parent()` silently no-ops; every inbound call became a fresh root trace. AC never exercised this (no inbound gRPC boundary), so R-56's inbound half was latently broken. **Fix**: new self-contained tower `SpanLayer` (`crates/mh-service/src/grpc/span_layer.rs`, ~90 LoC, no `common` changes) that creates+enters a bare per-request span before the interceptor runs and `.instrument()`s the response future, wired via `.layer(SpanLayer)` in `main.rs` alongside `.layer(auth_layer)` and mirrored in `grpc_rig.rs`. Backed by `test_inbound_register_meeting_reparents_handler_span_to_injected_trace`. **This is production code beyond the named directives → explicit Gate-3 review focus for code-reviewer (tower Service correctness), observability (span semantics), semantic-guard (no blocking / no auth leak), and security.**

**Guardrails flagged by Lead at setup**:
- **No GSA edits expected.** `proto/**` is GSA, but task #2 already added the trace fields — task #27 only READS the generated `MhClientMessage` fields; **do not edit proto or `proto-gen`**. `crates/common/**` otel modules are consumed, not modified.
- `infra/services/mh-service/configmap.yaml` is MH's own service manifest (per media-handler INDEX). Surfacing three OTel env keys mirrors the AC configmap (task #25). Likely **Mine** or **Minor-judgment**; operations reviewer confirms at Gate 1/Gate 3.

---

## Planning

**Implementer plan received** (3 parts + tests, per §Task Overview). Key design decisions:
- Mirror AC exemplar (task #25) for main.rs registry composition + config `otel_config()`.
- **McClient interceptor typing**: `BoxedInterceptor` type alias wrapping `client_interceptor()` (vs threading a generic `I: Interceptor` param) — reviewer preference solicited from code-reviewer.
- **R-58 envelope reads**: local `Extractor` over `MhClientMessage`'s two trace strings; empty proto3-default fields → no-op root span (behaves exactly as today).
- New direct deps in `mh-service/Cargo.toml`: `opentelemetry`, `tracing-opentelemetry` (connection.rs names OTel types directly, unlike AC); dev-dep `opentelemetry_sdk` (testing) for `InMemorySpanExporter` span-parent assertions.
- **Scope decision flagged**: Kind-overlay `OTEL_ENABLED=true` patch + MH→collector NetworkPolicy egress treated as OUT of scope (OTEL default-off; R-59/infrastructure owns collector+netpol). Operations + observability ruling solicited.

### Gate 1 — Classification-sanity guard
`validate-cross-boundary-classification.sh` → `STATUS=OK REASON=cross-boundary-classification-clean-1-files`. Manual Lead check: no GSA paths; sole non-Mine row (`configmap.yaml`) is Minor-judgment with Owner=operations. PASS.

### Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | **confirmed** — no GSA touched, no auth leakage, propagator bounds-checks untrusted input. Gate-2 watch-item: WT extraction must use the *global* `get_text_map_propagator` (set by init_otel), not a fresh `TraceContextPropagator`, so W3C bounds apply |
| Test | **confirmed** — retry-continuity case addition satisfied its condition |
| Observability | **confirmed** — accepts netpol/overlay out-of-scope (condition: TODO.md tracking entry, aligns w/ operations); impl note: use `HashMap<String,String>` blanket Extractor + `otel_grpc::PROPAGATED_HEADERS[0]/[1]` instead of custom Extractor + hardcoded strings |
| Code Quality | **confirmed** — decision: McClient uses **generic-param** interceptor shape (NOT boxed); client_interceptor is already `Clone` → zero-alloc. Watch-item: drop/neutralize the dead `error!("Failed to load configuration")` after the config-before-tracing reorder (no subscriber yet) |
| DRY | confirmed — no true dup; 2 extraction opportunities logged to TODO.md §Cross-Service Duplication (per-service OTel config scaffold ripe on AC+MH; shared envelope W3C Extractor for MH+MC #6) — non-blocking |
| Operations | confirmed (configmap hunk ACK'd; will Gate-3 hunk-ACK via Approved-Cross-Boundary trailer) — 2 conditions folded into build (below) |
| Semantic Guard | confirmed — Gate-3 focus: no metadata/envelope into span *attributes* (only set_parent), extractor stays 2-header, no bearer tokens in new test fixtures |

**Operations conditions folded into implementation (both non-blocking, in-PR):**
1. **Configmap comment must be ADAPTED, not copied, from AC.** AC's comment claims the "collector egress NetworkPolicy rule (now present)"; MH has NO such egress rule (`mh-service/network-policy.yaml` egress = GC/MC/AC/DNS only) and no Kind-overlay OTel patch. MH's comment must state the egress rule is **not yet present** and that flipping `OTEL_ENABLED=true` today would CrashLoop the pod.
2. **Add a `docs/TODO.md` tracking bullet** (Observability/Infra Debt): "MH Kind-overlay OTEL_ENABLED=true patch + MH→otel-collector egress NetworkPolicy rule — deferred from task #27 (config wiring only); needed before MH traces validate end-to-end in dev." Owner: operations. → surfaces under §Accepted Deferrals at close as a pointer.

**Scope ruling (operations + pending observability):** Kind-overlay-enable + MH→collector egress netpol are OUT of scope for #27 (R-59/infrastructure owns collector+netpol; collector ingress already whitelists mh-service per task #28; base+overlay both stay OTEL_ENABLED=false so no live crash surface introduced).

**Required test addition (from test reviewer, folded into plan-of-record):** `otel_grpc_integration.rs` — McClient **retry-continuity** case: under `MockBehavior::FailThenAccept`, assert `client_interceptor()` injects the *same* trace_id+span_id (captured via the mock's per-attempt `traceparent` capture) on every retry attempt while the outer `#[instrument]` span stays entered across backoff. Malformed-traceparent bounds NOT re-tested at MH level (already covered by `BoundedTraceContextPropagator`'s rejection-matrix in common/task #24 — MH adds no parsing of its own).

### Consolidated Gate-1 build directives (to hand implementer on "Plan approved")
1. **McClient interceptor typing → GENERIC-PARAM shape, not boxed** (code-reviewer's arbitration): `client_interceptor()` is already `impl Interceptor + Clone + …`, so thread `I: Interceptor + Clone + Send + Sync + 'static` through `try_send`/`send_with_retry` and use `MediaCoordinationServiceClient<InterceptedService<Channel, I>>`. Zero-alloc; preserves the `Clone` contract. (Drop the `BoxedInterceptor` alias from the plan.)
2. **R-58 extraction → use `HashMap<String,String>` blanket Extractor, not a custom `Extractor` type** (observability): build a ≤2-entry map from `envelope.trace_parent`/`trace_state` (skip empties — proto3 default), keyed by `otel_grpc::PROPAGATED_HEADERS[0]/[1]` if `pub`, else the literal `"traceparent"`/`"tracestate"` with a comment. Mirror `otel_grpc.rs`'s discard-on-parse-failure shape (no unwrap/panic per ADR-0002).
3. **Configmap comment ADAPTED, not copied** (operations): MH has no MH→collector egress rule and no Kind-overlay OTel patch. Comment must state the egress rule is **not yet present** and that flipping `OTEL_ENABLED=true` today would CrashLoop the pod.
4. **Add `docs/TODO.md` tracking bullet** (operations + observability): deferred MH Kind-overlay `OTEL_ENABLED=true` patch + MH→otel-collector egress NetworkPolicy rule (mirrors AC's `overlays/kind/services/ac-service/configmap-otel-patch.yaml` + `ac-service/network-policy.yaml` egress); deferred pending GC #26 / MC #6 so dev-cluster OTel enablement lands once across all four services. Owner: media-handler + operations. → pointer under §Accepted Deferrals at close.
5. **Neutralize the dead `error!("Failed to load configuration")`** after reordering `Config::from_env()` before subscriber init (code-reviewer watch-item): drop it (match AC's bare `?` + comment) or note it's a pre-subscriber no-op fallback ahead of the default stderr Debug dump.
6. **Add the McClient retry-continuity test case** (test, per above).
7. **WT extraction via the GLOBAL propagator** (security watch-item): use `opentelemetry::global::get_text_map_propagator(...)` (registered by this task's `init_otel`), NOT a fresh `TraceContextPropagator` — so the `BoundedTraceContextPropagator` W3C bounds-checking applies to this untrusted browser-originated input path automatically.

---

## Pre-Work

None.

---

## Implementation Summary

### R-55 partial — MH OTel init + Config + configmap
| Item | Detail |
|------|--------|
| `main.rs` | `init_otel("media-handler", env!("CARGO_PKG_VERSION"), &config.environment, otel_cfg).await?` gated on `config.otel_config()`; `(otel_layer, _otel_guard)` split composed onto bare `tracing_subscriber::registry()`; guard held to end of `main`. Config load reordered before subscriber init (mirrors AC #25). Dead pre-subscriber `error!` neutralized. |
| `config.rs` | `otel_enabled: bool` (default false), `otel_endpoint`, `otel_sample_rate: f64` (default 1.0), `environment` (DEPLOYMENT_ENVIRONMENT, default "development"); `otel_config() -> Option<OtelConfig>` (Some iff enabled); env parse + enabled-requires-endpoint guard; 12 new unit tests mirroring AC. |
| `configmap.yaml` | `OTEL_ENABLED="false"`, `OTLP_ENDPOINT`, `OTEL_SAMPLE_RATE="1.0"`, `DEPLOYMENT_ENVIRONMENT="development"` — comment **adapted** (not copied) to state MH has no egress rule / overlay yet + CrashLoop warning. |

### R-56 partial — gRPC trace-propagation interceptors
| Boundary | Change |
|----------|--------|
| MH→GC (`gc_client.rs`) | `client_interceptor()` on both `MediaHandlerRegistryServiceClient` sites (`try_register`, `send_load_report`). |
| MH→MC (`mc_client.rs`) | `client_interceptor()` via **generic-param** shape (`I: Interceptor + Clone + Send + Sync + 'static` threaded through `try_send`/`send_with_retry`, `MediaCoordinationServiceClient<InterceptedService<Channel, I>>`) — code-reviewer's arbitration over boxing. |
| Inbound MC→MH (`main.rs` + new `grpc/span_layer.rs`) | `server_interceptor()` on `MediaHandlerServiceServer`, **plus** new `SpanLayer` tower Layer (see §Scope Addition) required for the interceptor's `set_parent` to take effect. |

### R-58 MH side — MhClientMessage envelope trace reads
`connection.rs:handle_connection()`: after decode, builds a ≤2-entry `HashMap<String,String>` from `envelope.trace_parent`/`trace_state` (skips empties), extracts via the **global** `get_text_map_propagator` (so `BoundedTraceContextPropagator` W3C bounds apply to untrusted browser input), `set_parent` on the `mh.webtransport.connection` span → existing spans become children; empty fields → clean root (no-op).

### Tests
`otel_grpc_integration.rs` (5: McClient outbound inject, no-OTel-layer negative, retry-continuity, inbound reparent + no-auth-in-attrs, inbound no-op), `otel_webtransport_integration.rs` (2: WT valid reparent + empty no-op), GC outbound inject (2, in `gc_integration.rs`), shared `tests/common/otel_capture.rs` (InMemorySpanExporter helper), config unit tests (12).

---

## Files Modified

```
 crates/mh-service/Cargo.toml                     |  11 +
 crates/mh-service/src/config.rs                  | 272 ++++++++++++++++++++++-
 crates/mh-service/src/grpc/gc_client.rs          |  13 +-
 crates/mh-service/src/grpc/mc_client.rs          |  31 ++-
 crates/mh-service/src/grpc/mod.rs                |   2 +
 crates/mh-service/src/main.rs                    |  65 +++++-
 crates/mh-service/src/webtransport/connection.rs |  29 +++
 crates/mh-service/tests/common/grpc_rig.rs       |  14 +-
 crates/mh-service/tests/common/mock_mc.rs        |  29 +++
 crates/mh-service/tests/common/mod.rs            |   3 +
 crates/mh-service/tests/common/wt_client.rs      |  21 +-
 crates/mh-service/tests/gc_integration.rs        | 154 +++++++++++++
 docs/TODO.md                                     |   7 +-
 infra/services/mh-service/configmap.yaml         |  19 ++
 14 files changed, 644 insertions(+), 26 deletions(-)
```
New files (untracked until commit): `crates/mh-service/src/grpc/span_layer.rs`, `crates/mh-service/tests/common/otel_capture.rs`, `crates/mh-service/tests/otel_grpc_integration.rs`, `crates/mh-service/tests/otel_webtransport_integration.rs`.

---

## Devloop Verification Steps

**Gate 2: PASSED** (Layers 1–6 by implementer + Layer 7 by Lead on a clean cluster).

| Layer | Status | Notes |
|-------|--------|-------|
| 1 Compile | OK | workspace build clean |
| 2 Format | OK | `cargo fmt` clean |
| 3 Guards | OK | incl. manual Layer-A scope-drift (guard binary built; classification table reconciled) |
| 4 Test | OK | mh-service 127 lib + 63 integration across 12 binaries, 3× no flakes |
| 5 Lint | OK | `cargo clippy --workspace --all-targets -D warnings` clean |
| 6 Audit | OK | |
| 7 Env-tests | **OK** | `RESULT=OK REASON=env-tests-passed`, DURATION=1100s. All 13 env-test binaries green, 0 failed: cluster_health 7, auth_smoke 5, auth_flows 5, cross_service 12, mc_gc 3, meeting_creation 6, join_flow 9, auth_security 9, **mh_quic 6 (+1 ignored, 75s)**, observability 2, gc_telemetry 11, resilience 2, doc-tests 1. Clean Lead-owned run after 3 prior operator-lane PRECONDITION_FAILUREs (concurrency collision, disk-pressure timeout, dirty-cluster Calico AlreadyExists) — none code-related; see §Issues Encountered. |

---

## Code Review Results

**Gate 3 opened** — "Start Review" unicast to all 7 reviewers after Gate 2 pass. Each briefed with their Gate-1 focus items + the `span_layer.rs` scope addition flagged for scrutiny.

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security | **CLEAR** | verified global propagator (connection.rs:186-212), no auth leakage (real Bearer+traceparent E2E test asserts no auth in span attrs), configmap no secrets, span_layer carries no attrs. No findings. |
| Test | **CLEAR** | ran all new/changed files (60+ tests, 0 fail); retry-continuity asserts per-attempt byte-identical traceparent; **empirically proved** reparenting test non-tautological (removed `.layer(SpanLayer)` → test fails → reverted, diff unchanged); negative no-traceparent test rules out false-positive; shared grpc_rig extension caused no regression in auth_layer/mc_client suites. No findings. |
| Observability | **CLEAR** | all 7 Gate-1 items verified landed (service.name, sample 1.0, single global propagator ×3 boundaries, R-58 reparent w/ real WT E2E test + no-op lock, disabled=true-no-op, netpol/overlay TODO tracked, HashMap Extractor shape); span_layer span-semantics sound (sync-then-instrument, correct layer ordering, zero attrs, no perturbation of existing emission). No findings. |
| Code Quality | **CLEAR** | ADR-0002/0011/0024 all PASS; generic-param McClient confirmed (no boxing); dead error! resolved; span_layer traced structurally (tower layer-ordering via tower-layer source: SpanLayer outermost/before auth+interceptor; correct-by-construction re-entry via `.instrument()`; poll_ready delegates; Send/'static sufficient; scoped to inbound only). No findings. |
| DRY | **CLEAR** | no true dup (all common consumed as-is, no common edits; McClient generic > boxed noted as improvement); corrected own envelope-extraction TODO entry (HashMap idiom, not bespoke type); added SpanLayer extraction-candidate (N=1, MC R-56-inbound future consumer); otel_capture.rs fine in-crate. Non-blocking TODO items only. No findings. |
| Operations | **CLEAR** | configmap Minor-judgment hunk **ACK'd** (adapted comment correct); both Gate-1 conditions Fixed (comment accuracy + TODO deferral bullet); deploy-safe (default-off inert, fail-hard-at-init only when enabled); span_layer operationally inert. Opted to skip optional Approved-Cross-Boundary trailer. |
| Semantic Guard | **CLEAR** (native SAFE) | all 4 checks pass: credential-leak (carrier only 2 W3C fields, span_layer zero attrs, E2E no-auth-in-span test), actor-blocking (sync extract, non-blocking tower middleware), error-context (infallible extract, decode error handling untouched), metrics-path N/A. No findings. |

---

## Accepted Deferrals

**No findings were deferred — all 7 reviewers returned CLEAR (zero findings).** The pointers below are forward-looking items (a scope deferral + DRY extraction opportunities) added to `docs/TODO.md` during this devloop per SKILL Step 9, NOT deferred findings:

- `docs/TODO.md` §Observability/Infra Debt — MH Kind-overlay `OTEL_ENABLED=true` + MH→otel-collector egress NetworkPolicy (scope-deferred from #27; owner media-handler+operations)
- `docs/TODO.md` §Cross-Service Duplication — per-service R-55 OTel config-parse/wiring scaffold hoist (now ripe on AC+MH; land after MC #6)
- `docs/TODO.md` §Cross-Service Duplication — shared envelope W3C extract idiom (HashMap-carrier + global-propagator; MC R-57/#6 = second consumer)
- `docs/TODO.md` §Cross-Service Duplication — `SpanLayer` ambient-span tower Layer extraction candidate (N=1; future MC inbound-R-56 consumer)

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `1fe546be10747860e74715d83f81d225336ad83b`
2. Review: `git diff 1fe546be10747860e74715d83f81d225336ad83b..HEAD`
3. Soft reset: `git reset --soft 1fe546be10747860e74715d83f81d225336ad83b`
4. Hard reset: `git reset --hard 1fe546be10747860e74715d83f81d225336ad83b`

---

## Issues Encountered & Resolutions

### Issue 1: Implementer auth failure mid-implementation
**Problem**: The implementer agent died with `Not logged in · Please run /login` partway through test authoring (~05:36). Production code (main.rs, config.rs, both gRPC clients, connection.rs, configmap.yaml, TODO.md, Cargo.toml — 431 insertions across 11 files) survived in the working tree; the test suite was incomplete, leaving an untracked exploratory `otel_grpc_scratch.rs` (a `scratch_probe_...` probe) instead of the two committed integration files.
**Resolution**: User re-logged in successfully. Lead verified the surviving diff against all 7 Gate-1 directives (D1 generic-param McClient ✓, D3 global-propagator+HashMap WT extraction ✓, D4 adapted configmap comment ✓, D5 TODO bullet ✓) and resumed the implementer via SendMessage with a precise finish-list: promote the scratch probe into `otel_grpc_integration.rs` + `otel_webtransport_integration.rs` (all cases incl. retry-continuity), delete the scratch file, verify config unit tests, self-check directives, run `./scripts/layer-all.sh`. No work lost; no rollback needed.

### Issue 2: Layer 7 PRECONDITION_FAILURE (transient cluster-op collision)
**Problem**: The implementer's first Layer-7 run terminated `STATUS=PRECONDITION_FAILURE REASON=cluster-rebuild-failed` (exit 2, operator lane — NOT a test failure). Root cause from `/tmp/devloop/helper.log`: the Layer-7 `teardown` at 20:31:43 was `rejected_busy` because a `rebuild-all` was already in flight (two concurrent cluster ops).
**Resolution**: Per Layer-7 policy, PRECONDITION_FAILURE does not consume an attempt (retry once, then escalate to operations). Lead confirmed the in-flight `rebuild-all` completed cleanly (20:35:07, exit 0) and the helper returned to idle/healthy (status calls succeeding). To prevent a second collision, Lead told the implementer to stand down on all cluster ops and took the single Layer-7 retry directly from the Lead session (`scripts/layer7.sh`, bg task).

**Retry outcome — SECOND PRECONDITION_FAILURE (different cause)**: the retry's `teardown` (20:39, exit 0) and `setup` (20:44, exit 0) succeeded, but the helper daemon (PID 2761755) then **crashed** during `rebuild-all`'s release-image build while compiling `gc-service` (an UNRELATED service) — `RESULT=PRECONDITION_FAILURE REASON=cluster-rebuild-failed`, "Helper connection lost before command completed." Disk at 94% (64G free); likely resource exhaustion during 4 simultaneous Rust release builds. **Two consecutive operator-lane failures → escalated to operations** per Layer-7 policy (retry once, then escalate). MH code is NOT implicated: Layers 1–6 green + full local unit/integration suite pass (127 lib + 63 integration, 3× no flakes); failure was in an unrelated service's build.

**Operations corrected the diagnosis (authoritative — cluster-helper owner)**: the helper daemon did NOT crash — socket answering live, `dev-cluster status` exit 0, cluster healthy (`/proc/2761755` gone = stale `helper.pid` bookkeeping only). Real cause: the **client** connection died mid-stream (`podman build gc: stream write failed: client disconnected` / `Broken pipe`, ~16.7 min into a build that took only 3.3 min at 20:35) — disk-pressure-degraded I/O (93–94% used, ~68G free) pushing the build past a tool/harness command-timeout ceiling, NOT OOM/daemon-crash. Maps to a **pre-existing tracked gap** (`docs/TODO.md` ~L318: `cmd_rebuild`'s `podman build` path lacks `setup.sh`'s `check_build_disk_space` precondition; owner operations+infrastructure) — not fixed inline in this devloop. **Operations is running `dev-cluster rebuild-all` itself** (cluster already healthy from the 20:44 setup, so no teardown/setup needed — rebuild only), backgrounded with a 600s ceiling so the slow build isn't harness-killed again. Lead holding all Layer-7 attempts until operations reports the result.

**Resolution path (operations-driven)**: operations' manual `rebuild-all` progressed with heavy podman cache hits (same build, far faster — confirms the disk-pressure/slow-build theory, not OOM). Operations' call for the definitive Layer-7 run: drive the **full `scripts/layer7.sh`** (NOT a hand-run `cargo test -p env-tests`) so the Phase-1 readiness gates (pod-health wait, Prometheus HARD-probe, Loki soft-wait) and the standard `STATUS=`/`LAYER=7` formatted output are preserved; the redundant inner `rebuild-all` is now a warm-cache near-instant hit. Operations drives it (background, timeout-avoidance) and reports the definitive outcome. Lead concurs; continues to hold.

**Update (2026-07-04 ~04:37)**: Disk pressure resolved (`/` 94%→51%, 471G free); operations' `rebuild-all` at 21:08 completed clean (2.5 min warm cache). However, per helper.log the canonical `scripts/layer7.sh` was NOT launched after that (no teardown/setup/rebuild activity in ~7.5h; operations idled). To unblock Gate 2 (Lead-owned), **Lead reclaimed the Layer-7 run** — messaged operations to stand down (avoid collision), then launched `scripts/layer7.sh` in the background (bg task `b789t6kwd`, log `scratchpad/layer7-lead-final.log`). Conditions favorable (warm caches, disk healthy). Awaiting terminal STATUS. Prior evidence env-tests pass: `/tmp/layer7-run.log` earlier showed `RESULT=OK env-tests-passed` (11+2+1, 0 failed) — treated as signal, not the official verdict.

**Update (2026-07-04 ~04:40)**: That Lead run hit a THIRD PRECONDITION_FAILURE (`cluster-setup-failed`, Calico `AlreadyExists`) — caused by a **race**: operations had a `layer7.sh` already in-flight when Lead reclaimed; operations' run tore the cluster down (exit 0) then was `TaskStop`ped mid-setup, leaving a partial cluster (`exists:true, pods healthy:false`); Lead's run then reused that half-state non-destructively and collided on Calico. Operations then fully stood down (confirmed). **Per user directive ("do it directly, don't ask operations"), Lead now owns the cluster outright**: ran explicit `dev-cluster teardown` (clean, exit 0, 13.2s) to clear the dirty state, then launched `scripts/layer7.sh` on a clean slate (bg task `b7x484fho`, log `scratchpad/layer7-lead-clean.log`). Disk healthy (51%), caches warm, no other party touching the cluster. Awaiting terminal STATUS.

---

## Lessons Learned

1. **`with_interceptor` alone silently no-ops `set_parent` on tonic servers.** The interceptor runs synchronously before the handler's `#[instrument]` span exists, so an ambient tower span (`SpanLayer`) must wrap+`.instrument()` the request future for inbound trace extraction to work. AC never caught this (HTTP-only). The implementer found it empirically while writing the inbound test and flagged it as a scope addition rather than sliding it through — the exemplar-mirroring approach would otherwise have shipped R-56's inbound half latently broken.
2. **Env-test infra flakiness ≠ code failure.** Layer 7 hit three consecutive operator-lane PRECONDITION_FAILUREs (concurrency collision, disk-pressure build timeout, dirty-cluster Calico AlreadyExists) — none code-related. The `STATUS=`/`RESULT=` operator-lane vs test-lane distinction is what kept these from being misread as regressions. Lesson: one owner for cluster ops at a time; explicit `dev-cluster teardown` clears a dirty half-state that setup.sh's non-destructive reuse can't recover.
3. **Route untrusted trace-context through the GLOBAL propagator**, not a fresh `TraceContextPropagator`, so `BoundedTraceContextPropagator`'s W3C bounds-checking applies to browser-originated input for free.
4. **Reviewer plan-stage scrutiny paid off**: operations caught that copying AC's configmap comment would be materially false for MH; test caught an untested retry-continuity invariant; code-reviewer arbitrated generic-param over boxing. All before implementation.
