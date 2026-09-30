# Client SDK Metrics Catalog

**Service**: Browser SDK (`@darktower/sdk-core`)
**Implementation**: `packages/sdk-core/src/telemetry/` (sinks, name guard, providers)
**Stored labels**: `job="otel-collector"`, `exported_job="darktower-sdk-core"` — the
SDK's `service.name` does **not** become `job`. Full shape in §Stored series shape.

> ## Scope
>
> This catalog covers the `dt_client_*` **join-flow** metrics (browser-client-join
> story, R-25) and the `dt_client_*` **media-path** metrics (ADR-0036 story 1).
> Buckets, SLOs, dashboards and alerts for the media path land with the
> observability and operations tasks of that story.
>
> **Both families are EMITTED.** The join-flow emission sites are in
> `MeetingSession` and `MediaTransport`; the media-path sites are the cached
> handles in `packages/sdk-core/src/media/setup/mediaMetrics.ts`. The
> declared-but-not-yet-emitted caveat this file used to carry applied to the
> scaffolding-only state before those tasks landed and no longer applies to
> anything below — a blanket not-yet-emitted banner over a file containing live
> counters is the kind of stale honesty note that makes an operator distrust the
> whole catalog.

All client SDK metrics follow ADR-0011 naming conventions with the `dt_client_`
prefix (ADR-0028 §9). They are emitted via the OTel JS `Meter` (production
`OtelMetricsSink`) and exported OTLP-HTTP/proto to the GC telemetry proxy
(`POST /api/v1/telemetry/v1/metrics`).

> ## WHICH METRICS IN THIS CATALOG ARE QUERYABLE — 23 OF 28, AND THE SPLIT IS DELIBERATE
>
> This block **replaces** a "not one metric here is queryable" notice. It was not
> deleted when the exporter landed, because deleting it would have left a catalog
> of 28 metrics under a header implying all 28 are queryable — the same
> overclaim-by-omission the original was written to prevent, inverted. Read the
> per-metric **`Exported:`** marker; it is authoritative and machine-checked
> **against the committed collector config** (`dt-guard client-metrics-export`
> asserts set-equality against the name allowlist in both directions, so a metric
> marked exported and absent from the collector, or vice versa, is a red build).
> **Nothing ties that green to the collector that is actually running.** The
> collector config is content-addressed (ADR-0038 §2), so APPLYING an edit rolls
> the pod; but an edit that was never applied (no deploy route from inside a
> devloop container yet, see `docs/TODO.md`) or a hand-edited live ConfigMap still
> leaves the running filter list behind the committed file with this guard green.
> A metric marked `Exported: yes` that is absent from Prometheus for that reason
> renders identically to "no browser is running" - the same ambiguity this block
> warns about below, reached by a different route. **So this marker means
> *catalogued as exported*, not *observed in Prometheus*.**
>
> **What else the same guard checks (story 2 task 16).** The export decision is
> forced for EVERY `dt_client_*` literal in non-test `packages/sdk-core/src`, not
> just `mediaMetrics.ts` — rule 4 walks the whole tree, so a name emitted from
> `media/MediaTransport.ts` or `session/MeetingSession.ts` without a heading and an
> `Exported:` marker here is a red build. Rule 2 (`dead_alert_reference`) fails the
> build if a loaded alert rule references a `dt_client_*` name that is not
> `Exported: yes`, or selects or groups on a label the collector's `keep_keys` does
> not keep — the exact way `MCMediaMissingKeyMaterial` once sat dead. Rule 1
> (`label_key_not_forwarded`) fails it if an `Exported: yes` name is emitted with a
> label key that GC does not forward or the collector does not keep. None of these
> ties the green to the RUNNING collector (above).
>
> **The 23 media-path metrics ARE queryable.** The original 14 were verified end
> to end on the Kind stack from sdk-core's own built bundle through the GC proxy —
> not a synthetic payload. The six KEK-custody and decode-lane counters added by
> ADR-0036 story 2 task 7, and the capture-source gauge and the two receive-slot
> counters added by story 2 task 13, are exported by the same allowlist; that
> end-to-end read-back predates them. `MCMediaMissingKeyMaterial`'s full expression returned `0.286` against
> live data; it had never been able to match before.
>
> **The 5 ADR-0028 join-flow metrics are deliberately NOT exported.** They are the
> only `dt_client_*` metrics carrying `meeting_id_hash`, and:
>
> > §11 grandfathers the label's presence on those metrics as emitted, not its
> > export into central Prometheus storage; the harm R1 bars is realised at the
> > stored series, not at emission.
>
> That sentence is the **test**, not just the outcome — apply it before adding
> any name to the allowlist. Stripping the hash and exporting anyway was
> considered and rejected: it would publish a series whose shape contradicts this
> catalog, which is worse than an absent one. Two independent controls keep the
> hash out of storage (the name allowlist, and the collector's `keep_keys`), and
> `{__name__=~"dt_client_.+",meeting_id_hash!=""}` returns empty on the live
> cluster.
>
> **Absence is no longer proof of a broken pipeline — and that is a real loss.**
> Before, an empty panel meant exactly one thing. Now it is ambiguous between
> "no drops occurred", "no browser connected recently", and "the pipeline broke".
> There is **no per-browser `up` signal and there structurally cannot be one** (a
> per-session `service.instance.id` is barred by ADR-0036 §11), so series presence
> is the *only* liveness signal for the client fleet. Discriminate with the
> three-step procedure in `docs/observability/dashboards.md` §Client SDK Media
> Path; do not guess from an empty panel.

---

## Stored series shape

What the SDK emits and what Prometheus stores are not the same label set. Observed
on the live cluster, not inferred:

| Stored label | Value | Why |
|---|---|---|
| `job` | `otel-collector` | the **scrape's** job. `honor_labels` is left at its default (false), so the scrape wins. |
| `instance` | the collector pod | **not a browser.** There is no per-browser instance and cannot be one (§11). |
| `exported_job` | `darktower-sdk-core` | the SDK's `service.name`, demoted by the `exported_` prefix. |
| `exported_instance` | *absent* | no `service.instance.id` exists. Its absence is the §11 decision, not an oversight. |
| `otel_scope_name`, `otel_scope_version` | scope identity | added by the **exporter**, downstream of `keep_keys` — which therefore cannot drop them and must not be expected to. |
| `client_version`, `org_id`, `key_custody` + one discriminator | see each entry | the only attributes surviving `keep_keys`. |

**Assert on the absence of `meeting_id_hash`, never on an exact label set.** The
`otel_scope_*` pair is version-dependent, so an exact-set assertion breaks on an
image bump for no real reason.

**No dashboard or alert selects on `job`, `instance` or `exported_job`** — every
`dt_client_*` expression uses a bare `sum()`/`max()`. Keep it that way; a job
selector added here would silently pin queries to one collector deployment.

### Timestamps are collector-arrival, not browser-event

Delta points are restamped with the collector's clock before accumulation (this
removes the misaligned-start-timestamp class outright rather than tuning around
it). Values are unaffected — a delta's value does not depend on its timestamps —
but **temporal attribution is arrival time**, which produces two spike-shaped
artefacts that are *not* bursts:

1. **Session teardown.** `flushMetrics()` forces pending deltas out off-cadence,
   so a session's residual counts land at one instant rather than spread across
   the session that produced them.
2. **A backgrounded tab.** Browsers throttle background timers; a resumed tab
   flushes accumulated deltas at resume. This one happens with nobody doing
   anything.

A collector outage therefore does not merely lose data, it **re-dates** whatever
the browsers buffered across it — the recovery spike is an artefact of the outage,
not a second incident.

### How long until a fresh series is queryable

