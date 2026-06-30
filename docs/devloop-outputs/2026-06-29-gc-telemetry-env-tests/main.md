# Devloop Output: Backfill env-tests for the GC telemetry proxy endpoint (task #10)

**Date**: 2026-06-29
**Task**: Add cluster-level env-tests for `POST /api/v1/telemetry/v1/{metrics,traces}` (auth gate, rate limit, size cap, content-type, PII filter)
**Specialist**: global-controller (paired-with: test)
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-57`
**Duration**: ~2 days wall-clock (2026-06-29 → 06-30), dominated by the env-test-infra-reliability detour (disk + Phase-1f + netpol fixes) that had to land before Layer 7 could run the suite.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `f34f3a666fd32aa8d7f0d1b8db1923f1456104de` |
| Branch | `feature/browser-client-join-task-57` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `PAUSED — Gate 2 blocked on env-test infra (ops devloop precedes)` |
| Implementer | `implementer` (global-controller) |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `plan-confirmed` |
| Test (paired) | `plan-confirmed` |
| Observability | `plan-confirmed` |
| Code Quality | `plan-confirmed` |
| DRY | `plan-confirmed` |
| Operations | `plan-confirmed` |
| Semantic Guard | `plan-confirmed` |

### Gate 1 — Plan Approval (2026-06-29)

All 7 reviewers confirmed after one revision round: R1 collector-substitution
oracle (verify PII filter via GC's own `gc_telemetry_pii_attributes_dropped_total{kind}`
counter + 202 delivery — endorsed by security + test), R2 absent-before-series⇒0,
R6 synthetic planted values, R7 distinct-`{kind}` per filter test
(g=resource/h=datapoint/t-c=span — closes the vacuous-fill race), C1 concurrent
`JoinSet` dispatch for the rate-limit test (drop order-dependent "last is 429"),
C2 direct `GET /metrics` as primary drop-counter read, C3 R3 re-grounded on the
real in-cluster 100/1-min/IP registration limit, R4 bidirectional ConfigMap↔test
cross-ref. Classification relabeled Owner=test (×3) + Owner=operations (configmap);
classification-sanity guard green. Code-reviewer's classification ESCALATE resolved
by Lead via Path 1.

**Gate-3 carry-forward (owner-involvement artifacts to verify at commit):**
1. Domain-judgment `31_gc_telemetry.rs`: `--paired-with=test` stays live; test
   exercises the Ownership Lens at Gate 3 with code-reviewer.
2. Minor-judgment `gc_client.rs`: commit needs `Approved-Cross-Boundary: test <reason>`
   trailer (hunk-ACK on the additive `raw_ingest_telemetry` helper).
3. Minor-judgment `configmap.yaml`: commit needs `Approved-Cross-Boundary: operations <reason>`
   trailer (comment hunk; pre-confirmed at Gate 1).

**Follow-up (separate from this devloop):** ratify the "implementer owns env-test
additions for cluster-observable behavior it introduces" convention via ADR/debate
(with test + AC/MC/MH specialists) so future backfills avoid this ownership ambiguity.

### Gate 2 — Validation: PAUSED 2026-06-30 (blocked on env-test cluster infra)

**Layers 1–6: GREEN** (`gate2-verdict` RUN_AT 2026-06-29T23:37:41Z): L1 compile OK,
L2 fmt OK, L3 guards OK (incl. cross-boundary classification + scope with the 5-row
table), L4 **3065 unit tests pass** / 0 fail, L5 clippy OK, L6 audit OK (cargo+pnpm
+buf-breaking; N/A aggregate). Implementer also fixed a Layer-2 fmt issue in
`31_gc_telemetry.rs` (rustfmt-wrapped the `raw_ingest_telemetry` calls).

**Layer 7: BLOCKED — env-test cluster infrastructure, NOT the diff.** `layer7.sh`
teardown+rebuilds the cluster every run; that rebuild failed **3/3** with three
distinct infra causes — (1) host disk exhaustion mid-`COPY` (`podman system prune -af`
then reclaimed ~2 TB; root cause = `COPY . .` of 27 GB `target/`, no `.containerignore`);
(2) `observability-prometheus-not-ready` (Phase-1 hard gate timed out after an ~84-min
cold rebuild); (3) `setup.sh` exit-1 after ~21 min (disk was fine — 128 GB free —
cause host-side, not introspectable from the devloop sandbox; left GC flapping:
`/health` returns 200 then ConnectionReset). Each is `PRECONDITION_FAILURE` (operator
lane); the sanctioned retry was used. The 11 env-tests themselves **never executed** —
all failed at their own Phase-precondition `.expect()` (`31_gc_telemetry.rs:92/96`,
"AC/GC service must be running"), which is the matrix-(b) **fail-loud-when-cluster-down**
behavior working AS DESIGNED, not a defect.

**DECISION (user, 2026-06-30):** fix the env-test infra FIRST via a dedicated
operations devloop (per-slug image tags + `cleanup()` GC + disk guard + `setup.sh`
reliability — see `docs/TODO.md` §"Devloop Container Resource Hygiene & Build
Isolation"), THEN resume this devloop. The env-test rebuild is too flaky to land any
cluster-dependent devloop until then.

**UPDATE 2026-06-30 — infra devloop landed, but STILL BLOCKED on a new gate.** The
`2026-06-30-envtest-infra-reliability` devloop committed (`d54143c`): the `.dockerignore`
keystone fixed the disk exhaustion (cluster rebuild now SUCCEEDS), + `cleanup()` GC + disk
guard + the `layer-all.test.sh` execute-bit (the latter committed THERE, so this devloop
DROPS its 5th classification row — `scripts/layer-all.test.sh` is no longer this devloop's
concern). **BUT** that devloop's Gate-2 uncovered a SEPARATE, deterministic env-test-infra
bug now blocking BOTH devloops: the `layer7.sh` Phase-1f Prometheus `/-/ready` gate fails
in-run even at a 900s budget (`REASON=observability-prometheus-not-ready`) though Prometheus
is healthy — `docs/TODO.md` item D. **This devloop CANNOT resume to `GATE2=PASS` until item D
is root-caused** (its env-test suite is gated by the same Phase-1f). Remains PAUSED, now
blocked on item D (Prometheus) rather than disk.

**RESUME RECIPE (after the ops-infra devloop lands + cluster is reliably healthy):**
1. Working tree still holds the 4 deliverable files (`crates/env-tests/{Cargo.toml,
   src/fixtures/gc_client.rs,tests/31_gc_telemetry.rs}`, `infra/services/gc-service/
   configmap.yaml`) + the `scripts/layer-all.test.sh` mode-bit (5th classification row).
   The two verdict-excluded doc fixes already landed in commit `5b9bb30`.
2. Sanity: `cargo test -p env-tests --features all --test 31_gc_telemetry` against the
   healthy cluster (env vars `ENV_TEST_{AC,GC,PROMETHEUS,GRAFANA,LOKI}_URL`) to confirm
   the 11 tests go green directly.
3. `./scripts/layer-all.sh` → expect `GATE2=PASS` → unicast "Start Review" to the 7
   reviewers for Gate 3 → commit (Step 8). Re-stage the chmod with
   `git update-index --chmod=+x scripts/layer-all.test.sh` so the mode persists.
4. Gate-3 owner-trailer obligations still apply (see Gate-1 carry-forward above):
   `Approved-Cross-Boundary: test …` (gc_client.rs) + `Approved-Cross-Boundary:
   operations …` (configmap.yaml + the layer-all.test.sh mode row).

---

## Task Overview

### Objective
Backfill env-tests against the live Kind cluster for the GC telemetry proxy
endpoint introduced by task #10. The `#[sqlx::test]` integration tests cover
handler logic against a postgres sidecar but do not verify the deployed
configuration (real OTel collector, real auth, real rate-limit state, real PII
filter). First backfill under the convention "implementer-specialist owns
env-test additions for cluster-observable behavior they introduce, paired with
test specialist."

