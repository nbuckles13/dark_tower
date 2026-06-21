# Devloop Output: GC Telemetry Proxy Endpoint

**Date**: 2026-06-21
**Task**: GC telemetry proxy endpoint (R-2, R-3 remaining, R-51 partial) — `POST /api/v1/telemetry/v1/{metrics,traces}` behind `require_user_auth`, OTLP-proto decode + 12-key allowlist PII filter, governor rate limit, forward to `otel_collector_endpoint`.
**Specialist**: global-controller
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task10-5-26`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `53464c0ee76f9fa363583dc4288db3e57c967111` |
| Branch | `feature/browser-client-join-task10-5-26` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `global-controller` |
| Iteration | `1` |
| Security | `RESOLVED-FIXED` |
| Test | `RESOLVED-FIXED` |
| Observability | `RESOLVED-FIXED` |
| Code Quality | `RESOLVED-FIXED` |
| DRY | `CLEAR` |
| Operations | `RESOLVED-FIXED` |
| Semantic Guard | `RESOLVED-FIXED` |

### Gate 3 — Final Approval (cleared 2026-06-21)

All 7 reviewers passed. Gate 2 pipeline (`layer-all.sh`) green: Layers 1–5 OK,
L4 tests green, L6 audit clean (`cargo audit` 0 vulns; `governor 0.10.4`,
`opentelemetry-proto 0.7.0`), L7 env-tests N/A (no Kind cluster in this env;
unit + integration suites cover the proxy).

| Reviewer | Verdict | Findings (fixed in-diff) |
|----------|---------|--------------------------|
| Security | RESOLVED-FIXED | CRUX-A (PII value-smuggling via non-scalar `AnyValue` on allowlisted keys) + point-B size-cap re-touch |
| Test | RESOLVED-FIXED | F1 (oversize-413 emitted no metric — pre-handler layer reject) + CRUX-A coverage + F2 |
| Observability | RESOLVED-FIXED | Obs-D1 (dashboard rate→increase, ADR-0029) + Obs-D2 (401/413 metric-path completeness + catalog honesty) + F1/F2 (Gate-1) |
| Code Quality | RESOLVED-FIXED | delta re-review of CRUX-A gate + Option-1 ceiling; all ADRs pass |
| DRY | CLEAR | no true-dup; 2 extraction opportunities TODO-tracked |
| Operations | RESOLVED-FIXED | `rejected_auth` declared-but-unemitted catalog honesty + error-aggregation triage note |
| Semantic Guard | RESOLVED-FIXED | metrics-path-completeness (pre-handler 401/413 exits) |

**Lead design adjudication**: the metric-coverage fix landed as **Option 1**
(handler emits `rejected_size` at the exact `max_bytes` pre-decode gate;
`DefaultBodyLimit` raised to a `2×max_bytes` saturating DoS backstop;
`rejected_auth` declared-but-unemitted, 401s observable via
`gc_http_requests_total{401}` + `gc_jwt_validations_total`). Both lane-owners
(security = size-cap, observability = taxonomy) endorsed Option 1; test withdrew
its middleware proposal after security identified an unbounded-`payload_kind`
cardinality defect in a layer/middleware emitter. `status="error"` collapse
(415/400/502/503) approved by observability (HTTP status_code discriminates).

---

## Task Overview

### Objective
Add `POST /api/v1/telemetry/v1/{metrics,traces}` proxy endpoint to GC. Behind `require_user_auth`. Pipeline:
1. Size check (`telemetry_proxy_max_bytes`, default 256 KiB) → 413 on overflow
2. Content-Type `application/x-protobuf` → 415 otherwise
3. Per-JWT-`sub` rate limit via `governor` (`telemetry_proxy_rate_limit_per_minute`, default 60) → 429
4. Decode OTLP/HTTP-proto, drop attributes outside the 12-key allowlist (PII filter), re-serialize
5. Forward via `reqwest` to `otel_collector_endpoint` → 502 on collector 4xx/5xx/timeout
6. 202 Accepted on success

12-key allowlist: `client_version`, `service.name`, `service.version`, `meeting_id_hash`, `org_id`, `dt.event`, `dt.duration_ms`, `dt.failure_stage`, `dt.close_reason`, `dt.mh_index_bucket`, `http.status_code`, `error.code`.

New files: `handlers/telemetry.rs`, `services/telemetry_filter.rs`, `services/telemetry_forwarder.rs`.
New Config keys: `otel_collector_endpoint`, `telemetry_proxy_max_bytes`, `telemetry_proxy_rate_limit_per_minute`.
New `GcError::PayloadTooLarge` (413) + `GcError::BadGateway` (502); also need 415 (UnsupportedMediaType).
Metrics (R-51 partial): `gc_telemetry_ingest_total{status, payload_kind}`, `gc_telemetry_ingest_duration_seconds{status}`, `gc_telemetry_payload_bytes{payload_kind}`, `gc_telemetry_rate_limited_total{reason}`, `gc_telemetry_pii_attributes_dropped_total{kind}`.

### Scope
- **Service(s)**: global-controller
- **Schema**: No
- **Cross-cutting**: New deps (`governor`, `opentelemetry-proto`) in workspace + gc-service `Cargo.toml`.

### Debate Decision
NOT NEEDED — design is fully specified in the user story (task #10, design §global-controller).

---

## Cross-Boundary Classification

Per-file classification. All gc-service files are **Mine**. Workspace `Cargo.toml` dep
additions are **Mine** (workspace `Cargo.toml` is NOT a Guarded Shared Area per the task
brief). No GSA edits, no other-service edits.

| File | Mine / Not mine | Judgment tier | Notes |
|------|-----------------|---------------|-------|
| `Cargo.toml` (workspace) | Mine | Minor-judgment | Add `governor` + `opentelemetry-proto` to `[workspace.dependencies]`. Not GSA; @security + @operations scrutinize the new deps (justified below). |
| `crates/gc-service/Cargo.toml` | Mine | Mechanical | Reference the two new workspace deps. |
| `crates/gc-service/src/config.rs` | Mine | Minor-judgment | Add 3 telemetry Config keys + `DEFAULT_*` consts + `ConfigError` variants + Debug + parse/validate + unit tests, following the existing pattern. |
| `crates/gc-service/src/errors.rs` | Mine | Minor-judgment | Add `PayloadTooLarge` (413), `UnsupportedMediaType` (415), `BadGateway` (502); update `status_code()`, `error_type_label()`, `IntoResponse`, tests. |
| `crates/gc-service/src/handlers/telemetry.rs` (new) | Mine | Domain-judgment | The proxy handler(s): size → content-type → rate-limit → decode+filter → forward → 202. |
| `crates/gc-service/src/handlers/mod.rs` | Mine | Mechanical | Register `telemetry` module + re-export handlers. |
| `crates/gc-service/src/services/telemetry_filter.rs` (new) | Mine | Domain-judgment | Pure 12-key allowlist PII filter; heavily unit-tested; emits dropped-attr counts. |
| `crates/gc-service/src/services/telemetry_forwarder.rs` (new) | Mine | Domain-judgment | reqwest forwarder to `otel_collector_endpoint`; 502 on collector 4xx/5xx/timeout. |
| `crates/gc-service/src/services/mod.rs` | Mine | Mechanical | Register the two new service modules. |
| `crates/gc-service/src/routes/mod.rs` | Mine | Minor-judgment | Wire the 2 routes under `require_user_auth`; add per-user rate-limiter to `AppState` (or handler-local shared state). |
| `crates/gc-service/src/observability/metrics.rs` | Mine | Domain-judgment | 5 new recording fns + histogram bucket config (200ms p99 SLO) + ADR-0032 in-src cluster tests. |
| `crates/gc-service/tests/telemetry_proxy_tests.rs` (new) | Mine | Domain-judgment | Integration: 202 / 401 / 413 / 415 / 429 / 502 via harness + wiremock collector. |
| `crates/gc-service/tests/telemetry_metrics_integration.rs` (new) | Mine | Domain-judgment | ADR-0032 MetricAssertion-backed metric assertions. |
| `crates/gc-service/src/main.rs` | Mine | Minor-judgment | Build `TelemetryState` from config, add to `AppState`, spawn the 60s rate-limiter eviction task (lifecycle spawn). |
| `crates/gc-test-utils/src/server_harness.rs` | Mine | Mechanical | Add `telemetry: TelemetryState` to the harness `AppState` construction (built from config defaults). |
| `crates/gc-service/tests/auth_tests.rs` | Mine | Mechanical | Add `telemetry` field to the in-file `AppState` construction. |
| `crates/gc-service/tests/meeting_create_tests.rs` | Mine | Mechanical | Add `telemetry` field to the in-file `AppState` construction. |
| `crates/gc-service/tests/meeting_tests.rs` | Mine | Mechanical | Add `telemetry` field to the two in-file `AppState` constructions. |
| `docs/observability/metrics/gc-service.md` | Mine | Minor-judgment | Add "Telemetry Proxy Metrics" section + cardinality rows. |
| `infra/grafana/dashboards/gc-overview.json` | Mine | Minor-judgment | Add a minimal "Telemetry Ingest" row (5 panels) so the new metrics satisfy `validate-application-metrics`; enriched in task #16. PromQL/types per ADR-0029 (counter→rate, histogram→`_bucket`+histogram_quantile, `$datasource`, `$__rate_interval`). |
| `docs/TODO.md` | Mine | Mechanical | Track the shared-client-builder DRY note (pointer bullet in main.md), per `validate-todo-tracking`. |

---

## Planning

### Problem framing (mechanism-language)

This is **not** "proxy telemetry." Precisely: **a sanitizing OTLP reverse-proxy with a
deny-by-default attribute allowlist**. The security-load-bearing property is the
deny-by-default filter — every attribute key NOT in the 12-key allowlist is dropped at
every nesting level before bytes leave GC toward the collector. Framing it as a "proxy"
under-sells that the filter is the reason this endpoint exists on GC rather than letting
browsers hit the collector directly: GC is the trust boundary that strips PII a browser
might attach (user_id, email, IP, raw meeting id, arbitrary span attributes). The rate
limit + size cap are abuse controls on an unauthenticated-origin-adjacent surface (it IS
behind `require_user_auth`, but the caller is a browser the user controls).

Framing gaps surfaced to reviewers:
- **Deny-by-default vs scrub-known-bad**: design says drop everything not allowlisted. We
  do NOT maintain a denylist. A new business attribute requires an allowlist edit — that's
  the intended friction.
- **Filter must survive re-serialization round-trip**: we decode → mutate in place → re-encode
  with prost. We forward the re-encoded bytes, never the original body, so a malformed/oversized
  nested structure cannot smuggle un-filtered attributes through.
- **`/v1/logs` is reserved**: decision below.

### Key design decisions

1. **Dependencies** — `opentelemetry-proto = 0.7` (features `gen-tonic-messages`, `metrics`,
   `trace`) is the version matching the workspace `opentelemetry = 0.24` line; it resolves with
   the workspace `prost = 0.13`. `governor` (latest 0.6/0.7 line) for the per-user token-bucket
   rate limit. Both added to `[workspace.dependencies]`, referenced from `gc-service/Cargo.toml`.
   - Justification for @security/@operations: `opentelemetry-proto` is the canonical OTLP message
     crate (we only pull message types, NOT the gRPC transport — we re-encode with prost and
     forward over reqwest, so no extra network stack). `governor` is the standard in-process
     rate-limiter (GCRA, no background task). Neither adds a C dependency.

2. **`/v1/logs` route** — **OMIT** the route this story (return Axum's default 404/405 for
   `/v1/logs`). Rationale: the design says logs is "reserved for a future story." Stubbing a
   route that returns 501 invites a half-built code path the PII filter would not cover (logs
   have a different attribute nesting: `LogRecord.attributes` + resource/scope). Omitting is the
   safer deny-by-default posture — the path simply does not exist until the logs filter exists.
   The `payload_kind="log"` metric label is still declared (bounded label space) but never emitted
   this story. **Confirming this choice with reviewers.**

3. **`otel_collector_endpoint` URL composition** (R-59) — **CONFIRMED by @operations: config holds
   the BARE BASE; GC owns the per-signal suffix.** Config value = bare base, NO path, NO trailing
   slash (e.g. `http://otel-collector.default.svc.cluster.local:4318`); forwarder appends the
   compile-time literal `/v1/metrics` or `/v1/traces` per matched route. One config key serves both
   signals. (Rejected: two per-signal keys — invites drift.)
   - **CROSS-TASK CONTRACT HAZARD (flagged to @team-lead)**: R-59/INFRA-OTEL currently writes the GC
     env value WITH the suffix baked in (`…:4318/v1/{metrics,traces}` template). If infra populates
     the var with a suffix AND GC appends one → `…/v1/metrics/v1/metrics` → collector 404 → 502 on
     every forward. **Contract: the env var MUST hold the bare base; GC appends.** INFRA-OTEL must
     populate the bare base, not the templated suffix form. Documented in a doc-comment on the
     `otel_collector_endpoint` Config field ("bare base, no `/v1/...` suffix — GC appends per-signal").
   - **Trailing-slash robustness (per @operations 1b)**: the suffix-join trims one trailing `/` from
     the configured base before appending, so a pasted `…:4318/` doesn't double-slash.
   - **Disabled-state / rollback posture (per @operations)**: `otel_collector_endpoint` defaults to
     the in-cluster collector DNS (`http://otel-collector.default.svc.cluster.local:4318`) so the
     standard deploy works unset. If set to EMPTY string, the route short-circuits to a clean **503**
     (telemetry disabled) WITHOUT crashing GC — config does NOT make the key required (no boot
     failure). This is the "feature-flag off" posture (story rollback note) without a new toggle key.