Three terms, none restated here because all three are set elsewhere and would
drift: the SDK export interval (`DEFAULT_METRIC_EXPORT_INTERVAL_MS`,
`packages/sdk-core/src/telemetry/telemetryConfig.ts`), accumulation in the
collector, and the scrape interval of the `otel-collector` job
(`infra/kubernetes/observability/prometheus.yml`). The **shape** is
`export interval + accumulation + scrape interval`, and that shape survives any
configuration; a computed total would only ever have been true of one.
`flushMetrics()` collapses the first term, which is why a test that joins, tears
down, then polls is reliable where "join, then query" flakes.

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
- **Implicit labels — two sets, and the media-path set is not the join set.**
  - **Join-flow metrics** carry `client_version`, `meeting_id_hash`, `org_id`. This set is the **closed, enumerated ADR-0036 §11 exception**: it is grandfathered *as a set*, it is not extended, and nothing joins it.
  - **Media-path metrics** (`dt_client_media_*` and `dt_client_time_to_first_media_frame_ms`) carry `client_version`, `org_id`, `key_custody=operator` — **and never a meeting, participant, or stream dimension, hashed or otherwise.** They are built by allow-list in `media/setup/mediaMetrics.ts`, never by spreading and pruning the join set.
  - **No metric, log field, span attribute, or sentence in this file may carry an end-to-end or zero-trust boolean** (ADR-0036 §11: the default deployment is neither).
  - **Enforcement in TypeScript is PARTIAL, and it is on the pipe, not at the call site.** `dt-guard client-metrics-export` (story 2 task 16) extracts the label keys of every emission of an `Exported: yes` name across non-test sdk-core and fails if a key is not forwarded by GC (`ALLOWLIST` ∪ `MEDIA_DATAPOINT_EXTRA`) and kept by the collector (`label_key_not_forwarded`), and fails if any forwarded or kept key is identity-shaped per the `identity-label-policy` block in `docs/observability/label-taxonomy.md` §R4 (`identity_key_forwarded`). So an identity key cannot reach STORAGE silently. What stays unenforced: a non-exported name's labels (the name filter drops the whole series, so the rule does not apply to it), and the SDK-side shape itself — `dt-guard`'s media-path deny is Rust-only and `ts_pii.rs` scans only `console.*`/`logger.*` call sites. The rules are in `docs/observability/label-taxonomy.md` R1-R4 — **read them before adding a label here**; they are not restated in this file, because a second home drifts.

  **The grandfathered set is FROZEN, and these are its members.** A category with
  no roster opens by analogy — the next author decides their metric is
  "join-flow enough" — so the list is enumerated and **it does not grow**:

  | Grandfathered member | What it observes |
  |---|---|
  | `dt_client_join_attempts_total` | one join attempt, at its outcome |
  | `dt_client_signaling_connection_total` | one signaling connection outcome |
  | `dt_client_mh_connection_total` | one media-handler connection outcome, at handshake |
  | `dt_client_time_to_signaling_ready_ms` | join → signaling ready |
  | `dt_client_time_to_first_mh_connected_ms` | join → first MH connected |

  **The test is what a metric OBSERVES, not which directory it lives in.**
  Connect-lifecycle — once per connection, at setup, before any frame exists — is
  grandfathered. Media-carrying — per frame, per stream, or on the media data
  path — is under the bar. A directory-shaped rule fails in BOTH directions, and
  the failure that will actually happen is permitting a media counter someone
  places outside the media tree. `dt_client_mh_connection_total` lives in
  `packages/sdk-core/src/media/MediaTransport.ts` and is grandfathered anyway,
  for exactly that reason.

  **`dt_client_time_to_first_media_frame_ms` is the nearest neighbour to a
  grandfathered member and is deliberately NOT one.** The difference is what it
  observes, not what it is called — see its entry, and see
  `dt_client_time_to_first_mh_connected_ms`'s.

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
  - `org_id` = the org **UUID**, stamped server-side by GC from the authenticated
    `UserClaims.org_id`. **The payload value is never trusted** — GC overwrites it,
    so a client cannot label its telemetry with another tenant's identifier, and the
    value is bounded by construction rather than by assuming an honest client. That
    second property is the load-bearing one: the GC filter bounds attribute *keys*
    and never descends into *values*, so a client-chosen `org_id` would have been an
    unbounded series generator against shared Prometheus storage — an availability
    concern, not only an attribution one.
    - **This replaced the org subdomain**, and the caveat that used to sit here —
      "correlating client↔server metrics by org requires the subdomain→uuid
      mapping" — is now resolved rather than merely edited: client and server org
      dimensions occupy the **same identifier space** and join directly. Recorded
      rather than deleted so a reader of the old note can see where it went.
    - The cost, stated plainly: a UUID is not human-readable on a legend. No panel
      breaks out by org today, so nothing regressed. It was accepted because a value
      an operator can *trust* and *join* beats one they can read.
    - The change was free of migration cost only because **nothing had ever been
      exported** — no historical series existed under the old semantics. A
      label-value semantics change is normally expensive precisely because both
      coexist under one name. That window will not reopen.

---

## Join-flow metrics (R-25)

**Pre-existing label-key mismatch — harmless today, and why it will not stay silent.**
The SDK emits these metrics with BARE label keys — `status`, `failure_stage`,
`close_reason` (`session/MeetingSession.ts`) and `mh_index_bucket`
(`media/MediaTransport.ts`) — while GC's telemetry filter `ALLOWLIST`
(`crates/gc-service/src/services/telemetry_filter.rs`) carries only the `dt.`-dotted
spellings (`dt.failure_stage`, `dt.close_reason`, `dt.mh_index_bucket`) and no
`status` at all. So GC strips all four today. That is harmless ONLY because every
metric here is `Exported: no`: the collector's name allowlist drops the whole series
anyway. The day any of them is exported, `dt-guard client-metrics-export` rule 1
(`label_key_not_forwarded`) fires on the stripped key. Fix the spelling on one side
then, deliberately; do not widen `keep_keys` to make the finding go away.

### `dt_client_join_attempts_total`
- **Exported**: no — deliberately NOT exported. ADR-0036 §11 grandfathers `meeting_id_hash` on this metric AS EMITTED (browser-console log↔metric correlation); it does not grandfather exporting a per-meeting identifier into stored Prometheus series, which `docs/observability/label-taxonomy.md` R1 bars. Stripping the hash and exporting anyway would publish a series whose shape does not match this entry — a silently different metric.
- **Type**: Counter
- **Description**: Total client-side meeting-join attempts, by outcome and the
  stage a failure occurred at.
- **Labels**:
  - `status`: `success`, `failure`
  - `failure_stage`: `none` (on success), `signup`, `credential_invalid`,
    `gc_create_token`, `gc_join`, `mc_signaling_connect`, `mc_join_response`,
    `mh_connect`, `internal`