### Scope
- **Service(s)**: GC (telemetry proxy), env-tests crate
- **Schema**: No
- **Cross-cutting**: env-tests crate; paired with test specialist

New file: `crates/env-tests/tests/31_gc_telemetry.rs` (~8 metrics tests + 2-3
traces tests). Possibly a `telemetry_client` fixture in
`crates/env-tests/src/fixtures/`.

Concrete cases (metrics): (a) authed POST → 202; (b) unauth → 401;
(c) service-token → 401; (d) oversize → 413; (e) wrong Content-Type → 415;
(f) rate-limit: N+1 CONCURRENT POSTs → ≥1 is 429 (see §Planning C1); (g) PII filter non-allowlisted
key → 202 + drop-counter verified; (h) scalar-only AnyValue non-scalar → 202 + drop-counter verified.
Traces: parallel-structure subset (auth + rate-limit + filter).

### Debate Decision
NOT NEEDED — additive env-test coverage within existing ADR-0014/ADR-0030
testing conventions; no new architectural decision.

---

## Cross-Boundary Classification

<!-- Filled by implementer at planning; verified by Lead + reviewers at Gate 1. -->

Ownership is per the CURRENT ratified model: `crates/env-tests/` is the `test` specialist's
cross-cutting territory, so all three env-tests edits are **Not mine, Owner=test** — satisfied
operationally via `--paired-with=test` (already the loop config). The task's structural-meta
"implementer owns env-test additions" convention is real but UNRATIFIED, and a GC test-backfill
cannot ratify a cross-cutting change touching AC/MC/MH env-tests (Lead ruling, Gate-1; follow-up
tracked separately). Judgment tiers below apply to these CROSS-boundary edits.

| Path | Classification | Owner |
|------|----------------|-------|
| `crates/env-tests/tests/31_gc_telemetry.rs` (NEW) | Not mine, Domain-judgment | test — satisfied via `--paired-with=test` (convention unratified; see follow-up) |
| `crates/env-tests/src/fixtures/gc_client.rs` (MODIFY: add `raw_ingest_telemetry`) | Not mine, Minor-judgment | test — hunk-ACK on additive `raw_*` helper |
| `crates/env-tests/Cargo.toml` (MODIFY: add `opentelemetry-proto` dev-dep) | Not mine, Mechanical | test — one dev-dep line, review-only |
| `infra/services/gc-service/configmap.yaml` (MODIFY: reverse-pointer comment only) | Not mine, Minor-judgment | operations — pre-confirmed at Gate-1 (R4 item ii); comment-only reverse cross-ref to `31_gc_telemetry.rs` N=60 / 256 KiB |

None are Guarded Shared Areas (no `proto/**`, no `crates/common/src/jwt.rs`, no `db/migrations/**`).
After relabel: no GSA rows; Owner=test and Owner=operations are valid specialists → classification
guard green.

**Gate-2 note (resolved).** Three PRE-EXISTING base-commit (`f34f3a6`) blockers surfaced during
this devloop's Gate 2: two Gate-2-verdict-EXCLUDED doc fixes (`infrastructure/INDEX.md`
`stale_pointer`; an `inline_debt_body` in the 2026-06-26 artifact) — landed as docs commit
`5b9bb30`; and a non-executable Layer-3 meta-test (`scripts/layer-all.test.sh`, committed 100644).
The chmod was initially folded here as a 5th row, but it ultimately landed in the
`2026-06-30-envtest-infra-reliability` devloop (commit `d54143c`) where it belongs thematically
(test-runner reliability) — so this devloop's changeset is back to its 4 deliverable files. The
deeper env-test-infra blockers this devloop hit (disk exhaustion → the Phase-1f Prometheus gate →
the GC→collector 502) were all root-caused and fixed in the `2026-06-30-*` infra devloops; with
those landed, this devloop's 11 env-tests pass live (11/11) and Layer 7 is green.

