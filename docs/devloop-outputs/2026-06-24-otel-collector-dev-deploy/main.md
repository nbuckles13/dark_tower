# Devloop Output: Dev OTel Collector Deployment (R-59)

**Date**: 2026-06-24
**Task**: Deploy a dev OTel collector to the Kind cluster (Kustomize base + kind overlay), wire AC/GC/MC/MH otel endpoints, add OTel export-failure alert, document collector-upgrade discipline.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2)
**Branch**: `feature/browser-client-join-task-28`
**Duration**: ~4h (planning start 2026-06-24 ~21:34 → commit 2026-06-25 ~01:36)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `5ea7d720da6a37b22ef4230e4040d3d8cbac7daf` |
| Branch | `feature/browser-client-join-task-28` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-FIXED` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |
| Semantic Guard | `CLEAR` |

<!-- Gate 1 PASSED 2026-06-24: all 7 reviewers confirmed. Classification-sanity:
     dt-guard binary not pre-built (REASON=dt-guard-binary-missing) → Lead manual
     fallback per SKILL/ADR-0024 §6.8 #1. Manual check PASS: no GSA paths; all
     Minor-judgment rows (otel-alerts.yaml, gc-deployment.md, env-tests cluster_health)
     have owners who are reviewers on this loop. config.rs deferred (E) — not in diff. -->
<!-- Gate 2 note: run-guards.sh/layer-all.sh build dt-guard; ensure that happens before validation. -->
<!-- GATE 2 DISPOSITION (2026-06-25): R-59's own changes PASS all applicable layers
     (L1 compile, L2 fmt, L4 test, L6 audit OK; L7 N/A=wave2-pending). Attempt 1 found
     two R-59 issues — both FIXED: (a) validate-env-config orphan keys → wired OTLP_ENDPOINT
     into ac statefulset + OTEL_COLLECTOR_ENDPOINT into gc deployment; (b) env-test timeout race.
     Remaining RED is SOLELY pre-existing sdk-core branch debt, NOT R-59:
       - L3 validate-todo-tracking: inline-debt-body in 2026-06-23-sdk-core main.md:428
       - L5 nx-lint sdk-core:lint: prettier on 3 committed sdk-core TS files (our diff touches 0 TS)
     The Lead's attempt to fix the todo-tracking item by editing the sdk-core main.md tripped the
     cross-boundary-scope `2-main-mds` collision guard (a fix cannot live in another loop's main.md),
     so that edit was REVERTED. Per USER DECISION (2026-06-25): proceed to review + commit R-59 with
     the pipeline red on pre-existing items only; user is fixing the sdk-core debt IN PARALLEL;
     do NOT add a docs/TODO.md entry for it. -->



---

## Task Overview

### Objective
Deploy a development OpenTelemetry collector into the Kind cluster (R-59, task #28 of the
browser-client-join story) so AC/GC/MC/MH OTLP-gRPC traces and the GC `/api/v1/telemetry`
OTLP-HTTP proxy path have an in-cluster sink. Dev exporters only (logging/debug); no
production exporters this story. Add the runtime export-failure alert and document
collector-upgrade discipline.

### Scope
- **Service(s)**: New `otel-collector` workload; ConfigMap value wiring on AC/GC/MC/MH; GC `otel_collector_endpoint`.
- **Schema**: No.
- **Cross-cutting**: Yes — touches infra manifests, prometheus alert rules, a runbook, and the Kind setup script.

### Debate Decision
NOT NEEDED — design fully specified by R-59 + story task #28; building within existing Kustomize/Kind patterns.

### Requirements (R-59 + story §infrastructure Task INFRA-OTEL)
1. `infra/services/otel-collector/` Kustomize base: `deployment.yaml` (`otel/opentelemetry-collector-contrib`, resource limits, readiness probe on `:13133`), `configmap.yaml` (OTLP-gRPC `:4317` + OTLP-HTTP `:4318` receivers; `logging` + `debug` exporters), `service.yaml` (ClusterIP 4317/4318/13133), `network-policy.yaml`, `kustomization.yaml`.
2. Kind overlay `infra/kubernetes/overlays/kind/services/otel-collector/` registered in `overlays/kind/services/kustomization.yaml`.
3. AC/GC/MC/MH `otel_endpoint` Kustomize values point at the collector gRPC service; GC `otel_collector_endpoint` points at the HTTP `/v1/{metrics,traces}` path.
4. `infra/kind/scripts/setup.sh` verifies collector readiness BEFORE AC/GC/MC/MH deploy (load-bearing under R-54 fail-hard-at-init).
5. `infra/docker/prometheus/rules/otel-alerts.yaml` (new) with `OTelExportFailureRate` warn alert (`rate(dt_otel_export_failures_total[5m]) > 0 for 10m`).
6. Collector-upgrade-discipline note in `docs/runbooks/gc-deployment.md`.

### Lead pre-flight findings (verify during planning)
- **Namespace mismatch**: all services live in namespace `dark-tower`, NOT `default`. The task text's
  `otel-collector.default.svc.cluster.local` is wrong for this cluster — use the `dark-tower` namespace
  (`http://otel-collector.dark-tower.svc.cluster.local:4317` / `:4318`, or the short `otel-collector.dark-tower:PORT`
  form used by existing ConfigMaps e.g. `ac-service.dark-tower:8082`). **Confirm with observability/operations.**
- **setup.sh wiring**: add `deploy_otel_collector` to `main()` ordered AFTER `deploy_observability` /
  `deploy_redis` and BEFORE `deploy_ac_service`; readiness gate via `kubectl wait --for=condition=Ready pod -l app=otel-collector`.
  Also add an `otel` case to `deploy_only_service()`. `preload_third_party_images()` auto-extracts images from the
  rendered kind overlay, so no manual image-preload edit is needed once the overlay is registered.
- **Prometheus rule loading**: `infra/docker/prometheus/rules/*.yaml` is the existing home (gc/mc/mh-alerts.yaml).
  New `otel-alerts.yaml` must pass `validate-alert-rules.sh` (severity taxonomy {page,warning,info}; `runbook_url`
  MUST be repo-relative under `docs/runbooks/` and the anchor must exist). Pick/author a real runbook anchor
  (candidate: the new collector-upgrade-discipline section in `gc-deployment.md`).