- **Cardinality**: Low (2 statuses × 9 stages = 18, before implicit labels).
- **Emission honesty** (task #14 — which stages the browser client actually
  emits):
  - `gc_create_token` is **RESERVED, never emitted by this client**. The browser
    issues a single `joinMeeting` POST; the token-mint and join happen server-side
    behind it, so the client cannot observe that sub-stage separately. It is kept
    in the catalog only for parity with the server-side join pipeline.
  - `credential_invalid` (task #58) is where **token-mode credential failures land**:
    either a caller-supplied `userToken` rejected locally by `validateUserToken`
    (malformed / control chars), or a **401** from GC's `joinMeeting`. Alert / dashboard
    authors: this is a **caller-input** failure — deterministic and caller-fixable — so
    it is deliberately NOT `internal` (the SDK-fault bucket). Treating it as an SDK
    fault would make `internal` non-actionable.
    - **Deliberately excluded — do not widen this to match a future code change**: a
      **403** is an authorization decision on a *valid, live* token (org meeting limit,
      insufficient permissions, external participants not allowed) and stays on
      `gc_join`; a **malformed meeting code** fails client-side in `validateMeetingCode`
      before any HTTP and also stays on `gc_join`. Both were routed here at first and
      corrected at Gate 3 — putting either in this bucket points oncall at auth for a
      quota breach or a user's typo.
  - `signup` is **unreachable on the token path**, which is the path the demo app uses.
    It covers only an AC `register`/`login` call failing, and `MeetingSession.join` with
    `{mode:'token'}` makes no AC call at all. A flatlining `signup` panel after task #58
    is the expected state, not an outage — do not alert on its absence. It remains live
    for the standalone-join path (`{mode:'login'|'register'}`), which no first-party app
    currently uses.
  - **Latency-histogram note**: a token-mode join is structurally faster than the
    pre-#58 flow — it skips an AC round-trip — so `dt_client_time_to_signaling_ready_ms`
    and `dt_client_time_to_first_mh_connected_ms` shifted downward once at rollout. No
    discriminator label was added: the demo moved wholesale to the token path, so there
    is no mixed population, and the shift is a one-time improvement rather than an
    ongoing bimodality. Re-evaluate if a first-party caller ever adopts the password
    path, which WOULD create two populations under one series.
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
- **Exported**: no — deliberately NOT exported. ADR-0036 §11 grandfathers `meeting_id_hash` on this metric AS EMITTED (browser-console log↔metric correlation); it does not grandfather exporting a per-meeting identifier into stored Prometheus series, which `docs/observability/label-taxonomy.md` R1 bars. Stripping the hash and exporting anyway would publish a series whose shape does not match this entry — a silently different metric.
- **Type**: Histogram
- **Description**: Wall-clock ms from `MeetingSession.join` to the MC
  `JoinResponse` being received (signaling readiness).
- **Labels**: implicit only.
- **Buckets**: TBD (deferred — full catalog).
- **Usage**: client-perceived time-to-signaling-ready; the join-latency SLI the
  server-side handler timing cannot capture (includes browser→MC RTT).

### `dt_client_time_to_first_mh_connected_ms`
- **Exported**: no — deliberately NOT exported. ADR-0036 §11 grandfathers `meeting_id_hash` on this metric AS EMITTED (browser-console log↔metric correlation); it does not grandfather exporting a per-meeting identifier into stored Prometheus series, which `docs/observability/label-taxonomy.md` R1 bars. Stripping the hash and exporting anyway would publish a series whose shape does not match this entry — a silently different metric.
- **Type**: Histogram
- **Description**: Wall-clock ms from `MeetingSession.join` to the first MH
  connection being established.
- **Labels**: the **join-flow** implicit set (`client_version`,
  `meeting_id_hash`, `org_id`). A frozen-roster member.
- **Buckets**: TBD (deferred).
- **Usage**: client-perceived time-to-**connect**-readiness. It measures the
  HANDSHAKE completing, **before any media frame exists** — it is not a
  media-path measurement, and the earlier wording "time-to-media-path-ready"
  invited exactly that reading.
- **DO NOT COPY THIS LABEL SET TO `dt_client_time_to_first_media_frame_ms`.**
  That metric is its nearest neighbour by name, is a media-path metric, and
  carries `client_version`, `org_id`, `key_custody` and **no meeting dimension**.
  The two are separated by what they observe, not by what they are called.

### `dt_client_signaling_connection_total`
- **Exported**: no — deliberately NOT exported. ADR-0036 §11 grandfathers `meeting_id_hash` on this metric AS EMITTED (browser-console log↔metric correlation); it does not grandfather exporting a per-meeting identifier into stored Prometheus series, which `docs/observability/label-taxonomy.md` R1 bars. Stripping the hash and exporting anyway would publish a series whose shape does not match this entry — a silently different metric.
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
- **Exported**: no — deliberately NOT exported. ADR-0036 §11 grandfathers `meeting_id_hash` on this metric AS EMITTED (browser-console log↔metric correlation); it does not grandfather exporting a per-meeting identifier into stored Prometheus series, which `docs/observability/label-taxonomy.md` R1 bars. Stripping the hash and exporting anyway would publish a series whose shape does not match this entry — a silently different metric.
- **Type**: Counter
- **Description**: Client media-handler (browser→MH) connection outcomes.
- **Labels**:
  - `status`: `success`, `failure`
  - `mh_index_bucket`: bounded bucket of the MH peer index — `0`, `1`, `2+`
    (the raw index is bucketed so cardinality stays bounded as the MH fan-out
    grows).
- **Cardinality**: Low (2 statuses × 3 buckets = 6, before implicit labels).

---

## Media-path label conventions (ADR-0036 §11)

**This catalog is the only home for which labels a media-path metric carries.** Any
change routes through here — never peer-to-peer between an emitter and a dashboard,
and never by editing a panel description to match a label somebody added. Nothing in
the tree compares a label key or a label value to anything: `dt-guard`'s
`application-metrics` and `dashboard-panels` validate metric **names** against code,
catalogs, dashboards and alerts, and `alert_rules.rs` pins an alert `expr`
byte-for-byte against its `alerts.md` entry, but **no guard reads a label roster**. A
roster written anywhere else decays silently and CI stays green.

Note the division of labour with `docs/observability/label-taxonomy.md`, because the
two are easy to conflate: a **shared label's** name, bounded value set, and the reason
it is bounded live in the taxonomy — it is the *first* home for a label that more than
one service emits. **Which labels a given metric carries** lives here. Neither is a
copy of the other.

### Declared, not yet carried: `media_kind` and `content_kind`

Media-path counters gain two dimensions when video and content share land (**story 3**,
not this story):

| Label | State |
|---|---|
| `media_kind` | **Declared, not carried.** No media-path metric emits it today. Values in `label-taxonomy.md`. |
| `content_kind` | **Declared, not carried.** No media-path metric emits it today. Values in `label-taxonomy.md`. |

Both are registered in `docs/observability/label-taxonomy.md` as shared labels, which
is where their value sets are authoritative. They are recorded here as *declared* so
that story 3 adds a dimension to an existing scheme rather than inventing one, and so
that a reader who greps for them today finds "not carried yet" instead of silence.
**Do not write a dashboard query or an alert that selects on either** — the selector
would match nothing and yield an empty series rather than an error.

### The transmit/receive convention

`direction` (`ingress` | `egress`) is carried **only where both directions exist on one
counter**. The convention itself — that it is pipeline-relative and never
participant-relative, why the `uplink`/`downlink` reading is barred, why it is admitted
on relay metrics at all, and the explicit statement that the acceptance **does not
generalise to the client's drop counter** — is stated once in
`docs/observability/label-taxonomy.md` §Permitted partner: `direction`. It is not
restated here, because a partial copy that dropped either of those last two clauses
would read as complete.

**Receive-only counters carry no `direction` label at all.** A direction label with one
possible value is a label that can only ever be wrong: it invites a selector that will
one day match nothing, and it implies a second direction that does not exist.

### What that means for the client SDK

**Identity dimensions: exactly two.** `client_version` and `org_id`. Nothing else — and
specifically **no meeting, participant or stream dimension, hashed or otherwise**
(ADR-0036 §11 R1).

Plus one **fixed non-identity** label: `key_custody=operator` (ADR-0036 §4). It is not a
dimension in the cardinality sense, having exactly one value by construction, and R-26
requires it. The media-path label set is therefore *three* labels; describing it as
"only `client_version` and `org_id`" would be false against the wire.

**Why a hash does not help, stated because it is the obvious-looking escape hatch.** A
hashed meeting id has **identical cardinality and identical per-meeting aggregation** to
a raw one. Both properties are exactly what R1 is about, so hashing buys nothing the
rule cares about and merely makes the violation harder to see in a label value.

**The concrete inertia risk.** `packages/sdk-core/src/media/events.ts` already threads
the join-flow implicit label set into the media module, so `meeting_id_hash` would
attach to every media metric **by default** if the media set were built by spreading the
join set and pruning it. It is not: the media set is built by **allow-list** in
`packages/sdk-core/src/media/setup/mediaMetrics.ts`. Stronger than a policy, the code
makes the leak **unrepresentable**: `mediaMetricLabels(identity: MediaMetricIdentity)`
takes two named strings (`clientVersion`, `orgId`) — there is no `MetricLabels` bag at
that boundary, so `meeting_id_hash` is not *excluded* from the set, it is a value a
caller cannot pass even by mistake, because no parameter could carry it. That is a type,
not a filter: a filter (`pick(labels, [...])` over an ambient bag) is a control that has
to *notice*, and it fails open the moment a caller reaches the sink by a path that skips
it — the type signature has nothing to notice. Keep it that way: an allow-list fails
closed when a new label appears upstream, and a prune-list fails open. This matters
specifically here because `MediaTransport`'s `#emitMetric` legitimately *does* spread the
join label set for the one grandfathered metric, so a deletion-based helper would read as
the local idiom to a future refactor — and the catalog is what that refactor consults.

`meeting_id_hash` remains grandfathered for the ADR-0028 **join-flow** metrics as a
**closed set**: it is grandfathered *as a set*, it is not extended, and nothing joins it.

**No `direction` label on any client media metric.** `dt_client_media_frames_dropped_total`
is receive-only and `dt_client_media_send_dropped_total` is send-only — two single-direction
counters, not one bidirectional counter. `label-taxonomy.md`'s §Permitted partner:
`direction` names the client drop counter explicitly as outside its acceptance, so that
block must not be cited as precedent for adding one here.


---

## Media-path metrics (ADR-0036 §11)

Emitted from `packages/sdk-core/src/media/setup/mediaMetrics.ts`, which is the
ONLY file under `packages/sdk-core/src/media/**` permitted to name a media-path
metric — asserted by `media/__tests__/hotPathLayout.test.ts`. That test is the
call-site control; `dt-guard client-metrics-export` covers the pipe (label keys of
exported names, identity-shaped keys, and the export decision for every
`dt_client_*` literal in non-test sdk-core — see the block at the top of this file).

**Every metric in this section carries `client_version`, `org_id` and
`key_custody=operator`, and nothing else beyond its own bounded discriminator.**
No meeting, participant, or stream dimension, hashed or otherwise. See
§Naming convention for the two label sets and why the join set is not this one.

### The receive-path accounting identity

> **`received = accepted + sum(drops by reason)`**

`dt_client_media_frames_received_total` is counted **at the wire**, before any
parse or verification, and every datagram then leaves the receive path through
exactly one of `dt_client_media_frames_accepted_total` or
`dt_client_media_frames_dropped_total{reason}`.

**It is an ACCOUNTING identity, not a playback guarantee.** Its job is to prove
that no drop path fails to count itself. It says nothing about audibility: all
seventeen `drops_frame: true` reasons fire at or before decoder handoff, so the
identity is exact **by construction** at the crypto/parse boundary, and at the
playback boundary it would be FALSE — a frame lost between handoff and audible
decrements nothing on the right-hand side.

**ADR-0036 / R-25 prose spells the middle term `played`. This catalog
deliberately supersedes that spelling**, because `played` names an identity that
does not hold: a frame handed to a decoder is not played (the decoder can error,
the output can be discarded, the context can be suspended). The
accepted→audible segment is not covered by this identity and is covered only
partially, by `dt_client_media_decoder_errors_total` and
`dt_client_media_decode_queue_dropped_total` — both POST-ACCEPT, so neither may
ever become a `frames_dropped_total` reason; extending the identity to
the playback boundary would require playback-side drop reasons — a vocabulary
extension needing its own planning, not a word swap.

**Per client, and unchanged by N senders (story 2 R-28).** The identity is written
per receiving client and holds for a client receiving from N senders exactly as it
did for one: every datagram, from whichever sender, leaves through exactly one term.
There is **no sender split** on either side and none is needed — adding a sender
label to make it "per sender" would be the identity dimension ADR-0036 §11 bars, and
the identity does not require it. `dt_client_media_decode_queue_dropped_total` is
POST-ACCEPT and never enters it (see its entry). The server-side fan-out identity
(one MH ingress frame becomes E egress attempts) is a different identity with a
different unit, stated in `docs/observability/metrics/mh-service.md`
§`mh_media_frames_forwarded_total`.

**Counting-point migration (video story).** When several frames share one
stream, `dt_client_media_frames_received_total` must move from the TRANSPORT
boundary to the PARSE boundary, and the identity must be re-established there.
Recorded here rather than only in a code comment because the constraint outlives
the comment's reader.

---

### `dt_client_media_frames_sent_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Datagrams that left the device on the media datagram path — **one increment per target handler per captured frame**, not one per frame. A sender whose edges span two handlers (ADR-0036 §9; story 2 task 20) advances this twice for one captured frame.
- **THE NAME SAYS FRAMES; THE VALUE COUNTS DATAGRAMS — and that matches `dt_client_media_frames_received_total`**, which has counted datagrams-at-the-wire since story 1 (stated in `docs/runbooks/client-dev-local.md`'s green-signal table). The divergence is a **family convention documented on both sides**, not a defect on one: sent and received are in the SAME unit, so the loopback pair and every cross-end comparison against `mh_media_*` stay unit-comparable — a two-handler sender's datagrams are split across two handlers, and each handler sees only its own, so the aggregate still reconciles. Do not "fix" either name without moving both.
- **Labels**: base only.
- **Usage**: the denominator for the send-drop ratio — **and the unit is the reason it is correct.** `dt_client_media_send_dropped_total` is raised inside the per-lane send path (`packages/sdk-core/src/media/pipeline/egress.ts`), so it is per-send-attempt by construction. Counting *frames* here while counting *attempts* there would make one captured frame that succeeded on handler A and was refused on handler B read as **50% loss on a sender that reached handler A perfectly**. Both sides count datagrams, so the ratio is a real loss fraction. Any future change to either counting point must move both or the ratio silently stops meaning loss.

### `dt_client_media_send_dropped_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `reason` — a NEW bounded vocabulary, **not** the frame reject
  taxonomy.
- **Permitted values**:

  | `reason` | Shared with MH? | Meaning and triage |
  |---|---|---|
  | `egress_queue_overflow` | **shared** | A lane's bounded queue (one lane per target handler, each bounded independently) was full and that lane's OLDEST frame was evicted — so a sender under back-pressure on two lanes counts two evictions per captured frame. Back-pressure at the application layer, where we can see it. |
  | `transport_send_refused` | **shared** | The transport refused a datagram it should have accepted. **Fleet contract: reads zero forever; alertable at `> 0`.** |
  | `oversize_datagram` | **shared** | The frame exceeded the transport maximum and was never offered. Separate from the row above precisely so a configuration condition cannot poison an invariant counter. |
  | `connection_closed` | **shared** | The connection closed underneath a send. **Routine** — a participant leaves every meeting, many times. Not alertable. |
  | `not_connected` | **client-only** | The lane's directed target has no connected transport. **Reads zero forever; alertable at `> 0`** — but since story 2 task 20 it has TWO arms with opposite owners, and the alert must not presume the first. (1) **Client lifecycle ordering** — a send attempted before any transport was up. (2) **Server-side, and this is the new one**: MC directed this sender at a handler the client has no session with. MC derives a sender's targets from the handlers owning its edges, and an edge exists only where *both* parties are connected to that handler (ADR-0036 §9), so a directed target outside this client's own connected set means **MC's connectivity view is stale** — not a client bug. Fork on whether the client holds any transport at all: none means arm (1), some-but-not-this-one means arm (2). Arm (2) is the only signal available today for stale MC connectivity after a lost `NotifyParticipantDisconnected`, so it is a **partial mitigation** for the MH-observed-vs-client-reported divergence counter deferred in `docs/TODO.md` — partial because it fires only for a *sender* holding an edge on the stale handler. |

- **Cross-end comparison**: the four shared spellings match
  `MediaDropReason` in `crates/mh-service/src/observability/metrics.rs`, so
  `sum by(reason)` compares across the two ends of one hop. **`not_connected`
  has no counterpart** — MH never initiates — so a cross-end sum is meaningful
  only for the shared four. Nothing mechanically guards that the two lists agree.
- **Why this counter exists at all**: ADR-0036 §11 calls the client-side send
  drop the one that matters most, because it happens in the sender and **MH
  structurally cannot observe it**. WebTransport exposes no send-side drop event,
  so the SDK keeps the transport queue shallow, owns a bounded queue above it,
  makes the decision there, and counts it — making the drop observable BY
  CONSTRUCTION.
- **Mute is NOT a send drop.** While client-muted nothing is encoded, so nothing
  enters the queue and nothing is dropped. Mute is
  `dt_client_media_mute_transitions_total` and nothing else.

### `dt_client_media_send_queue_depth`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Gauge
- **Description**: Depth, in frames, of the DEEPEST of the sender's bounded
  application egress lanes — one lane per target handler (story 2 task 20: a
  sender sends to every handler owning one of its edges), each bounded
  independently.
- **Labels**: base only.
- **Usage**: read against the configured bound (default 10 frames = 200 ms at
  20 ms/frame). The application bound trips BEFORE the transport high-water mark
  — asserted at setup — so a rising depth here is back-pressure we can count
  rather than loss inside the user agent. Because it is the deepest lane, a full
  gauge beside healthy `dt_client_media_frames_sent_total` means ONE lane
  (one handler's path) is stalled, not the whole sender — do not go to
  capture/encode/device on it. A low depth still means no lane is backed up.
- **SINGLE-WRITER-MEANINGFUL — with N browsers this is the most recent reporter's
  depth, NOT a fleet maximum.** It is a Gauge, and every browser writes to one
  stream identity (no per-instance label, §11), so the stored value is
  last-writer-wins rather than an aggregate. `max()` over it on the dashboard is a
  max over **one** series and does not mean "the worst browser". Two consequences:
  a healthy browser can mask a backed-up one, and on the dev cluster a *finished*
  run's stale value can dominate the panel until the collector's `metric_expiration`
  elapses. No processor can fix this without an identity dimension, and identity
  dimensions are barred — so this is a stated limitation, not an open defect. If
  you need per-browser queue depth, it is not available from this signal at all.

### `dt_client_media_frames_received_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Datagrams received on the media path, counted **at the wire**
  before any parse or verification.
- **Labels**: base only.
- **Usage**: the left-hand side of the accounting identity above, and the signal
  that distinguishes "nothing arriving" from "arriving and failing". See the
  counting-point migration note.

### `dt_client_media_frames_dropped_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `reason` — the frozen frame-reject vocabulary, whose SSoT is
  `proto/test-vectors/frame-v2.vectors.json` → `reject_reasons`. Membership of
  THIS counter is carried by `drops_frame`: seventeen of the eighteen tokens.
  `wrap_key_id_mismatch` is the only `false` and **must never appear here**: the
  frame it describes is *accepted*, and is already counted on
  `dt_client_media_frames_accepted_total`. Counting it here as well would put one
  frame on **both** sides of `received = accepted + sum(drops by reason)` — the
  right-hand side then exceeds the left, and the identity fails **silently, only in
  aggregate, and long after the label set is frozen**. It is kept structurally out
  of reach: it arrives on the receive path's success channel as a `WrapOutcome`,
  never as a thrown reject, so "catch it into the drop counter" is not a path that
  exists.
- **All eight structural codec tokens are emitted INDIVIDUALLY** — `unknown_version`,
  `reserved_flag_bit_set`, `payload_length_exceeds_max`,
  `payload_length_exceeds_available`, `truncated`, `extensions_too_large`,
  `extensions_malformed`, `trailing_bytes` — never collapsed into a
  `decode_reject` bucket. Collapsing destroys R-31's only lever
  (`unknown_version` staying individually visible is how a version-skewed
  rollback is detected — rollback for this feature is redeploy-only with no
  finer-grained control) and breaks `sum by(reason)` comparability with MH.
- **The KEK generation split — two reasons, not one.** A frame whose wrap names a
  KEK generation this receiver cannot open is counted by DIRECTION:
  `no_kek_for_generation` (newer than any held) or `kek_generation_stale` (older
  than retained). They point at different causes (a push that has not arrived,
  versus sender skew past retention), which is why they are split.
- **The four KEY-DELIVERY reasons — exactly `MCMediaMissingKeyMaterial`'s selector**
  (`infra/docker/prometheus/rules/mc-alerts.yaml`) — are the three key-material
  reasons below plus `unwrap_failed` (next bullets). `unwrap_failed` joined the
  selector at story 2 task 16; before that it was in no alert and no panel.
  Membership is held by a PARTITION check, not by this list: every
  `drops_frame: true` token in `proto/test-vectors/frame-v2.vectors.json` must sit in
  exactly one of that alternation or the `NOT_KEY_DELIVERY` const in
  `crates/dt-guard/src/client_metrics_export.rs` (which carries each exclusion's
  reason), so a new token fails the build until classified.
- **The three key-material reasons** are `no_kek_for_generation` (the frame's
  wrap announces a KEK generation NEWER than the newest this receiver holds — MC's
  push has not arrived), `kek_generation_stale` (the wrap announces a generation
  OLDER than this receiver still retains — the client holds the current generation
  plus at most one previous, and the previous is kept only for
  `min(W/2, ceiling)` where W is MC's `kek_rotation_debounce_seconds`), and
  `no_roster_entry` (no usable identity key for the frame's `key_id.sender_id`,
  INCLUDING the case where MC published an empty key). Both KEK tokens fire only
  when no usable transmit key for the frame's key id is already cached; with one
  cached, the frame is accepted and counted as the wrap outcome
  `kek_generation_not_held` instead, from either direction. All three are expected
  transients at join and after a KEK rotation; **the sustained case is the
  signal**, and it is the only signal for a join or rotation path that has
  silently stopped delivering keys. For a sustained `kek_generation_stale`: At the shipped default W (`MC_KEK_ROTATION_DEBOUNCE_SECONDS` in `infra/services/mc-service/config.env`), client retention is **already at its ceiling** (`KEK_RETENTION_CEILING_MS`), so raising `MC_KEK_ROTATION_DEBOUNCE_SECONDS` does **not** lengthen it — it only flips the fleet to `ceiling_clamped`. The only lever that lengthens retention past `KEK_RETENTION_CEILING_MS` is the client ceiling itself, an SDK constant requiring a release **and** a deliberate key-lifetime bound (ADR-0036 §4) — a security decision, not a remedy an operator applies. Lowering W *shortens* retention and makes this worse. A sustained `kek_generation_stale` at default configuration therefore points at **sender-side rotation skew**, not at MC's debounce.
  See `dt_client_media_kek_retention_anomalies_total` for why retention was what
  it was.
- **`sender_not_assigned`** (layer `assignment`) is a frame that parsed and
  verified from a sender NOT in this client's MC slot-assignment set; it is
  dropped at the slot-edge gate, after signature verification and **before**
  decryption, so it advances no replay window and caches no transmit key. It is
  not a key-material reason. **A sustained non-zero rate means a media handler
  forwarded a frame from a sender this client holds no slot for** — MH
  misrouting, or a compromised handler — or a lagging assignment at a remap edge
  when brief. It is **deliberately NOT alerted** (decided at story 2 task 16) and
  **deliberately EXCLUDED from `MCMediaMissingKeyMaterial`**: its remedy lives in
  MH/MC placement, not in KEK or roster delivery. The exclusion is recorded as a
  `NOT_KEY_DELIVERY` entry (misrouting) in `crates/dt-guard/src/client_metrics_export.rs`.
- **`unwrap_failed` versus `decrypt_failed`** are two AES-GCM failures on one
  receive path routing to opposite teams: `unwrap_failed` is the KEK unwrap, so
  it is key DISTRIBUTION; `decrypt_failed` is the SFrame payload, so it is the
  key schedule or the sender.
- **`unwrap_failed` is INSIDE `MCMediaMissingKeyMaterial`'s selector** (story 2
  task 16), consistent with its key-DISTRIBUTION classification — the selector
  used to exclude it, which contradicted this entry. **It is an AUTHENTICATED-frame
  failure, never transit corruption**: the §3 signature covers the publisher
  region, which includes the wrapped-key block, and `verifyFrame` runs BEFORE any
  unwrap (`packages/sdk-core/src/media/frame/receivePath.ts`, verify-before-decrypt
  is structural). A frame corrupted in transit therefore fails as
  `signature_invalid` and never reaches the unwrap. What remains is (a) a KEK-bytes
  split under one generation between sender and receiver (key distribution —
  `dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}` is its
  per-client witness), (b) a sender wrap or key-schedule bug, or (c) an
  authenticated member emitting bad wraps. It has **no healthy transient**: with a
  usable cached key the same tag mismatch is the non-dropping
  `wrap_key_id_mismatch` wrap outcome instead.
- **`no_transmit_key`** is a frame with neither a cached key nor a usable wrap: a
  protocol violation, not a third key reason and not a decode reject.

### `dt_client_media_frames_accepted_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Frames that completed the receive path — verified, replay-
  checked, decrypted — and were handed to the audio decoder.
- **Labels**: base only.
- **NAMED `accepted`, NOT `played`.** A frame handed to a decoder is not played.
  Silent audio with this counter climbing means the fault is downstream of the
  handoff — the decoder, the output device, or a suspended audio context — and
  **not** that frames are playing. See the identity above.

### `dt_client_media_key_wrap_outcomes_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `outcome` — the NON-DROPPING wrapped-key outcomes (no count is
  restated here: the list below is the operator-facing artifact, and a second
  encoding of its length only rots). The frame was ACCEPTED in every case; none
  ever reaches the drop counter.
- **Permitted values**:
  - `kek_generation_not_held` — the frame's wrap announces a KEK generation this
    receiver does not hold, but a usable transmit key for that key id was already
    cached, so the frame plays off the cache. **An honest sender of this SDK
    cannot produce it**: the wrap is computed once per transmit key, and on
    every KEK change the sender rotates to a NEW key id (R-13), which arrives
    uncached and so lands on the drop reasons instead. It needs one key id
    under a second wrap — a sender that re-wraps an existing transmit key
    under a new KEK rather than rotating, which the format permits and R-13
    exists to end. **A sustained non-zero rate means a non-conforming or
    hostile sender, not rotation lag.**
  - `wrap_generation_conflict` — the frame's wrap is for a key id whose
    transmit key is ALREADY cached, and it announces a KEK generation this
    receiver DOES hold but which is not the generation that key id was
    unwrapped under. **The refusal is what makes it a conflict rather than an
    overwrite**: the wrap is not unwrapped and no second cache entry is created
    (story 2 E-1; ADR-0036 §4 invariant 3), and the frame plays off the cached
    key. Refused because replay state is scoped by the generation a key id was
    unwrapped under; moving the key id to the newer generation would let a
    captured frame without a wrapped key replay into a bucket that never saw
    it. **Reads zero for R-13-conforming senders** — they rotate to a new key id
    on every KEK change and structurally cannot reach this outcome. **A sustained
    non-zero rate means a non-conforming or hostile sender.** Not a
    reads-zero-forever contract: the format permits the behaviour, and this SDK
    does not control every sender.
  - `wrap_key_id_mismatch` — the SSoT spelling, carried verbatim from
    `frame-v2.vectors.json` → `vectors[].expected.outcome`. **A rename toward the
    observable is pending under `docs/TODO.md`** (proposed target spelling:
    `unwrap_failed_key_held`); the rename is protocol-owned because the token is
    pinned in a Guarded Shared Area, and it must move both `reject_reason` and
    `expected.outcome` on the same row.
- **TRIAGE ON THE CONDITION, NOT THE TOKEN NAME.** What this value actually
  observes is: **the KEK unwrap failed — a one-bit GCM tag mismatch, which is all
  the receiver gets — AND a usable transmit key for that key id was already
  held, so the frame plays.** The receiver **cannot** detect a mis-bound wrap:
  the wrap's bound key id is nowhere on the wire (the block is
  `kek_generation || 32 wrapped || 16 tag`, and the binding exists only as the
  seal-time AAD), so a mis-bound wrap and a wrong KEK are indistinguishable. The
  **realistic production cause is a KEK-generation skew** from a sender whose
  transmit key this receiver already holds — not anyone mis-binding wraps.
- **Cross-reference — `unwrap_failed` and `wrap_key_id_mismatch` are two halves
  of one AEAD failure**: *same AEAD failure, forked by receiver state; the drop
  counter carries the half that lost the frame, the wrap-outcome counter carries
  the half that kept it.* An operator separates key-distribution breakage from
  KEK-generation skew by which half is moving.
  **The two CANNOT be summed or ratio'd in a single expression**, and the reason
  is the identity above: `received = accepted + sum(drops by reason)` is what
  forces a non-dropping outcome off the drop counter in the first place.
- **Cross-reference — `kek_generation_not_held` and `wrap_generation_conflict`
  are two halves of one R-13 non-conformance**: *one condition — a key id
  already cached, and a wrap announcing a KEK generation other than the one it
  was unwrapped under — forked purely by receiver state: the announced
  generation is not held (`kek_generation_not_held`) or held
  (`wrap_generation_conflict`). In both the wrap is ignored and the frame plays
  off the cache.* Same cause, same remedy (find the sender that re-wraps instead
  of rotating).
  **Unlike the pair above, these two SHOULD be summed**: they are values of the
  SAME metric under the same label, so
  `sum(rate(dt_client_media_key_wrap_outcomes_total{outcome=~"kek_generation_not_held|wrap_generation_conflict"}[…]))`
  is the correct answer to "is a non-conforming re-wrap sender present?", and
  neither value alone is. The prohibition above does not carry across: that
  pair spans two metric NAMES, and summing it would break the accepted/dropped
  identity; this pair lives inside one metric. (`label-taxonomy.md`'s
  no-cross-metric-aggregation rule for `outcome` concerns metric names, not
  values within one metric.)
- **Deliberately NOT alerted** (the non-conforming re-wrap pair; @security,
  story 2 task 9). Two reasons. **Bounded, self-directed impact**: a sender that
  re-wraps instead of rotating weakens protection only of media IT authored, and
  the beneficiary is a departed member who already held the old KEK; it cannot
  touch another sender's streams, read anything new, or forge — §3's signature
  still binds every frame to a roster key. **No attribution available**: the
  media-path label set is closed (`client_version`, `org_id`, `key_custody`), so
  a firing alert could say only "somewhere in the fleet a sender is not
  rotating", with no path from the alert to the sender — an alert whose
  provenance cannot support its own runbook. A low-rate warning on the summed
  pair was considered (@observability) and overruled on the attribution
  argument. **The panel and this entry are the whole control.** **Revisit
  when**: per-sender attribution becomes available to an operator, or either
  outcome becomes reachable by a frame WITHOUT a valid roster signature.

### `dt_client_media_downlink_gap_frames_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Frames MISSING between each media handler's egress and this
  client's ingress, measured as gaps in the relay hop sequence.
- **Labels**: base only. `stream_id` is bounded internal state and is **never** a
  label.
- **Increments by the SIZE of each gap, not by one per gap event** — a counter of
  missing numbers is comparable against `dt_client_media_frames_received_total`
  as a loss rate; a counter of events is not.
- **OVER-COUNTS TRUE LOSS BY EXACTLY THE REORDER COUNT.** QUIC datagrams are
  unordered, so a reordered frame first opens a gap and then arrives late. **The
  honest loss estimate is `gap_frames - reorder`.** This is the single most
  misreadable value in the media set: without the subtraction, normal reordering
  reads as loss in the first congestion incident.
- **Why this lives on the client**, mirroring the blockquote on
  `mh_media_frames_dropped_total` in `mh-service.md`:

  > This is the far-end compensating control for a blind spot MH structurally
  > cannot close. Beneath MH's own bounded queue, quinn evicts datagrams from its
  > send buffer **silently** — it pops the oldest, emits a trace-level log,
  > increments no `ConnectionStats` field, and (because an evicted datagram was
  > never transmitted) never enters quinn's lost-packet statistics either. So
  > under congestion severe enough to saturate quinn's buffer but not MH's,
  > `mh_media_frames_dropped_total{reason="egress_queue_overflow"}` **reads flat
  > at exactly the moment loss is worst**. MH writes the downlink hop sequence,
  > so it can never see a gap in a number it generates itself. Only the receiving
  > end can.

### `dt_client_media_downlink_reorder_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Datagrams arriving at or below the running hop-sequence
  high-water mark.
- **Labels**: base only.
- **Usage**: the subtrahend in `gap_frames - reorder`. Kept separate from the gap
  counter so the subtraction is possible at all.

### `dt_client_media_undeclared_stream_id_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Frames arriving on a relay `stream_id` this client never
  declared in its `ReceiveCapability`.
- **Labels**: base only.
- **Usage**: the relay region is UNAUTHENTICATED — a media handler writes
  `stream_id` freely and nobody signs it — so an undeclared value creates NO
  receiver state and is counted here instead. The frame itself is still verified
  and attributed from its own key id. A sustained rate means a handler is routing
  to slots this subscriber did not ask for.

### `dt_client_media_decoder_errors_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: The audio decoder's terminal error callback fired — **one
  decoder per ACTIVE SENDER** (one per client before story 2 task 7), counted
  event-once per decoder instance. A fault class affecting every lane (a codec
  or platform fault) can therefore advance this up to N times for N active
  senders, and its magnitude is **not comparable with pre-story-2 history**.
- **Labels**: base only. **No `reason`** — `AudioDecoder` provides no bounded
  one, and an unbounded label here would be the cardinality hazard §11 exists to
  prevent.
- **Usage**: the ONLY counter covering the accepted→audible segment, which the
  receive-path identity deliberately does not reach. Event-driven, never
  per-frame.

### `dt_client_media_decode_queue_dropped_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Frames evicted from a per-sender decode lane's bounded pending
  queue while that lane's decoder is being created or replaced (overflow drops
  the oldest) — **one increment per evicted frame**.