---

## Planning

### Grounding — what actually exists (cited)

**Endpoint pipeline** (`crates/gc-service/src/handlers/telemetry.rs:225-328`), in order:
0. disabled (empty endpoint) → 503 (`:234-240`); not reachable in cluster (endpoint is set).
1. `body.len() > max_bytes` → 413 `rejected_size` (`:256-261`). Runs BEFORE content-type/decode.
2. Content-Type base (`;`-split, trimmed) must equal `application/x-protobuf` → else 415 (`:263-275`).
3. per-`sub` GCRA `check_key(&user_claims.sub)` → 429 (`:279-283`).
4. prost-decode (malformed → 400, `:292-308`) + `telemetry_filter::filter_{metrics,traces}` + re-encode.
5. `forwarder.forward(...).await?` → 502 on any non-2xx/connect/timeout (`:317-323`,
   `services/telemetry_forwarder.rs:89-126`).
6. `202 Accepted` (`:326-327`). **Load-bearing for this plan: a 202 is returned ONLY after the
   collector accepts the forwarded bytes** — so 202 ⇔ collector reachable + accepted.

**Auth gate**: `require_user_auth` (`routes/mod.rs:130-133`) → `validate_user()`
(`middleware/auth.rs:82-96`). A service (client-credentials) token fails `validate_user` →
GcError → 401. Confirmed already-proven by `tests/24_join_flow.rs:311-346`
(`test_gc_join_rejects_service_token` asserts **401**). So case (c) ⇒ **401** (not 403).

**Filter** (`services/telemetry_filter.rs`): `filter_attrs` (`:109-121`) retains an attr iff
`ALLOWLIST.contains(key) && value_is_scalar(kv)` — i.e. a non-allowlisted KEY **or** a non-scalar
value (kvlist/array/bytes) on an allowlisted key is dropped (`:126-139`). `ALLOWLIST` is the 12 keys
at `:25-38` (e.g. `org_id`, `client_version`, `service.name`). Drops are counted per `AttrKind`
(resource/scope/datapoint/span/span_event/span_link) and **emitted** via
`metrics::record_telemetry_pii_dropped(kind, n)` (`:96-103`).

**Deployed config** (`infra/services/gc-service/configmap.yaml`): only `OTEL_COLLECTOR_ENDPOINT =
http://otel-collector.dark-tower:4318` is overridden. `TELEMETRY_PROXY_MAX_BYTES` and
`TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE` are **NOT** set → compiled defaults apply
(`config.rs:38,41`): **max_bytes = 262144 (256 KiB)**, **rate_limit = 60/min** (GCRA burst 60,
replenish 1/s). This fixes the constants the tests assume; if the ConfigMap later sets either, the
tests must read it (noted as risk R4).

**Collector** (`infra/services/otel-collector/{configmap,service,deployment}.yaml`): single
`otel/opentelemetry-collector-contrib:0.103.1` replica in ns `dark-tower`, OTLP-gRPC :4317 +
OTLP-HTTP :4318, **`ClusterIP`** (no NodePort/port-forward → unreachable from the env-test host),
sole exporter `debug` at **`verbosity: normal`**. It is a **shared** sink for AC/GC/MC/MH R-55
spans + GC's R-2 proxy forwards.

### CRITICAL DESIGN DECISION — collector-verification mechanism for (g)+(h)

**Decision: verify via GC's own `gc_telemetry_pii_attributes_dropped_total{kind}` counter (scraped
into Prometheus) + the 202-status delivery semantics. REJECT both candidate mechanisms named in the
task brief.**

Why the two candidates are not viable here (evidence, not preference):

1. **`kubectl logs` of the collector debug exporter — BLOCKED by an intentional security control.**
   The exporter is pinned at `verbosity: normal` (`otel-collector/configmap.yaml`, exporters block).
   At `normal`, the OTel debug exporter logs span/metric **names + counts only — never attribute
   keys or values**. The ConfigMap comment states this is deliberate: `detailed` "dumps every span
   attribute value to the pod log … any PII/secret … would land in the collector log in cleartext —
   and `test_secrets_not_in_logs` only scans ac-service logs, not the collector pod". So the debug
   log **cannot** show whether a non-allowlisted key was stripped — the attribute is simply never
   printed. Raising verbosity to make the assertion possible would re-open exactly the PII-leak path
   security closed; that is a regression, not a test fixture.

2. **Thin test-only OTLP receiver — out of scope / changes production deploy.** GC forwards to the
   one configured `OTEL_COLLECTOR_ENDPOINT` (set in the deployed ConfigMap). An env-test cannot
   redirect a single request to a side sink; pointing GC at a test receiver means editing the
   deployed ConfigMap + redeploying GC (or standing up + wiring a second collector). That is an
   infra change owned by infrastructure/observability, not an additive test-backfill, and it mutates
   the deployment under test.

What the chosen mechanism proves, end-to-end in the **deployed** path:
- **202** ⇒ payload accepted, decoded, filtered, **forwarded, and the live collector returned 2xx**
  (`handlers/telemetry.rs:317-327`). A 202 alone does not prove anything was *stripped* (a clean
  payload also gets 202).
- **`gc_telemetry_pii_attributes_dropped_total{kind}` increment** ⇒ the deployed filter actually
  *dropped* the planted attribute(s) before re-encoding (`telemetry_filter.rs:96-103,109-121`). This
  is the externally-observable proof of filtering — queried with the existing `PrometheusClient`
  fixture, exactly as `tests/30_observability.rs` queries PromQL, wrapped in
  `assert_eventually(ConsistencyCategory::MetricsScrape, …)` (`src/eventual.rs`) to absorb scrape
  lag. Counter is cumulative with a bounded `kind` label (no per-test label), so assert
  **after ≥ before + expected** against a pre-POST PromQL snapshot (≥ tolerates concurrent
  increments from other tests / parallel runs → idempotent, matrix c/d).

