# Client SDK Metrics Catalog

**Service**: Browser SDK (`@darktower/sdk-core`)
**Implementation**: `packages/sdk-core/src/telemetry/` (sinks, name guard, providers)
**Job Label**: `darktower-sdk-core` (OTel resource `service.name`)

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

> ## NOT ONE METRIC IN THIS CATALOG IS QUERYABLE FROM PROMETHEUS TODAY
>
> **Emitted and exported is not the same as queryable, and the chain stops one
> hop short.** The GC proxy forwards to the OTLP collector, whose metrics
> pipeline is `receivers: [otlp]` / `exporters: [debug]`
> (`infra/services/otel-collector/configmap.yaml`). `debug` writes names and
> counts to the collector's own container log. There is no `prometheus` or
> `prometheusremotewrite` exporter, and no Prometheus job scrapes the collector.
>
> **So every metric below is real, correctly emitted, carries the labels this
> file documents — and cannot be graphed, alerted on, or queried.** One alert
> already rests on this (`MCMediaMissingKeyMaterial`); it is loaded, evaluating,
> and structurally incapable of matching. Tracked in `docs/TODO.md`
> §Observability Debt, owner infrastructure for the wiring.
>
> This block exists because the sentence above it — accurate about emission and
> export — invites the inference that the chain continues, and a catalog is
> exactly where someone goes to decide whether a signal is available before
> building on it. Stating the export path and stopping is how a reader concludes
> the metric is usable. Delete this block when the exporter lands, **not before,
> and not because the catalog looks pessimistic**: the state it describes is the
> state of the tree.

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
  - **Nothing mechanical enforces this in TypeScript.** `dt-guard`'s media-path deny is Rust-only, its metric-label scanner reads `crates/`, and `ts_pii.rs` scans only `console.*`/`logger.*` call sites. The rules and their enforcement status are in `docs/observability/label-taxonomy.md` R1/R2/R3 — **read them before adding a label here**; they are not restated in this file, because a second home drifts.

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

## Media-path metrics (ADR-0036 §11)

Emitted from `packages/sdk-core/src/media/setup/mediaMetrics.ts`, which is the
ONLY file under `packages/sdk-core/src/media/**` permitted to name a metric —
asserted by `media/__tests__/hotPathLayout.test.ts`, since nothing mechanical
enforces the media-path telemetry rules in TypeScript.

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
fifteen `drops_frame: true` reasons fire at or before decoder handoff, so the
identity is exact **by construction** at the crypto/parse boundary, and at the
playback boundary it would be FALSE — a frame lost between handoff and audible
decrements nothing on the right-hand side.

**ADR-0036 / R-25 prose spells the middle term `played`. This catalog
deliberately supersedes that spelling**, because `played` names an identity that
does not hold: a frame handed to a decoder is not played (the decoder can error,
the output can be discarded, the context can be suspended). The
accepted→audible segment is not covered by this identity and is covered only
partially by `dt_client_media_decoder_errors_total`; extending the identity to
the playback boundary would require playback-side drop reasons — a vocabulary
extension needing its own planning, not a word swap.

**Counting-point migration (video story).** When several frames share one
stream, `dt_client_media_frames_received_total` must move from the TRANSPORT
boundary to the PARSE boundary, and the identity must be re-established there.
Recorded here rather than only in a code comment because the constraint outlives
the comment's reader.

---

### `dt_client_media_frames_sent_total`
- **Type**: Counter
- **Description**: Frames that left the device on the media datagram path.
- **Labels**: base only.
- **Usage**: the denominator for the send-drop ratio.

### `dt_client_media_send_dropped_total`
- **Type**: Counter
- **Labels**: `reason` — a NEW bounded vocabulary, **not** the frame reject
  taxonomy.
- **Permitted values**:

  | `reason` | Shared with MH? | Meaning and triage |
  |---|---|---|
  | `egress_queue_overflow` | **shared** | The SDK's bounded queue was full and the OLDEST frame was evicted. Back-pressure at the application layer, where we can see it. |
  | `transport_send_refused` | **shared** | The transport refused a datagram it should have accepted. **Fleet contract: reads zero forever; alertable at `> 0`.** |
  | `oversize_datagram` | **shared** | The frame exceeded the transport maximum and was never offered. Separate from the row above precisely so a configuration condition cannot poison an invariant counter. |
  | `connection_closed` | **shared** | The connection closed underneath a send. **Routine** — a participant leaves every meeting, many times. Not alertable. |
  | `not_connected` | **client-only** | A send attempted before any transport was up. A lifecycle ordering bug; **reads zero forever; alertable at `> 0`.** |

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
- **Type**: Gauge
- **Description**: Current depth of the bounded application egress queue, in
  frames.