- **Labels**: base only.
- **ACCOUNTING BOUNDARY — POST-ACCEPT, and deliberately NOT a
  `frames_dropped_total{reason}` value.** Every frame counted here was already
  counted on `dt_client_media_frames_accepted_total`, so it sits DOWNSTREAM of
  `received = accepted + sum(drops by reason)`, in the accepted→audible segment.
  Folding it into `frames_dropped_total` — the natural-looking "fix" for a
  counter named `..._dropped_total` beside the drop counter — would put one frame
  on **both** sides of the identity, breaking it silently and only in aggregate:
  the same failure `wrap_key_id_mismatch`'s `drops_frame: false` exists to
  prevent. It must never become a reason.
- **Panel only, no alert** (Group B, §Operator surface for the task-7 counters).
- **Why it exists**: a queue eviction fires no decoder error callback, so
  `dt_client_media_decoder_errors_total` does not see this loss. It mirrors the
  send-side precedent: the bounded egress queue counts its evictions because the
  platform exposes no event, and a receive-side queue that evicts silently would
  be that construct with the counter removed.

### `dt_client_media_mute_transitions_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `action` — `mute`, `unmute`.
- **Usage**: client mute is enforced at CAPTURE and does not depend on the server
  honouring it; MC keeps the send directive active throughout. This counter, not
  frame absence, is how mute state is observed — *muted*, *silent* and *the
  network died* are indistinguishable from absence alone.