**This also satisfies verification-matrix (b) "fail loudly when collector is down":** every test
(a, d-via-success, g, h, traces) *unconditionally asserts 202* — there is no "if collector
unavailable, skip" branch. Collector down ⇒ forward fails ⇒ handler returns **502** ⇒ the 202
assertion fails loudly. The 202 assertion **is** the collector-liveness detector.

(Secondary signal considered & rejected: collector `debug` at `normal` does print received
metric/span *counts*, so a kubectl-logs delta on counts is theoretically possible — but the sink is
shared by all 4 services' R-55 spans, making count deltas racy/noisy, and the 202 already proves
delivery. Not worth the flake.)

### Auth-token + payload acquisition (mirrors existing patterns)

- **User JWT**: `AuthClient::register_user(UserRegistrationRequest::unique(..))` →
  `response.access_token` (`fixtures/auth_client.rs:175-205,241-248`), exactly as
  `tests/24_join_flow.rs:82-90`. **AC enforces a registration rate limit** — in-cluster it is
  100/1-min-window per IP (`ac-service/configmap.yaml:14-15`; see R3, re-grounded — NOT the "~5/hr"
  the join-flow comment cites). I still mirror join-flow's `OnceCell` sharing defensively: a
  **`SHARED_USER` `OnceCell`** (1 registration) reused by all cases that just need *a* valid user,
  plus **one** dedicated `RATELIMIT_USER` `OnceCell` (1 registration) so case (f) can exhaust its
  own per-`sub` GCRA window without 429-ing the shared user. **Total = 2 registrations/run** (≤ 4
  for matrix-c double-run; flagged R3).
- **Service token**: `AuthClient::issue_token(TokenRequest::client_credentials("test-client",
  "test-client-secret-dev-999", "test:all"))` (`fixtures/auth_client.rs:111-137`); creds are seeded
  by `infra/kind/scripts/setup.sh:473-478`. Mirrors `24_join_flow.rs:319-325`.
- **OTLP protobufs**: add `opentelemetry-proto = { workspace = true }` to env-tests
  `[dev-dependencies]` (workspace dep already exists at root `Cargo.toml:84`, v0.7 w/
  `gen-tonic-messages,metrics,trace`; `prost` already a dev-dep, `Cargo.toml:59`). Build minimal
  `ExportMetricsServiceRequest` / `ExportTraceServiceRequest` with file-local helpers.
  **Builder mirror (Gate-1 @dry-reviewer):** the closer analog is
  `crates/gc-service/tests/telemetry_proxy_tests.rs` — `metrics_payload_with_pii():219`,
  `empty_metrics_payload():252`, `metrics_payload_with_value_smuggling():260` already return ENCODED
  `Vec<u8>` bodies with planted PII (exactly what (a)/(d)/(g)/(h) need), vs. the struct-returning
  `telemetry_filter.rs` unit builders. Mirror that file's `Vec<u8>`-returning shape (and the same
  shape for a traces payload). Not true duplication (none reachable cross-crate), so file-local is
  acceptable; @dry-reviewer will file a Gate-2 docs/TODO.md entry for a future shared
  OTLP-payload-builder home (likely `gc-test-utils`) — that cross-crate extraction is OUT of scope
  for this additive backfill.

### Fixture change

Add **one** method to `GcClient` (`fixtures/gc_client.rs`) mirroring the existing `raw_*` helpers
(`:525-562`) — no new fixture file (all telemetry routes are GC endpoints; GcClient already owns GC
raw requests):
```
pub async fn raw_ingest_telemetry(
    &self, signal_path: &str,          // "metrics" | "traces"
    token: Option<&str>,
    content_type: Option<&str>,        // None ⇒ omit header (415 case)
    body: Vec<u8>,
) -> Result<reqwest::Response, GcClientError>
```
POSTs to `{base}/api/v1/telemetry/v1/{signal_path}`, sets `Authorization: Bearer` iff `token`,
sets `Content-Type` iff `content_type`, sends raw `body`. Name mirrors both the sibling `raw_*`
helpers and the handlers `ingest_metrics`/`ingest_traces` (Gate-1 @code-reviewer naming nit, taken).

### Test list — `crates/env-tests/tests/31_gc_telemetry.rs` (`#![cfg(feature = "flows")]`)

Shared scaffolding mirrors `24_join_flow.rs`: `CLUSTER`/`SHARED_USER`/`RATELIMIT_USER` `OnceCell`s;
`cluster()` checks AC+GC health. File-local OTLP builder helpers.

Metrics (8):
- **(a)** `test_metrics_authed_post_returns_202` — SHARED_USER + valid minimal OTLP-metrics +
  `application/x-protobuf` → **202**. (Also the collector-liveness detector for matrix b.)
- **(b)** `test_metrics_unauthenticated_returns_401` — no token → **401**.
- **(c)** `test_metrics_service_token_returns_401` — `test-client` service token →
  **401** (per `require_user_auth`; matches join-flow ruling).
- **(d)** `test_metrics_oversize_returns_413` — body ≈ 300 KiB (in `(262144, 524288]`, i.e. above
  `max_bytes` but ≤ the `2×` `DefaultBodyLimit` ceiling so it hits the **handler's** load-bearing
  gate, not the layer) + valid token/CT → **413**.
- **(e)** `test_metrics_wrong_content_type_returns_415` — small body, `Content-Type:
  application/json` + valid token → **415**.