- **Labels**: base only.
- **Usage**: read against the configured bound (default 10 frames = 200 ms at
  20 ms/frame). The application bound trips BEFORE the transport high-water mark
  — asserted at setup — so a rising depth here is back-pressure we can count
  rather than loss inside the user agent.

### `dt_client_media_frames_received_total`
- **Type**: Counter
- **Description**: Datagrams received on the media path, counted **at the wire**
  before any parse or verification.
- **Labels**: base only.
- **Usage**: the left-hand side of the accounting identity above, and the signal
  that distinguishes "nothing arriving" from "arriving and failing". See the
  counting-point migration note.

### `dt_client_media_frames_dropped_total`
- **Type**: Counter
- **Labels**: `reason` — the frozen frame-reject vocabulary, whose SSoT is
  `proto/test-vectors/frame-v2.vectors.json` → `reject_reasons`. Membership of
  THIS counter is carried by `drops_frame`: fifteen of the sixteen tokens.
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
- **The two key-material reasons** are `no_kek_for_generation` (no meeting KEK
  for the generation the frame's wrap announces) and `no_roster_entry` (no usable
  identity key for the frame's `key_id.sender_id`, INCLUDING the case where MC
  published an empty key). Both are expected transients at join and after a KEK
  rotation; **the sustained case is the signal**, and it is the only signal for a
  join or rotation path that has silently stopped delivering keys.
- **`unwrap_failed` versus `decrypt_failed`** are two AES-GCM failures on one
  receive path routing to opposite teams: `unwrap_failed` is the KEK unwrap, so
  it is key DISTRIBUTION; `decrypt_failed` is the SFrame payload, so it is the
  key schedule or the sender.
- **`no_transmit_key`** is a frame with neither a cached key nor a usable wrap: a
  protocol violation, not a third key reason and not a decode reject.

### `dt_client_media_frames_accepted_total`
- **Type**: Counter
- **Description**: Frames that completed the receive path — verified, replay-
  checked, decrypted — and were handed to the audio decoder.
- **Labels**: base only.
- **NAMED `accepted`, NOT `played`.** A frame handed to a decoder is not played.
  Silent audio with this counter climbing means the fault is downstream of the
  handoff — the decoder, the output device, or a suspended audio context — and
  **not** that frames are playing. See the identity above.

### `dt_client_media_key_wrap_outcomes_total`
- **Type**: Counter
- **Labels**: `outcome` — the two NON-DROPPING wrapped-key outcomes. The frame
  was ACCEPTED in both cases; neither ever reaches the drop counter.
- **Permitted values**:
  - `kek_generation_not_held` — the frame's wrap announces a KEK generation this
    receiver does not hold, but a usable transmit key for that key id was already
    cached, so the frame plays off the cache. **This is the KEK-rotation-lag
    signal**: the sender re-wrapped under a generation whose push has not landed.
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

### `dt_client_media_downlink_gap_frames_total`
- **Type**: Counter
- **Description**: Frames MISSING between the media handler's egress and this
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
- **Type**: Counter
- **Description**: Datagrams arriving at or below the running hop-sequence
  high-water mark.
- **Labels**: base only.
- **Usage**: the subtrahend in `gap_frames - reorder`. Kept separate from the gap
  counter so the subtraction is possible at all.

### `dt_client_media_undeclared_stream_id_total`
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
- **Type**: Counter
- **Description**: The audio decoder's terminal error callback fired.
- **Labels**: base only. **No `reason`** — `AudioDecoder` provides no bounded
  one, and an unbounded label here would be the cardinality hazard §11 exists to
  prevent.
- **Usage**: the ONLY counter covering the accepted→audible segment, which the
  receive-path identity deliberately does not reach. Event-driven, never
  per-frame.

### `dt_client_media_mute_transitions_total`
- **Type**: Counter
- **Labels**: `action` — `mute`, `unmute`.
- **Usage**: client mute is enforced at CAPTURE and does not depend on the server
  honouring it; MC keeps the send directive active throughout. This counter, not
  frame absence, is how mute state is observed — *muted*, *silent* and *the
  network died* are indistinguishable from absence alone.

### `dt_client_media_kek_updates_total`
- **Type**: Counter
- **Labels**: `source` — `join_response` (the only source this story;
  KEK-push rotation adds one when it lands).
- **Usage**: the KEK arriving through the KEK-source seam. **Never the key
  itself, and never its generation** — the generation is monotonic over the
  meeting's life, so as a label its cardinality is unbounded over TIME rather
  than bounded by its type, and it advances on the leave debounce, which makes a
  per-meeting generation series a membership-change trace.

### `dt_client_time_to_first_media_frame_ms`
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