### `dt_client_media_kek_updates_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `source` — `join_response` (the KEK carried on the join
  response) or `kek_update` (a KEK pushed over signaling). **A reconnect
  re-issue and a rotation arrive in the same `kek_update` message and are
  indistinguishable on this label — deliberately**: the discriminator would be
  the generation, which is barred below. This counter is the denominator for
  `dt_client_media_kek_retention_anomalies_total`.
- **THE TWO-VALUE `source` VOCABULARY IS DELIBERATE — do not "fix" it with a third
  value.** `MEDIA_KEK_SOURCES` in `packages/sdk-core/src/media/setup/mediaMetrics.ts`
  is exactly `join_response` and `kek_update`. **Corrected at story 2 task 16
  (paired-client, verified against code):** an earlier version of this paragraph
  said a reconnect re-issue arrives as a `MeetingKekUpdate`. It does not. The
  reconnect response is JoinResponse-shaped
  (`proto/dark_tower/signaling/v1/signaling.proto`), and MC's R-15 re-issue is a
  field on the reconnect result (`crates/mc-service/src/actors/meeting.rs`), so a
  re-issue counts as `join_response`, merged with a first join. `MeetingKekUpdate`
  is sent only by the rotation path. The two values stay deliberate for the correct
  reason: **the client cannot tell a rotation's CAUSE** (`participant_left` vs
  `sender_space_exhausted`), and the only discriminator on the message, the
  generation, is barred as a label (below). **The split exists, on the server: MC's
  `trigger` label on `mc_meeting_kek_generated_total`.** A rotation increments its
  trigger; a reconnect re-issue increments nothing
  (`docs/observability/metrics/mc-service.md`). Ask MC which it was; do not ask the
  client.
