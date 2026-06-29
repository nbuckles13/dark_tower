# Client SDK Metrics Catalog

**Service**: Browser SDK (`@darktower/sdk-core`)
**Implementation**: `packages/sdk-core/src/telemetry/` (sinks, name guard, providers)
**Job Label**: `darktower-sdk-core` (OTel resource `service.name`)

> ## Story 1 stub — full catalog deferred
>
> This catalog documents the `dt_client_*` join-flow metrics **defined** by the
> browser-client-join story (R-25) and the `dt_client_*` naming convention
> (R-24/R-27). It is a **stub**: task #12 lands the telemetry **scaffolding**
> only (the `MetricsSink` contract + sinks, the name guard, the single global
> `MeterProvider`/`WebTracerProvider`, the trace-injection helper, the
> bounded-event logger). The full catalog — buckets, SLOs, dashboards, alerts —
> is deferred to a later story.

> ## Declared-vs-emitted honesty
> (mirrors the `gc-service.md` §Telemetry-Proxy convention)
>
> **Every metric below is DECLARED here but NOT YET EMITTED.** The emission
> sites (the `meter.createCounter(...).add(...)` / `.record(...)` calls in the
> join flow) land in tasks **#13/#14**. Until then, no `dt_client_*` series
> exists in any backend. **Alert / dashboard authors: do NOT key a PromQL
> selector on any value below yet — it cannot appear.** The metric NAMES + label
> spellings here are the contract those later tasks implement; treat this as the
> wire contract, not a live signal.

All client SDK metrics follow ADR-0011 naming conventions with the `dt_client_`
prefix (ADR-0028 §9). They are emitted via the OTel JS `Meter` (production
`OtelMetricsSink`) and exported OTLP-HTTP/proto to the GC telemetry proxy
(`POST /api/v1/telemetry/v1/metrics`).

---

## Naming convention (R-24, R-27)

- **Prefix**: every client application metric name MUST match the compiled
  guard regex `^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$` — a `dt_client_`
  prefix, a lowercase-letter first body char, `[a-z0-9_]` body, no trailing
  underscore, ≤ 64 chars total. This is enforced in **two** lockstepped places:
  1. **Runtime** — the sink boundary (`nameGuard.ts`): a non-compliant name
     **throws** in dev/test and **warns + drops** in prod.
  2. **CI (static)** — the `dt-guard` `ts-name-guard-dt-client` rule
     (`crates/dt-guard/src/ts_metric_naming.rs`). The runtime regex is copied
     VERBATIM from the compiled CI rule so a name that passes one passes the
     other (a regression test in `nameGuard.test.ts` locks this — note the
     `{0,53}` doc-form that appears elsewhere DIFFERS on a trailing underscore;
     the compiled `{0,52}…[a-z0-9]` form is authoritative).
- **Implicit labels** (R-25): every metric carries `client_version`,
  `meeting_id_hash`, and `org_id`. **Never** label by `user_id`, `email`, `ip`,
  `user_agent`, or the **raw meeting id**. `meeting_id_hash` is a
  SHA-256-truncated digest, never the raw code (R-23). The SDK does NOT
  auto-attach implicit labels at the sink (`OtelMetricsSink` passes
  caller-supplied labels straight through) — `MeetingSession.join` computes the
  label set ONCE and threads it into every emission site (the facade, plus
  `MediaTransport` for `mh_connection_total`).