- **R-55 overlap**: per-service `OTEL_*` ConfigMap *keys* are R-55's deliverable (backend OTel tasks). This task
  populates/sets the endpoint *values*. Check whether the keys already exist before adding them to avoid collision.

---

## Cross-Boundary Classification

<!-- Implementer fills this during planning; Lead validates at Gate 1. -->

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `infra/services/otel-collector/**` | Mine | — |
| `infra/kubernetes/overlays/kind/services/otel-collector/**` | Mine | — |
| `infra/kubernetes/overlays/kind/services/kustomization.yaml` | Mine | — |
| `infra/kind/scripts/setup.sh` | Mine | — |
| `infra/services/ac-service/configmap.yaml` (`OTLP_ENDPOINT` value) | Mine (infra manifest); KEY=`OTLP_ENDPOINT` is AC's live reader (config.rs:237), value semantics confirmed w/ @observability | infrastructure |
| `infra/services/ac-service/statefulset.yaml` (`OTLP_ENDPOINT` env `configMapKeyRef`) | Mine (infra manifest) — added Gate-2: AC uses per-key env refs not envFrom, so the ConfigMap key must be wired into the workload or it's orphaned (`validate-env-config`) | infrastructure |
| `infra/services/gc-service/configmap.yaml` (`OTEL_COLLECTOR_ENDPOINT` value) | Mine (infra manifest); KEY is GC's live R-2 reader (config.rs:288), bare-base value confirmed w/ @observability | infrastructure |
| `infra/services/gc-service/deployment.yaml` (`OTEL_COLLECTOR_ENDPOINT` env `configMapKeyRef`) | Mine (infra manifest) — added Gate-2: wires the key into the GC workload (also the wiring that points the R-2 telemetry proxy at the collector) | infrastructure |
| `infra/docker/prometheus/rules/otel-alerts.yaml` | Not mine, Minor-judgment (alert rule ↔ runbook coupling); ships DORMANT — emitter is R-54/R-55's (Adjudication A1) | operations / observability |
| `docs/runbooks/gc-deployment.md` | Not mine, Minor-judgment (runbook prose) | operations |
| `docs/TODO.md` | Mine (one-line forward-dependency pointer: R-55 emitter activates the dormant alert) | infrastructure |
| `crates/env-tests/tests/00_cluster_health.rs` (collector-readiness assertion) | Not mine, Minor-judgment — @test REQUESTED it + specified the exact shape/scope; I author, test owns the tier | test |

<!-- MC/MH configmap rows DROPPED per team-lead B1: no reader on this branch, deferred to R-55 — those files -->
<!-- are NOT in this task's diff, so they must not appear here or the Gate-2 Layer-A scope-drift guard flags it. -->
<!-- crates/common/src/observability/otel.rs counter (A2) REJECTED by team-lead — not touched; stays observability/R-54. -->
<!-- None of these paths are Guarded Shared Areas (no proto/crypto/migration/webtransport). -->

---

## Planning

### Decisions (verified against the branch, not just the brief)

- **Namespace**: `dark-tower` (confirmed — all services live there; `default` in the task text is wrong).
  Collector DNS: short form `otel-collector.dark-tower:PORT` (matches existing `ac-service.dark-tower:8082`
  / `gc-service.dark-tower:50051` convention in the configmaps).