- **Usage**: the KEK arriving through the KEK-source seam. **Never the key
  itself, and never its generation** — the generation is monotonic over the
  meeting's life, so as a label its cardinality is unbounded over TIME rather
  than bounded by its type, and it advances on the leave debounce, which makes a
  per-meeting generation series a membership-change trace.

### `dt_client_media_kek_retention_violations_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: The KEK holder would have retained more than one previous
  generation; the retention guard (run after every install) trimmed and zeroized
  the excess.
- **Labels**: base only.
- **Fleet contract: reads zero forever; alertable at `> 0`** — the same contract
  form as `dt_client_media_send_dropped_total{reason="transport_send_refused"}`.
- **A ZERO-FOREVER TRIPWIRE AGAINST A FUTURE REFACTOR — NOT A DETECTOR FOR A LIVE
  CONDITION.** The retention bound is an invariant the SDK holds by construction and
  a unit test verifies: `RetentionGuard` in
  `packages/sdk-core/src/media/setup/kekSource.ts` runs after every install, and
  `media/setup/__tests__/kekSource.test.ts` drives it with a fabricated
  over-retained list and proves the check runs on every live install. There is no
  known legitimate non-zero cause and no operator remedy; a non-zero value means an
  SDK regression broke the bound, and the remedy is to roll back the SDK release
  (`client_version` on the series names it).
- **Alert**: `ClientKekRetentionViolation` (warning,
  `infra/docker/prometheus/rules/client-alerts.yaml`), **presence-shaped**:
  `sum(dt_client_media_kek_retention_violations_total) > 0`, never `increase()` or
  `rate()` — the series is absent until the first increment and appears already at
  1, so a rate would never see the only event that matters. It clears when the
  collector's exporter expires the idle series (`metric_expiration`,
  `infra/services/otel-collector/collector.yaml`), not when the condition stops.
  Group A (see §Operator surface for the task-7 counters).
- **A COUNTER, NOT A GAUGE — and why that is an ADR-0036 §11 corollary.** N browsers
  with identical label sets (no per-browser label: §11 bars one) write one stream
  identity, so a gauge would be last-writer-wins at the collector: one healthy
  browser's `0` would erase a violating browser's `1`. A counter's deltas SUM in the
  collector instead. The collision did not show §11 was wrong; **it showed the
  metric was shaped wrong** — a per-client state published as a gauge. The same
  corollary shaped `dt_client_media_receive_source_deficit_total` and
  `dt_client_media_kek_generations_retained_total`.

### `dt_client_media_kek_retention_anomalies_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `outcome` — this metric's own domain, exactly
  `floor_substituted`, `ceiling_clamped`, `below_rewrap_latency`. `outcome` is a
  per-metric vocabulary (`docs/observability/label-taxonomy.md`); never
  aggregate it across metric names.
- **Counting point**: evaluated per KEK message (`join_response` or
  `kek_update`), when the client derives how long to keep the previous
  generation: `min(W/2, ceiling)` where W is the message's
  `kek_rotation_debounce_seconds`. A nominal derivation counts nothing.
- **Permitted values**:
  - `floor_substituted` — W was zero or absent, so the client used its retention
    floor. That means an MC older than the field. **Reads zero forever EXCEPT
    during a deliberate one-version MC rollback, which is the SUPPORTED path and
    not an incident** — rolling MC below the field flips every live client to the
    floor and this climbs fleet-wide; do not page on it and do not roll further
    back. **An expected-non-zero configuration/rollback state: NEVER an alert
    input.** It is the fleet's rollback detector: the client also logs a WARN, but a
    browser console reaches no operator, so **this counter is the only
    cluster-visible evidence**.
  - `ceiling_clamped` — W/2 exceeded the client ceiling and retention was clamped.
    **A CONFIGURATION STATE, not an incident**: if
    `MC_KEK_ROTATION_DEBOUNCE_SECONDS` is raised so W/2 exceeds the ceiling, this
    is permanently non-zero on a correctly configured fleet. It reads zero at
    today's W — but only just: **the shipped default W already sits at the
    ceiling** (at `MC_KEK_ROTATION_DEBOUNCE_SECONDS`' default in
    `infra/services/mc-service/config.env`, W/2 EQUALS `KEK_RETENTION_CEILING_MS`;
    `clientConfig.ts` records the same fact), so **ANY increase to W makes this
    permanently non-zero while changing retention by exactly zero** — a first
    operator who raises W must not read the climbing counter as a fault. It is where an operator meets the fact that W is no longer
    the binding parameter. **Never an alert input.**
  - `below_rewrap_latency` — the derived retention does not exceed the client's
    transmit-key re-wrap latency T (the frames already queued for egress when the
    sender rotates), so frames wrapped under the previous generation can outlive
    it at the receiver. The client validates FLOOR > T at startup, so this can
    only fire when MC's W/2 is below T; **the remedy is in MC** — raise
    `MC_KEK_ROTATION_DEBOUNCE_SECONDS`. Retention is not raised client-side
    (that could exceed W).
