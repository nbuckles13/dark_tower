# Devloop Output: AC OTel Wiring (Task #25, R-55 partial / R-56 partial)

**Date**: 2026-06-25
**Task**: AC `main.rs` OTel SDK wiring via the common `init_otel` helper (R-54/task #24), AC `Config` gains `otel_enabled`/`otel_endpoint`/`otel_sample_rate`, surfaced in `infra/services/ac-service/configmap.yaml`. R-56 inbound trace-context extraction at AC's GC-facing server boundary.
**Specialist**: auth-controller
**Mode**: Full mode (8 teammates)
**Branch**: `feature/browser-client-join-task-25`

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5003d0802d8156d9b96baded514414b764b73e1a` |
| Branch | `feature/browser-client-join-task-25` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementing Specialist | `auth-controller` |
| Iteration | `1` |

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed (ruled R-56 N/A; accepted deferral) |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed (Minor-judgment infra; dev OTEL_ENABLED=true via Kind overlay; netpol egress row) |
| Semantic Guard | confirmed |

### Gate-3 Verdicts (final)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | netpol egress hunk ACKed; no token/authorization in spans; GSA clean |
| Test | RESOLVED-FIXED | 1 | 1 | 0 | sample_rate parse-only finding fixed by Y→X revert |
| Observability | RESOLVED-DEFERRED | 1 | 1 | 1 | range-check finding fixed; deferral: R-56 HTTP extraction (init_otel doc-nit since resolved in-commit) |
| Code Quality | RESOLVED-FIXED* | 2 | 2 | 0 | range-check fixed; *issued RESOLVED-DEFERRED at Gate 3 (helper-doc example spun out), but the lone spin-out was fixed in-commit post-review (user direction) → effectively RESOLVED-FIXED |
| DRY | RESOLVED-DEFERRED | 1 | 1 | 0* | duplicated bound finding fixed; *DEFERRED label is from the forward-DRY extraction breadcrumb only, not unfixed code |
| Operations | RESOLVED-DEFERRED | 1 | 1 | 1 | gc-deployment bullet fixed; 7 ops files hunk-ACKed; deferral: DEPLOYMENT_ENVIRONMENT prod-overlay |
| Semantic Guard | CLEAR | 0 | 0 | 0 | credential-leak/actor-blocking/error-context/metrics-path all clean |

**Gate 3: PASS** — all verdicts CLEAR / RESOLVED-FIXED / RESOLVED-DEFERRED; zero ESCALATED. Four reviewers landed RESOLVED-DEFERRED (accepted deferrals / forward-items below).

### Lead-frozen decisions (audit trail)
- **Endpoint env var = `OTLP_ENDPOINT`** (no rename; honors task #28; field `otel_endpoint`). Frozen after a 4× naming flip-flop; @observability confirmed "locked: OTLP_ENDPOINT".
- **`OTEL_SAMPLE_RATE` = PARSE-ONLY (option X)** — config parses, the `[0,1]` bound is single-owned by `init_otel`. Frozen; implementer briefly flipped to a config-load range check (Y), reverted to X on Lead ruling; all reviewers re-verified against X.
- **Kind overlay = 2-file strategic-merge** (output-identical to inline JSON6902; frozen to stop a form-choice flip).

---

## Task Overview

### Objective
Wire the OpenTelemetry SDK into the Auth Controller using the common `init_otel(...)` helper shipped in task #24 (`crates/common/src/observability/otel.rs`). AC is the **first** per-service R-55 wiring — no exemplar exists yet; this devloop establishes the pattern subsequent services (#26 GC, #6 MC, #27 MH) will mirror.

### Scope
- **Service(s)**: `crates/ac-service/` + `infra/services/ac-service/configmap.yaml`
- **Schema**: No
- **Proto**: No
- **Cross-cutting**: Observability (instrumentation init), Operations (configmap env vars)

### Open Questions for Planning (Lead-flagged from pre-spawn recon)
1. **AC has no gRPC server.** AC is a pure axum HTTP service — no `tonic` dependency, no `AcAuthClient`, no inbound gRPC boundary. The task's R-56 framing ("interceptor on AC's inbound gRPC server, GC-facing auth/token-introspection paths") has no target in the current codebase; GC↔AC is HTTP. The implementer must investigate and surface: is the R-56 portion (a) genuinely N/A for AC (document + defer), or (b) realized as inbound HTTP trace-context extraction? Observability owns the R-56 design and must confirm the resolution at Gate 1.
2. **Config shape reconciliation.** `Config` already has `otlp_endpoint: Option<String>` (presence-gated on `OTLP_ENDPOINT`, config.rs:237) and the configmap already ships `OTLP_ENDPOINT`. R-55 specifies the three-field shape `otel_enabled: bool` (default false) / `otel_endpoint: String` / `otel_sample_rate: f64` (default 1.0). Reconcile: rename/restructure vs. keep presence-gating + add sample_rate. Decision affects the configmap. GC's `otel_collector_endpoint` (4318/HTTP) is the browser-telemetry proxy — distinct from this gRPC `:4317` path; do not conflate.
3. **Subscriber composition ordering.** `init_otel` returns `OtelInit { guard, layer }`; the layer must compose into the `tracing_subscriber::registry()` stack via `.with(...)` before `.init()`. Currently AC initializes the subscriber first thing in `main` (before config load). The OTel layer requires the config endpoint, so subscriber init must move after config load + `init_otel`. When `otel_enabled=false`, no layer is added (conditional `Option<Layer>` composition). `init_otel` is async and probes the collector eagerly (fail-hard at init per task #24 / Clarification Q13) — call it inside the `#[tokio::main]` runtime, hold the `OtelGuard` in `main` until shutdown.

### Common helper API (task #24, for reference)
- `async init_otel(service_name: &str, service_version: &str, environment: &str, OtelConfig { endpoint: String, sample_rate: f64 }) -> Result<OtelInit, OtelInitError>`
- `OtelInit { guard: OtelGuard, layer: OpenTelemetryLayer<Registry, Tracer> }` — compose `layer` into the subscriber, hold `guard` in `main`.
- `OtelInitError`: `ConfigInvalid` / `CollectorUnreachable` / `Sdk`. Propagate from `main` so the pod fails readiness on misconfig.
- R-56 interceptors: `otel_grpc::client_interceptor()` / `server_interceptor()` (Tonic). Note constraint #1 above re: applicability to AC.

### Guarded Shared Area note
Span enrichment on token-issuance + key-rotation paths is **automatic** via the `tracing-opentelemetry` bridge — do **NOT** modify `crates/ac-service/src/jwks/**`, `src/token/**`, `src/crypto/**`, or `src/audit/**` (GSA, ADR-0024 §6.4). Scope is `main.rs` + `config.rs` + configmap.

### Debate Decision
NOT NEEDED — full backend OTel rollout already resolved in user-story Clarification Q7; fail-hard-at-init resolved Q13.

---

## Plan (auth-controller, iteration 1)

### Resolution of Open Question 1 — R-56 inbound trace extraction

**Resolved: N/A-as-framed for AC; HTTP realization DEFERRED to an @observability-owned follow-up.**

Confirmed by recon: AC has **no** tonic dependency, no `Server::builder`, no inbound gRPC
boundary anywhere in `crates/ac-service/` or `crates/ac-test-utils/`. GC→AC is HTTP (axum).
The task's R-56 framing ("interceptor on AC's inbound gRPC server") has **no target**. The
common `server_interceptor()` (`otel_grpc.rs`) is a `tonic::service::Interceptor` and can only
attach to a Tonic server — there is nothing to wire it into.

Realizing R-56 as inbound **HTTP** trace-context extraction is **out of scope for this wiring
devloop** for three reasons:
1. No HTTP extraction middleware exists in `common` — only the two Tonic interceptors and the
   global `BoundedTraceContextPropagator`. Building an axum/tower-http W3C-extraction layer is a
   **new cross-service design** that @observability owns (it sets the pattern for #26/#6/#27).
2. There is **no producer** today: nothing injects `traceparent` over the GC→AC HTTP hop. The
   common helper ships no HTTP-client injector, and GC wiring is #26. Inbound extraction now
   would be an orphan with nothing to extract end-to-end and nothing to test against.
3. `init_otel` already registers `BoundedTraceContextPropagator` globally even for AC, so the
   propagator is **ready** the moment an HTTP-extraction middleware is designed — no rework lost.

Action: add a `docs/TODO.md` entry "R-56 inbound HTTP trace-context extraction for AC (and the
common axum/tower-http helper it needs)" owned by @observability. **@observability must confirm
this resolution at Gate 1.**

### Resolution of Open Question 2 — Config shape reconciliation

Replace the single `otlp_endpoint: Option<String>` with the R-55 three-field shape (no parallel
endpoint concepts left). Env-var surface:

| Config field | Env var | Default | Notes |
|---|---|---|---|
| `otel_enabled: bool` | `OTEL_ENABLED` | `false` | NEW. Gates `init_otel` + layer. Replaces the old "presence of endpoint = enabled" semantics. |
| `otel_endpoint: String` | `OTLP_ENDPOINT` | `""` | Reuses the existing R-59 key; honors task #28 (NOT superseded). Field `otel_endpoint` (R-55 shape), env var stays `OTLP_ENDPOINT` (OTLP wire endpoint, mirrors `OTEL_EXPORTER_OTLP_ENDPOINT`). The field≠env split is intentional: only the GATING semantics change (presence → `OTEL_ENABLED` bool); config.rs still reads `vars.get("OTLP_ENDPOINT")`. **No rename, no supersession** (@observability final lock 2026-06-25; team-lead frozen — any new naming message routes to the Lead). Zero churn on the R-59 configmap/statefulset/docker-compose/docs. |
| `otel_sample_rate: f64` | `OTEL_SAMPLE_RATE` | `1.0` | NEW. **PARSE-ONLY** at config load (non-numeric → `ConfigError`). The `[0.0, 1.0]` range has a SINGLE owner — `init_otel` (otel.rs:212), re-validated at startup on the enabled path — config does NOT duplicate it. **FROZEN (X) by team-lead 2026-06-25**: duplication-avoidance is the tiebreaker (@code-reviewer objected to a dup, @dry-reviewer flagged it; @test on parse-only + verified init_otel tests the bound). Out-of-range parses at load, hard-fails at init when enabled. |
| `environment: String` | `DEPLOYMENT_ENVIRONMENT` | `"development"` | NEW (R-55 gap @observability flagged: `init_otel` requires `environment` for the `deployment.environment` resource attr; AC had no such field). Surfaced in configmap. Mirrors the `deployment.environment` attr key directly. Exemplar for #26/#6/#27. |

Validation at config load: `otel_enabled=true` requires a non-empty `otel_endpoint` → clear
`ConfigError` (this check earns its place: clearer/earlier than init_otel's URL-parse failure).
`OTEL_ENABLED` accepts `true`/`false` (case-insensitive); other values → `ConfigError`.
`OTEL_SAMPLE_RATE` is PARSE-ONLY (non-numeric → `ConfigError`); the `[0.0, 1.0]` range is owned by
`init_otel` (single owner; NOT duplicated at config load). **FROZEN (X) by team-lead 2026-06-25.**

Error shape (per @semantic-guard review-time checks): the new `ConfigError` variants are distinct
per-field (mirroring `InvalidJwtClockSkew`/`InvalidBcryptCost`/`InvalidRateLimitConfig`) and
**include the offending non-secret env value** (the sample_rate string, the enabled string) for ops
debugging. Credential-leak path verified: the reorder makes config-load failure Debug-print to
stderr from `main`, but existing `ConfigError` variants embed no secret material (`InvalidMasterKey`/
`InvalidHashSecret` carry byte-length only; `Base64Error` a decode position) and the new OTel
variants are non-secret — no secret-bearing variant rides that path.

DRY note (@dry-reviewer): config-side `[0.0,1.0]` check DROPPED — single owner for the bound
(`init_otel`, otel.rs:212); no duplication. **Team-lead FROZE parse-only (X) 2026-06-25**, citing
duplication-avoidance as the tiebreaker. @dry-reviewer records the forward-DRY breadcrumb (hoist the three-knob config
shape when the SECOND identical sibling lands — #6 MC / #27 MH, NOT #26 GC which is the 4318/HTTP
browser-telemetry shape) in TODO.md §Cross-Service
Duplication at verdict.

Prefix split is intentional and blessed: `OTLP_ENDPOINT` names the protocol endpoint (mirrors
`OTEL_EXPORTER_OTLP_ENDPOINT` + the live collector configmap); `OTEL_ENABLED`/`OTEL_SAMPLE_RATE`
are SDK-behavior knobs; `DEPLOYMENT_ENVIRONMENT` mirrors the resource-attr key. This is the
convention #26/#6/#27 mirror.

**Enablement semantics change** (presence-of-endpoint → explicit `OTEL_ENABLED` bool) makes
`docs/runbooks/ac-service-deployment.md:471` and `docs/runbooks/gc-deployment.md:1282` stale
(both describe presence-gating, "no boolean flag"). Those runbooks are operations-owned and out
of code scope — flagged to @operations to update (or TODO).

### Resolution of Open Question 3 — Subscriber composition ordering

Restructure `main.rs`: load `Config` FIRST (before subscriber), then gate `init_otel` on
`otel_enabled`, then build the subscriber once with conditional `Option<Layer>` composition:

```rust
let config = Config::from_env()?;                 // pre-subscriber; errors -> non-zero exit (Debug to stderr)
let otel = match config.otel_config() {           // Some(OtelConfig) iff otel_enabled
    Some(cfg) => Some(init_otel("auth-controller", env!("CARGO_PKG_VERSION"),
                                &config.environment, cfg).await?),
    None => None,
};
let (otel_layer, _otel_guard) = match otel {       // hold guard in main() until shutdown
    Some(init) => (Some(init.layer), Some(init.guard)),
    None => (None, None),
};
tracing_subscriber::registry()
    .with(otel_layer)                              // Option<Layer>: None = no-op (default-off adds nothing)
    .with(EnvFilter…)
    .with(fmt::layer().json())
    .init();
```

`config.otel_config() -> Option<OtelConfig>` is a new `Config` method that encodes the gating —
trivially unit-testable (returns `None` when disabled; `Some` with the right endpoint/rate when
enabled). Note: `OtelInit.layer` is typed `OpenTelemetryLayer<Registry, Tracer>` (S=`Registry`),
so the otel layer must compose onto a bare `registry()` (applied first); will verify exact order
compiles and adjust if needed (the helper's doc example shows it last — illustrative). No new AC
Cargo deps expected: `common` is already a dependency and the layer/guard types are inferred (not
named) at the call site.

**Lost-log note:** moving `Config::from_env()` before subscriber init means a config-load failure
no longer emits a JSON `error!` log — it propagates via `?` (Debug to stderr, non-zero exit).
Documented inline; preempting code-reviewer.

### Tests
- `config.rs` (all in the existing test module, matching the bcrypt/clock-skew/rate-limit idiom):
  - Defaults: `otel_enabled=false`, `otel_endpoint=""`, `otel_sample_rate=1.0`, `environment="development"`.
  - `OTEL_ENABLED`: `true` / `false` parse; **invalid value → `ConfigError`** (error-variant assertion, NOT just "doesn't panic" / silent-false). Case-insensitive accept.
  - `OTLP_ENDPOINT` → `otel_endpoint` mapping.
  - **Sample-rate PARSE-ONLY (team-lead FREEZE X):** valid values (`0.0`/`0.5`/`1.0`) parsed+stored; non-numeric → `ConfigError` (carries offending value); out-of-range (`-0.1`/`1.1`) ACCEPTED at config load (range deferred to `init_otel`) — a test locks that intentional deferral. Range-reject coverage lives in `common`'s `init_otel` tests, not duplicated in AC.
  - `DEPLOYMENT_ENVIRONMENT` custom value.
  - `otel_enabled=true` + empty `otel_endpoint` → `ConfigError`.
  - `otel_config()` returns `None` when disabled / `Some{endpoint,sample_rate}` when enabled.
  - **(@test ask B) Gating-precedence regression:** `OTLP_ENDPOINT` set + `OTEL_ENABLED` unset/false → `otel_config()` returns `None` (endpoint populated but disabled). Locks the Q2 semantic change (presence-of-endpoint → explicit bool) against a silent refactor regression.
- shutdown-flush: **@test ACCEPTED the boundary.** Flush-on-drop is owned + tested in `common`
  (`OtelGuard::drop` + catch_unwind); AC must NOT reimplement/re-test it, and must NOT add a hollow
  `#[cfg(test)] mod` in main.rs (mock-the-impl, rejected per ADR-0034). AC's contribution is the
  structural guarantee that `_otel_guard` is held to end of `main` — a review-gate property.
  @test will verify at REVIEW: (1) guard binding is a held `_otel_guard`, NOT bare `let _ = …`
  (immediate-drop footgun); (2) existing harness/integration suites run default-off, so their green
  IS the de-facto default-off-startup coverage — the field-swaps in server_harness.rs:74 /
  test_state.rs:45 / routes/mod.rs:343,393 must keep them green.

---

## Cross-Boundary Classification

| File | Classification | Owner | Notes |
|------|----------------|-------|-------|
| `crates/ac-service/src/main.rs` | Mine / Domain-judgment | auth-controller | Reorder (config→otel→subscriber), gated `init_otel`, hold guard. |
| `crates/ac-service/src/config.rs` | Mine / Domain-judgment | auth-controller | Replace `otlp_endpoint` with 4 fields + `otel_config()` + validation + tests. |
| `crates/ac-service/src/routes/mod.rs` | Mine / Mechanical | auth-controller | Test-only `Config` literals (lines 343, 393) — field swap. |
| `crates/ac-service/tests/common/test_state.rs` | Mine / Mechanical | auth-controller | Test fixture `Config` literal (line 45) — field swap. |
| `crates/ac-test-utils/src/server_harness.rs` | Mine / Mechanical (test util) | auth-controller (test) | Test harness `Config` literal (line 74) — field swap. |
| `infra/services/ac-service/configmap.yaml` (BASE = prod base) | **Minor-judgment** | operations (owner-elected) | I edit, @operations signs off Gate 1 + Gate 3. NO rename — keep `OTLP_ENDPOINT`; ADD `OTEL_ENABLED: "false"` (base/prod-safe default), `OTEL_SAMPLE_RATE: "1.0"`, `DEPLOYMENT_ENVIRONMENT: "development"` (@operations APPROVED "development", NOT "kind"). Note wording (per @operations, precise — not bare "live"): *wired into `init_otel` (R-55), gated off by default (`OTEL_ENABLED=false`); enabling requires `OTEL_ENABLED=true` + the AC→collector egress rule (now present) + a reachable collector (fail-hard at init).* |
| `infra/kubernetes/overlays/kind/services/ac-service/kustomization.yaml` | **Minor-judgment** | operations (owner-elected) | I edit, @operations signs off. Adds `patches: - path: configmap-otel-patch.yaml` (strategic-merge mechanism, per @operations' confirmed call). |
| `infra/kubernetes/overlays/kind/services/ac-service/configmap-otel-patch.yaml` (NEW) | **Minor-judgment** | operations (owner-elected) | I edit, @operations signs off. NEW strategic-merge patch: partial `ConfigMap/ac-service-config` setting `OTEL_ENABLED: "true"` for dev (prod base stays `"false"`). Verified: `kubectl kustomize infra/kubernetes/overlays/kind` renders `OTEL_ENABLED: "true"` in dev. |
| `infra/services/ac-service/statefulset.yaml` | **Minor-judgment** | operations (owner-elected) | I edit, @operations signs off (APPROVED). `OTLP_ENDPOINT` is ALREADY wired via R-59 (task #28); per-key `configMapKeyRef` (not `envFrom`), so this devloop ADDS the 3 NEW keys (`OTEL_ENABLED`, `OTEL_SAMPLE_RATE`, `DEPLOYMENT_ENVIRONMENT`) — each needs an env entry or it orphans (`validate-env-config`). (`OTLP_ENDPOINT` entry kept; net = 4 OTel env entries, no duplicate.) |
| `infra/services/ac-service/network-policy.yaml` (BASE) | **Minor-judgment** | operations (owner-elected) | I edit, @operations co-signs (REQUIRED, raised by @operations). Default-deny egress has only PG(5432)+DNS(53); no rule to the collector on 4317 → with `init_otel` fail-hard, `OTEL_ENABLED=true` → blocked probe → CrashLoop. Add egress to `podSelector{app: otel-collector, component: observability}` (mirrors the collector ingress-from-AC selector), `protocol: TCP` / `port: 4317`; DNS egress already present. Establishes the #26/#6/#27 pattern. |
| `docs/runbooks/ac-service-deployment.md` | **Minor-judgment** | operations (owner text, applied by implementer) | APPLIED — @operations' VERBATIM text: 4 env rows (`OTEL_ENABLED`/`OTLP_ENDPOINT`[required-when-enabled]/`OTEL_SAMPLE_RATE`/`DEPLOYMENT_ENVIRONMENT`) + the "fail-hard contract" note. ONE clause corrected: ops' `OTEL_SAMPLE_RATE` "validated at config load" → "parsed at config load; range enforced by init_otel at startup" to match the frozen X (parse-only). **@operations hunk-ACKs that one-clause fix at Gate 3.** |
| `docs/runbooks/gc-deployment.md` | **Minor-judgment** | operations (owner text, applied by implementer) | APPLIED — @operations' VERBATIM break-glass bullets: (1) AC `OTEL_ENABLED=false` path (presence-gating replaced by the boolean); (2) GC/MC/MH not-yet-applicable (no `init_otel`; R-55 = #26/#6/#27). **@operations hunk-ACKs at Gate 3.** |
| `docs/TODO.md` | Mine / Mechanical | auth-controller | Add R-56 deferral under §Observability Debt (owner @observability), verbatim text: "R-56 inbound HTTP trace-context extraction for AC — build the shared `common` axum/tower-http W3C-extraction layer (the GC→AC HTTP hop's consumer), coordinated with GC #26's inbound-HTTP work; AC then wires it. Blocked-by: a producer injecting `traceparent` on GC→AC (GC #26). **Security constraint (@security): the extraction layer MUST read only `traceparent`/`tracestate` — never `authorization` or any bearer metadata, and never copy request headers into span attributes.**" PLUS a prod-graduation line (per @operations point 4): "AC `DEPLOYMENT_ENVIRONMENT` is hardcoded `development` in the prod base configmap — prod OTel graduation MUST overlay-patch it so prod telemetry isn't mistagged." |
| `crates/common/src/observability/otel.rs` | **Minor-judgment** | observability (owner-elected) | **POST-REVIEW addition (user direction)**: doc-comment-only fix folding in @code-reviewer's Gate-3 spin-out / @observability's doc-nit deferral — reorder the `ignore`d caller-wiring example to compose `otel.layer` FIRST onto the bare `registry()`, with a comment on the `OpenTelemetryLayer<Registry, Tracer>` = `Layer<Registry>` constraint, so #26/#6/#27 don't trip. No logic change; the `ignore` block never compiles in CI. `crates/common/**` outside GSA (not in the §6.4 list); fix spec authored by @code-reviewer, owner @observability. Carried as `Approved-Cross-Boundary: observability` trailer. |

### REQUIRED commit trailer (team-lead directive)
The eventual commit MUST carry:
```
Approved-Cross-Boundary: operations — AC OTel env keys + statefulset configMapKeyRef wiring + collector egress rule + kind-overlay enable + runbook updates; values/scope per ops Gate-1 spec
```
RESOLVED (team-lead + @operations): single Gate-3 commit authored by the team-lead (`git add -A`)
under this trailer. It covers ALL operations-owned edits — configmap.yaml, statefulset.yaml,
network-policy.yaml, the Kind-overlay enable, AND the 2 runbooks. @operations supplies the exact
runbook text, I paste it into the working tree (one writer), @operations hunk-ACKs at Gate 3. No
separate operations commit. (Trailer wording updated 2026-06-25 per @operations to add "kind-overlay
enable" + "runbook updates".)

**RESOLVED — overlay mechanism (team-lead ruling 2026-06-25):** **(a) 2-file strategic-merge stands**
(`kustomization.yaml` `patches: - path: configmap-otel-patch.yaml` + the partial-ConfigMap patch file) —
the idiomatic kustomize pattern for patching `data` (JSON6902 on ConfigMaps interacts awkwardly with
name hashing); output-identical to inline, so the lead froze the working version to stop churn. Both
table rows stay. @operations may raise a substantive (non-form) objection at Gate 3 if any.

### Dev-enablement decision — RESOLVED by @operations
Code default stays `otel_enabled=false` (covers any env without the var — local dev, tests,
partial clusters). Layered config:
- **Base** `infra/services/ac-service/configmap.yaml` (= prod base, `AC_CLUSTER_NAME: dark-tower-prod`):
  `OTEL_ENABLED: "false"` — prod-safe default; OTel is dev-only + default-off today.
- **Kind overlay** patches `OTEL_ENABLED: "true"` so dev is enabled end-to-end (collector via R-59 +
  egress rule lands here). Pending @operations confirm of the patch mechanism + in-scope-this-devloop.

`DEPLOYMENT_ENVIRONMENT: "development"` in the base is @operations-APPROVED ("development", not the
cluster-type "kind"). **Caveat → `docs/TODO.md`:** the base is the prod base, so a hardcoded
"development" will mistag prod telemetry once prod OTel lands; prod graduation MUST overlay-patch
`DEPLOYMENT_ENVIRONMENT`. (Not this devloop's work — OTel is dev-only + default-off today.)

---

## Accepted Deferrals

- **R-56 inbound HTTP trace-context extraction for AC** — N/A as gRPC (no AC gRPC boundary; confirmed N/A by @observability at Gate-1; the global W3C propagator is registered by `init_otel`). Full entry + @security constraint tracked in `docs/TODO.md` §Observability Debt.
- **AC `DEPLOYMENT_ENVIRONMENT` prod-graduation overlay-patch** — pointer bullet; full entry in `docs/TODO.md` §Observability Debt (owner operations).
- **`init_otel` doc-example composes the layer LAST** — ~~deferred~~ **RESOLVED in-commit** (post-review, at user direction): `crates/common/src/observability/otel.rs` doc example reordered to compose `otel.layer` first, with a comment explaining the `Layer<Registry>` type constraint. No longer a deferral; TODO entry removed. (observability-owned helper; carried as an `Approved-Cross-Boundary: observability` trailer.)
- **Per-service R-55 OTel `main.rs` + config-parse scaffold extraction** — DRY forward-extraction breadcrumb (not duplication; cannot extract at N=1); full entry in `docs/TODO.md` §Cross-Service Duplication. Hoist trigger: the second three-knob sibling (#6 MC or #27 MH).

---

## Final Summary

AC is the **first per-service R-55 OTel wiring** — the exemplar for #26 (GC), #6 (MC), #27 (MH). `crates/ac-service/src/main.rs` loads `Config` first, gates `init_otel(...)` on `config.otel_config()` (default-off composes zero layers / no collector probe), composes the OTel layer first onto the bare `registry()`, and holds the named `_otel_guard` to end of `main` (flush-on-drop). `Config` gains `otel_enabled` / `otel_endpoint` (env `OTLP_ENDPOINT`) / `otel_sample_rate` (parse-only; bound owned by `init_otel`) / `environment`, surfaced in the configmap (base `OTEL_ENABLED=false`, Kind overlay → `true`), statefulset (per-key `configMapKeyRef`, no orphans), and a new collector-egress NetworkPolicy rule (TCP 4317). **R-56 is N/A-as-gRPC** for AC (no inbound gRPC boundary); the global W3C propagator is registered by `init_otel`, HTTP inbound extraction deferred to GC #26. GSA paths (jwks/token/crypto/audit) untouched — span enrichment is automatic via the tracing-opentelemetry bridge.

**Gate 2:** layers 1–5 OK (compile/format/guards/test 159s/lint); layer 6 audits passed; layer 7 wave2-pending. A pre-existing `inline_debt_body` violation in task #12's devloop doc was cleared in a standalone commit (`2dc5ca7`).

**Files changed (12 + new overlay patch):** config.rs, main.rs, routes/mod.rs, tests/common/test_state.rs, ac-test-utils/server_harness.rs, docs/TODO.md, runbooks/ac-service-deployment.md, runbooks/gc-deployment.md, infra overlay kustomization.yaml + configmap-otel-patch.yaml, configmap.yaml, network-policy.yaml, statefulset.yaml.

**Commit trailer:** `Approved-Cross-Boundary: operations` (7 ops-owned files, Minor-judgment).