- **R-55 keys do NOT exist yet on this branch.** Only GC's `OTEL_COLLECTOR_ENDPOINT` env var exists
  (`crates/gc-service/src/config.rs:288`, default points at the wrong `default` ns — I OVERRIDE via ConfigMap
  value, not by editing the Rust default). The per-service `otel_endpoint` / `otel_enabled` / `otel_sample_rate`
  (R-55) Rust config readers are NOT on this branch, so no env keys to collide with. I will add ConfigMap KEYS
  `OTEL_ENDPOINT`, `OTEL_ENABLED`, `OTEL_SAMPLE_RATE` (unprefixed, matching GC's unprefixed `OTEL_COLLECTOR_ENDPOINT`).
  **@observability: confirm these three key names + that `otel_endpoint` wants the bare gRPC base
  `http://otel-collector.dark-tower:4317`** (R-54 builds an OTLP-gRPC exporter from it).
- **Image pin**: `otel/opentelemetry-collector-contrib:0.103.1` (specific tag, not `latest` / `0.x`).
- **Alert severity is `warning`** (not "warn") — the guard's taxonomy is `{page, warning, info}`.
- **Alert `runbook_url`** points at the NEW collector-upgrade-discipline section in `gc-deployment.md`
  (anchor `#otel-collector-upgrade-discipline`). The guard checks the file exists + path is under `docs/runbooks/`;
  I author a heading whose GitHub slug matches the anchor so it genuinely resolves.
- **Prometheus rule wiring**: out of scope — dev `prometheus.yml` `rule_files` is commented out for ALL existing
  alert files (gc/mc/mh). The deliverable is the file + guard pass, matching the existing pattern. Not touching
  prometheus.yml.

### DESIGN FORK RESOLVED: kind sets `OTEL_ENABLED=true` (not R-55's `false` default)

**Choice**: the kind overlay ConfigMaps set `OTEL_ENABLED="true"` for AC/GC/MC/MH.

**Verified fact that de-risks this**: `init_otel` is NOT called by any service `main.rs` on THIS branch
(`grep -rn init_otel crates/*/src/main.rs` → empty; R-54's helper exists in `common`, but R-55 per-service
wiring has not landed). So on this branch `OTEL_ENABLED` is read by NOBODY — it is inert today.

**Rationale**:
1. R-59/INFRA-OTEL intent is a *working* dev sink, not a dormant one — the story says populate the endpoint
   values pointing at the collector.
2. On this branch there is **zero CrashLoop risk** (no R-55 reader connects), so enabling it now cannot red
   Layer-7 env-tests today. The setup.sh readiness gate is correctly ordered but exercises nothing yet.
3. When R-55 lands and rebases on top, `OTEL_ENABLED=true` + collector Ready-gated BEFORE AC/GC/MC/MH is
   exactly the ordering that makes R-54 fail-hard-at-init SAFE. Leaving it `false` would force R-55's author to
   come back and flip it, and would make my gate decorative until then — a worse seam to hand off.
4. **Why the gate earns its place even while inert**: it is the guardrail that makes "enabled" safe across the
   R-55 rebase (collector Ready → services boot → init_otel connects). It is load-bearing for the NEXT change,
   deliberately landed now so R-55 inherits a correct bring-up order instead of having to add it.

**Residual risk + mitigation**: a wrong namespace/label/timeout in the gate would CrashLoop all four services
*once R-55 is live*. Mitigated by Gate-2 local validation: `kubectl kustomize overlays/kind/` render +
`kubeconform`, and verifying the Deployment's `app=otel-collector` label exactly matches the
`kubectl wait -l app=otel-collector` selector. The bug, if any, surfaces a full devloop before it can bite.

**Confirm-against**: @test (Layer-7 implication) + @operations (bring-up ordering). Resolving with @test directly.

### ADJUDICATIONS — RULED by team-lead 2026-06-24: A1 APPROVED (A2 rejected), B1 APPROVED

**SCOPE-SPLIT + FORWARD-DEPENDENCY NOTE (not a deferral — a deliberate cross-owner split):**
- The `dt_otel_export_failures_total` counter EMISSION is **R-54/R-55 / observability's deliverable**, NOT
  this task's. R-59 scopes infra to the ALERT FILE only. `otel.rs:33-37` forbids registering the counter in
  that module. So `otel-alerts.yaml` ships **DORMANT**: valid rule, never fires until R-55 wires `init_otel`
  into a service `main.rs` and R-54's emitter increments the counter.
- Forward-dependency closure: a one-line pointer goes in **`docs/TODO.md`** so R-55 closes the loop
  (emitter lands → alert goes live). Runbook documents the dormant state.
- MC/MH otel ConfigMap keys are **deferred to their R-55 tasks** — no reader on this branch; minting
  `OTEL_ENABLED`/`OTEL_ENDPOINT`/`OTEL_SAMPLE_RATE` placeholders would risk naming-drift vs R-55's eventual
  readers. AC (`OTLP_ENDPOINT`) + GC (`OTEL_COLLECTOR_ENDPOINT`) are the only real readers → only they get
  values now. Cross-Boundary table drops the MC/MH rows accordingly (they are NOT in the diff).
- **Pending: @observability explicit co-sign of A1 (dormant framing) + B1 (MC/MH deferral)** before team-lead
  issues "Plan approved".

**A. The `OTelExportFailureRate` alert references a metric NO service emits.**
`crates/common/src/observability/otel.rs:33-37` defers `dt_otel_export_failures_total` to "task #28" (this
task) AND explicitly says *"Do not add per-failure logging or a phantom metric registration in this module."*
The counter would be a BatchExporter error-hook in that observability-owned `common` module — Rust code my
infra-only brief did NOT scope to me, and the BatchExporter only exists at runtime once R-55 wires
`init_otel` into a service `main.rs` (not landed). Two coherent options:
  - **(A1) Ship the alert now; counter lands with the emitter.** The alert file is the infra deliverable; the
    counter is observability Rust + needs the R-55 runtime path. Document in the runbook that the alert is
    dormant until the counter+R-55 land. Risk: a dormant alert for a few devloops.
  - **(A2) Pull the counter into THIS task.** Expands infra-only scope into observability `common` Rust,
    contradicts the otel.rs module-doc directive, and has no runtime exercise until R-55. Needs @observability
    to own/co-sign the Rust edit + team-lead scope expansion.
  **Recommendation: A1.** Until resolved I implement the alert file but flag it dormant in runbook prose.

**B. Per-service env-var KEY names are NOT uniform — corrected against the branch.**
Verified readers on THIS branch:
  - **AC**: reads `OTLP_ENDPOINT` (`config.rs:237`, `Option<String>` — *presence* = enabled; NO `OTEL_ENABLED`,
    NO `OTEL_SAMPLE_RATE` for AC today).
  - **GC**: reads `OTEL_COLLECTOR_ENDPOINT` (`config.rs:288`, the R-2 proxy base). GC's own gRPC-export
    `otel_endpoint` is R-55, not landed.
  - **MC / MH**: read NO OTLP env var today (R-55 not landed).
  My earlier uniform `OTEL_ENABLED`/`OTEL_ENDPOINT`/`OTEL_SAMPLE_RATE` assumption was WRONG. Revised wiring:
  - AC configmap: `OTLP_ENDPOINT: "http://otel-collector.dark-tower:4317"` (real reader, scheme required per
    `Endpoint::from_shared` at otel.rs:221).
  - GC configmap: `OTEL_COLLECTOR_ENDPOINT: "http://otel-collector.dark-tower:4318"` (bare base, no `/v1/...`;
    forwarder appends; real reader).
  - MC/MH: forward-compat placeholders ONLY — I must NOT invent key names that won't match R-55.
  **Recommendation: AC + GC get real values now; MC/MH deferred to their R-55 tasks (B1)** unless
  @observability gives the exact R-55 key names to use as placeholders.

  **@observability ANSWERED (2026-06-24)**: key spelling is **`OTLP_ENDPOINT`** (OTLP, not OTEL) — the live
  key AC + the common config loader read today; R-55 MC/MH will reuse the common loader, so `OTLP_ENDPOINT` is
  the safe forward-compat key. `OTEL_ENABLED` / `OTEL_SAMPLE_RATE` are net-new with NO current reader — OK as
  forward-compat but MUST be flagged as not-yet-load-bearing. **RESOLUTION: B1 + `OTLP_ENDPOINT`.** AC gets
  `OTLP_ENDPOINT` (real). GC gets `OTEL_COLLECTOR_ENDPOINT` (real). MC/MH: defer the endpoint key to R-55 (their
  tasks own the reader); I will NOT mint `OTEL_ENABLED`/`OTEL_SAMPLE_RATE` phantom keys this task. (Awaiting
  team-lead confirm on the MC/MH defer.)

**C. Endpoint forms (confirmed by @observability):** gRPC values MUST include `http://` (parsed via
`Endpoint::from_shared`; bare host:port → `ConfigInvalid` fail-hard at init); GC proxy value is the BARE base
(no `/v1/...`). Folded into B.

**D. Collector exporter nit (@observability) — RESOLVED-MOOT at Gate-3 (dropped `logging`).** Planning kept both
`debug` + `logging` for literal R-59 spec-match. At Gate-3 (team-lead priority + @test finding) I verified the
ACTUAL pinned-image behavior: `logging` was deprecated at collector v0.86.0 and REMOVED at v0.111.0 (replaced
by `debug`). On the pinned `0.103.1` it still loads (warning-only, no CrashLoop), but it's a redundant alias +
a future-bump CrashLoop trap, so I DROPPED it — `debug`-only is the forward-safe dev config. This also resolves
@code-reviewer's double-export FYI. Adjudication D is now moot (no two-exporter question remains).

**E. GC Rust constant `DEFAULT_OTEL_COLLECTOR_ENDPOINT` is WRONG (caught by @dry-reviewer) — team-lead RULED:
DEFER-with-owner (global-controller / R-55); do NOT fix in this loop.** `crates/gc-service/src/config.rs:34-35` =
`"http://otel-collector.default.svc.cluster.local:4318"` — the compiled-in FALLBACK for the same
`OTEL_COLLECTOR_ENDPOINT` env var I'm setting in the GC ConfigMap. Two defects: (i) wrong namespace `default`
(should be `dark-tower`); (ii) long FQDN form. Since the ConfigMap SETS the env var, the constant is the unused
fallback in Kind today → the divergence is masked at runtime, but it is a latent footgun and the constant's own
doc says it's "Contract with R-59/INFRA-OTEL" (i.e. coupled to THIS task).
  - **Endpoint-form decision (DRY ask #1)**: ConfigMaps use the SHORT `otel-collector.dark-tower:43xx` form
    (matches sibling endpoints `ac-service.dark-tower:8082` etc.; pod search-domain resolves it). For the Rust
    DEFAULT constant I'd keep the FQDN form but FIX the namespace → `otel-collector.dark-tower.svc.cluster.local
    :4318` (a default constant used when no env var is set should be fully-qualified, not search-domain-reliant).
    These then AGREE on scheme+namespace+port+bare-base; short-vs-FQDN is semantically equivalent in-cluster.
  - **Scope (DRY ask #2)** — **team-lead RULED: DEFER-with-owner. Do NOT fix `config.rs` in this loop.**
    Routing: `config.rs` is global-controller's domain; a namespace change is value-semantics (Minor-judgment)
    requiring the owner present, and GC is not a reviewer here. No urgency: the constant is the unused fallback
    (ConfigMap always sets the env var), so runtime impact on this branch is zero. Natural home: the GC
    R-55/backend-otel task already edits `config.rs` for OTel config. So: I do NOT touch `config.rs`; I add a
    **docs/TODO.md** line citing `config.rs:34-35`, the wrong value (`...default.svc.cluster.local:4318`) and
    the corrected `http://otel-collector.dark-tower.svc.cluster.local:4318`, owner = global-controller. This is
    a scope-boundary / forward-dependency note (a pre-existing GC bug routed to its owner), NOT an §Accepted
    Deferral of a finding in THIS task's diff. My ConfigMap value stays the canonical-correct endpoint (the real
    R-59 deliverable).

**F. Pre-existing sdk-core branch debt — REVERTED my attempted fix; handled by user in parallel (Gate-2).**
`./scripts/layer-all.sh` RED is SOLELY pre-existing sdk-core branch debt, NOT R-59: L3 `validate-todo-tracking`
(`docs/devloop-outputs/2026-06-23-sdk-core-http-api-error-hierarchy/main.md` inline body under §Accepted
Deferrals) + L5 `nx-lint sdk-core:lint` (prettier on 3 committed sdk-core TS files — our diff touches 0 TS).
Both fail on a clean tree; neither is R-59.
- I initially relocated the inlined prose out of that loop's §Accepted Deferrals to clear L3. **Team-lead
  REVERTED that edit**: fixing todo-tracking by editing a SECOND devloop's main.md tripped the
  `validate-cross-boundary-scope` `2-main-mds` collision guard (a fix may not live inside another loop's
  main.md). That file is back to its committed state — it is NOT in the R-59 diff.
- **USER DECISION**: proceed to review + commit R-59 with the pipeline RED on these pre-existing items only.
  The user is fixing the sdk-core debt **IN PARALLEL** and explicitly does NOT want a `docs/TODO.md` entry for
  it. So: no sdk-core line in `docs/TODO.md` (verified absent), no sdk-core file in the R-59 diff, and R-59's
  OWN changes pass every applicable layer (L1/L2/L4/L6 OK; L7 N/A wave2-pending).
- Net: this item is NOT an R-59 deliverable, NOT a deferral, and leaves NO artifact in this diff. Recorded here
  only as the Gate-2 disposition trail.

### Deliverables / file plan

1. `infra/services/otel-collector/` base (redis pattern, ns `dark-tower`):
   - `configmap.yaml` (`otel-collector-config`): receivers `otlp` (grpc `0.0.0.0:4317`, http `0.0.0.0:4318`);
     exporter `debug` ONLY (verbosity: **normal**); extensions `health_check` (`0.0.0.0:13133`);
     `service.extensions: [health_check]`; pipelines `traces` + `metrics` (both receivers→`[debug]`).
     **@test/@code-reviewer/@team-lead Gate-3 — `logging` exporter DROPPED (verified against the pinned image)**:
     R-59 item 1 named both `debug` + `logging`, but the `logging` exporter was DEPRECATED upstream at collector
     v0.86.0 and REMOVED at v0.111.0 (last present v0.110.0), replaced by `debug`. The pinned image is `0.103.1`
     → `logging` STILL loads there (between v0.86.0 and v0.111.0), so it's a warning-only deprecation, NOT a
     startup error on this version (no bring-up CrashLoop risk on 0.103.1 — verified via the upstream removal
     announcement, not the spec text). BUT it's a redundant alias of `debug`, adds deprecation log noise, AND
     becomes a hard startup CrashLoop the moment the image is bumped past 0.111.0. So `debug`-only is the
     correct, forward-safe dev config; this also resolves @code-reviewer's double-export FYI and makes
     **Adjudication D moot**.
     **@security Gate-3 finding FIXED (option a)**: `verbosity` is `normal`, NOT `detailed`. `detailed` dumps
     every span attribute to the pod log → once R-55 makes services emit, PII/secrets on a span would land in
     the collector log in cleartext, and `test_secrets_not_in_logs` only scans ac-service (not the collector
     pod), so the leak would be unguarded. `normal` removes the leak path while keeping span names/counts for
     dev debugging — fix inside this changeset, no TODO needed.
   - `deployment.yaml`: `otel/opentelemetry-collector-contrib:0.103.1`, `--config=/etc/otelcol-contrib/config.yaml`,
     resource req/limits, readiness+liveness probe httpGet `:13133/`, non-root securityContext (matching redis:
     `runAsNonRoot`, `runAsUser: 10001`, `readOnlyRootFilesystem`, `allowPrivilegeEscalation: false`, drop ALL),
     config mounted from ConfigMap, `tmp` emptyDir.
   - `service.yaml`: ClusterIP exposing 4317 (otlp-grpc) / 4318 (otlp-http) / 13133 (health).
   - `network-policy.yaml`: Ingress 4317 from {ac,gc,mc,mh}-service pods; ingress 4318 from gc-service pod only;
     ingress 13133 from same ns (probes); egress DNS only.
   - `kustomization.yaml`: lists the four manifests + `managed-by` label (redis shape).
2. Kind overlay `infra/kubernetes/overlays/kind/services/otel-collector/kustomization.yaml` (redis-overlay shape)
   + register `otel-collector/` in `overlays/kind/services/kustomization.yaml`.
3. ConfigMap value wiring (REVISED per @observability adjudication B — only real readers, `OTLP_ENDPOINT`
   spelling, MC/MH deferred):
   - AC `configmap.yaml`: add `OTLP_ENDPOINT: "http://otel-collector.dark-tower:4317"` (live reader,
     `config.rs:237`; presence = enabled; scheme required).
   - GC `configmap.yaml`: add `OTEL_COLLECTOR_ENDPOINT: "http://otel-collector.dark-tower:4318"` (bare base, NO
     `/v1/...` path). **VERIFIED (re @test's /v1 question)**: `telemetry_forwarder.rs:3-5,33-37` takes the bare
     base and APPENDS `Signal::suffix()` = `/v1/metrics` or `/v1/traces` itself. So base-only is functionally
     correct — the spec's "/v1/{metrics,traces}" phrasing describes the EFFECTIVE forward target after
     concatenation. A value WITH `/v1/...` would double-append (`/v1/metrics/v1/metrics`) → 404. Base-only = right.
   - MC/MH: NO otel keys this task — deferred to their R-55 tasks (no reader on this branch; minting keys risks
     mismatch with R-55). **CONFIRMED by team-lead (B1 accepted)**: this legitimately narrows the brief's
     "AC/GC/MC/MH" to "AC/GC real now, MC/MH at R-55".
   - NOTE: the earlier "OTEL_ENABLED=true" design-fork resolution is now MOOT for the actual wiring — AC enables
     by *presence* of `OTLP_ENDPOINT` (no boolean), and MC/MH are deferred. The setup.sh readiness gate still
     lands now (bring-up-critical once R-55 makes MC/MH read the collector).
4. `infra/kind/scripts/setup.sh`: new `deploy_otel_collector()` (kubectl apply -k overlay + `kubectl wait
   --for=condition=Ready pod -l app=otel-collector -n dark-tower --timeout=120s`); call it in `main()` AFTER
   `deploy_observability` / BEFORE `create_ac_secrets`+`deploy_ac_service`; add `otel)` case to
   `deploy_only_service()`.
5. `infra/docker/prometheus/rules/otel-alerts.yaml` (NEW): group `otel-pipeline-warning`, alert
   `OTelExportFailureRate`, `expr: rate(dt_otel_export_failures_total[5m]) > 0`, `for: 10m`, `severity: warning`,
   per-service via `by (service)` so `{{ $labels.service }}` annotates which service degraded,
   `runbook_url: docs/runbooks/gc-deployment.md#otel-collector-upgrade-discipline`. **DORMANT pending
   Adjudication A1 (team-lead RULED; A2 rejected)** — `dt_otel_export_failures_total` is not emitted by any
   code on this branch; the alert ships now as a valid-but-DORMANT rule and the runbook notes it activates once
   R-54's emitter + R-55 wiring land. I do NOT touch `common/observability/otel.rs` (counter is R-54/R-55's,
   pending @observability co-sign).
   **@operations confirmed (constraints):** all three annotations present (summary/description/impact) with
   `{{ $labels.service }}` + `{{ $value }}` templating; **NO collector FQDN/host text in ANY annotation** —
   refer to it by role ("the in-cluster OTel collector") so the hygiene denylist can't trip (operations asserts
   `.svc.cluster.local` is rejected; complying removes all risk regardless of guard version); runbook anchor
   slug kept in lockstep with the header.
