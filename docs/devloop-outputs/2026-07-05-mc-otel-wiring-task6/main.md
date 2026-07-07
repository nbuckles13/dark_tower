# Devloop Output: MC OTel SDK Init + Trace Propagation + MediaConnectionUpdate Handler

**Date**: 2026-07-05
**Task**: MC OTel SDK init + trace propagation (R-55 partial, R-57) + MediaConnectionUpdate handler (R-60 MC half)
**Specialist**: meeting-controller (paired with test)
**Mode**: Agent Teams (v2), full + `--paired-with=test`
**Branch**: `feature/browser-client-join-task-6`
**Duration**: (in progress)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `d585d16f6a76b5f54cfb3f1a7bb39f6883c16c23` |
| Branch | `feature/browser-client-join-task-6` |
| User story | `docs/user-stories/2026-05-02-browser-client-join.md` (task #6) |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` (spawned) |
| Implementing Specialist | `meeting-controller` |
| Iteration | `1` |
| Security | `security` (spawned) |
| Test (paired) | `paired-test` (spawned) |
| Observability | `observability` (spawned) |
| Code Quality | `code-reviewer` (spawned) |
| DRY | `dry-reviewer` (spawned) |
| Operations | `operations` (spawned) |
| Semantic Guard | `semantic-guard` (spawned) |

---

## Task Overview

### Objective
Wire the OpenTelemetry SDK into MC and complete the browser→MC→MH trace-context
propagation path, plus turn the Task-#2 `MediaConnectionUpdate` handler stub into
a real per-`(participant, mh_url)` state recorder on the participant actor.

Three requirement threads:
- **R-55 partial** — MC `main.rs` calls `common::observability::otel::init_otel(...)`
  with `service.name=meeting-controller`; MC `Config` gains `otel_enabled` /
  `otel_endpoint` / `otel_sample_rate` (+ `environment`); Kustomize ConfigMap
  surfaces them (default `OTEL_ENABLED=false`); Kind overlay enables.
- **R-57** — MC uses the **global** `BoundedTraceContextPropagator` (registered by
  `init_otel`) to extract W3C context from `ClientMessage.trace_parent`/`trace_state`
  at the `webtransport/connection.rs` dispatch site; existing `#[instrument]` spans
  become children automatically. Outbound `ServerMessage` populates the fields
  symmetrically. R-56 Tonic interceptor wired on MC's outbound `GcClient` +
  `MhClient` and inbound `MediaCoordinationService` / `MeetingControllerService`
  servers.
- **R-60 MC half** — replace the Task-#2 `MediaConnectionUpdate` handler stub
  (`connection.rs:556`) with per-MH state recording on `ParticipantActor`
  (`HashMap<String, MhConnectionStatus>` keyed by `mh_url`); emit
  `mc_participant_mh_status_total{state}` (final shape adjudicated by observability).

### Scope
- **Service(s)**: meeting-controller (`crates/mc-service/`) + MC-owned infra
- **Schema**: No
- **Cross-cutting**: OTel wiring + new instrumentation (observability mandatory);
  client-controlled-string handling (security mandatory)
- **Proto/GSA**: **NONE** — proto changes (`MediaConnectionUpdate`, `MhConnectionStatus`,
  trace fields 20/21) already landed in Task #2; `proto-gen` is build-time generated.
  This task consumes generated types only.

### Debate Decision
NOT NEEDED — design is fully specified by the task directive + the #25/#26/#27
exemplars + ADR-0028 §7 (env-test ownership) + R-54/R-55/R-56/R-57/R-60.

### Scope decision — R-60 media-connection observability surfaces (Lead + user, 2026-07-07)
Task #2 deleted the `MediaConnectionFailed` proto + `mc_media_connection_failures_total`
metric AND its dependent surfaces (`MCMediaConnectionAllFailed` alert, `mc-incident-response.md`
Scenario 11, dashboard panel, post-deploy gates), leaving TODO breadcrumb (`docs/TODO.md:182`,
`mc-incident-response.md` changelog) saying **Task #6 reintroduces all six atop the new
`mc_participant_mh_status_total{state}` metric**. Lead review at Gate 3 found the first-pass diff
delivered (a) metric, (d) dashboard panel, (f) catalog — but NOT (b) alert, (c) Scenario 11,
(e) post-deploy gates. The formal task #6 scope (user-story row L497 + R-60 text + Design section)
lists only metric+handler+tests; the three surfaces are assigned ONLY by the Task-#2 breadcrumb.
Shipping the new metric with no replacement alert is an operational regression (media-all-fail no
longer pages). **User decision (2026-07-07): reintroduce (b)(c)(e) in THIS devloop.** observability
owns the new alert PromQL/threshold spec (metric semantics changed); operations owns Scenario 11 +
post-deploy gates (incl. the cross-boundary `mh-deployment.md` stanza). Added as a 6th finding-class
work item; gates the commit.

**As-landed + owner-correct deviation from the breadcrumb (record for commit):**
- **(b)** `MCMediaConnectionAllFailed` reintroduced in `mc-alerts.yaml`, `severity: page`, as a **failed-share ratio**
  `sum(rate(...{state="failed"}[5m])) / sum(rate(...{state=~"connected|failed"}[5m])) > 0.80` + volume floor `> 0.1/s`,
  `for: 5m`, fleet-level. observability-specced + byte-verified (`validate-alert-rules.sh` OK).
- **(c)** `mc-incident-response.md` Scenario 11 reintroduced (TOC + Version-History closing the 2026-05-03 removal note);
  `runbook_url` anchor resolves. operations-authored.
- **(e)** post-deploy gates in `mc-deployment.md` + `mh-deployment.md` (cross-boundary; `Approved-Cross-Boundary: operations` trailer provided).
- **DEVIATION (owner-approved):** the TODO:183 breadcrumb specified the gate as `{state="failed"} increase == 0`. That
  text was written for the DELETED all-fail counter; the new per-MH counter increments `failed` on any single client
  hiccup, so `== 0` would false-fail every deploy. observability (metric/alert taxonomy owner) reformulated it to a
  **failed-share ratio `< 0.20`** (with `clamp_min` no-traffic guard), consistent with the alert's `> 0.80`. Owner-correct
  improvement, NOT scope drift — surfaced so the `==0`→ratio change is visible at Gate 3.

---

## Cross-Boundary Classification

<!-- Implementer-populated at planning. Lead runs the classification-sanity guard
     at Gate 1 before "Plan approved". Tiers per ADR-0024 §6.2. -->

**No `proto/**` or other GSA path is touched** — the proto changes (`MediaConnectionUpdate`,
`MhConnectionStatus`, `ConnectionState`, trace fields 20/21) landed in Task #2; `proto-gen`
is build-time generated. This task consumes generated types only. `crates/common/**` OTel
modules are consumed, not modified (no hoist this devloop — see DRY note).

| Path | Classification | Owner (if not mine) | Notes |
|------|----------------|---------------------|-------|
| `crates/mc-service/src/main.rs` | Mine, Domain-judgment | — | (A) `init_otel` wiring + config-before-subscriber reorder + dead-`error!` neutralize; (B) inbound `TraceLayer::new_for_grpc()` + `server_interceptor()` on both `MeetingControllerServiceServer` & `MediaCoordinationServiceServer` |
| `crates/mc-service/src/config.rs` | Mine, Domain-judgment | — | 4 new fields + `otel_config()` + `ConfigError::InvalidOtelConfig` + Debug entries + ~12 tests |
| `crates/mc-service/Cargo.toml` | Mine, Mechanical | — | direct `opentelemetry` + `tracing-opentelemetry` (workspace); dev-dep `opentelemetry_sdk` (testing) + reuse existing `tokio-stream` dev-dep |
| `crates/mc-service/src/grpc/gc_client.rs` | Mine, Domain-judgment | — | (B) `client_interceptor()` on outbound `GlobalControllerServiceClient` (3 sites) + fix 2 literal `Config { .. }` test constructions (~672-695, 716-739) for the 4 new non-`Default` fields |
| `crates/mc-service/src/grpc/mh_client.rs` | Mine, Mechanical | — | (B) `client_interceptor()` on outbound `MediaHandlerServiceClient` (1 site) |
| `crates/mc-service/src/webtransport/connection.rs` | Mine, Domain-judgment | — | (B) R-57 inbound extract on JoinRequest + post-join dispatch; outbound inject on `ServerMessage` literals; (C) R-60 handler replaces stub, threads participant handle through `run_bridge_loop`/`handle_client_message` |
| `crates/mc-service/src/webtransport/handler.rs` | Mine, Domain-judgment | — | outbound `ServerMessage` trace-field inject in `encode_participant_update` |
| `crates/mc-service/src/webtransport/trace.rs` (NEW) | Mine, Domain-judgment | — | (B) local WT trace helpers `reparent_current_span`/`inject_current_context` over the GLOBAL propagator + `PROPAGATED_HEADERS` (kept local per DRY adjudication) |
| `crates/mc-service/src/webtransport/mod.rs` | Mine, Mechanical | — | `pub mod trace;` module decl |
| `crates/mc-service/src/actors/participant.rs` | Mine, Domain-judgment | — | (C) `HashMap<String, MhConnectionStatus>` field + `RecordMhStatuses` handler + `record_mh_statuses()` on handle + metric emit |
| `crates/mc-service/src/actors/messages.rs` | Mine, Domain-judgment | — | (C) new `ParticipantMessage::RecordMhStatuses` variant |
| `crates/mc-service/src/observability/metrics.rs` | Mine, Domain-judgment | — | (C) `record_participant_mh_status(state)` wrapper + MetricAssertion/Cat-B coverage (ADR-0032) |
| `crates/mc-service/tests/common/mod.rs` | Mine, Mechanical | — | module decl for new `otel_capture` |
| `crates/mc-service/tests/common/otel_capture.rs` (NEW) | Mine, Domain-judgment | test (paired) | `InMemorySpanExporter` harness (propagator install + scoped subscriber) — mirrors GC/MH |
| `crates/mc-service/tests/otel_grpc_inbound_continuity.rs` (NEW) | Mine, Domain-judgment | test (paired) | inbound span-parent test (catches the no-op gotcha; proven non-tautological) |
| `crates/mc-service/tests/otel_webtransport_integration.rs` (NEW) | Mine, Domain-judgment | test (paired) | WT reparent: valid `trace_parent` → child; empty → clean root |
| `crates/mc-service/tests/media_connection_update_integration.rs` (NEW) | Mine, Domain-judgment | test (paired) | R-60 all-CONNECTED / partial / all-FAILED state + metric |
| `crates/mc-service/tests/otel_grpc_outbound_integration.rs` (NEW) | Mine, Domain-judgment | test (paired) | outbound `client_interceptor()` inject round-trip + no-OTel-layer negative |
| `crates/mc-service/tests/gc_integration.rs` | Mine, Mechanical | — | add the 4 new non-`Default` OTel fields to the `test_config` `Config { .. }` literal (third literal beyond gc_client.rs's two) |
| `crates/env-tests/tests/26_mh_quic.rs` (extend) | Mine, Domain-judgment | meeting-controller + test | R-60 `mc_participant_mh_status_total` PromQL-delta live test + `#[ignore]`d trace-continuity stub (ADR-0028 §7 — in scope). Landed in `26_mh_quic.rs` (join+update plane), NOT `22_mc_gc_integration.rs` (HTTP-user-POV only), per @paired-test |
| `infra/services/mc-service/configmap.yaml` | Not mine, Minor-judgment | operations | 4 OTel keys, base `OTEL_ENABLED: "false"` |
| `infra/services/mc-service/mc-0-deployment.yaml` | Not mine, Minor-judgment | operations | surface 4 keys via per-key `configMapKeyRef` → `mc-service-config` (confirmed by @operations; MC has NO `deployment.yaml` — it's a 2-instance topology) |
| `infra/services/mc-service/mc-1-deployment.yaml` | Not mine, Minor-judgment | operations | same 4 keys — `validate-env-config` checks BOTH instance files (easy to half-do) |
| `infra/services/mc-service/network-policy.yaml` | Not mine, Minor-judgment | operations | additive `:4317` OTLP-gRPC egress to collector |
| `infra/kubernetes/overlays/kind/services/mc-service/configmap-otel-patch.yaml` (NEW) | Not mine, Minor-judgment | operations | Kind overlay flips `OTEL_ENABLED: "true"` (path confirm w/ @operations) |
| `infra/kubernetes/overlays/kind/services/mc-service/kustomization.yaml` | Not mine, Minor-judgment | operations | `patches:` entry |
| `docs/observability/metrics/mc-service.md` | Mine, Mechanical | observability (co-review) | `mc_participant_mh_status_total` catalog entry |
| `infra/grafana/dashboards/mc-overview.json` | Mine, Mechanical | observability (co-review) | two timeseries panels for `mc_participant_mh_status_total` + `mc_participant_mh_status_dropped_total` (validate-application-metrics `metric_no_dashboard` requires every emitted metric on a dashboard) |
| `docs/runbooks/mc-deployment.md` | Mine, Domain-judgment | operations (co-review) | OTel break-glass section + (e) R-60 media-connection post-deploy acceptance gates (`{state="failed"}` PromQL) |
| `infra/docker/prometheus/rules/mc-alerts.yaml` (SCOPE-ADD b) | Not mine, Domain-judgment | observability (PromQL spec) + operations | reintroduce `MCMediaConnectionAllFailed` alert atop `mc_participant_mh_status_total{state}` — **observability owns the new PromQL/threshold spec** (metric semantics changed from all-fail counter to per-state); meeting-controller implements per that spec |
| `docs/runbooks/mc-incident-response.md` (SCOPE-ADD c) | Mine, Domain-judgment | operations (co-review) | reintroduce Scenario 11 (Media Connection Failures) atop the new metric/alert |
| `docs/runbooks/mh-deployment.md` (SCOPE-ADD e) | Not mine, Minor-judgment | operations | R-60 post-deploy acceptance gate stanza (cross-boundary into MH's deployment runbook; media-handler not in this loop → operations owns) |
| `docs/TODO.md` | Mine, Mechanical | — | DRY/deferral breadcrumbs |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine, Mechanical | — | task #6 status (flip at close) |
| `docs/devloop-outputs/2026-07-05-mc-otel-wiring-task6/main.md` | Mine, Mechanical | — | this file |

**Span-layer choice (gotcha #4):** Using **option (a) `tower_http::trace::TraceLayer::new_for_grpc()`** (zero new deps — `tower-http` "trace" feature already in `Cargo.toml`), NOT MH's `SpanLayer`. Reasoning: MC's inbound builder mirrors **GC #26's** exactly (single `Server::builder()` with an auth layer bound to `http::Response<BoxBody>` serving two services), not MH's. Per the gotcha + GC's finding, `TraceLayer` MUST be the first/outermost `.layer()` (auth second) or `McAuthService`'s exact-`BoxBody` bound fails to compile (E0271). Copying MH's `SpanLayer` would add ~90 LoC of new local code + DRY debt for no benefit. This **resolves** #27's DRY breadcrumb "SpanLayer extraction candidate (future MC inbound-R-56 consumer)" as *not-consumed*.

---

## Planning Notes (Lead → Implementer context)

Passed to the implementer as detailed requirements; see the spawn prompt. Key
exemplar-derived gotchas (from #25/#26/#27 digest) captured so they are in scope
from the start:

1. **Config reorder** — move `Config::from_env()` **before** `tracing_subscriber`
   init in `main.rs` (currently after, ~line 89); neutralize the now-dead
   pre-subscriber `error!` at ~main.rs:90 (MH #27 hit the identical thing).
2. **Layer-FIRST** — `.with(otel.layer)` composes onto the **bare** `registry()`
   first (the layer's concrete type is `OpenTelemetryLayer<Registry, Tracer>`),
   then EnvFilter, then fmt.
3. **Named guard** — hold `_otel_guard` (named binding, NOT `let _ =`) to end of
   `main` for flush-on-shutdown.
4. **TOWER LAYER ORDER GOTCHA** — `server_interceptor()`'s `set_parent` is a silent
   no-op unless a span-creating tower `Layer` runs BEFORE the interceptor. Two
   proven options: (a) `tower_http::trace::TraceLayer::new_for_grpc()` (#26 —
   zero new deps, already available via MC's `tower-http` "trace" feature, but MUST
   be the **first/outermost** `.layer()` because `McAuthService` is bound to
   `http::Response<BoxBody>` exactly, same as GC); (b) copy MH's generic
   `SpanLayer` (#27, `crates/mh-service/src/grpc/span_layer.rs`, ~90 LoC, sidesteps
   the BoxBody constraint). **Verify with an integration test that inbound spans
   have the expected parent (not root); prove non-tautological by temporarily
   removing the span layer.**
5. **Global propagator** — extract via `opentelemetry::global::get_text_map_propagator`
   (the bounded W3C propagator installed by `init_otel`), NOT a fresh
   `TraceContextPropagator`. Reuse `otel_grpc::PROPAGATED_HEADERS`. Skip empty
   fields (proto3 defaults → clean root).
6. **Security** — `floor_char_boundary(256)` truncation on client-controlled
   `mh_url` / `failure_reason` / `failure_code` before any logging (carried over
   from the stub's TODO at connection.rs:563).
7. **Config blast radius** — new non-`Default` `Config` fields break the literal
   `Config { .. }` constructions in `gc_client.rs` tests (~lines 672-695, 716-739).
8. **DRY breadcrumb** — the per-service R-55 four-knob scaffold (`main.rs` init +
   Config) now has AC#25 + GC#26 + MH#27 siblings; #26 flagged "hoist at MC #6."
   DRY reviewer to adjudicate whether hoist lands here or is logged as debt.

---

## Implementer Plan (Gate 1)

### Thread A — R-55: OTel SDK init (main.rs + Config)
Mirror GC #26 / MH #27 exactly.
- **Config**: add `otel_enabled: bool` (`OTEL_ENABLED`, default `false` — explicit boolean gating, NOT presence-of-endpoint), `otel_endpoint: String` (`OTLP_ENDPOINT`, default `""`), `otel_sample_rate: f64` (`OTEL_SAMPLE_RATE`, default `1.0`, parse-only), `environment: String` (`DEPLOYMENT_ENVIRONMENT`, default `"development"`). Add `otel_config(&self) -> Option<common::observability::otel::OtelConfig>` (Some iff `otel_enabled`). Add `ConfigError::InvalidOtelConfig` fail-fast when `otel_enabled && otel_endpoint.trim().is_empty()`. Add the 4 (non-secret) fields to the hand-rolled `Debug` impl as plain `.field(...)` entries. ~12 new tests (defaults, parse, enabled+empty-endpoint fail-fast) mirroring GC/MH.
- **main.rs**: move `Config::from_env()` **above** `tracing_subscriber` init (currently ~line 89, after); neutralize the now-pre-subscriber `error!("Failed to load configuration")` (bare `?` + comment, matches MH #27). Call `init_otel("meeting-controller", env!("CARGO_PKG_VERSION"), &config.environment, otel_cfg)` gated on `config.otel_config()`. Compose `.with(otel.layer)` onto the **bare** `registry()` FIRST (type is `OpenTelemetryLayer<Registry, Tracer>`), then EnvFilter, then `fmt::layer().json()`. Hold `_otel_guard` as a **named binding** to end of `main`.
- Config-literal blast radius: fix the two `Config { .. }` test constructions in `gc_client.rs` (~672-695, 716-739) with the 4 new fields.

### Thread B — R-56/R-57: trace propagation
**gRPC (R-56):**
- Outbound: `client_interceptor()` on `GcClient` (`GlobalControllerServiceClient::with_interceptor(self.channel.clone(), otel_grpc::client_interceptor())` at the 3 per-call construction sites) and `MhClient` (`MediaHandlerServiceClient::with_interceptor(channel, …)` at its 1 site).
- Inbound: single `Server::builder()` in `main.rs` — `.layer(TraceLayer::new_for_grpc())` **first/outermost**, `.layer(mc_auth_layer)` second, then `MeetingControllerServiceServer::with_interceptor(svc, server_interceptor())` **and** `MediaCoordinationServiceServer::with_interceptor(svc, server_interceptor())`. (Span-layer choice + ordering rationale in §Cross-Boundary Classification.)

**WebTransport (R-57):** local private helpers in `connection.rs` (mirror MH #27's inline idiom):
- `extract_parent(trace_parent, trace_state) -> opentelemetry::Context` — build a ≤2-entry `HashMap<String,String>` carrier keyed by `otel_grpc::PROPAGATED_HEADERS[0]`/`[1]`, skip empties, `global::get_text_map_propagator(|p| p.extract(&carrier))`.
- `inject_current() -> (String, String)` — inject the current context into a carrier via the global propagator, read the two header values back out (empty when no active span/context).
- **Inbound extract, primary site — the JoinRequest** (`handle_connection`, right after `ClientMessage::decode` at ~line 128): if trace fields non-empty, `Span::current().set_parent(cx)` on the `mc.webtransport.connection` span → the whole join flow + `mc.actor.participant` spawn become children of the browser's `dt_client.join` span. Mirrors MH #27's `handle_connection` reparent.
- **Inbound extract, post-join site** (`handle_client_message`, named by the directive): the bridge loop runs inside the connection span. **@observability question:** for post-join `MediaConnectionUpdate` (which now does real R-60 work), do you want (i) a fresh per-message **child span** parented to that message's extracted context (semantically cleaner — a long-lived connection span shouldn't be re-parented per message), or (ii) literal `set_parent` on the connection span per the directive text? I lean (i). Confirm.
- **Outbound inject:** populate `trace_parent`/`trace_state` (currently `String::new()` literals) at the `ServerMessage` construction sites — JoinResponse (`connection.rs:374`), Error (`connection.rs:667`), and participant updates (`handler.rs::encode_participant_update`) — via `inject_current()`.

### Thread C — R-60: MediaConnectionUpdate handler
- `actors/messages.rs`: add `ParticipantMessage::RecordMhStatuses { statuses: Vec<MhConnectionStatus> }` (proto type, per directive; derives `Debug` via prost).
- `actors/participant.rs`: add `mh_statuses: HashMap<String, MhConnectionStatus>` field (keyed by **truncated** `mh_url`); handler upserts by key and emits `record_participant_mh_status(state)` per status; add `ParticipantActorHandle::record_mh_statuses(...)`.
- `connection.rs`: thread `&join_result.participant_handle` through `run_bridge_loop` → `handle_client_message` (make it `async`); replace the no-op arm with a real call.
- **SECURITY:** truncate client-controlled `mh_url` / `failure_reason` / `failure_code` to 256 bytes on a char boundary **before storing or logging**. `str::floor_char_boundary` is still unstable → I'll add a tiny local `truncate_utf8(&str, 256)` helper (no common helper exists; grep confirms). @security: confirm truncating the **map key** (`mh_url`) too, to bound per-participant map growth from a hostile client (metric label is `state` only, so no metric cardinality risk).
- **Metric — @observability please confirm final shape:** proposing `mc_participant_mh_status_total{state}` counter, `state ∈ {connected, failed, disconnected, unspecified}` (lower-snake from `ConnectionState`), incremented once per `MhConnectionStatus`. Emitted from the actor handler. Confirm name/label/values; also confirm whether an `unspecified` bucket should be recorded or dropped.

### DRY posture (for @dry-reviewer)
1. **R-55 four-knob scaffold** (main.rs init + Config) now has AC#25+GC#26+MH#27 siblings; #26 flagged "hoist at MC #6." Proposing: **do NOT hoist this devloop** (keep per-service, log/refresh the `docs/TODO.md` breadcrumb) — hoisting a config/main scaffold across 4 crates is its own devloop, out of scope here. Adjudicate.
2. **WT W3C extract/inject idiom** — now N=2 (MH #27 + MC #6). Proposing: **keep local** in `connection.rs` (mirror MH's inline form) + refresh the TODO breadcrumb; MC additionally needs the **outbound inject** side MH didn't. Adjudicate whether N=2 warrants a `common` hoist (which would add a `crates/common/**` Not-mine boundary).

### Tests (in scope — @paired-test collaborates)
Mirror the GC/MH exemplars (`gc-service/tests/otel_grpc_inbound_continuity.rs`, `mh-service/tests/otel_webtransport_integration.rs`, shared `otel_capture.rs`):
1. Config unit tests (~12).
2. **Inbound span-parent** (`otel_grpc_inbound_continuity.rs`): real gRPC client → MC's live inbound RPC with a `traceparent` metadata value; `InMemorySpanExporter` wired into the test `OpenTelemetryLayer`; assert finished handler span `trace_id` EQUALS the injected one. **Prove non-tautological** by temporarily removing `TraceLayer` and confirming failure, then revert.
3. **WT reparent** (`otel_webtransport_integration.rs`): `ClientMessage`/JoinRequest with valid `trace_parent` → child span reparented; empty → clean root no-op.
4. **R-60 handler** (`media_connection_update_integration.rs`): all-CONNECTED / partial / all-FAILED → correct per-MH state recorded + metric emitted (via `metrics-util` snapshot).
5. **Outbound inject** (`otel_grpc_outbound_integration.rs`): `client_interceptor()` injects `traceparent` on outbound (mock Tonic server captures metadata) + no-OTel-layer negative.
6. **Env-test** (ADR-0028 §7): MC-side trace continuity (inbound gRPC from GC, outbound to MH) — landing/fixture discipline coordinated with @paired-test (`22_mc_gc_integration.rs` / `26_mh_quic.rs`).

### Infra (all CONFIRMED by @operations)
- `configmap.yaml`: 4 keys, base `OTEL_ENABLED: "false"`, `OTLP_ENDPOINT: "http://otel-collector.dark-tower:4317"` (**namespace is `dark-tower`, NOT `default`** — corrected by @operations; wrong ns CrashLoops both MC pods once overlay flips enabled, init is fail-hard), `OTEL_SAMPLE_RATE: "1.0"`, `DEPLOYMENT_ENVIRONMENT: "development"`.
- **`mc-0-deployment.yaml` AND `mc-1-deployment.yaml`** (MC is 2-instance; there is NO `deployment.yaml`): add all 4 keys to BOTH via per-key `configMapKeyRef` → `mc-service-config` (matches GC/AC per-key pattern). Adding to base configmap alone does NOT reach pods; `validate-env-config` checks both files.
- `network-policy.yaml`: single NEW additive egress block to `app: otel-collector` TCP `:4317` (copy GC's `network-policy.yaml:103-119`). MC needs ONLY `:4317` — no `:4318` (that's GC's telemetry-proxy path). Collector ingress already admits `mc-service` on `:4317` (R-59, `otel-collector/network-policy.yaml:32-46`).
- **Kind overlay** (task directive: enable this devloop): `infra/kubernetes/overlays/kind/services/mc-service/configmap-otel-patch.yaml` (NEW, targets `mc-service-config` with `OTEL_ENABLED: "true"`, namespace `dark-tower`) + `patches: - path: configmap-otel-patch.yaml` in that dir's `kustomization.yaml`. Same 2-file strategic-merge shape as GC #26.
- Runbook `mc-deployment.md`: MC is Deployment-based (like GC, not AC's StatefulSet) but 2-instance — break-glass = patch `mc-service-config` `OTEL_ENABLED: "false"` (or drop overlay patch) + `kubectl rollout restart deployment/mc-0 deployment/mc-1 -n dark-tower`. Keep GC's fail-hard framing, pluralize restart target.

### LOCKED Gate-1 resolutions (reviewer-adjudicated)
**Metric (@observability, definitive):** `mc_participant_mh_status_total` counter, single label `state` ∈ {`connected`,`failed`,`disconnected`,`unspecified`} (cardinality 4). Increment by 1 **per `MhConnectionStatus` entry**. Derive `state` via a **total match** on `ConnectionState` with a catch-all → `unspecified` (NEVER `format!` a raw int). Emit through a new facade `pub fn record_participant_mh_status(state: &str)` in `observability/metrics.rs` (never `counter!` at the dispatch site). FORBIDDEN as labels: `mh_url`, participant/meeting id, `failure_reason`, `failure_code`. Catalog entry under "MH Coordination Metrics" + Cardinality table row (+4 series) in `docs/observability/metrics/mc-service.md`. MetricAssertion tests with `assert_unobserved` adjacency on absent states.

**Security (@security, definitive) — Thread C hardening:**
- `truncate_utf8(&str, 256)` bounds **≤256 BYTES** on a char boundary (not chars) — applied to `mh_url` (incl. the map KEY), `failure_reason`, `failure_code` before store/log.
- `const MAX_MH_STATUSES_PER_PARTICIPANT: usize = 16`. Cap semantics: updates to an already-present key are **ALWAYS allowed**; only inserting a NEW distinct key when already at cap is refused. Bounds a hostile flood at exactly 16.
- **No warn/info log on the truncation or cap-refused path** (log-amplification vector) — at most `debug!`, prefer a bounded counter. → **@observability Q:** OK to add `mc_participant_mh_status_dropped_total{reason=cap}` counter for cap-refused inserts?
- 4 security tests folded into matrix (no-auth-leak inbound gRPC, no-auth-leak WT, 10KB multi-byte truncation-not-panic, 100-key flood → map stays at 16 + existing-key update still lands).

**DRY (@dry-reviewer, confirmed):** TraceLayer (not SpanLayer); no four-knob hoist this PR (mirror AC/GC/MH byte-identically, refresh TODO count at verdict); reuse all `common` OTel APIs.

**Test targets (for @paired-test):** inbound-continuity test asserts on span **`mc.grpc.media_coordination.connected`** (MediaCoordinationService MH→MC handler) + negative (no traceparent → fresh trace-id). WT test asserts on span **`mc.webtransport.connection`** + empty→root negative. Shared constants `KNOWN_TRACE_ID_U128 = 0x4bf9_2f35_77b3_4da6_a3ce_929d_0e0e_4736`, `KNOWN_SPAN_ID_U64 = 0x00f0_67aa_0ba9_02b7`. MC has no OTel test harness → new local `tests/common/otel_capture.rs` (third copy per #27 precedent).

### Gate-1 FINAL — post-adjudication deltas (supersedes conflicting text above)

**1. R-60 storage → thin bounded DOMAIN struct, NOT raw proto** (@code-reviewer + @security, decisive). `ParticipantMessage::RecordMhStatuses { statuses: Vec<BoundedMhStatus> }` where `struct BoundedMhStatus { state: MhState, failure_reason: Option<String>, failure_code: Option<String> }` — all three client-controlled strings truncated to ≤256 bytes (char boundary) ONCE at the connection boundary (`handle_client_message`), `state` mapped off the prost `ConnectionState` there too. Actor holds `HashMap<String, BoundedMhStatus>` keyed by the **truncated** `mh_url`. This closes the truncation gap (raw proto would leave the value's 3 fields untruncated), matches the existing domain-typed `ParticipantMessage` convention (no proto crosses into the actor), and gives one truncation site at the trust boundary. `MhState` is a small local enum → drives the metric label directly.

**2. R-57 post-join → fresh per-message CHILD span** (@observability AUTHORIZED deviation from the directive's literal "set_parent on the connection span"). A span has one parent fixed at creation; re-parenting the long-lived connection span per message corrupts the tree. So: PRIMARY reparent stays on the JoinRequest (connection span → browser join trace, once). For post-join `MediaConnectionUpdate`, create a NEW span `#[instrument(target = "mc.webtransport.connection", name = "mc.media_connection_update", skip_all)]` and `set_parent(extracted_cx)` on IT. Empty trace fields → clean root (NO fallback to the connection span).

**3. Env-test → two parts (@paired-test fixture-discipline call, @observability CO-SIGNED):**
- **(1) REAL metric env-test** — extend `26_mh_quic.rs` (full AC→GC→MC→MH join + `PrometheusClient` counter-delta idiom, `#[serial]`, 60/90s budgets, absent⇒0; NOT `22_mc_gc_integration.rs` which is HTTP-user-POV only). The test's WT client fixture **constructs and frames the `ClientMessage{MediaConnectionUpdate}` proto directly** (post-join framed message, fully in-test — does NOT depend on SDK task #14), sends it to the deployed MC, then asserts the `mc_participant_mh_status_total{state}` PromQL delta moved. This is the genuinely-new cluster-observable behavior. *(Confirmed to @observability: harness sends the message itself, so the delta is real, not SDK-dependent.)*
- **(2) Trace-continuity as a documented `#[ignore]`d stub** (R-33#6 precedent) cross-referencing all THREE component-tier files — `otel_grpc_inbound_continuity.rs` + `otel_webtransport_integration.rs` + `otel_grpc_outbound_integration.rs` (@team-lead: the outbound-injection hop is part of the continuity story; all three named + greppable in the doc-comment) — as authoritative coverage, with the co-signed reason (verbatim @observability): *"Cross-service trace-id continuity is not cluster-observable by design — the dev OTel collector's debug exporter runs `verbosity: normal` (`otel-collector/configmap.yaml:44`, names+counts only, no attributes, a deliberate PII control), so the trace-id cannot be read back in-cluster. Trace-id linkage is proven at component tier (inbound-gRPC span-parent + WT reparent tests with `InMemorySpanExporter`). The MC env-test asserts the R-60 `mc_participant_mh_status_total{state}` metric only."* Rejected: (a) collector-log grep (would force `verbosity: detailed`, breaking the @security PII control) and (b) MC→MH single-hop metadata capture (partial signal, real infra cost). **`docs/TODO.md` breadcrumb (Observability/Infra Debt, @team-lead):** "cluster trace-continuity env-test blocked on a dev-cluster trace backend (R-59 deferred Tempo/Honeycomb)."

**4. Expanded test matrix** (@paired-test must-resolves):
- **Inbound (test 2):** cover an RPC on BOTH inbound services — `MediaCoordinationServiceServer` (`mc.grpc.media_coordination.connected`) AND `MeetingControllerServiceServer` (`assign_meeting_with_mh` span) — each has its own `.with_interceptor`, so one-service coverage would miss a dropped interceptor. Layer stack must be **visible + removable inside the test file** (mirror GC's `start_test_grpc_server`), NOT hidden behind a shared rig — else the non-tautology proof is undemonstrable. Positive + negative (no-traceparent → fresh id) + no-auth-leak (traceparent + Bearer → reparent, no token in attrs).
- **WT (test 3):** reuse `AcceptLoopRig` (drives real `handle_connection`). Valid reparent + empty→root + no-auth-leak (JWT in JoinRequest → no token in span attrs).
- **R-60 (test 4):** route a real `ClientMessage{MediaConnectionUpdate}` through `handle_client_message` (NOT a direct `record_mh_statuses()` poke — must exercise decode + truncation). Assert BOTH map contents per scenario AND metric with adjacency. Add truncation test: >256-byte multi-byte `mh_url`/`failure_reason` → bounded key + value, no panic. Add cap test: 100 distinct keys → map stays at 16, existing-key update still lands.
- **Outbound (test 5):** one inject test per client — `GcClient` AND `MhClient` (distinct `with_interceptor` sites) + no-OTel-layer negative.

**5. ADR-0002 discipline** (@code-reviewer, @semantic-guard): `init_otel(...)?` context-preserving propagation (no `.ok()`/`let _ =`/log-and-discard); `Config::from_env()?` bare (error type carries context); `OTEL_SAMPLE_RATE` parse → `ConfigError` or 1.0 default, never `unwrap`; `truncate_utf8` floors to char boundary (never mid-codepoint panic); `handle_client_message`'s `.send().await` handles `Err(SendError)` explicitly (actor gone mid-connection) — log w/ truncated fields + graceful continue, no panic; `record_participant_mh_status(state)` emitted UNCONDITIONALLY per status (all 3 real states on every path).

**6. WT outbound inject uses the GLOBAL propagator** (@dry-reviewer condition): `get_text_map_propagator(|p| p.inject_context(&cx, &mut carrier))` into a `HashMap<String,String>` injector, copy only the `PROPAGATED_HEADERS` (`traceparent`/`tracestate`) keys onto the `ServerMessage` — NOT a hand-rolled `00-{trace_id}-{span_id}-{flags}` string (that would reimplement the bounded propagator = true duplication).

**7. Cap-drop counter (@observability SIGNED OFF)** — SEPARATE counter `mc_participant_mh_status_dropped_total{reason}` (`reason ∈ {cap}`, cardinality 1, room to grow) for cap-refused inserts, via facade `record_participant_mh_status_dropped(reason)`. **Recorded and dropped are mutually exclusive** — a refused entry does NOT also bump the main counter; pin with `assert_unobserved` adjacency on the main counter for overflow entries (and vice-versa). Increment per refused entry. Catalog: immediately after `mc_participant_mh_status_total` under "MH Coordination Metrics" + Cardinality row (`reason | 1 (room to grow) | cap`, total +1). NOT folded into the main `state` taxonomy (`dropped` is not a `ConnectionState`).

---

## Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | **CONFIRMED** (domain-struct + cap + byte-bounded truncation accepted) |
| Test (paired) | **CONFIRMED** (env-test (1)+(2) + A/B/C + map-cap DoS test; test 2 asserts default span name `assign_meeting_with_mh`) |
| Observability | **CONFIRMED** (metric + cap-drop counter + per-message-child-span + env-test co-sign) |
| Code Quality | **CONFIRMED** (ADR-0002/0011/0023/0028§7/0032 pass + Ownership Lens: no GSA, monotonic) |
| DRY | **CONFIRMED** (TraceLayer + per-service scaffold + WT-inject global-propagator, delta #6) |
| Operations | **CONFIRMED** (2-instance deploy + `dark-tower` ns fix + :4317 egress + Kind overlay; will provide `Approved-Cross-Boundary: operations` trailer) |
| Semantic Guard | **CONFIRMED** (3 Gate-3 watch-items folded into delta #5) |

---

## Gate 2 — Validation

**PASS (Lead-independent run, 2026-07-06).** Layers 1–6 verified green on the Lead's own
machine (not a rubber-stamp of the implementer's self-run):

| Layer | Result | Reason |
|-------|--------|--------|
| 1 Compile | OK | `cargo-build-passed` |
| 2 Format | OK | `cargo-fmt-passed` |
| 3 Guards | OK | `guards-passed` (33/33) |
| 4 Test | OK | `cargo-test-passed` (178s) |
| 5 Lint | OK | `cargo-clippy-passed` |
| 6 Audit | N/A | `not-applicable-to-this-lang` + `buf-breaking-passed` (documented self-justifying gap; no audit-triggering deps changed) |
| 7 Env-tests | **OK (authoritative)** | Lead ran the same-tree authoritative Layer 7 on the freshly re-provisioned cluster (rebuild-all from final tree incl. `crossbeam-epoch 0.9.20`): `STATUS=OK REASON=env-tests-passed`, DURATION=1014s. Full env-test suite green incl. R-60 metric plane, PII-filter, network-policy resilience. |

**Validation-delay note (not a code issue):** the extended Gate-2 timeline was a harness
artifact — three long `layer-all.sh` runs were killed mid-Layer-4 by session re-auth reaping
the orchestrator process (fast layers 1–3 always completed; the ~3–9 min layers 4/7 kept
getting cut off). Zero exit-1 (test-logic) failures ever appeared. Every Layer 7 non-pass was
operator-lane precondition (exit 2): `cluster-rebuild-failed` (re-auth-orphaned helper
collision) then `cluster-setup-failed` (cluster degraded from repeated teardown/cancel churn).
Per Lead policy these do NOT consume a Gate-2 attempt. **Layer 7 confirming pass on a freshly
re-provisioned cluster is a pre-commit (Gate 3) gate — operations engaged.**

---

## Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | **RESOLVED-FIXED** | 1 | 1 | 0 | Per-message `statuses` fan-out DoS → `MAX_MH_STATUSES_PER_UPDATE=64` + `.take()` + `dropped{reason=over_limit}` counter; verified in-diff. NOTE: `over_limit` expands `..._dropped_total{reason}` to cardinality 2 — observability to confirm w/ its cardinality-row finding. |
| Test (paired) | **RESOLVED-DEFERRED** | 1 | 0 | 1 (deferral) | Accepted the inbound component-test deferral (`otel_grpc_inbound_continuity.rs` covers `MediaCoordinationService`; `MeetingControllerService`/`McAssignmentService` needs a live `FencedRedisClient` no component test can build) → env-tier; tracked in docs/TODO.md. All 4 spot-check items verified. |
| Observability | **RESOLVED-FIXED** | 1 | 1 | 0 | Stale Cardinality table → added `state` row (4) + split `reason` fencing vs mh-status-drop (`cap`,`over_limit`), total ~105→~111. Metric shape + trace wiring verified clean as-built. |
| Code Quality | **RESOLVED-FIXED** | 2 | 2 | 0 | (1) config-parse sibling-divergence FIXED (case-insensitive + hard-fail both knobs, comment corrected, 3 tests repurposed) — real `OTEL_ENABLED=TRUE`-silently-off footgun closed; (2) stale `record_participant_mh_status_dropped` doc-comment FIXED. Full ADR pass + Ownership Lens (no GSA, trace.rs local, monotonic) re-confirmed. |
| DRY | **RESOLVED-DEFERRED** | 1 | 1 | 0 (hoist-debt only) | Config-parse divergent-copy finding FIXED in-PR; code clean, nothing duplicated left in diff. DEFERRED label driven solely by non-empty DRY extraction opportunities (otel_capture 3rd copy, four-knob N=4, WT symmetric-inject helper, SpanLayer→informational) — all TODO-tracked, NOT broken code. |
| Operations | **RESOLVED-DEFERRED** | 1 | 0 | 1 (spin-out) | 6 infra files hand-verified correct + `Approved-Cross-Boundary: operations` trailer. Finding: `validate-env-config` silently skips MC+MH — dt-guard `find_workload()` (`crates/dt-guard/src/env_config.rs:53-58`) doesn't recognize `mc-{0,1}-deployment.yaml`; config↔deployment invariant not guard-enforced (manifests correct by hand-check, no compensating coverage). Spun out to infrastructure (dt-guard multi-workload discovery); recorded in docs/TODO.md §Infrastructure Validation in Devloops. Runtime unaffected. |
| Semantic Guard | **CLEAR** | 0 | — | — | 3 watch-items verified + standing checks (error-ctx, truncation, metric-completeness, no cred-leak, no actor-block) |

**SCOPE-ADD status (2026-07-07):** all three surfaces landed + signed off.
- (b) alert `MCMediaConnectionAllFailed` — observability-specced + **byte-verified** (`validate-alert-rules.sh` OK).
- (c) Scenario 11 + (e) gates — **operations-authored** (authoring = its ownership commitment) + `Approved-Cross-Boundary: operations` trailer for the `mh-deployment.md` cross-boundary edit.
- observability **part-2 gate sign-off IN**: staged gate PromQL matches the corrected failed-share-ratio `< 0.20` form (not stale `==0`); `runbook_url` anchor resolves.
- **Sole remaining gate before commit:** implementer task #4 — close `docs/TODO.md:183` breadcrumb + `layer-all` green on the FULL delta → "Scope-add ready". Then Lead re-validates (independent Layers 1–6 + authoritative Layer 7 on the warm cluster) → commit.

---

## Accepted Deferrals

(none yet)