4. **Pipeline order** (fail-cheap-first): size cap → Content-Type 415 → per-`sub` rate limit 429
   → decode (malformed OTLP → **400 BadRequest**, `status=error` metric; 415 reserved for
   content-type) → filter → forward (502 on collector failure) → 202.
   - **Decode-failure status label PINNED to `error`** (per @code-reviewer #3): malformed/truncated
     OTLP → reuse the existing `GcError::BadRequest` (→400), `gc_telemetry_ingest_total{status="error"}`.
     NOT a new decode-specific GcError variant, NOT `rejected` (that's not a declared status value).
     The declared `status` set is exactly {success, rejected_auth, rejected_size, rejected_rate,
     rejected_pii, error}; decode failure maps to `error`.
   - **ADR-0004 double-version path is COMPLIANT-with-rationale** (per @code-reviewer #2): the route
     `/api/v1/telemetry/v1/{metrics,traces}` has two `v1` segments — the OUTER `/api/v1` is GC's API
     version (ADR-0004); the INNER `/v1/metrics`|`/v1/traces` is the FIXED OTLP/HTTP spec path
     (vendored, not our versioning). NOT an ADR-0004 violation. A one-line comment at the route
     definition will state this so a future reader doesn't read it as a double-version smell.
   - **Size cap is on REAL bytes, not Content-Length** (per @security B): the load-bearing gate
     is a `DefaultBodyLimit::max(telemetry_proxy_max_bytes)` layer on the telemetry routes, which
     returns 413 before the handler buffers the full body. Content-Length is at most a fast-path
     hint, never the enforced cap (client-supplied, can under-report / absent on chunked). A
     belt-and-suspenders explicit length check on the collected `Bytes` backs it up. 413 fires
     pre-decode.

5. **Rate limiter** — `governor::RateLimiter` keyed by JWT `sub` (`DashMap`-backed keyed state).
   Quota = `telemetry_proxy_rate_limit_per_minute` per minute. Stored in `AppState` (shared
   `Arc`); `sub` is the UUID string from `UserClaims.sub` (request extensions, never a header).
   - **Eviction (per @security A)**: the keyed map grows one cell per distinct `sub` and never
     shrinks on its own → memory-exhaustion vector. Mitigation: a lightweight tokio interval task
     (60s cadence, lifecycle-spawned in `main.rs`) calls `limiter.retain_recent()` to reclaim
     fully-replenished (idle) per-`sub` cells, bounding the map to ~active-users-per-minute.

6. **Metrics** — `payload_kind ∈ {metric, trace, log}` (log declared, unemitted). `status` per
   spec. `gc_telemetry_ingest_duration_seconds` buckets chosen for 200ms p99 SLO:
   `[0.005,0.010,0.025,0.050,0.100,0.150,0.200,0.300,0.500,1.0]`. `gc_telemetry_payload_bytes`
   buckets sized for ≤256 KiB: `[1024, 4096, 16384, 65536, 131072, 262144]`. PII-dropped counter
   `gc_telemetry_pii_attributes_dropped_total{kind}` where `kind ∈ {resource, scope, datapoint,
   span, span_event, span_link}` (bounded, NOT the dropped key — that would be unbounded/PII).
   `gc_telemetry_rate_limited_total{reason}` with `reason ∈ {per_user, per_org, global}`, only
   `per_user` wired.
   - **F1 (per @observability) — TWO distinct bucket matchers in `init_metrics_recorder()`**:
     `Matcher::Prefix("gc_telemetry_ingest")` → the seconds buckets (duration), AND a SEPARATE
     `Matcher::Prefix("gc_telemetry_payload")` → the byte buckets. Without the second matcher the
     `_payload_bytes` histogram silently inherits seconds-scale DEFAULT buckets and the 1KB–256KB
     range collapses into the overflow bucket (p99 payload size unobservable). Prefixes don't
     overlap (`gc_telemetry_ingest` vs `gc_telemetry_payload`). A metrics test asserts a mid-range
     payload (e.g. 50 KiB) lands in a mid bucket, not overflow.
   - **F2 (per @observability) — catalog honesty (ADR-0032 Step 5 "declared vs emitted")**: the new
     `docs/observability/metrics/gc-service.md` "Telemetry Proxy Metrics" section MUST mark, for
     this story: `status="rejected_pii"` declared-but-unemitted (drops-forward-as-success design);
     `payload_kind="log"` declared-but-unemitted (route omitted); `reason ∈ {per_org, global}`
     reserved/unwired (only `per_user`). Prevents task #16 alert authors writing PromQL against a
     value that can never appear. Metric NAME/label spelling is load-bearing — task #16's
     rejection-rate + absent-over-time alerts reference `gc_telemetry_ingest_total` by exact name.
   - **No PII in telemetry-path logging (R-26)**: handler/filter/forwarder tracing carries NO
     `sub`, NO attribute keys/values, NO body bytes in any span field or log line. Only bounded
     facts (payload_kind, status, counts, collector status code). `#[instrument(skip_all)]`.

7. **Forwarder timeout (per @operations — deploy-safety; INJECTABLE per @test G3)** —
   `telemetry_forwarder.rs` builds its `reqwest::Client` with an EXPLICIT bounded total timeout
   (default `TELEMETRY_FORWARD_TIMEOUT_SECS = 5`) + connect timeout (`2s`), mirroring `ac_client.rs`.
   reqwest's default is effectively unbounded; without the cap a slow/hung collector piles up GC
   handler tasks → GC exhaustion. On timeout/connect-failure/4xx/5xx → **502**, `status="error"`.
   - **Testability (per @test G3)**: the forwarder constructor takes the timeout as a parameter
     (defaulting to the 5s const in prod wiring) so the 502-timeout integration test can inject a
     ~100ms timeout against a wiremock delay — NO real wall-clock wait, NO flake. Not a Config key
     this story (the prod default is the const), but injectable at construction for tests.

8. **Exemplar `kind` bucket (pin per @test G1)** — exemplar `filtered_attributes` drops fold into
   `kind="datapoint"` (exemplars are sub-structures of a datapoint; a 7th label value for a rarely-
   used OTLP feature isn't worth the cardinality). So `kind` stays the 6-value set
   {resource, scope, datapoint, span, span_event, span_link}. Stated + tested.

9. **Metrics-completeness via record-on-drop guard (per @semantic-guard #1)** — `ingest_total` AND
   `ingest_duration_seconds` are emitted on EVERY exit path (202, 413, 415, 429, 400-decode, 502)
   via ONE guard, not scattered call sites. Implementation: a small RAII/guard struct created at
   handler entry holding `start: Instant` + a settable `status`/`payload_kind`; on `Drop` (or an
   explicit `finish(status)` at each return) it records `ingest_total{status,payload_kind}` +
   `ingest_duration_seconds{status}`. This makes it structurally impossible for a future edit to add
   an exit path that forgets to emit.
   - **(a) `payload_kind` on pre-decode rejects (413/415/429) derived from ROUTE, not content**:
     the metric/trace path is known from which handler matched, so `payload_kind` is a literal
     (`"metric"` or `"trace"`) even when the body was never decoded. Never derived from payload
     content → always a bounded literal.
   - **(b) duration recorded on every exit** including cheap rejects, via the single guard.

10. **Error/credential hygiene (per @semantic-guard #2 + #4)** —
    - Decode failure → 400 with a GENERIC client message; the prost `DecodeError` is preserved ONLY
      in the server-side `tracing` source, and that log line does NOT dump raw body bytes (the body
      is the PII we strip — it must never appear in a decode-error log).
    - 502 client message is generic; the collector URL appears only in SERVER logs, never in the
      client-facing 502 response.
    - **Fixed-depth traversal (per @semantic-guard #3)**: VERIFIED — the only recursive OTLP type is
      `AnyValue` (via `ArrayValue`/`KeyValueList` in attribute VALUES). Our filter operates on
      attribute KEYS only (`retain(|kv| ALLOWLIST.contains(kv.key))`) and NEVER descends into
      `kv.value`, so traversal is strictly fixed-depth over the structural message tree — no
      user-controlled depth multiplier, no recursion, no `spawn_blocking` needed at ≤256 KiB.
      Forwarder timeout is a client/request-level reqwest timeout (not a manual select).

### Filter traversal (verified against opentelemetry-proto 0.7 message types)

- **Metrics**: `ExportMetricsServiceRequest.resource_metrics[]` →
  `{ resource.attributes, scope_metrics[].{ scope.attributes, metrics[].data → one of
  Gauge|Sum|Histogram|ExponentialHistogram|Summary, each `.data_points[].attributes` } }`.
  (Exemplars carry `filtered_attributes` — also filtered, counted under `kind="datapoint"`.)
- **NON-UNIFORM DATAPOINT SHAPE (per @security code-review nuance, verified at exact lines)**: the
  5 metric data variants do NOT have a uniform datapoint type, and exemplars are NOT universal:
  - `Gauge`/`Sum` → `NumberDataPoint` (HAS `exemplars`, metrics.v1.rs:320)
  - `Histogram` → `HistogramDataPoint` (HAS `exemplars`, :423)
  - `ExponentialHistogram` → `ExponentialHistogramDataPoint` (HAS `exemplars`, :520)
  - `Summary` → `SummaryDataPoint` (**NO `exemplars` field** — :571; the Summary arm must NOT
    reference exemplars). All four datapoint types HAVE `.attributes`.
  → The per-variant walk matches on `metric.data` and handles each datapoint type's fields
  explicitly; it does NOT assume a shared datapoint struct. Unit tests cover all 5 variants.
- **Traces**: `ExportTraceServiceRequest.resource_spans[]` →
  `{ resource.attributes, scope_spans[].{ scope.attributes, spans[].{ attributes, events[].attributes,
  links[].attributes } } }`.
  `Span.status` is `{ message, code }` — NO attributes field (trace.v1.rs:403). Documented non-gap;
  a code comment at the span-status site states this so it doesn't read as forgotten (@security ask).
- Filter = `attrs.retain(|kv| ALLOWLIST.contains(kv.key.as_str()) && value_is_scalar(kv))`, counting
  drops per kind.
- **CRUX-A — value-shape filtering (deny-by-default on VALUE, not just KEY; @team-lead/@security
  crux)**: an allowlisted KEY can still smuggle PII in its VALUE. An attribute survives only if its
  value is a SCALAR (`StringValue`/`BoolValue`/`IntValue`/`DoubleValue`) or absent. A non-scalar
  value — `ArrayValue`/`KvlistValue` (nested attributes) or `BytesValue` (opaque blob) — causes the
  whole attribute to be DROPPED (and counted). We do NOT deep-recurse into the value (that would be
  an unbounded, attacker-controlled-depth traversal) — dropping the non-scalar is bounded and the
  safer deny-by-default posture. Unit tests (scalar survives; kvlist/array/bytes dropped) + an
  integration test (allowlisted `org_id` with a nested-kvlist PII value stripped from the forwarded
  bytes; raw-wire-bytes scan confirms the smuggled string is absent).

### Test plan (from @test plan review — all accepted)

**Integration (`tests/telemetry_proxy_tests.rs`)** — harness + wiremock collector:
- 202 happy path (valid OTLP, collector 200).
- **202-with-drops (@test #2, highest-value)** — ONE test asserting THREE things: (a) 202;
  (b) `gc_telemetry_pii_attributes_dropped_total` += expected count; (c) the body wiremock ACTUALLY
  RECEIVED, decoded, no longer contains the dropped key. (c) proves re-serialization from the
  filtered struct, not raw forward — capture forwarded body in wiremock and prost-decode it.
- 401 unauthenticated.
- **413 boundary (@test G4)** — exactly `max_bytes` → 202; `max_bytes + 1` → 413. PLUS a
  lying/absent Content-Length with an oversize real body → still 413 (the `DefaultBodyLimit` /
  real-bytes gate, not the header, is load-bearing — security-relevant bypass test).
- 415 wrong Content-Type.
- **429 (@test #1)** — inject quota=2 via Config; send 2 → both 202; 3rd same window → 429. Do NOT
  rely on wall-clock window rollover mid-test. PLUS per-`sub` isolation: a SECOND `sub` still gets
  202 after the first is throttled (proves keyed, not global).
- **400 decode-failure (@test G2)** — `application/x-protobuf` + garbage bytes → 400,
  `gc_telemetry_ingest_total{status="error"}` += 1, forwarder NEVER called (forward metric +
  dropped-attr counter `assert_unobserved`).
- **502 all three triggers (@test G3)** — (a) collector 5xx; (b) collector 4xx (confirm 4xx ALSO
  → 502, not passed through); (c) timeout via wiremock delay + injected ~100ms forwarder timeout.
- **Hard-form unobserved (@test #3)** — on 415/413/429/400 reject paths: assert
  `gc_telemetry_pii_attributes_dropped_total` AND forward-duration are `assert_unobserved` (the hard
  form, NOT `assert_delta(0)`) — filter/forward never ran. Catches always-emit refactor regressions.
- **Wire-side `.expect(0)` belt-and-suspenders (@test plan-confirm note)** — the 415, 400, and
  413-boundary reject-path integration tests mount the collector with wiremock `.expect(0)`
  (verified on MockServer drop), pinning "forwarder never called" from the WIRE side alongside the
  metric-side `assert_unobserved`. (429 mixes accepted + rejected requests, so wire-side expect-0
  is N/A there — per-sub isolation is its proof.)

**Metric integration (`tests/telemetry_metrics_integration.rs`)** — ADR-0032 MetricAssertion:
- per-status `gc_telemetry_ingest_total` deltas; payload-bytes histogram (50 KiB → mid bucket, not
  overflow — proves F1 matcher); duration histogram. Single snapshot per test (drain-on-read).
  `flavor="current_thread"` where any path uses `tokio::spawn` (thread-local recorder doesn't cross
  spawn).

**Unit:**
- **Filter (`services/telemetry_filter.rs`)** — drop a planted non-allowlisted key at EACH of the 6
  levels; allowlisted keys preserved; **per-kind adjacency (@test G1)**: drive a drop at each of the
  6 `kind` values and assert ONLY that kind's counter moved (`assert_only_<kind>`-style), catching
  label-swap bugs; exemplar drops counted under `kind="datapoint"`.
- **Config (@test G5)** — parse/validate/default for the 3 keys; reject
  `telemetry_proxy_rate_limit_per_minute=0` (would break governor `NonZeroU32`) and
  `telemetry_proxy_max_bytes=0` (define+test the documented rejection). Mirror config.rs pattern.
- **errors.rs** — status_code + error_type_label + IntoResponse for 413/415/502.

### Verified API facts (scratch-crate prototype, before implementation)

Confirmed in a throwaway crate that these compile + behave as designed against the
exact pinned versions:
- **Versions**: `opentelemetry-proto 0.7.0` resolves with `prost 0.13.5` (matches workspace
  `prost = 0.13`). `governor 0.10.4` (features `std`, `dashmap`) for the keyed limiter.
- **Keyed rate limiter**: `RateLimiter::keyed(Quota::per_minute(NonZeroU32::new(n)?))`,
  type `RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>`. `.check_key(&sub)`
  → `Ok(())` when within quota, `Err(_)` when exceeded; per-key buckets are independent
  (verified: 2-quota allows 2 then rejects the 3rd for one `sub`, a different `sub` still passes).
  Stored as `Arc<...>` in `AppState`.
- **prost round-trip**: `req.encode_to_vec()` → `Msg::decode(&bytes[..])` (trait `prost::Message`
  in scope) round-trips after `attributes.retain(...)`; dropped count = `len_before - len_after`.
  Forward the re-encoded bytes only.

### Security requirements (from @security pre-flight, all accepted)

1. **Filter recursion** — walk EVERY attribute-bearing level (enumerated above). `Span.status`
   has no attribute field in OTLP (`Status { message, code }` only — verified) → documented
   non-gap, not a skipped level. Exemplar field is `filtered_attributes` (verified). Unit tests
   plant a non-allowlisted key at each level and assert it's dropped.
2. **Size cap before decode** — route carries `DefaultBodyLimit::max(telemetry_proxy_max_bytes)`
   so oversize is rejected by the extractor pre-buffer; plus explicit collected-`Bytes` length
   check → 413. `prost::decode` runs only on already-bounded bytes.
3. **Rate-limit keying + eviction** — key = `UserClaims.sub` from request extensions (never a
   client header). Keyed `governor` map is bounded via a periodic `limiter.retain_recent()` sweep
   on a 60s tokio interval task spawned at startup (lifecycle spawn in `main.rs`), dropping
   fully-replenished (idle) keys → map bounded to ~active-users-per-minute.
4. **SSRF** — forward URL = `config.otel_collector_endpoint` (base, Config-only) + a compile-time
   literal suffix (`/v1/metrics`|`/v1/traces`) selected by matched route. No request-controlled
   bytes reach the URL.
5. **Error hygiene** — `BadGateway`(502) mirrors `ServiceUnavailable`: generic client message,
   collector details logged server-side only, collector response body never echoed.
6. **No-panic (ADR-0002)** — decode returns `Result` → 400; filter uses `retain`/iteration, no
   indexing/unwrap/expect on attacker input. crate already denies `unwrap_used`/`expect_used` in
   prod code.
7. **Deps (minimal features)** — `opentelemetry-proto = 0.7.0` features
   `["gen-tonic-messages","metrics","trace"]` (NO transport/logs/serde/zpages);
   `governor = 0.10.4` features `["std","dashmap"]`. Both pin prost 0.13.5 = workspace.

---

## Implementation Summary

Implemented to plan decisions #1–#10. All gc-service tests green (318 lib + 12 telemetry
proxy integration + 7 metrics integration + existing suites), `cargo clippy --all-targets`
clean (zero warnings; no-panic policy upheld), `cargo fmt --check` clean, `cargo check
--workspace` clean.

**Dependencies** (`Cargo.toml`): `opentelemetry-proto = 0.7` (`gen-tonic-messages`, `metrics`,
`trace`) + `governor = 0.10` (`std`, `dashmap`) added to `[workspace.dependencies]`; referenced
from `gc-service/Cargo.toml` along with `prost` + `bytes`. Resolves with workspace `prost 0.13.5`.

**Config** (`config.rs`): `otel_collector_endpoint` (default in-cluster DNS, bare-base
doc-comment), `telemetry_proxy_max_bytes` (262144), `telemetry_proxy_rate_limit_per_minute` (60).
Parse/validate/Debug/defaults + tests (reject 0 for both numeric keys; empty endpoint allowed →
disabled-state).

**Errors** (`errors.rs`): `PayloadTooLarge` (413), `UnsupportedMediaType` (415), `BadGateway`
(502) with status_code/error_type_label/IntoResponse + tests. BadGateway mirrors ServiceUnavailable
(generic client message, collector detail server-side only).

**Filter** (`services/telemetry_filter.rs`): deny-by-default 12-key `ALLOWLIST`, single
`filter_attrs` helper at every nesting site; `filter_metrics` (resource/scope/all 5 data variants
+ exemplars, Summary correctly has no exemplars) + `filter_traces` (resource/scope/span/event/link;
Status documented no-attrs). `DropCounts` aggregates per-kind then `emit()`s. 7 unit tests incl.
per-kind adjacency + all-5-variants.

**Forwarder** (`services/telemetry_forwarder.rs`): reqwest client (mirrors ac_client), INJECTABLE
total timeout (default 5s), bare-base + GC-owned per-signal suffix, trailing-slash trim,
4xx/5xx/timeout/connect-fail → 502. 9 unit tests.

**Handler** (`handlers/telemetry.rs`): `ingest_metrics`/`ingest_traces`, `#[instrument(skip_all)]`,
pipeline disabled→503 / size→413 / content-type→415 / per-`sub` rate→429 / decode→400 /
filter+re-encode / forward→502 / 202. `IngestGuard` record-on-drop emits ingest_total +
duration on EVERY exit; `payload_kind` from route. `TelemetryState::from_config` +
`spawn_rate_limiter_eviction` (60s `retain_recent` lifecycle task).

**Routes/wiring** (`routes/mod.rs`, `main.rs`): 2 routes under `require_user_auth` +
`DefaultBodyLimit::max(max_bytes)`; ADR-0004 double-version comment; `TelemetryState` on AppState;
eviction task spawned in main.rs. Test harness + 4 existing test-file AppState sites updated.

**Metrics** (`observability/metrics.rs`): 5 recording fns + 2 distinct bucket matchers
(`gc_telemetry_ingest` seconds, `gc_telemetry_payload` bytes — F1). In-src cluster tests +
`tests/telemetry_metrics_integration.rs` (per-status/per-kind cartesian, hard-form
`assert_unobserved` on reject paths, F1 byte-bucket-not-overflow render check).

**Catalog** (`docs/observability/metrics/gc-service.md`): "Telemetry Proxy Metrics" section with
declared-vs-emitted honesty notes (rejected_pii / payload_kind=log / reason per_org+global) and the
"name is task #16's contract" note. Dashboards/alerts left to task #16.

---

## Accepted Deferrals

- **`/v1/logs` route** — omitted this story (reserved for a future logs story); `payload_kind="log"` declared-but-unemitted. Deny-by-default: path does not exist until the logs filter exists.
- **`gc_telemetry_ingest_total{status="rejected_pii"}`** — declared-but-unemitted (drops-forward-as-success design); reserved for a future reject-if-PII mode.
- **`gc_telemetry_rate_limited_total{reason ∈ {per_org, global}}`** — reserved/unwired; only `per_user` wired this story.
- **Full dashboards + alerts** (`gc-overview.json` enrichment, `gc-alerts.yaml`) — task #16 (observability hand-off); this story adds only the minimal panels the metrics guard requires.
- **Configmap surfacing** of the 3 env vars — task #28/INFRA-OTEL. Cross-task contract: `OTEL_COLLECTOR_ENDPOINT` must be the BARE BASE (no `/v1/...` suffix); flagged to @team-lead for #28 coordination.
- **Smoke tests + runbook** — task #21/O-2.
- **Shared reqwest-client-builder extraction** (forwarder vs ac_client ~8-line overlap) → tracked in `docs/TODO.md` §Cross-Service Duplication (DRY), "Shared reqwest client-builder…" (task #10 follow-up, @dry-reviewer Pattern-B).