6. `docs/runbooks/gc-deployment.md`: new `## OTel Collector Upgrade Discipline` section. Must satisfy
   @operations' Gate-3 checklist (a)-(f) — the section is the alert's `runbook_url` target, so NOT a stub:
   - (a) **Causal chain**: R-54 fail-hard-at-init → botched collector image/config upgrade → all four services
     fail init simultaneously → fleet-wide CrashLoopBackoff.
   - (b) **Mitigation**: collector upgrades in a SEPARATE change window from service deploys; verify collector
     Ready independently before/without rolling services.
   - (c) **Triage** (the key step): distinguish RUNTIME degradation (`OTelExportFailureRate` warn alert by
     name — spans dropping, data plane UP) from INIT-time collector failure (fleet-wide pod Unready/CrashLoop —
     services DOWN).
   - (d) **Rollback**: revert collector image tag.
   - (e) **Break-glass / escape hatch**: disable per-service OTel init to unblock a service deploy that can't
     wait for a separate window. **Branch-accurate form**: TODAY = remove/blank AC's `OTLP_ENDPOINT` (enable is
     by presence, no boolean yet); note that R-55 will add the `otel_enabled=false` no-op path as the canonical
     break-glass once wiring lands.
   - (f) **Escalation/ownership**: collector is owned by infrastructure; a fleet-wide CrashLoop from this cause
     is a deployment-discipline incident, not a service bug.
   - **DORMANT-alert note** (team-lead + @observability A1 condition 1 — put it IN the alert's own runbook
     anchor, not buried): exact sentence — "This alert is dormant until `dt_otel_export_failures_total` is
     emitted by the service OTel exporter init (R-55); until then it cannot fire." So an operator who sees the
     alert isn't sent chasing a non-firing alert.
   - **Blast-radius note** (operations, explicit honesty sentence requested): ON THIS BRANCH the collector is
     enabled-but-inert — no service `main.rs` calls `init_otel` yet (R-55 not wired), so the readiness gate
     blocks only on the collector's OWN readiness and a collector outage/upgrade has NO service impact. **The
     fleet-wide-CrashLoop hazard described above becomes ACTIVE only once per-service `init_otel` wiring (R-55)
     lands.** State this plainly so an operator doesn't over-read the current blast radius.
   - Nice-to-have: a one-line pre-upgrade checklist (Ready-check collector; confirm no service deploy in the
     same window).
   - Header `## OTel Collector Upgrade Discipline` slugifies to `otel-collector-upgrade-discipline` — keep in
     lockstep with the alert `runbook_url`.