- **(f)** `test_metrics_rate_limit_returns_429` — RATELIMIT_USER; dispatch **M = 130** valid minimal
  POSTs **CONCURRENTLY** (`tokio::task::JoinSet`, no `futures` dep) and assert **≥1 of M is `429`**
  (PRIMARY, collector-independent — the GCRA `check_key` precedes the forward) plus a SECONDARY
  `≥1 is 202` (collector-liveness; fails loudly under collector-down per matrix b). Failure messages
  print `n_202/n_429/n_other/n_err` for drift diagnosis. **C1′ (RATIFIED @test — supersedes C1.)**
  C1 assumed 61 trips a single pod's burst-60; that is FALSE here — see §"Implementation discovery":
  GC runs **2 replicas** behind a load-balancing NodePort and the GCRA limiter is **per-pod,
  in-memory**, so 61 split ~30/30 trips neither. Pigeonhole: M over 2 pods ⇒ busiest ≥ ⌈M/2⌉; need
  > burst 60 ⇒ M ≥ 122; use **M = 130** (busiest ≥ 65 > 60 ⇒ ≥1 429 for ANY split; concurrent
  wall-clock ≪ 1 s so replenishment is negligible). "Last is 429" stays DROPPED (nondeterministic
  completion order). Coupling comment ties M to `replicas × burst` (deployment.yaml:10 ×
  TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE). Unique `sub` ⇒ own window (matrix d); fresh user (matrix c).
- **(g)** `test_metrics_pii_nonallowlisted_key_filtered` — SHARED_USER; metrics payload with a
  non-allowlisted attr key planted at **RESOURCE level** (synthetic key/value per R6, e.g. key
  `dt_not_allowlisted`, value `synthetic-pii-marker`) alongside an allowlisted scalar (`org_id`) →
  **202**; `#[cfg(feature = "observability")]` block: before/after delta on the
  `{kind="resource"}` drop counter ≥ planted count.
  **C2′ (RATIFIED @observability — supersedes C2) — verification mechanism:** the PRIMARY read is
  PromQL **`sum(gc_telemetry_pii_attributes_dropped_total{kind="resource"})`** via
  `PrometheusClient::query_promql`, wrapped in `assert_eventually(ConsistencyCategory::MetricsScrape)`;
  an absent/empty result vector ⇒ **0** (R2). The `sum` AGGREGATES the counter across GC's **2
  replicas** (each pod is a separate Prometheus target via the static `prometheus-config.yaml`
  `role: pod` scrape at 15 s), so the assertion is correct regardless of which pod served the POST —
  this is why the superseded C2 "direct `GET /metrics` parse, single-replica authoritative" is WRONG
  here (a direct scrape reads only one pod and misses POSTs routed to the other). The 30 s
  `MetricsScrape` budget is ADEQUATE (15 s worst-case staleness, full cycle of headroom; not bumped).
  The planted drop guarantees the after-series exists; `after ≥ before + planted` (`≥` tolerates
  concurrency). **Distinct `{kind}` per filter test (R7, @security #3 — MORE load-bearing under
  `sum`):** (g) at `resource`, (h) at `datapoint`, (t-c) at `span` — each `sum`-series has exactly
  ONE writer in the suite, so no sibling's legitimate drop can vacuously fill another's `≥` delta.
  (`filter_metrics` counts resource-level drops as `AttrKind::Resource` — `telemetry_filter.rs:147-149`.)
- **(h)** `test_metrics_pii_nonscalar_value_dropped` — SHARED_USER; allowlisted key `org_id` at
  **DATAPOINT level** whose value is a non-scalar (`KvlistValue`, holding synthetic values per R6) →
  **202**; observability-gated before/after delta on `sum(…{kind="datapoint"})` via the same C2′
  PromQL path (scalar-only rule, `value_is_scalar` `:126-139`). Absent series ⇒ 0 (R2); distinct
  kind per R7.

Traces (3, parallel structure — auth + filter; rate-limit/CT/size share the handler path already
proven for metrics):
- **(t-a)** `test_traces_authed_post_returns_202` — SHARED_USER + minimal OTLP-traces → **202**.
- **(t-b)** `test_traces_unauthenticated_returns_401` → **401**.
- **(t-c)** `test_traces_pii_nonallowlisted_span_attr_filtered` — non-allowlisted SPAN attr → **202**
  + observability-gated before/after delta on `sum(…{kind="span"})` via the C2′ PromQL path; absent
  series ⇒ 0 (R2); distinct kind per R7.

**Dual feature-gating (confirmed intentional, @test):** the file is `#![cfg(feature = "flows")]`;
the drop-counter verification in (g)/(h)/(t-c) lives in `#[cfg(feature = "observability")]` blocks.
So under **`flows`-only** these three tests degrade to **202-only asserts** (no drop-counter check)
— deliberate: a 202 still proves the filter is strip-not-reject in the deployed path, and we do NOT
fabricate a metric assertion the feature set can't back (no false confidence). Under **`all`** (Layer
7 + verification-matrix a) BOTH the 202 and the drop-counter delta run. Under C2′ the drop-counter
read is a **PromQL query against Prometheus** (`sum`-aggregated across the 2 replicas), so it genuinely
REQUIRES the Prometheus server — which makes the `observability` feature-gate exactly right (not a
mere convention): flows-only correctly cannot run it.

### Verification-matrix mapping
- (a) all pass under `cargo test -p env-tests --features all` after `dev-cluster setup` +
  `rebuild-all` (the `flows`+`observability` features both active under `all`).
- (b) collector-down ⇒ 502 ⇒ the unconditional 202 asserts fail loudly (no skip branch). ✓
- (c) idempotent: fresh users per run; counter deltas use `≥`; no absolute counter values. ✓
- (d) no cross-test rate state: (f) on a dedicated `sub`; shared-user POSTs total ≪ 60/min. ✓
- (e) Layer 7 already runs `cargo test -p env-tests --features all` (`scripts/layer7.sh:402`) inside
  `layer-all.sh`’s loop (`scripts/layer-all.sh:103`) — the new file is picked up automatically; no
  script edit needed.

### Open questions / risks
- **R1 (resolved)** Collector-side payload inspection is infeasible by security design
  (`verbosity: normal`) — decision above substitutes GC drop-metric + 202. Flag for `test` +
  `observability` Gate-2 sign-off that this substitution is acceptable (it is the crux call).