- **No denominator of its own**: the denominator is
  `dt_client_media_kek_updates_total`, and any ratio needs the non-zero-denominator
  guard (no KEK messages in the window must read as no data, not as 0% or NaN
  noise).
- **Panel only, no alert** (Group B, §Operator surface for the task-7 counters).
  `floor_substituted` and `ceiling_clamped` are expected-non-zero configuration or
  rollback states and are NEVER alert inputs.

### `dt_client_media_kek_install_refusals_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Labels**: `outcome` — this metric's own domain, exactly `conflicting_key`,
  `older_generation`, `malformed`. **`outcome`, not `reason`**: `reason` is
  reserved for per-FRAME drops with exactly two homes
  (`docs/observability/label-taxonomy.md`), and a third family there would break
  cross-end `sum by(reason)`.
- **Permitted values**:
  - `conflicting_key` — a KEK for the CURRENT generation with different bytes.
    Refused; the held key is kept (compared in fixed time).
  - `older_generation` — a generation below the current. Refused; the holder
    never rolls back.
  - `malformed` — wrong width, all-zero, or a generation outside `0..65535`.
    Refused and scrubbed, on both arrival paths.
- **Alert on ONE arm**: `MCClientKekConflictingKey` (warning,
  `infra/docker/prometheus/rules/mc-alerts.yaml`), presence-shaped on
  `outcome="conflicting_key"` only — Group A. The justification is that it is the
  **SOLE OBSERVABLE witness of its arm**: a same-generation KEK-bytes split shows
  elsewhere only as `unwrap_failed` on peers, diluted inside
  `MCMediaMissingKeyMaterial`'s fleet-wide ratio, and that holds however client
  reconnect eventually lands. The rule comment lists every known non-zero cause.
  `older_generation` and `malformed` are panel-only (Group B).
- **Usage**: every refusal leaves the held state unchanged and working. **Both
  `conflicting_key` and `older_generation` are UNREACHABLE in an honest
  deployment today, and that is what makes this counter a tripwire rather than a
  delivery metric.** A client's KEK holder is per-session and the client has **no
  reconnect**, so no holder can outlive the KEK epoch it was given — see the
  refuse site in `packages/sdk-core/src/media/setup/kekSource.ts` for the
  MC-lifecycle detail and what the unreachability rests on. **Any increment is a
  defect — an MC bug, or a client holding one KEK holder across two sessions —
  not a delivery hiccup**: do not read it as "MC is delivering inconsistent KEK
  state" in the ordinary-operations sense, and do not go looking for a rollout as
  the cause. It is **not** reachable by an attacker without compromising MC,
  which already holds the KEK (ADR-0036 §4 operator custody); note however that a
  **forced refusal is how leave-rotation would be defeated** — the meeting keeps
  the key a departed member holds — so if an injection path onto the signaling
  channel ever appears, this counter is its detector. `malformed` is different in
  kind (a wire-shape violation, including a non-empty wrong-width key, rather
  than an epoch conflict) but reads the same way: zero from an honest MC. **The
  unreachability rests on the absence of client reconnect: re-check this entry
  when reconnect lands.**

### `dt_client_media_kek_generations_retained_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Retaining installs — **one increment per install that demoted
  a previous generation and kept it**. Not incremented on the first install at
  join, nor on an idempotent (same generation, same bytes) or refused install.
  The increment lands on the install, not on first use of the previous
  generation.
- **Labels**: base only.
- **Fleet signal is the PAIR**: `dt_client_media_kek_updates_total{source="kek_update"}`
  rising while this stays flat means rotations are happening and nothing is
  being retained — an audio gap at every rotation. **Neither series alone says
  anything**, so the panel reads this AGAINST that one on one panel (Group B, panel
  only); a panel of this counter by itself would not be a surface for it. It watches the opposite
  failure from `dt_client_media_kek_retention_violations_total` (retaining too
  few, rather than too many).
- **A counter, not a gauge — deliberately.** The story originally planned a
  retained-generations gauge over `{1, 2}`; with N browsers writing one stream
  identity (no per-instance label, §11) a gauge is last-writer-wins, so it would
  report one arbitrary browser's state. Use `increase()` over a rotation window.

### `dt_client_media_roster_key_rebinds_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: Changes to a LIVE sender's roster key — **one increment per
  event, not per purged key**. A first binding (new sender, or keyless→key) and
  an equal-bytes update are not counted.
- **Labels**: `outcome` — a per-metric vocabulary, two values that point at
  DIFFERENT fixes and carry DIFFERENT contracts:
  - `rebind` — the live `sender_id` bound to DIFFERENT, well-formed key bytes.
    MC never legitimately rebinds a LIVE id. It DOES reissue an id to a new
    identity after a KEK-epoch reset (story 2 R-16), but only one whose holder
    has left, and that holder's `ParticipantLeft` normally arrives first, so the
    reissue is a FIRST binding — not counted. **Near-zero, NOT
    reads-zero-forever.** A non-zero reading has three causes: an MC defect, an
    injected roster update, or a `ParticipantLeft` MC dropped under outbound
    backpressure (roster updates are sent with `try_send`), so this client saw
    the reissued id bound to a new key with no prior removal.
    The third cause REQUIRES a sender-id-exhaustion epoch reset in that meeting
    (`mc_meeting_kek_generated_total{trigger="sender_space_exhausted"}`; ids are
    reissued only under a later generation, `media_admission/epoch.rs`). No
    reset rules it OUT. `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}`,
    read over MC's process lifetime, only **supports** it; a short-window zero
    rules nothing out, because the lost Left can precede the reissue by hours.
    That counter is fleet-wide and per-mailbox, so it never confirms that any
    individual increment here was a lost leave. The delivery
    gap behind the third cause is filed in `docs/TODO.md` (roster removals are
    droppable). Key hygiene holds on all three: the purge below runs either
    way.
  - `downgrade` — a present key replaced by an empty, wrong-width or unusable
    one. **NOT zero-forever**: MC publishing an empty key is a documented
    occurrence (see `no_roster_entry` under `dt_client_media_frames_dropped_total`),
    which is exactly why it is split out — a merged counter could carry no
    alert contract without firing on a known condition.
- **Alert on ONE arm**: `MCClientRosterKeyRebind` (warning,
  `infra/docker/prometheus/rules/mc-alerts.yaml`), presence-shaped on
  `outcome="rebind"` — Group A. `rebind` is **near-zero, NOT zero-forever**: the rule
  comment lists all three causes, including the lost-`ParticipantLeft`-plus-reissue
  path, with `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}`
  as supporting evidence only. The decisive check is its necessary condition: a
  sender-id-exhaustion epoch reset (`trigger="sender_space_exhausted"`); none rules it out. `downgrade` is expected
  non-zero and is panel-only.
- **Effect**: the sender's cached unwrapped transmit keys are purged (zeroized),
  and the entry is known-keyless until the new key imports, so its frames drop
  as `no_roster_entry` in that window. The purge spans **every** KEK-generation
  scope (an older retained scope holds the departed holder's keys). **Replay
  state is retained** — no roster path touches it, in any scope. Roster-held identity keys are **trust-on-first-use**; this
  counter records a change of binding, it says nothing about which binding is
  authentic.

### `dt_client_media_capture_source`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Gauge, value `1`.
- **Description**: What feeds this client's send path. A **SAFETY OBSERVABLE**:
  a `test_tone` series outside a dev environment means a test-tone build
  (story 2 R-7, Vite build-time define `__DT_TEST_TONE__`) is serving real
  users, whose microphones are then not being sent at all.
- **Labels**: base + `mode` — `microphone` | `test_tone`, a closed, bounded,
  identity-free vocabulary (`MediaCaptureSourceMode` in
  `packages/sdk-core/src/media/setup/mediaMetrics.ts`). The value is a
  **build-time** property of the bundle, not per user. The `test_tone` token
  exists in the bundle only inside the define's gate, so a production bundle
  cannot emit it (asserted by `packages/web-app/tests/bundle-content.test.ts`).
  Row in `docs/observability/label-taxonomy.md`.
- **`microphone` means the default microphone path OR an embedder-injected
  capture factory** (a Playwright fake, a processed stream, ...); only the
  define-gated built-in tone reports `test_tone`. So the safety signal proves a
  test-tone build is EMITTING the tone, not merely that a build is test-tone: an
  injected factory in a `__DT_TEST_TONE__` build still reports `microphone`
  (`session/mediaSelection.ts:selectCaptureSource`).
- **THE SIGNAL IS THE PRESENCE OF THE `mode="test_tone"` SERIES, NOT ITS VALUE.**
  The value is always `1` and, because every browser writes one stream identity
  (no per-browser label, §11), the stored value is last-writer-wins: **it is not a
  count of browsers** and `sum()` over it is not "how many browsers run the tone".
  Only the active mode's series is emitted. A microphone build never
  writes a `test_tone` series at `0`: the signal is the series' PRESENCE, and a
  zero-valued series would make presence lie. Read it as
  `count(dt_client_media_capture_source{mode="test_tone"}) > 0`, or
  `sum by(mode)(...)` on a panel.
- **A gauge, and that is deliberate here** (the other story-2 client signals
  are counters because N browsers at one stream identity make a gauge
  last-writer-wins): last-writer-wins is harmless when every writer writes `1`
  and only the set of `mode` values matters.
- **Cadence**: set when the media pipeline starts and re-set on every
  export-interval tick of the pipeline's own timer (the SDK exports DELTA, and a
  last-value gauge is exported only for an interval in which it was recorded);
  the timer stops at teardown so a finished session does not pin the series.
  The pipeline timer and the OTel reader interval are separate clocks, so a
  single export can miss a recording: **presence is judged over the lookback
  window, not per scrape.**