7. `docs/TODO.md`: forward-dependency pointer(s) so R-55 closes the loop —
   - **(filed under the Observability section, per @observability A1 condition 2)** "Emit
     `dt_otel_export_failures_total{service,reason}` from the BatchExporter error hook in
     `crates/common/src/observability/otel.rs` when the R-55 exporter init lands; activates the dormant
     `OTelExportFailureRate` alert (`infra/docker/prometheus/rules/otel-alerts.yaml`)." — cite this TODO entry
     in the impl summary.
   - "R-55 (MC/MH): add the OTel endpoint ConfigMap key(s) MC/MH read once their `init_otel` wiring lands —
     deferred from task #28 (collector deploy), which wired AC `OTLP_ENDPOINT` + GC `OTEL_COLLECTOR_ENDPOINT`
     only (the sole live readers on that branch)."
   - "R-55: once services call `init_otel` (collector dependency real), add an export-path/data env-test in
     `crates/env-tests/tests/30_observability.rs` (asserts traces actually reach the collector). Task #28
     already added the cluster-health READINESS assertion (collector pod Ready, ns `dark-tower`, selector
     `app=otel-collector`, in `00_cluster_health.rs`); the R-55 data assertion is the teeth the gate gains once
     services depend on the collector — needs the emitter R-55 lands."
   - "global-controller / R-55: fix `crates/gc-service/src/config.rs:34-35` `DEFAULT_OTEL_COLLECTOR_ENDPOINT` —
     currently `http://otel-collector.default.svc.cluster.local:4318` (WRONG namespace `default`); correct to
     `http://otel-collector.dark-tower.svc.cluster.local:4318`. Unused fallback today (task #28's GC ConfigMap
     sets the env var), but the constant's doc cites the R-59 contract. Owner: global-controller (lands with the
     GC R-55/backend-otel task already in this file). Deferred from task #28 per team-lead Adjudication E."