- **R2** PromQL drop-counter delta depends on GC `/metrics` being scraped (service-monitor exists)
  and on the `{kind=…}` series existing. **CORRECTION (Gate-1, @observability):** the series is
  NOT pre-created — `record_telemetry_pii_dropped` early-returns when `count == 0`
  (`crates/gc-service/src/observability/metrics.rs:657-665`), so `DropCounts::emit`
  (`telemetry_filter.rs:96-103`) creates NO zero-valued series. The `{kind="datapoint"}` /
  `{kind="span"}` series is created **lazily on the FIRST real drop**. Consequence for (g)/(h)/(t-c):
  on a fresh cluster the pre-POST "before" PromQL snapshot can return an **EMPTY result set, not
  `0`**. **Binding rule for plan + impl: an absent before-series is treated as 0** — the delta/parse
  logic MUST map a missing/empty PromQL vector to `0` and never `.expect()`/panic/misread it. The
  oracle is unaffected: the planted drop guarantees the after-series exists, and `after >= before +
  planted` with `≥` tolerates the shared-counter concurrency (before defaults to 0 when absent).
  Under C2′ the read is `sum(…{kind})` over Prometheus, polled with `assert_eventually(MetricsScrape)`;
  the 30 s budget is adequate (15 s scrape interval, full cycle of headroom — not bumped). (The
  earlier "lag-free direct `GET /metrics`" fallback is RETIRED — it reads a single pod and is unsound
  for 2-replica GC; see §"Implementation discovery" / C2′.)
- **R6 (advisory, @observability)** Planted PII-test attribute values MUST be obviously SYNTHETIC
  (e.g. `not-allowlisted-marker`, a synthetic token — NOT real-looking emails/secrets). Defense in
  depth: if a future verbosity bump or error-body log ever surfaces a planted value, it carries no
  resemblance to real PII.
- **R3 (re-grounded, Gate-1 @test C3)** The AC registration limit that ACTUALLY runs in-cluster is
  **100 attempts / 1-min window per IP** (`infra/services/ac-service/configmap.yaml:14-15`:
  `AC_REGISTRATION_RATE_LIMIT_{WINDOW_MINUTES=1,MAX_ATTEMPTS=100}`, keyed by IP) — NOT the ~5/hour
  compiled default cited in `24_join_flow.rs` (the compiled default is window=60 min,
  `config.rs:50`, but the dev ConfigMap overrides it). Evidence it's relaxed: `23_meeting_creation.rs`
  registers a fresh user in all 4 tests with no `OnceCell` and stays green. So the plan's **2
  registrations/run** budget is trivially safe and matrix-c double-run is a non-issue. I keep the
  `SHARED_USER`/`RATELIMIT_USER` `OnceCell`s anyway — defensive + keeps `auth_events` table noise
  down — but the reasoning no longer hangs on the wrong "~5/hr" premise.
- **R4** Tests hard-code N=60 / max_bytes=256 KiB from compiled defaults (ConfigMap doesn’t override
  them today). If a future ConfigMap sets `TELEMETRY_PROXY_*`, (d)/(f) break. **Bidirectional
  coupling doc (Gate-1 @operations rec, accepted):** (i) forward — comment in `31_gc_telemetry.rs`
  tying the N=60 / 256 KiB constants to `infra/services/gc-service/configmap.yaml`; (ii) reverse —
  a comment in `gc-service/configmap.yaml` at the spot where `TELEMETRY_PROXY_MAX_BYTES` /
  `_RATE_LIMIT_PER_MINUTE` would be set ("if you set these, update 31_gc_telemetry.rs N / max-bytes"),
  mirroring the existing bidirectional `OTEL_COLLECTOR_ENDPOINT` configmap↔config.rs cross-ref. The
  configmap edit is comment-only, Not-mine/Owner=operations (pre-confirmed at Gate-1), and is listed
  in §Cross-Boundary Classification so the Gate-2 scope-drift guard sees it.