- **Panel**: `infra/grafana/dashboards/client-media.json`. No alert —
  deliberate: dev and test environments legitimately carry `test_tone`, and the
  environment dimension is not on the series.

### `dt_client_media_receive_source_deficit_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter. **Unit: assignment-intervals.**
- **Description**: Per export interval, the number of **ACTIVE** receive
  assignments from which this client decoded **no** frame in that interval. An
  active assignment is a declared slot to which MC has assigned a sender and
  which MC has not marked source-muted (`slot_state = active`); NOT the declared
  slot count N. Observed decode activity is the receiver's ground truth, and its
  divergence from MC's claim is the signal: mirroring slot state would read
  healthy exactly when MC says active and nothing decodes.
- **Grace rule**: an assignment counts only if it was already active at the
  previous tick — i.e. it has been active for a full interval — so a join or a
  slot re-map does not tick the counter spuriously.
- **Reading it**: `rate(dt_client_media_receive_source_deficit_total[5m]) *
  export_interval_s` is the mean number of silent active assignments across the
  fleet (`export_interval_s` = the SDK's metric export interval,
  `DEFAULT_METRIC_EXPORT_INTERVAL_MS` in
  `packages/sdk-core/src/config/clientConfig.ts`, read by the tick through
  `getMetricExportIntervalMs()`). Premise: the encoder runs with DTX off
  (`lifecycle/muteState.ts`), so a live, unmuted sender always produces frames
  and zero decoded frames is a real fault, not silence.
- **Labels**: base only. **No sender, slot or mode label** — ever.
- **A counter, not a gauge — deliberately.** N browsers with identical label sets
  (no per-browser label, ADR-0036 §11) write one stream identity, so a gauge is
  last-writer-wins at the collector: one healthy browser's `0` would erase a
  starving browser's deficit. A counter's deltas sum in the collector instead. **The
  §11 corollary: the collision revealed the metric was shaped wrong, not that §11
  was wrong** — see `dt_client_media_kek_retention_violations_total`.
  Initialised with `add(0)` at pipeline start, so `rate()` has a series before
  the first deficit.
- **Blind spot**: a declaration MC rejects whole produces ZERO active
  assignments, so this reads 0 — see
  `dt_client_media_receive_slots_rejected_total` for that case.
- **Panel**: `infra/grafana/dashboards/client-media.json`. Triage:
  `docs/runbooks/client-dev-local.md` (receive-source deficit).

### `dt_client_media_receive_slots_rejected_total`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Counter
- **Description**: `startMedia()` calls refused because the client's declared
  receive-slot count N exceeds the cap MC advertised on
  `JoinResponse.max_receive_slots` (`MC_MAX_RECEIVE_SLOTS`). MC rejects such a
  declaration WHOLE, so the SDK refuses it locally, loudly, before sending —
  never shrinking N to fit. One increment per refusal. With an MC that predates
  the field (cap unknown) the SDK declares anyway and MC's
  `mc_media_receive_capability_declarations_total{outcome="slot_count_over_cap"}`
  counts it instead.
- **Labels**: base only.
- **Reads zero on a correctly configured deployment**: N is
  `VITE_DT_RECEIVE_SLOTS` (a build-time client knob outside `dt-guard
  env-config`'s reach), and the fix for a non-zero reading is to lower it or
  raise `MC_MAX_RECEIVE_SLOTS`. Initialised with `add(0)` at session start.
- **The ONLY signal for this failure**: the participant hears nobody, and
  `dt_client_media_receive_source_deficit_total` stays at 0 because there are no
  active assignments to be silent.
- **Panel**: `infra/grafana/dashboards/client-media.json` (its own panel, next
  to the deficit). Triage: `docs/runbooks/client-dev-local.md`.

### `dt_client_time_to_first_media_frame_ms`
- **Exported**: yes — reaches Prometheus through the collector's metric-name allowlist (`infra/services/otel-collector/collector.yaml`).
- **Type**: Histogram
- **Description**: Wall-clock ms from media pipeline start to the first media
  frame arriving at the wire.
- **Labels**: the **media-path** set (`client_version`, `org_id`,
  `key_custody`). **NOT a frozen-roster member**, notwithstanding its
  name's resemblance to `dt_client_time_to_first_mh_connected_ms` — the
  difference is what it observes, not what it is called.
- **Buckets**: TBD (deferred with the media dashboards).
- **OBSERVED, NEVER GATED (ADR-0036 §10).** No test asserts a wall-clock
  threshold against it, and none may: asserting an end-to-end latency target on a
  local cluster produces a permanent flake, ADR-0028 forbids quarantining quality
  gates, so the test would be deleted and the headline objective would end with
  ZERO coverage. Sampled from the start, so the measurement exists for every
  session rather than only for sessions that got far enough to be instrumented.
- **A HEALTHY VALUE HERE IS NOT EVIDENCE THAT AUDIO WAS HEARD — pair it with
  `dt_client_media_frames_accepted_total` before reading it as one.** This is a
  **wire-boundary** measurement, taken at the same point as
  `dt_client_media_frames_received_total`: `IngressPipeline.accept()` calls the
  observer as its FIRST statement, before decode, before signature verification,
  before decrypt. So a session in which every single frame is subsequently
  dropped still records a perfectly normal first-media time.
  That is not hypothetical. During ADR-0036 story 1's first end-to-end run this
  metric read **31 ms** while `frames_accepted` sat at **0** across 55 samples —
  every frame was arriving and being rejected `no_roster_entry` — and the pair
  was initially read as a broken counter rather than a broken receive path,
  by a reader with the source open. It is the entry most likely to be put on a
  dashboard on its own, and on its own it means only *datagrams are reaching
  us*. See §The receive-path accounting identity above for the three terms that
  together say whether media is actually working.

### No retention gauge — retention is derived, and W is in seconds end to end

**No gauge publishes client KEK retention, deliberately.** Retention is derived at
the client, in exactly one function, `deriveKekRetention` in
`packages/sdk-core/src/config/clientConfig.ts`: `min(W/2, KEK_RETENTION_CEILING_MS)`,
with `KEK_RETENTION_FLOOR_MS` substituted when W is absent or zero (the
`floor_substituted` outcome). A gauge would always be a function of W, and a
per-client gauge would be last-writer-wins (the §11 corollary above). Why a
retention was what it was is `dt_client_media_kek_retention_anomalies_total{outcome}`.

**W's chain, verified against code — all SECONDS, no conversion anywhere between them:**

| Home | Name | Unit |
|---|---|---|
| MC config | `MC_KEK_ROTATION_DEBOUNCE_SECONDS` → `Config::kek_rotation_debounce_seconds: u32` (`crates/mc-service/src/config.rs`) | seconds |
| MC runtime | `KekLifecycle::window_seconds` (`crates/mc-service/src/media_admission/rotation.rs`), the stored `u32` itself | seconds |
| Wire | `kek_rotation_debounce_seconds` (`uint32`, `JoinResponse` and `MeetingKekUpdate`, `proto/dark_tower/signaling/v1/signaling.proto`) | seconds |
| Metric | `mc_meeting_kek_rotation_window_seconds` (`window().as_secs_f64()`, a widening of the same seconds) | seconds |

The ONLY unit change in the whole path is inside `deriveKekRetention`, which turns
W/2 into the milliseconds its return value is expressed in. Nothing between MC's
config and the client's derivation converts.

### Operator surface for the task-7 counters — Group A / Group B (story 2 task 16)

Story 2 task 7 shipped six exported client counters and scheduled an operator surface
for one. All six are decided here; a panel nobody browses is not a surface for a
counter whose whole value is that something watches it.

**Group A — warning-tier ALERT plus panel** (`severity: warning` is the ticket tier):

| Counter (arm) | Alert | Why it is alerted |
|---|---|---|
| `dt_client_media_kek_retention_violations_total` | `ClientKekRetentionViolation` (`client-alerts.yaml`) | zero-forever tripwire against an SDK regression |
| `dt_client_media_kek_install_refusals_total{outcome="conflicting_key"}` | `MCClientKekConflictingKey` (`mc-alerts.yaml`) | the SOLE-OBSERVABLE witness of its arm |
| `dt_client_media_roster_key_rebinds_total{outcome="rebind"}` | `MCClientRosterKeyRebind` (`mc-alerts.yaml`) | near-zero, NOT zero-forever: a lost `ParticipantLeft` plus a reissue reaches it (correlator `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}`) |

**Group A rules are PRESENCE-shaped, `sum(X{sel}) > 0`** — never `increase()`/`rate()`.
These are lazy DELTA series: absent until the first increment, first stored already
at 1, so a rate never sees the first event, which for a tripwire is the only one that
matters. `dt-guard client-metrics-export` (`tripwire_rate_wrapped`) enforces the shape.
Each rule comment names every known non-zero cause, and a newly found legitimate cause
is ADDED there — the rule is never retired or silenced for it.

**Group B — panel only** (`infra/grafana/dashboards/client-media.json`):

- `dt_client_media_kek_retention_anomalies_total` — `floor_substituted` and
  `ceiling_clamped` are expected-non-zero configuration/rollback states and are NEVER
  alert inputs (the shipped default W already sits at the ceiling).
- `dt_client_media_kek_install_refusals_total`'s other two arms, `older_generation`
  and `malformed`.
- `dt_client_media_decode_queue_dropped_total`.
- `dt_client_media_kek_generations_retained_total`, read AGAINST
  `dt_client_media_kek_updates_total{source="kek_update"}` on one panel: rotations
  rising while retentions stay flat is the signal; neither series alone says anything.

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