8. `crates/env-tests/tests/00_cluster_health.rs` (NEW test, per @test): add a Layer-7 collector-readiness
   assertion. Cluster-health tier (NOT 30_observability.rs — collector is inert, no trace/metric data to assert).
   - Uses the existing `Command::new("kubectl")` pattern (the file's `test_secrets_not_in_env_vars` shape), NOT
     `ClusterConnection` (:13133 is cluster-internal, no port-forward).
   - Assertion: `kubectl wait --for=condition=Ready pod -l app=otel-collector -n dark-tower --timeout=10s`,
     assert exit 0. (`kubectl wait` on a selector matching ZERO pods returns "no matching resources found"
     IMMEDIATELY regardless of `--timeout` — so a typo'd selector is still caught, not vacuously passed: the
     point @test raised. The `10s` only governs how long an existing matched pod is given to reach Ready —
     changed from `0` to `10s` to match this suite's other health checks (cluster.rs uses 5-10s probe timeouts)
     so a transient readiness blip when smoke tests start doesn't false-fail, per @test's alignment note.)
   - **CRITICAL**: namespace `dark-tower` (NOT `default` — the two existing kubectl tests in this file have a
     pre-existing `-n default` bug; do NOT copy it). Label selector `app=otel-collector` MUST be the SAME string
     as the setup.sh `kubectl wait` selector AND the Deployment pod-template label — the three-way match is the
     invariant the test guards.
   - File is already `#![cfg(feature = "smoke")]` (line 6), so no per-test cfg needed.
   - This converts the residual ns/label/timeout gate-regression risk into a Layer-7-caught regression NOW,
     making the setup.sh gate a checked invariant while otel is still inert.

### Local validation plan
- `kubectl kustomize infra/kubernetes/overlays/kind/` renders clean.
- `kubeconform` on rendered otel-collector manifests.
- `shellcheck infra/kind/scripts/setup.sh`.
- `scripts/guards/simple/validate-alert-rules.sh` passes (severity + runbook anchor on disk).
- `cargo test -p env-tests --features smoke --no-run` compiles the new readiness assertion (can't run Layer-7
  without a live cluster, but compile-check catches signature/selector-string errors).

### Pre-load gotchas folded in (from @operations + @dry-reviewer via team-lead)
- **`severity: warning`** (not `warn`) — confirmed against the guard taxonomy.
- **Alert annotations carry NO endpoint/host text** — symptoms only (`{{ $labels.service }}` export
  failures). The collector endpoint lives in runbook prose. (Note: I read `dt-guard` HYGIENE_PATTERNS — there
  is no literal `.svc.cluster.local` denylist entry on this branch; the rejected hostname pattern is
  `-prod-`/`-stage-` infixed hosts + bare IPv4. My short DNS `otel-collector.dark-tower:PORT` is clean. I
  keep endpoints out of annotations anyway per the spirit of the guidance.)
- **`runbook_url` anchor covers triage/mitigation/escalation**, not a stub — the new gc-deployment.md section
  gets real upgrade-discipline + rollback content.
- **`for: 10m` with `rate(...[5m])`** is coherent and passes the for>=30s / expr-window check.
- **Short DNS form** `otel-collector.dark-tower:PORT` — pending @observability confirmation on the `http://`
  scheme prefix + that GC appends `/v1/{metrics,traces}` itself (it does, per `telemetry_forwarder.rs`).

### Cross-boundary confirmations needed at Gate 1
- **@observability**: the three ConfigMap KEY names (`OTEL_ENABLED`/`OTEL_ENDPOINT`/`OTEL_SAMPLE_RATE`), endpoint
  forms (gRPC 4317 base vs HTTP 4318 base), and the `dt_otel_export_failures_total{service,reason}` metric
  name/labels the alert keys off (must match R-54).
- **@operations** + **@observability**: `otel-alerts.yaml` semantics + `gc-deployment.md` upgrade-discipline prose
  (Minor-judgment cross-boundary).
- **@security**: NetworkPolicy restrictiveness (only 4 service pods on 4317 + GC on 4318; egress DNS-only).

---

## Implementation Summary

All 8 deliverables implemented (diff matches the Cross-Boundary table):

1. **`infra/services/otel-collector/` base** — `configmap.yaml` (collector config: OTLP grpc :4317 + http :4318
   receivers, `debug`+`logging` exporters with the deprecation comment, `health_check` :13133, traces+metrics
   pipelines), `deployment.yaml` (`otel/opentelemetry-collector-contrib:0.103.1`, req/limits, readiness+liveness
   httpGet :13133, non-root securityContext UID 10001 + readOnlyRootFS + drop ALL, config mount + `tmp`
   emptyDir), `service.yaml` (ClusterIP 4317/4318/13133), `network-policy.yaml` (4317 from 4 service pods, 4318
   from GC only, 13133 ns-scope, egress DNS UDP+TCP 53), `kustomization.yaml` (managed-by label).
2. **Kind overlay** `overlays/kind/services/otel-collector/kustomization.yaml` + registered `otel-collector/` in
   `overlays/kind/services/kustomization.yaml`.
3. **ConfigMap wiring** — AC `OTLP_ENDPOINT: http://otel-collector.dark-tower:4317`; GC `OTEL_COLLECTOR_ENDPOINT:
   http://otel-collector.dark-tower:4318` (bare base). **MC/MH untouched** (B1 — deliberate, no reader on branch).
4. **`setup.sh`** — `deploy_otel_collector()` (apply-then bare `kubectl wait -l app=otel-collector -n dark-tower
   --timeout=120s`, no swallow), called in `main()` after `deploy_observability` / before `run_migrations`+AC;
   `otel)` case in `deploy_only_service()`.
5. **`infra/docker/prometheus/rules/otel-alerts.yaml`** — `OTelExportFailureRate`, `sum by (service)
   (rate(dt_otel_export_failures_total[5m])) > 0`, `for: 10m`, `severity: warning`, 3 templated annotations
   (no FQDN), `runbook_url` → the dormant-note anchor. Ships **DORMANT** (emitter is R-54/R-55's — A1).
6. **`docs/runbooks/gc-deployment.md`** — new `## OTel Collector Upgrade Discipline` (+ TOC entry) with (a)-(f),
   the verbatim dormant sentence in the alert anchor, and the current-vs-future blast-radius honesty paragraph.
7. **`docs/TODO.md`** — 4 forward-dependency entries under Observability Debt: emitter (cites
   `common/observability/otel.rs`, owner observability), config.rs ns-fix (E, owner global-controller), MC/MH
   keys (B1, owner MC/MH), R-55 data env-test (owner test).
8. **`crates/env-tests/tests/00_cluster_health.rs`** — `test_otel_collector_ready` (kubectl-wait pattern, ns
   `dark-tower`, `app=otel-collector`, `--timeout=10s`, smoke-gated; three-way label match guarded).

**Forward-dependency / scope-split note (cite per @observability A1 cond. 2)**: the `dt_otel_export_failures_total`
emitter is intentionally NOT in this diff — it's observability/R-55's deliverable (BatchExporter error hook in
`crates/common/src/observability/otel.rs`), tracked in `docs/TODO.md` under Observability Debt. The alert ships
dormant until it lands. `crates/gc-service/src/config.rs` is NOT touched (Adjudication E — deferred to GC/R-55,
TODO-tracked). MC/MH configmaps are NOT touched (B1).

---

## Validation (Gate 2)

### Local validation run (implementer, pre-handoff)

| Check | Result |
|-------|--------|
| `kubectl kustomize infra/kubernetes/overlays/kind/` | **PASS** — renders clean (20113 lines); collector ConfigMap/Deployment/Service/NetworkPolicy present; AC `OTLP_ENDPOINT` + GC `OTEL_COLLECTOR_ENDPOINT` values present; image `otel/opentelemetry-collector-contrib:0.103.1`; `environment: kind` + `managed-by: dark-tower` labels applied. |
| `validate-alert-rules.sh` (dt-guard `alert-rules-policy`) | **PASS** — `STATUS=OK REASON=alert-rules-clean-4-files` (built dt-guard, ran via `DT_GUARD=target/debug/dt-guard`). Confirms severity=warning, runbook_url path under docs/runbooks/ + target exists, for/expr-window, annotation hygiene all pass on `otel-alerts.yaml`. |
| `cargo test -p env-tests --features smoke --no-run` | **PASS** — compiles; `00_cluster_health.rs` builds with the new `test_otel_collector_ready`. |
| `bash -n infra/kind/scripts/setup.sh` | **PASS** — syntax OK. |
| YAML parse + collector config shape (python) | **PASS** — all 5 base manifests parse; embedded config has receivers grpc+http, exporters debug+logging, health_check ext, traces+metrics pipelines. |
| Runbook anchor | **PASS** — `## OTel Collector Upgrade Discipline` (line 1217) → slug `otel-collector-upgrade-discipline` matches the alert `runbook_url`. |
| Three-way `app=otel-collector` label match | **PASS** — Deployment pod-template = setup.sh wait selector = env-test selector. |
| Diff scope vs Cross-Boundary table | **PASS** — 7 modified + new otel base/overlay/alert/devloop-outputs; `config.rs` NOT touched (E); no MC/MH configmap changes (B1). |

**Not runnable in sandbox** (ran under `./scripts/layer-all.sh` at Gate 2): `shellcheck` (not installed — `bash -n` passed as a partial substitute), `kubeconform` (not installed — kustomize render + YAML parse exercised structure). dt-guard built debug locally; `layer-all.sh` builds it itself.

### Gate-2 pipeline disposition (team-lead, user-decided 2026-06-24)

- **R-59's OWN changes pass every applicable layer**: L1/L2/L4/L6 OK; L7 N/A (wave2-pending). The two
  Gate-2 RED items from the first run were FIXED:
  - `validate-env-config` orphan keys (AC `OTLP_ENDPOINT` / GC `OTEL_COLLECTOR_ENDPOINT` not referenced by the
    workload) → fixed by adding per-key `configMapKeyRef` env entries in the AC StatefulSet + GC Deployment.
  - env-test `--timeout` race → frozen at `10s`.
- **Remaining pipeline RED is SOLELY pre-existing sdk-core branch debt, NOT R-59** (see §ADJUDICATIONS F):
  L3 `validate-todo-tracking` (sdk-core main.md inline body) + L5 `nx-lint sdk-core:lint` (prettier on 3
  committed sdk-core TS files; R-59 touches 0 TS). My attempted L3 fix was REVERTED by team-lead (2-main-mds
  collision guard). **USER DECISION**: proceed to review + commit R-59 with the pipeline red on these
  pre-existing items only; user fixes sdk-core in parallel, NO `docs/TODO.md` entry.

---

## Review Verdicts (Gate 3)

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | Gate-3: 1 finding FIXED, rest CLEAR | netpol/securityContext/service/configmaps/alert all verified clean; FINDING: collector exporter `verbosity: detailed` dumps span attrs → PII/secret leak to collector pod log once R-55 emits (test_secrets_not_in_logs scans only ac-service) | FIXED — `detailed`→`normal` on both exporters (option a, in-changeset) | — | leak path removed, no TODO needed |
| Test | Gate-3: 1 finding FIXED (pre-resolved) | verified 3-way label match, setup.sh order, env-test compiles, alert-rules clean, GC bare-base, runbook anchor. FINDING: `[debug, logging]` pipeline = L7 bring-up risk (logging removed upstream → startup-validation CrashLoop, uncatchable here since L7 N/A) | PRE-RESOLVED — `logging` already dropped at Gate-3 (def + both pipelines); now `[debug]` only, exactly @test's preferred fix | data env-test→R-55 | @test cited removal v0.120, my source (#11337) says v0.111.0; moot — dropped entirely, safe either way |
| Observability | Gate-3 CLEAR (zero findings) | full diff verified: collector config, endpoints/scheme, alert by(service)+for+anchor, A1 cond 1 (dormant sentence in anchor) + cond 2 (TODO names emitter), setup.sh gate + env-test, alert-rules clean, no dashboard orphan | n/a | counter→R-54/R-55; MC/MH keys→R-55 | reviewed pre-`logging`-drop; drop aligns w/ their deprecation flag, doesn't affect CLEAR |
| Operations | Gate-3 CLEAR (NO FINDINGS) | both Minor-judgment hunks (otel-alerts.yaml + gc-deployment.md) + setup.sh gate verified clean; alert STATUS=OK, anchor resolves, role-based annotations, dormant note honest; runbook (a)-(f) + blast-radius + break-glass all present; called out env-test zero-pods invariant + by(service) no-fan-out as good ops work | n/a | — | Ownership-Lens ACK both hunks; CLEAR to team-lead |
| Code Quality | Gate-3 CLEAR (no blocking findings) | diff clean + convention-faithful; 1 minor non-blocking observation: `[debug, logging]` double-log | PRE-RESOLVED — `logging` already dropped at Gate-3 (now `[debug]` only), which is exactly the single-exporter form CR suggested | — | observation crossed with the logging-drop; not held |
| DRY | Gate-3 CLEAR (no true dup, no extraction) | 3 items confirmed in diff: config.rs TODO line (+3 more well-scoped deferral entries), short-form AC/GC endpoints w/ TODO cross-ref, otel-alerts authored-fresh (no placeholder drift) + overlay not redis-clone; caught config.rs:35 → Adjudication E | form→short; E → docs/TODO.md, owner global-controller | E deferred to GC/R-55 | config.rs untouched; divergence TODO-tracked |
| Semantic Guard | Gate-1 CONFIRMED (nothing outstanding) | (1) bare `kubectl wait` no swallow under set -euo pipefail (line 34 + deploy_redis shape); (2) `by (service)` confirmed right (no `reason` fan-out; counter keeps `{service,reason}` at source); (3) no secrets in configmaps/annotations; dormant=contract-only, not a dead path | n/a | — | real diff pass at Start Review |

---

## Accepted Deferrals

None. All Gate-3 findings were fixed in-changeset (Security: exporter `verbosity` detailed→normal; Test: `logging` exporter dropped). Both reviewers with findings landed RESOLVED-FIXED; the other five are CLEAR. Zero deferrals, zero spin-outs.

Scope items are NOT deferrals (they are scope-boundary / forward-dependency splits, documented in §Adjudications + docs/TODO.md, with no finding left in this diff):
- **A1** — `OTelExportFailureRate` ships dormant; the `dt_otel_export_failures_total` emitter is R-54/R-55's deliverable (observability).
- **B1** — MC/MH OTel endpoint keys deferred to their R-55 tasks (no reader on this branch).
- **E** — pre-existing GC `config.rs:34-35` default-namespace bug routed to the global-controller/R-55 owner (TODO-tracked; `config.rs` not in this diff).

The pre-existing sdk-core pipeline failures (L3 `validate-todo-tracking`, L5 `nx-lint` prettier) are unrelated to R-59, untouched by this diff, and are being fixed by the user in parallel — committed per explicit user decision (2026-06-25); no docs/TODO.md entry per user directive.