- **R5** Cluster availability is a precondition, not a risk this task owns — Layer 7 gates it.
- **R7 (Gate-1 @security #3, must-fix — folded into the test list)** (g)/(h)/(t-c) each write the
  same-named `gc_telemetry_pii_attributes_dropped_total` counter with no per-test label; parallel
  Cargo execution means a sibling’s legitimate drop on a shared `{kind}` series could vacuously
  satisfy another’s `≥` delta, masking a filter regression. Only GC telemetry-proxy POSTs write this
  counter (R-55 service spans use OTLP-gRPC and bypass the filter), so the sibling tests are the only
  colliding writers. FIX: plant at distinct nesting levels so each test owns a distinct `{kind}`
  series — (g)=`resource`, (h)=`datapoint`, (t-c)=`span`. Residual (two fully-concurrent test
  PROCESSES on a shared cumulative counter) is inherent and accepted; the matrix-c rerun is
  sequential.

### Implementation discovery — GC runs 2 replicas (RATIFIED supersession of C1/C2)

Found during implementation: **GC runs `replicas: 2`** in the dev cluster
(`infra/services/gc-service/deployment.yaml:10`; no kind-overlay patch —
`overlays/kind/services/gc-service/kustomization.yaml`; `infra/kind/scripts/setup.sh:753,758`
prints "Running in-cluster (2 replicas)"), reached via a **load-balancing NodePort**,
`sessionAffinity: None` (`overlays/kind/services/gc-service/nodeport.yaml`). This invalidated two
single-pod premises in the original C1/C2; the corrections below were **RATIFIED** — C1′ by @test,
C2′ by @observability (Lead, 2026-06-29) — and the C1/C2/R2 wording above has been rewritten to match.

- **C2′ (RATIFIED — supersedes the C2 direct-`/metrics`-primary mechanism + the R2 "single-replica"
  line).** The drop counter is process-local; a POST increments one pod's counter but a separate
  `GET /metrics` can be routed to the OTHER pod → false negative. So the **PRIMARY** verification for
  (g)/(h)/(t-c) is PromQL **`sum(gc_telemetry_pii_attributes_dropped_total{kind=…})`** which
  AGGREGATES across both pods → correct regardless of routing. Both pods are separate scrape targets
  via the **static `prometheus-config.yaml`** (`role: pod`, keep `app=gc-service`, global
  `scrape_interval: 15s`) — NOT the ServiceMonitor, which is inert in Kind (no prometheus-operator
  CRD). Wrapped in `assert_eventually(MetricsScrape)` (30 s budget adequate at 15 s interval — not
  bumped), absent/empty vector ⇒ 0 (R2). R7 distinct-`{kind}` stays and is MORE load-bearing (each
  `sum`-series has exactly one writer in the suite).
- **C1′ (RATIFIED — supersedes the C1 "61 requests" count).** The telemetry GCRA limiter is
  **per-pod, in-memory** (`RateLimiter::keyed` in each pod's `TelemetryState::from_config`,
  `handlers/telemetry.rs:90-109`; no shared store), so a single `sub`'s requests load-balanced
  across 2 pods get ~½ each and 61 trips neither pod's 60-burst. Send **M = 130 CONCURRENT** requests
  (pigeonhole: M over 2 pods ⇒ busiest ≥ ⌈M/2⌉ = 65 > 60 ⇒ ≥1 `429` for any split). **M is coupled
  to replicas × burst** — the greppable in-test comment (verbatim) states "M=130 > replicas(2) ×
  telemetry burst(60) = 120 … Recompute M > replicas × burst if either moves (3 replicas ⇒ M ≥ 181)"
  so it can't silently rot if GC scales.

- **Observation (task #10 design property, NOT this backfill's scope):** because the limiter is
  per-pod in-memory, the EFFECTIVE per-user cluster rate limit is ≈ `replicas × 60/min`
  (≈120/min at replicas=2), not 60/min — a single user's requests are limited independently on each
  pod they land on. Documented here as an observation only; this backfill does not change production
  behavior. (A shared/Redis-backed limiter would be the fix if a true cluster-wide per-user cap is
  ever required — out of scope.)

---

## Pre-Work

None.

---

## Implementation Summary

Added 11 env-tests for the GC telemetry proxy in a new `crates/env-tests/tests/31_gc_telemetry.rs`,
one fixture helper, one dev-dependency, and one reverse-pointer ConfigMap comment.

Tests (all gated `#![cfg(feature = "flows")]`; PII drop-counter asserts additionally
`#[cfg(feature = "observability")]`):
- Metrics: (a) authed→202, (b) unauth→401, (c) service-token→401, (d) 300 KiB oversize→413,
  (e) `application/json`→415, (f) M=130 concurrent→≥1 429 (C1′), (g) non-allowlisted RESOURCE attr→202
  + `sum(…{kind="resource"})` rises (C2′), (h) non-scalar value on allowlisted DATAPOINT key→202 +
  `{kind="datapoint"}` rises.
- Traces: (t-a) authed→202, (t-b) unauth→401, (t-c) non-allowlisted SPAN attr→202 + `{kind="span"}`
  rises.

Key mechanisms as ratified at Gate 1 + the in-impl 2-replica supersession (see §Planning):
- **R1 oracle:** PII filtering verified via GC's `gc_telemetry_pii_attributes_dropped_total{kind}`
  counter (collector debug exporter is `verbosity: normal` → can't read attributes back). A 202 ⇔
  collector accepted the forward, so every 202 assert is the matrix-b collector-down detector.
- **C2′:** drop counter read via PromQL `sum(…{kind})` aggregated across the 2 GC replicas, polled
  with `assert_eventually(MetricsScrape)`, absent/empty ⇒ 0 (R2). Distinct `{kind}` per filter test
  so each sum-series has one writer (R7).
- **C1′:** M=130 concurrent (`tokio::task::JoinSet`) > replicas(2) × burst(60); primary assert
  `≥1 of M is 429` (collector-independent), secondary `≥1 is 202`; diagnostic counts on failure.
- Synthetic planted values (R6); dedicated `RATELIMIT_USER` isolates (f)'s per-`sub` window (matrix d);
  `OnceCell`-shared user keeps registrations to 2/run (R3, AC limit 100/min/IP).

Compile status (local, pre-cluster): `cargo test -p env-tests --features all --no-run` and
`--features flows --no-run` both green; `cargo clippy -p env-tests --tests` clean under both feature
sets. Runtime behavior is exercised at Gate 2 / Layer 7 against the live Kind cluster.

Deviation note: skipped the optional `deployment.yaml:10` reverse-pointer edit (not in the
classification table; would trip the scope-drift guard / need a 5th ops-owned row). The in-test
coupling comment forward-references `deployment.yaml:10` and the ConfigMap comment covers the burst
knob, so the coupling is documented without touching an untracked file.

---

## Files Modified

| File | Change | Owner |
|------|--------|-------|
| `crates/env-tests/tests/31_gc_telemetry.rs` | NEW — 11 tests + scaffolding + OTLP builders + C2′ PromQL helper | test (paired) |
| `crates/env-tests/src/fixtures/gc_client.rs` | `+raw_ingest_telemetry(signal_path, token, content_type, body)` | test |
| `crates/env-tests/Cargo.toml` | `+opentelemetry-proto` (dev-dep) | test |
| `infra/services/gc-service/configmap.yaml` | reverse-pointer coupling comment (R4 ii) | operations |
| `scripts/layer-all.test.sh` | mode 100644→100755 (pre-existing blocker; see §Cross-Boundary Gate-2 note) | operations |

Matches the 4-row §Cross-Boundary Classification exactly (scope-drift guard: only these 4 files in
the diff).

---

## Devloop Verification Steps

**Gate 2: GATE2=PASS** (`gate2-verdict` RUN_AT 2026-06-30T21:13:33Z, commit-3 run). Layer 1 compile
OK, Layer 2 fmt OK, Layer 3 guards OK (incl. cross-boundary classification + scope on the 4-file
table), Layer 4 **3065 unit tests / 0 fail**, Layer 5 clippy OK, Layer 6 audit N/A, **Layer 7 OK
(`env-tests-passed`)** — `31_gc_telemetry` **11/11** against the live Kind cluster. This required the
three env-test-infra fixes landed in the 2026-06-30 devloops (see §Issues): the `.dockerignore`
keystone (`d54143c`), the Phase-1f Prometheus IFS gate + the GC→collector egress netpol (`cefd279`).
Committed legitimately (no `--no-verify`).

---

## Code Review Results

**Gate 1: all 7 reviewers confirmed the plan** (Security, Test [paired], Observability, Code Quality,
DRY, Operations, Semantic Guard) after one revision round (R1 collector-substitution oracle, R2/R6/R7,
C1–C3, classification relabel — see §Gate 1). Cross-boundary owner involvement (ADR-0024 §6.3) is
satisfied via `--paired-with=test` + the commit's `Approved-Cross-Boundary: test`/`operations`
trailers.

A separate Gate-3 per-reviewer diff review was **not** independently re-run: this devloop was paused
mid-Gate-2 for the env-test-infra detour and then closed pragmatically (user-directed) once the
infra fixes unblocked it. The deliverable is validated by **GATE2=PASS + 11/11 live** on top of the
Gate-1 plan approval. The implementation matches the approved plan (the C1′/C2′ 2-replica corrections
were ratified by test + observability during implementation; see §Gate 2 PAUSED notes).

| Reviewer | Basis |
|----------|-------|
| Security | Plan confirmed (R1 PII-safe oracle, 401 boundary, synthetic markers) |
| Test (paired) | Plan confirmed; C1′/C2′ ratified; validated 11/11 live |
| Observability | Plan confirmed (R2 absent-⇒-0, drop-counter oracle) |
| Code Quality | Plan confirmed (ADR-0004/0014/0030/0032; classification relabel) |
| DRY | Plan confirmed (fixture/builder reuse) |
| Operations | Plan confirmed (layer7 integration, R4 coupling) |
| Semantic Guard | Plan confirmed (test-only; synthetic planted values) |

---

## Accepted Deferrals

- (none remain in this diff)

Scope decisions (NOT deferrals): collector-emission verification for AC/MC/MH (→ tasks #25/#26/#27),
`/v1/logs` (reserved-not-routed), camelCase wire-key env-test (covered by #46/#49/#51) — all
explicitly out of scope per the task brief. The env-test-infra blockers this devloop surfaced
(disk / Phase-1f / 502) were FIXED (not deferred) in the 2026-06-30 devloops; the residual
per-slug-image-tagging (B) + `cmd_rebuild` disk-guard gap (C-followup) are tracked in `docs/TODO.md`
§Devloop Container Resource Hygiene.

---

## Rollback Procedure

1. Start commit: `f34f3a666fd32aa8d7f0d1b8db1923f1456104de`
2. Review: `git diff f34f3a6..HEAD`
3. Soft reset: `git reset --soft f34f3a6`
4. Hard reset: `git reset --hard f34f3a6`

(Test-only changes — no schema or infra rollback concerns.)

---

## Issues Encountered & Resolutions

The deliverable (the 11 env-tests) was correct early, but Layer 7 could not run them against a live
cluster — a cascade of latent env-test-infra bugs, each uncovered by fixing the previous one. All were
root-caused and fixed in dedicated 2026-06-30 infra devloops; this devloop was PAUSED until they landed.

1. **Cluster rebuild exhausted host disk** (`cluster-rebuild-failed`, "no space left on device"): the
   service Dockerfiles `COPY . .` with no `.dockerignore`, copying the 27 GB host `target/` per build.
   Fixed by the `.dockerignore` keystone (+ `cleanup()` GC + a pre-build disk guard) — `d54143c`.
2. **Phase-1f Prometheus `/-/ready` gate failed deterministically** even at a 900s budget, though
   Prometheus was healthy. Root cause: IFS word-split bug — `layer7.sh` runs under `IFS=$'\n\t'`, so the
   unquoted `$HTTP_PROBE` curl string wasn't split into argv → exit 127 every probe. Fixed by array-split
   in `__wait_http_ready` — `cefd279`.
3. **GC telemetry forward → 502** (surfaced once Phase-1f passed and the suite finally ran): GC's egress
   NetworkPolicy lacked the symmetric `→ otel-collector:4318` allow (task #10 added the forward +
   collector-ingress but not GC-egress). Fixed by adding the egress rule — `cefd279`.

The env-tests' fail-loud design (unconditional 202 asserts; precondition `.expect()`s) correctly surfaced
each layer of the problem rather than masking it — exactly the matrix-(b) behavior the plan required.

---

## Lessons Learned

1. **A test suite that has never run against the real target hides infra bugs.** Three latent
   env-test-infra defects (disk, the Phase-1f IFS gate, the asymmetric netpol) sat dormant because the
   pipeline never reached the live suite — each surfaced only as the prior was fixed. Adding the first
   real cluster-observable tests for an endpoint pays for itself by exercising that whole path.
2. **Fail-loud test design is what made the cascade diagnosable.** Unconditional 202 asserts + precondition
   `.expect()`s turned each infra failure into a precise, attributable signal instead of a silent skip.
3. **Self-tests with single-token fakes miss word-splitting/quoting bugs.** `layer7.test.sh` injected a
   single-token fake probe, so the `IFS`-sensitive multi-word real-curl path went untested for 6 weeks; the
   added regression case (multi-word probe under strict IFS) closes that class.
4. **Zero-trust NetworkPolicies need symmetric ingress+egress, and a forward-path feature must add both.**
   Task #10 added the collector-ingress allow but not the GC-egress allow → 502.
5. **Multi-replica + in-memory rate limiter changes test invariants** (C1′ M=130 pigeonhole; C2′ PromQL
   `sum by(kind)` across replicas) — verify deployed topology, don't assume single-replica.