- **Client-originated label conventions** (task #14 — the client ORIGINATES
  these; there is no server-side equivalent to mirror):
  - `meeting_id_hash` = **full SHA-256 over the `meetingId` UUID** (from
    `JoinMeetingResponse.meetingId`, NOT the low-entropy 12-char meeting CODE),
    hex-encoded, **truncated to 16 hex chars (64 bits)**. No salt (the UUID is
    122-bit). Computed via `crypto.subtle.digest('SHA-256', …)` (R-32: a digest,
    not randomness). The same digest is reused across all metrics (and the R-26
    logs, when wired) so logs↔metrics correlate.
    - **Pre-join-resolution sentinel** (task #14): the digest needs the `meetingId`
      UUID, which is only known AFTER the GC `joinMeeting` response. A join that
      fails BEFORE that point (`signup` / `gc_join` / `gc_create_token` stages) is
      emitted with the sentinel `meeting_id_hash="none"`. So `meeting_id_hash="none"`
      is the expected, bounded value for early-stage failures — not a bug or a
      cardinality leak (it is a single fixed series).
  - `org_id` = the org **subdomain** (PLAIN, not hashed — a public, org-level,
    low-cardinality, non-PII identifier). NOTE: this is the client-visible
    subdomain, NOT the server's org UUID; correlating client↔server metrics by
    org requires the subdomain→uuid mapping. (If a real org UUID ever lands in an
    API response, swap it — non-blocking follow-up.)

---

## Join-flow metrics (R-25)

### `dt_client_join_attempts_total`
- **Type**: Counter
- **Description**: Total client-side meeting-join attempts, by outcome and the
  stage a failure occurred at.
- **Labels**:
  - `status`: `success`, `failure`
  - `failure_stage`: `none` (on success), `signup`, `gc_create_token`,
    `gc_join`, `mc_signaling_connect`, `mc_join_response`, `mh_connect`,
    `internal`
- **Cardinality**: Low (2 statuses × 8 stages = 16, before implicit labels).
- **Emission honesty** (task #14 — which stages the browser client actually
  emits):
  - `gc_create_token` is **RESERVED, never emitted by this client**. The browser
    issues a single `joinMeeting` POST; the token-mint and join happen server-side
    behind it, so the client cannot observe that sub-stage separately. It is kept
    in the catalog only for parity with the server-side join pipeline.
  - `mc_signaling_connect` vs `mc_join_response` are **distinct**:
    `mc_signaling_connect` = the signaling channel never opened / no server response
    arrived — the LOCAL `SignalingErrorCode`s `Transport`, `Framing`, `Timeout`;
    `mc_join_response` = the channel opened but the server **rejected** the join via
    an `ErrorMessage` — every server-authored `SignalingErrorCode` (`Unauthorized`,
    `Forbidden`, `NotFound`, `Conflict`, `InternalError`, `CapacityExceeded`, …).
    The client splits on local-vs-server origin, so a server `UNAUTHORIZED`/`NOT_FOUND`
    is `mc_join_response`, while a connect timeout is `mc_signaling_connect`.
- **Usage**: client-side join success rate + failure-stage distribution
  (complements the server-side `gc_meeting_join_*` family — the client sees
  stages the server cannot, e.g. `mc_signaling_connect`, `mh_connect`).

### `dt_client_time_to_signaling_ready_ms`
- **Type**: Histogram
- **Description**: Wall-clock ms from `MeetingSession.join` to the MC
  `JoinResponse` being received (signaling readiness).
- **Labels**: implicit only.
- **Buckets**: TBD (deferred — full catalog).
- **Usage**: client-perceived time-to-signaling-ready; the join-latency SLI the
  server-side handler timing cannot capture (includes browser→MC RTT).

### `dt_client_time_to_first_mh_connected_ms`
- **Type**: Histogram
- **Description**: Wall-clock ms from `MeetingSession.join` to the first MH
  connection being established.
- **Labels**: implicit only.
- **Buckets**: TBD (deferred).
- **Usage**: client-perceived time-to-media-path-ready.

### `dt_client_signaling_connection_total`
- **Type**: Counter
- **Description**: Client signaling (browser→MC) connection outcomes.
- **Labels**:
  - `status`: `success`, `failure`
  - `close_reason`: **bounded enum** (NOT the raw close string — see below).
- **`close_reason` bounded value set** (cardinality + PII boundary): `normal`,
  `going_away`, `auth_failed`, `timeout`, `server_error`, `unknown`. The raw
  WebTransport / server close reason string is FREE-FORM (unbounded cardinality
  + a PII vector) and is **never** emitted as a label. The client normalizes a
  numeric close **code** into this enum via `normalizeCloseReason` (
  `packages/sdk-core/src/telemetry/closeReason.ts`); any unmapped code collapses
  to `unknown`.
- **Cardinality**: Low (2 statuses × 6 close reasons = 12, before implicit
  labels).
- **Emission + success-sentinel semantics** (task #14): emitted FACADE-DERIVED
  from `MeetingSession.join` off the `signaling.join()` resolve/reject (no
  telemetry plumbing into the shipped `SignalingClient`). On **success**,
  `close_reason` is the sentinel `normal` ("no abnormal close during join") — so
  the success×{auth_failed,timeout,…} cells are unreachable by design; dashboards
  should not read them as a gap. On **failure**, `close_reason` is mapped from the
  bounded `SignalingErrorCode` (`Unauthorized`/`Forbidden`→`auth_failed`,
  `Timeout`→`timeout`, `InternalError`→`server_error`, else
  `normalizeCloseReason(closeCode)`) — the join-reject `SignalingError` usually
  carries no numeric `closeCode`, so the code-based mapping alone would collapse
  to `unknown`; the `SignalingErrorCode` mapping preserves the real signal on the
  common failure paths.

### `dt_client_mh_connection_total`
- **Type**: Counter
- **Description**: Client media-handler (browser→MH) connection outcomes.
- **Labels**:
  - `status`: `success`, `failure`
  - `mh_index_bucket`: bounded bucket of the MH peer index — `0`, `1`, `2+`
    (the raw index is bucketed so cardinality stays bounded as the MH fan-out
    grows).
- **Cardinality**: Low (2 statuses × 3 buckets = 6, before implicit labels).

---

## Bounded-event structured logs (R-26)

Console-only this story (the GC `/api/v1/telemetry` **log** signal is deferred).
The join logger (`logger.ts`) emits a fixed-shape record with a bounded `event`
enum: `join.started`, `join.signup_complete`, `join.gc_token_received`,
`join.signaling_ready`, `join.mh_connected`, `join.failed`, `join.completed`.

- **Required fields**: `meeting_id_hash`, `client_version`, `trace_id`, `event`,
  `duration_ms`, `failure_stage` (`duration_ms` / `failure_stage` only when
  applicable).
- **EXCLUDED** (never logged): email, password, **raw meeting id**, JWTs, MLS
  keys, user-agent (above DEBUG). The logger projects only the allowlisted
  fields, so a structurally-wider argument cannot leak PII (R-23 / R-26).

> **Scope** (R-26): governs *structured logs* (browser console / GC telemetry
> log signal) only. The server-side `auth_events` audit table on AC (email + IP
> + user-agent for sign-up/login) is a database audit trail per ADR-0020 —
> unchanged, NOT a structured-log channel.

---

## Telemetry configuration (R-19, R-24)

- **Single global providers**: one `MeterProvider` + one `WebTracerProvider`,
  configured once via `configureTelemetry({ telemetryEndpoint, env })`
  (`telemetryConfig.ts`).
  - **R-22 note**: the story names `MeetingSession.configure` as the entry
    point. `MeetingSession` (R-22) does not exist yet; the config entry point
    lives in `configureTelemetry` for now, and **R-22's `MeetingSession.configure`
    will delegate to it** (one line) — the named-in-spec API stays traceable.
- **Resource attributes**: `service.name=darktower-sdk-core`,
  `service.version=__SDK_VERSION__` (build-time define),
  `service.namespace=darktower`, `deployment.environment=<env>`.
- **Trace propagation (R-19)**: the global `W3CTraceContextPropagator` is
  registered at configure time. `injectIntoClientMessage` populates W3C
  `trace_parent` / `trace_state` (proto fields 20/21) on BOTH outbound
  `ClientMessage` (→ MC) and `MhClientMessage` (→ MH) envelopes via ONE
  injection path. The `dt_client.join` root span itself is created by a later
  task; this story provides the helper + providers.
- **OTLP export + keepalive (R-24)**: metrics export OTLP-HTTP/proto to
  `${telemetryEndpoint}/v1/metrics`. **keepalive** is honored by the OTel
  browser fetch transport **adaptively** (on by default; it backs off only above
  the browser's ~60 KB / 9-concurrent cumulative keepalive budget). It is NOT a
  constructor option on the modern browser exporter — there is no flag to pass.
  The join-flow payloads are tiny, so keepalive is active in practice (metrics
  survive a page-unload / navigation mid-join), which is R-24's intent.
  - **Verifiable path** (traced @ `@opentelemetry/exporter-metrics-otlp-proto`
    0.219.0): the package's `browser` field resolves the BROWSER exporter entry
    → `createOtlpFetchExportDelegate` → `createFetchTransport`, whose
    `FetchTransport.send()` sets the W3C-`fetch` `keepalive` flag per request
    against the browser budget. This is distinct from — and we deliberately do
    NOT use — the node exporter's `keepAlive` HTTP-agent connection-pooling
    option (wrong concept for a browser SDK).

---

## References

- **ADR-0011**: Observability standards and metric naming conventions
- **ADR-0028 §9**: Client architecture — `dt_client_` prefix
- **ADR-0024 §6.5**: Cross-boundary ownership (Pattern B named convention author)
- **User story**: `docs/user-stories/2026-05-02-browser-client-join.md` (R-19,
  R-24, R-25, R-26, R-27)
- **Implementation**: `packages/sdk-core/src/telemetry/`
- **CI guard**: `crates/dt-guard/src/ts_metric_naming.rs` (`ts-name-guard-dt-client`)
