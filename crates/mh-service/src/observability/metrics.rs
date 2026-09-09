//! Metrics definitions for Media Handler per ADR-0011
//!
//! All metrics follow Prometheus naming conventions:
//! - `mh_` prefix for Media Handler
//! - `_total` suffix for counters
//! - `_seconds` suffix for duration histograms
//!
//! # Cardinality
//!
//! Labels are bounded to prevent cardinality explosion (ADR-0011). The
//! per-metric label sets, their permitted values and their cardinality are
//! catalogued in `docs/observability/metrics/mh-service.md`, which is the
//! single source of truth for them.
//!
//! They are deliberately **not** re-listed here. Four homes used to restate
//! them and three of the four were wrong: this block claimed `method` had
//! three values and omitted `register_meeting`, the only value the code has
//! ever emitted. A docstring that *points at* a binding cannot drift; one that
//! *restates* an enumeration is a copy.

use common::observability::labels::{KEY_CUSTODY_LABEL, KEY_CUSTODY_OPERATOR};
use media_protocol::codec::{RejectReason, ALL_REJECT_REASONS};
use metrics::{counter, gauge, histogram, Counter, Gauge, Histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::time::Duration;

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets.
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    PrometheusBuilder::new()
        // GC heartbeat latency buckets - internal service call (p95 < 100ms)
        .set_buckets_for_metric(
            Matcher::Prefix("mh_gc_heartbeat".to_string()),
            &[
                0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set GC heartbeat buckets: {e}"))?
        // GC registration latency buckets - registration can be slower
        .set_buckets_for_metric(
            Matcher::Prefix("mh_gc_registration".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set GC registration buckets: {e}"))?
        // Token refresh latency buckets
        .set_buckets_for_metric(
            Matcher::Prefix("mh_token_refresh".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set token refresh buckets: {e}"))?
        // WebTransport handshake latency buckets (R-26)
        .set_buckets_for_metric(
            Matcher::Prefix("mh_webtransport_handshake".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000,
            ],
        )
        .map_err(|e| format!("Failed to set WebTransport handshake buckets: {e}"))?
        // Media forward-path latency (ADR-0036 §11). Registered in the same
        // file as the `histogram!` that resolves the handle, which is what
        // `dt-guard histogram-buckets` scans; a bucket set registered anywhere
        // else fails Layer 3.
        //
        // The edge slice is [`MEDIA_FORWARD_LATENCY_BUCKETS`] rather than a
        // literal here, so [`MEDIA_FORWARD_OBJECTIVE_SECONDS`] can be asserted
        // to be one of the edges. The `Matcher::Prefix` argument stays a
        // literal: the guard's regex captures a quoted first argument.
        .set_buckets_for_metric(
            Matcher::Prefix("mh_media_forward_latency".to_string()),
            &MEDIA_FORWARD_LATENCY_BUCKETS,
        )
        .map_err(|e| format!("Failed to set media forward latency buckets: {e}"))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {e}"))
}

/// Record a GC registration attempt.
///
/// Metric: `mh_gc_registration_total`
/// Labels: `status` (success | error)
/// Cardinality: 2
pub fn record_gc_registration(status: &str) {
    counter!("mh_gc_registration_total", "status" => status.to_string()).increment(1);
}

/// Record GC registration RPC latency.
///
/// Metric: `mh_gc_registration_duration_seconds`
/// Labels: none
/// Buckets: [0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000]
pub fn record_gc_registration_latency(duration: Duration) {
    histogram!("mh_gc_registration_duration_seconds").record(duration.as_secs_f64());
}

/// Record a GC heartbeat (load report) attempt.
///
/// Metric: `mh_gc_heartbeats_total`
/// Labels: `status` (success | error)
/// Cardinality: 2
pub fn record_gc_heartbeat(status: &str) {
    counter!("mh_gc_heartbeats_total", "status" => status.to_string()).increment(1);
}

/// Record GC heartbeat RPC latency.
///
/// Metric: `mh_gc_heartbeat_latency_seconds`
/// Labels: none
/// Buckets: [0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000]
pub fn record_gc_heartbeat_latency(duration: Duration) {
    histogram!("mh_gc_heartbeat_latency_seconds").record(duration.as_secs_f64());
}

/// Record a token refresh attempt.
///
/// Metric: `mh_token_refresh_total`
/// Labels: `status` (success | error)
/// Cardinality: 2
///
/// On error, also increments `mh_token_refresh_failures_total` with `error_type`.
pub fn record_token_refresh(status: &str, error_type: Option<&str>, duration: Duration) {
    counter!("mh_token_refresh_total", "status" => status.to_string()).increment(1);
    histogram!("mh_token_refresh_duration_seconds").record(duration.as_secs_f64());

    if let Some(err_type) = error_type {
        counter!(
            "mh_token_refresh_failures_total",
            "error_type" => err_type.to_string()
        )
        .increment(1);
    }
}

/// Record metrics for a token-refresh attempt (ADR-0032 Category B extraction).
///
/// Callable from `main.rs`'s `TokenManager::with_on_refresh` closure and from
/// unit/integration tests. Maps `TokenRefreshEvent.success: bool` to the
/// bounded `status` label and forwards `error_category` + `duration` into
/// `record_token_refresh`. Production emission is byte-identical to the prior
/// inline closure body at `main.rs:114-122`.
pub fn record_token_refresh_metrics(event: &common::token_manager::TokenRefreshEvent) {
    let status = if event.success { "success" } else { "error" };
    record_token_refresh(status, event.error_category, event.duration);
}

/// The only method on `MediaHandlerService`, as a metric label value.
///
/// `internal.proto`'s service block is the single source of truth for this
/// value set: ADR-0036 §8 makes meeting registration the control plane, so the
/// service "gains FIELDS rather than sibling RPCs" and is expected to keep
/// exactly one method. The
/// `register`, `route_media` and `stream_telemetry` values retired with the
/// 2026-09-01 reshape can never appear again — the proto's tombstone block
/// forbids resurrecting the names.
const GRPC_METHOD_REGISTER_MEETING: &str = "register_meeting";

/// Record an incoming gRPC request from MC.
///
/// Metric: `mh_grpc_requests_total`
/// Labels: `method` (single value, bound by `GRPC_METHOD_REGISTER_MEETING`), `status` (success | error)
/// Cardinality: 2
///
/// The `method` label stays on the series — runbook queries and dashboard
/// panels select on `method="register_meeting"`, and an absent label yields an
/// empty result rather than an error, so removing it would silently blank them.
/// The *parameter* is gone, which is what makes the single value structural: a
/// second one cannot be introduced from a call site.
pub fn record_grpc_request(status: &str) {
    counter!(
        "mh_grpc_requests_total",
        "method" => GRPC_METHOD_REGISTER_MEETING,
        "status" => status.to_string()
    )
    .increment(1);
}

/// Record the outcome of an ADR-0036 §8 forwarding-policy apply.
///
/// Metric: `mh_media_policy_applies_total`
/// Labels: `outcome` (the [`PolicyApplyOutcome`] variants — the compile-checked
///   [`PolicyApplyOutcome::ALL`] is the count), `key_custody` (single value `operator`)
///
/// Counts **registrations whose policy MH considered** — every terminal path
/// from the first read of a policy-bearing field (`egress_streams`,
/// `selection_rules`, `policy_generation`) onward, exactly once. Checks that
/// read only the caller-identity and reachability scalars (`meeting_id`,
/// `mc_id`, `mc_grpc_endpoint`) are *pre-boundary* and stay on
/// `mh_grpc_requests_total{status="error"}` alone: "MC's assignment computation
/// produced a policy MH will not apply" and "this caller's endpoint is
/// malformed" have different owners and different remedies, and folding them
/// into one series makes the sum unusable as a denominator.
///
/// The label key is `outcome`, not `status`. `status` is `label-taxonomy.md`'s
/// *coarse, fleet-wide shared* classification; this is a fine-grained,
/// metric-local taxonomy in which each value names a distinct remedy — and
/// `internal.proto` already names the MC-side counterpart of this same RPC
/// `outcome`, so both ends of the handshake carry one label key.
///
/// Emits no generation value, no `meeting_id` (raw or hashed), and no stream
/// identity. Generations belong in the log line and in this metric's *value*;
/// as labels they would be unbounded, one new series per policy change.
pub fn record_media_policy_apply(outcome: PolicyApplyOutcome) {
    counter!(
        "mh_media_policy_applies_total",
        "outcome" => outcome.as_label(),
        KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
    )
    .increment(1);
}

/// Bounded `outcome` label values for `mh_media_policy_applies_total`.
///
/// An enum rather than a free `&str` so the label set is closed at the type
/// level: a typo or a sixth value is a compile error, not a new time series
/// discovered in production.
///
/// Each value exists because its **remedy differs**, which is the test for
/// whether a bounded outcome label is doing any work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyApplyOutcome {
    /// The live forward path reflects the generation MC sent. Covers a fresh
    /// install and an idempotent re-assert of an already-installed generation.
    ///
    /// The re-assert case belongs here, not under `RejectedStale`: ADR-0036
    /// §8's cadence re-asserts every meeting every <=10 s in perfect health, so
    /// counting it as a rejection would drive that series monotonically upward
    /// in the steady state and make any alert on it dead on arrival.
    Applied,
    /// A generation strictly lower than the installed one was ignored.
    /// Reordered or retried delivery on the MC→MH path; abnormal.
    RejectedStale,
    /// `policy_generation` was 0 — MC named no generation.
    ///
    /// The **expected steady state for the whole task-11→task-13 window**, when
    /// MC has not yet begun emitting real generations. It is also the value the
    /// *rejection* will land on once task 13 turns enforcement on: two eras,
    /// one value, no rename, and the series falls to zero exactly when MC
    /// starts sending >= 1.
    NoGeneration,
    /// The policy failed structural validation. MC sent bad policy; the remedy
    /// is upstream. Distinct from `ApplyFailed` so an MC policy bug is never
    /// indistinguishable from an MH internal fault.
    RejectedInvalid,
    /// MH could not install it — mailbox full, apply timed out, actor gone, or
    /// the aggregate egress-edge bound. The prior generation stays live.
    ApplyFailed,
}

impl PolicyApplyOutcome {
    /// Every value of this enum, in catalog order.
    ///
    /// **The one hand-maintained list, and it lives here.** `as_label`'s
    /// wildcard-free `match` is the only thing a new variant forces an update
    /// to; every *enumeration* of the variants elsewhere would compile clean
    /// while staying silently short, and the enumerations that go short first
    /// are the exhaustiveness tests whose whole job is to be complete.
    /// `media-protocol`'s `reject_reasons!` macro and its `ALL_REJECT_REASONS`
    /// slice exist for exactly this failure; that macro is not exported and
    /// lives in a Guarded Shared Area, so this applies the pattern locally.
    /// Every consumer — the recorders' tests, the cardinality test and the
    /// integration label assertions — iterates this slice.
    /// The length is written out rather than inferred: it is the catalogued
    /// cardinality of the `outcome` label, so a variant added to this array
    /// without the catalog and the dashboard being revisited fails to compile
    /// here first.
    pub const ALL: [Self; 5] = [
        Self::Applied,
        Self::RejectedStale,
        Self::NoGeneration,
        Self::RejectedInvalid,
        Self::ApplyFailed,
    ];

    /// The wire label value.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::RejectedStale => "rejected_stale",
            Self::NoGeneration => "no_generation",
            Self::RejectedInvalid => "rejected_invalid",
            Self::ApplyFailed => "apply_failed",
        }
    }
}

/// Record `count` frames dropped for one MH-local reason, from a SIBLING of the
/// media forward path.
///
/// # Why this exists beside `MediaMetricHandles`, rather than instead of it
///
/// The hot path must not touch a macro or a registry (ADR-0036 §11), so
/// `crates/mh-service/src/media/**` counts exclusively through handles resolved
/// once at setup. This function is for the **connection lifecycle**, which is a
/// sibling of that directory and is entitled to the macro: it fires at most
/// twice per connection — never per frame — and the connection handler holds no
/// `MediaMetricHandles` of its own.
///
/// `count` rather than a bare increment because both callers have a whole
/// connection's tally to record in one step, and a loop calling `increment(1)`
/// n times would be the same series with more work and a wider window for a
/// partial write.
///
/// A zero `count` is recorded as zero rather than skipped: the series is
/// pre-registered by `resolve_media_handles` at process start, so it reads
/// present-and-zero either way, and a conditional here would be a branch whose
/// only effect is to make the code look like it does something.
pub fn record_media_frames_dropped(reason: MediaDropReason, count: u64) {
    counter!(
        "mh_media_frames_dropped_total",
        "reason" => reason.as_str(),
        "direction" => reason.direction().as_str(),
        KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
    )
    .increment(count);
}

/// Record one connection's terminal media-session-start decision.
///
/// Metric: `mh_media_session_starts_total`
/// Labels: `outcome` (the [`MediaSessionStartOutcome`] variants — the
///   compile-checked [`MediaSessionStartOutcome::ALL`] is the count), `key_custody`
///   (single value `operator`)
///
/// **Counts connections that ATTEMPTED to start a media session**, exactly once
/// each, on every terminal path from the boundary onward. The boundary is: the
/// connection passed the JWT gate and **entered the media-session start
/// sequence**, whose first action is the MC-endpoint lookup. Pre-boundary
/// rejects — JWT validation failure, WebTransport handshake failure, framing
/// errors — land on `mh_jwt_validations_total` / `mh_webtransport_connections_total`
/// alone and never touch this counter: "this caller's token is bad" and "MC could
/// not name this participant's sender" have different owners.
///
/// **The boundary sits where it does deliberately, and one step later is the
/// tempting simplification.** With the boundary at "MH issued the RPC", a meeting
/// that was never registered on this handler would close every connection while
/// incrementing only an undifferentiated `mh_webtransport_connections_total{status="error"}`
/// and emitting a `warn!` — MH healthy, forwarding nothing, detectable only in
/// logs. That is the exact failure shape this metric exists to close, reproduced
/// one condition earlier.
///
/// **This is NOT a frame drop and must never be folded into
/// `mh_media_frames_dropped_total`.** No frame was dropped. The catalog defines
/// `forwarded + dropped = attempts`; a decline counted as a drop puts a non-frame
/// event in the drop-rate numerator and corrupts the denominator identity. The
/// two counters have disjoint units — sessions here, frames there.
///
/// The label key is `outcome`, not `status`, for the same reason as
/// [`record_media_policy_apply`]: `status` is `label-taxonomy.md`'s coarse,
/// fleet-wide classification, and this is a metric-local taxonomy in which each
/// value names a distinct remedy.
///
/// Emits no `sender_id`, no `participant_id`, no `meeting_id` and no connection
/// identity (ADR-0036 §11). `outcome` is the only variable label.
pub fn record_media_session_start(outcome: MediaSessionStartOutcome) {
    counter!(
        "mh_media_session_starts_total",
        "outcome" => outcome.as_label(),
        KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
    )
    .increment(1);
}

/// Bounded `outcome` label values for `mh_media_session_starts_total`.
///
/// An enum rather than a free `&str` so the label set is closed at the type
/// level, exactly as [`PolicyApplyOutcome`] is: a typo or a seventh value is a
/// compile error, not a new time series discovered in production.
///
/// Each value exists because its **remedy differs**, which is the test for
/// whether a bounded outcome label is doing any work. Ordered health-first, then
/// the three MC-allocator faults grouped, then the two MC-reachability faults;
/// the catalog table and the dashboard legend use the same order so the three
/// artefacts read alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaSessionStartOutcome {
    /// The binding was installed and the three media loops were spawned.
    ///
    /// **Means the loops were spawned, not that media flowed.** It is measured
    /// strictly upstream of `mh_media_frames_forwarded_total`.
    ///
    /// # It LOCALISES; it does NOT attribute — corrected, and the old reading
    /// misdirected three sessions
    ///
    /// This docstring used to say that `started` climbing while ingress frames
    /// stay flat at zero "is a client-side condition rather than an MH one".
    /// **That is false**, and it is the reading a responder acted on at story
    /// task 24: they concluded MH was receiving nothing because the client was
    /// sending nothing, when in fact *nothing in the suite had sent a frame at
    /// all* — the only test in the tree that sends a media datagram was
    /// `#[ignore]`d. Every other scenario opens a connection, starts a session
    /// and sends nothing, so that shape is what a HEALTHY suite looks like.
    ///
    /// The honest claim is narrower. `started` proves the loops were
    /// **spawned**, which is upstream of the loop **receiving** anything — so
    /// the pair localises a fault to "at or after the spawn" and says nothing
    /// about which side owns it. The question that has to be asked *first* is
    /// whether any datagram arrived, and the discriminators for that are
    /// `mh_media_frames_dropped_total{reason="transport_receive_dropped"}` and
    /// `{reason="no_media_session"}`, which are non-zero only if datagrams
    /// actually reached the handler.
    ///
    /// This is one of three homes of the retired claim; the other two are the
    /// triage row and the panel description named in
    /// `docs/observability/metrics/mh-service.md`. They move together.
    Started,
    /// MC answered with no usable `sender_id`.
    ///
    /// Under the bare-`uint32` field, absent and `0` are one observable, so this
    /// value deliberately does **not** claim which cause — the same two-eras,
    /// one-value shape as [`PolicyApplyOutcome::NoGeneration`], and no rename is
    /// needed if explicit presence is ever reintroduced.
    ///
    /// It is the **union of MC's four unresolved outcomes**
    /// (`mc_media_sender_binding_responses_total{outcome}` in
    /// `meeting_unknown`, `participant_unknown`, `registry_full`,
    /// `user_ambiguous`); all four answer the wire value `0`, and MH sees only
    /// that `0` and structurally cannot reconstruct which produced it. The split
    /// lives on MC's counter, not here — read it there when this rate moves.
    /// Note in particular that `registry_full` (MC's per-meeting connection
    /// registry at capacity) presents as an elevated-but-flat rate, so an
    /// increase in this MH-side value must not be characterised as a race.
    DeclinedNoSenderBinding,
    /// MC answered above the 16-bit key-id field.
    ///
    /// **Should read zero forever.** MC's allocator is bounded and `SenderId`
    /// wraps a `NonZeroU16`, so MC structurally cannot produce this: if it moves,
    /// the candidates are field corruption in transit or a mis-versioned or
    /// foreign peer answering. MC's allocator **range**.
    DeclinedSenderBindingOutOfRange,
    /// The `sender_id` MC returned is already bound to a **different**
    /// participant in that meeting, so MH refused rather than overwrote.
    ///
    /// **Should read zero forever.** Distinct from
    /// [`Self::DeclinedSenderBindingOutOfRange`] in cause — that is MC's
    /// allocator *range*, this is its *uniqueness / non-recycling* — and in
    /// consequence: this is the only decline where the binding MH refused could
    /// have crossed media between participants had it been accepted. The refusal
    /// is the control.
    ///
    /// **Two candidate causes and MH cannot tell them apart**: MC allocated one
    /// live ordinal to two participants, or MH did not unbind the previous holder
    /// on teardown. It is therefore the only value in this set with an MH-side
    /// first move; every other decline points upstream.
    ///
    /// Counts the **refused newcomer**, exactly once. The incumbent connection is
    /// untouched and increments nothing.
    DeclinedSenderBindingConflict,
    /// The RPC reached MC (or the network) and yielded no usable response —
    /// timeout, transport failure, or a non-auth error status — after the MC
    /// client's retries.
    ///
    /// MC availability or the network between them. **Not** a credential
    /// rejection, which is [`Self::DeclinedMcAuthRejected`]: MC refusing MH's
    /// token means MC is reachable and healthy, and reporting that as
    /// unavailability sends a responder to the wrong service. Read with
    /// `mh_mc_notifications_total{event_type="connected",status="error"}`, which
    /// is the **attempt**-level view of the same cause: one connection with three
    /// retries is three there and one here.
    ///
    /// Declines only after the whole retry budget, which is what distinguishes it
    /// live from the two fast-declining reachability outcomes.
    DeclinedMcUnavailable,
    /// MH's outbound credential failed — MC **refused** it
    /// (`UNAUTHENTICATED` / `PERMISSION_DENIED`), or MH could not **build** one.
    ///
    /// **MH's outbound auth, not MC's health** — in both routes MC may be
    /// entirely well; in the build route MH never even sent a request.
    ///
    /// The two are separated by a **log line**, not a counter: the build route
    /// is the only emitter of `"Authorization header parse failed"` **on target
    /// `mh.grpc.mc_client`**. The target qualifier is load-bearing, not
    /// decoration — `grpc::gc_client` emits the identical message on
    /// `mh.grpc.gc_client`, in the same process, off the same
    /// `TokenReceiver`, so an unqualified match fails OPEN: it hits under this
    /// very fault (both clients fail to build at once, so it is right for the
    /// wrong reason) and it also hits when a GC-path parse failure coincides
    /// with a genuine refusal, producing a confident wrong verdict.
    /// `mh_token_refresh_failures_total` is context for
    /// both and separates neither — a refresh can succeed and return a token MH
    /// cannot put on the wire, and a refresh can fail while the previously
    /// cached token still builds fine.
    ///
    /// More reachable than either allocator-fault value: an expired MH service
    /// token or a JWKS rotation fires this for **every connection on the handler
    /// at once** — precisely when someone is reading this label under pressure,
    /// and precisely when being told "MC is down" costs the most time.
    ///
    /// **The two routes differ in timing and the value does not claim
    /// otherwise**: a refusal is terminal and declines immediately, while a
    /// build failure is retried, because the token comes from a watch channel
    /// and a refresh landing mid-budget can genuinely fix it. So this value is
    /// *usually* fast but not always, and it must not be used as a timing
    /// discriminator on its own.
    DeclinedMcAuthRejected,
    /// MH holds no MC endpoint for this meeting, so it never asked.
    ///
    /// **Should read at or near zero** — the meeting should have been registered
    /// on this handler before any connection for it was accepted. A registration
    /// fault, not a reachability one: distinct from [`Self::DeclinedMcUnavailable`]
    /// because there MH knew where MC was and got no usable answer, and here it
    /// never knew.
    DeclinedMcEndpointUnknown,
}

impl MediaSessionStartOutcome {
    /// Every value of this enum, in catalog order.
    ///
    /// The one hand-maintained list, for the reason spelled out on
    /// [`PolicyApplyOutcome::ALL`]: `as_label`'s wildcard-free `match` is the
    /// only thing a new variant forces an update to, and every *enumeration* of
    /// the variants elsewhere would compile clean while staying silently short.
    /// Every consumer — the recorder tests, the cardinality test and the
    /// integration label assertions — iterates this slice.
    ///
    /// The length is written out rather than inferred: it is the catalogued
    /// cardinality of the `outcome` label, so a variant added here without the
    /// catalog and the dashboard being revisited fails to compile first.
    pub const ALL: [Self; 7] = [
        Self::Started,
        Self::DeclinedNoSenderBinding,
        Self::DeclinedSenderBindingOutOfRange,
        Self::DeclinedSenderBindingConflict,
        Self::DeclinedMcUnavailable,
        Self::DeclinedMcAuthRejected,
        Self::DeclinedMcEndpointUnknown,
    ];

    /// The wire label value.
    ///
    /// The decline tokens spell `sender_binding` rather than `sender_id` so an
    /// auditor grepping metric code for `sender_id` gets no false hit to triage.
    #[must_use]
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::DeclinedNoSenderBinding => "declined_no_sender_binding",
            Self::DeclinedSenderBindingOutOfRange => "declined_sender_binding_out_of_range",
            Self::DeclinedSenderBindingConflict => "declined_sender_binding_conflict",
            Self::DeclinedMcUnavailable => "declined_mc_unavailable",
            Self::DeclinedMcAuthRejected => "declined_mc_auth_rejected",
            Self::DeclinedMcEndpointUnknown => "declined_mc_endpoint_unknown",
        }
    }
}

/// Record an error for the global error counter.
///
/// Metric: `mh_errors_total`
/// Labels: `operation`, `error_type`, `status_code`
/// Cardinality: bounded by `MhError` variants x operations
pub fn record_error(operation: &str, error_type: &str, status_code: u16) {
    counter!(
        "mh_errors_total",
        "operation" => operation.to_string(),
        "error_type" => error_type.to_string(),
        "status_code" => status_code.to_string()
    )
    .increment(1);
}

/// Record a WebTransport connection event (R-26).
///
/// Metric: `mh_webtransport_connections_total`
/// Labels: `status` (accepted | rejected | error)
/// Cardinality: 3
pub fn record_webtransport_connection(status: &str) {
    counter!("mh_webtransport_connections_total", "status" => status.to_string()).increment(1);
}

/// Record WebTransport handshake duration (R-26).
///
/// Metric: `mh_webtransport_handshake_duration_seconds`
/// Labels: none
/// Buckets: [0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.000, 5.000]
pub fn record_webtransport_handshake_duration(duration: Duration) {
    histogram!("mh_webtransport_handshake_duration_seconds").record(duration.as_secs_f64());
}

/// Set the active WebTransport connections gauge (R-26).
///
/// Metric: `mh_active_connections`
/// Labels: none
pub fn set_active_connections(count: f64) {
    gauge!("mh_active_connections").set(count);
}

/// Record a `RegisterMeeting` provisional-accept timeout (R-26).
///
/// Metric: `mh_register_meeting_timeouts_total`
/// Labels: none
/// Cardinality: 1
///
/// Fires from the timeout arm of
/// `webtransport::connection::await_meeting_registration` when a
/// WebTransport client was accepted before MC finished assigning the
/// meeting to this MH, and `MC::RegisterMeeting` did not arrive within
/// the configured `register_meeting_timeout`. The connection is then
/// disconnected.
///
/// Does NOT fire on shutdown-driven cancellation or successful
/// registration (separate arms). This invariant is enforced by behavioral
/// tests co-located with the helper.
///
/// A non-zero rate signals MC → MH `RegisterMeeting` latency issues or a
/// client-side sequencing bug (client connected before MC finished assignment).
pub fn record_register_meeting_timeout() {
    counter!("mh_register_meeting_timeouts_total").increment(1);
}

/// Record an MC notification delivery attempt (R-16/R-17).
///
/// Metric: `mh_mc_notifications_total`
/// Labels: `event_type` (connected | disconnected), `status` (success | error)
/// Cardinality: 4 (2 event types x 2 statuses)
pub fn record_mc_notification(event_type: &str, status: &str) {
    counter!(
        "mh_mc_notifications_total",
        "event_type" => event_type.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

/// Record a JWT validation attempt (R-27).
///
/// Metric: `mh_jwt_validations_total`
/// Labels: `result`, `token_type`, `failure_reason`
///
/// Result values: "success", "failure"
/// Token type values: "meeting", "service"
/// Failure reason values: `none` (success), `signature_invalid`, `expired`,
///   `scope_mismatch`, `malformed`, `validation_failed`
/// Cardinality: bounded (2 x 2 x 6 = 24 max, but most combos are sparse in practice)
pub fn record_jwt_validation(result: &str, token_type: &str, failure_reason: &str) {
    counter!("mh_jwt_validations_total",
        "result" => result.to_string(),
        "token_type" => token_type.to_string(),
        "failure_reason" => failure_reason.to_string()
    )
    .increment(1);
}

// ============================================================================
// gRPC Auth Layer 2 Metrics (ADR-0003)
// ============================================================================

/// Record a caller `service_type` rejection by Layer 2 routing.
///
/// Metric: `mh_caller_type_rejected_total`
/// Labels: `grpc_service`, `expected_type`, `actual_type`
///
/// Cardinality: 1 x 1 x 3 = 3 max (1 gRPC service, 1 expected type, ~3 actual types + "unknown")
///
/// ALERT: Any non-zero value indicates a bug or misconfiguration — a service
/// is presenting a valid token but calling the wrong gRPC endpoint.
pub fn record_caller_type_rejected(grpc_service: &str, expected_type: &str, actual_type: &str) {
    counter!("mh_caller_type_rejected_total",
        "grpc_service" => grpc_service.to_string(),
        "expected_type" => expected_type.to_string(),
        "actual_type" => actual_type.to_string()
    )
    .increment(1);
}

// ---------------------------------------------------------------------------
// Media forward path (ADR-0036 §2 / §7 / §11)
// ---------------------------------------------------------------------------

/// Whether a media-path event happened on the way IN to the relay or on the
/// way OUT of it.
///
/// **Pipeline-relative, never participant-relative.** `ingress` is
/// publisher→relay and `egress` is relay→subscriber; the participant-relative
/// reading (`uplink`/`downlink`) is barred. Both readings are 2-valued and a
/// catalog entry cannot tell them apart, but only the pipeline-relative one is
/// *structurally incapable* of growing a third value that individuates a
/// participant — which is the property ADR-0036 §11 needs, not the arity.
///
/// `direction` is admitted on a media-path metric **because the relay is
/// keyless**: the leak a direction label would otherwise enable is an oracle
/// over receiver key-cache state, and a relay holds none. **That acceptance
/// does not generalise to the client's counter** (story task 19), which sits
/// on the other side of exactly that state. See
/// `docs/observability/label-taxonomy.md` §"Permitted partner: `direction`".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDirection {
    /// Publisher → relay.
    Ingress,
    /// Relay → subscriber.
    Egress,
}

impl MediaDirection {
    /// Every value, in catalog order.
    pub const ALL: [Self; 2] = [Self::Ingress, Self::Egress];

    /// The wire label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ingress => "ingress",
            Self::Egress => "egress",
        }
    }
}

/// The decomposition of MH's internal forward latency (ADR-0036 §11).
///
/// Service-local, not a fleet-shared vocabulary: these four names describe
/// MH's own three stages plus their sum, and each has a *different remedy* —
/// which is the whole reason §11 refuses an undifferentiated total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaLatencyPhase {
    /// Datagram received from the transport → popped off the ingress queue.
    ReceiveBuffer,
    /// Popped off the ingress queue → pushed onto a subscriber's egress queue.
    Processing,
    /// Pushed onto the egress queue → the transport send call returned.
    TransmitBuffer,
    /// Received from the transport → the transport send call returned.
    Total,
}

impl MediaLatencyPhase {
    /// Every value, in catalog order.
    pub const ALL: [Self; 4] = [
        Self::ReceiveBuffer,
        Self::Processing,
        Self::TransmitBuffer,
        Self::Total,
    ];

    /// The wire label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReceiveBuffer => "receive_buffer",
            Self::Processing => "processing",
            Self::TransmitBuffer => "transmit_buffer",
            Self::Total => "total",
        }
    }
}

/// MH-local `reason` values for `mh_media_frames_dropped_total`.
///
/// # One label space, two families — not two vocabularies
///
/// `reason` is a **shared, layered** label space. `media_protocol`'s
/// `reject_reasons!` macro documents its eight tokens as "the structural /
/// parse subset of a **shared** `reason` label space", and **MH is a
/// first-class member of the codec layer** —
/// `RejectReason::producible_by()` names `rewrite_relay_region` as a producing
/// entry point. So the codec tokens are emitted on this same metric, verbatim
/// via `RejectReason::as_str()`, and the tokens below are MH's own additions to
/// that space. Never re-spell a codec token here; never invent an MH spelling
/// for a condition the codec already names.
///
/// **MH must never emit a crypto- or key-layer token** — `signature_invalid`,
/// `decrypt_failed`, `unwrap_failed`, `replay_detected`,
/// `wrap_key_id_mismatch`, `no_kek_for_generation`, `no_roster_entry`,
/// `no_transmit_key`. MH is keyless and never opens a frame, so such a series
/// asserts a verification MH is structurally incapable of performing, and an
/// operator reads it as "MH validates frames" and then relies on a control that
/// does not exist. The collision test in
/// `crates/mh-service/tests/media_metrics_integration.rs` pins this against all
/// sixteen tokens in `proto/test-vectors/frame-v2.vectors.json`.
///
/// ANCHOR (DRY): the operator-facing meaning of each token, and the split
/// between the should-read-zero invariant-violation group and the
/// saturation-or-input group, live in
/// `docs/observability/metrics/mh-service.md` §Media Forward Path. That file is
/// the single source of truth for what these mean; this enum is the single
/// source of truth for how they are spelled.
///
/// Modelled on [`PolicyApplyOutcome`]: an `ALL` array plus a wildcard-free
/// `as_str`, so a typo or a new value is a compile error rather than a new
/// time series discovered in production.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDropReason {
    /// MH's bounded ingress queue shed its oldest frame. MH ingest is
    /// saturated — the forward loop is not keeping up with the receive loop.
    IngressQueueOverflow,
    /// MH's bounded egress queue shed its oldest frame. A slow subscriber;
    /// **this is expected load shedding**, not a fault.
    EgressQueueOverflow,
    /// The transport seam refused a datagram it should have accepted
    /// (`TooLarge` or `DatagramsUnsupported`). **Invariant violation: should
    /// read zero forever.**
    TransportSendRefused,
    /// The subscriber's connection closed mid-flight. **Routine** — a
    /// participant leaves every meeting, many times — and deliberately not
    /// folded into [`Self::TransportSendRefused`], which would make that
    /// counter an unalertable mixture of "we have a bug" and "someone hung up".
    ConnectionClosed,
    /// The received datagram's **byte length** exceeded the wire-format frame
    /// maximum, rejected before any parse.
    ///
    /// Deliberately **not** the codec's `payload_length_exceeds_max`, even
    /// though the same constant is behind both: that token means "the declared
    /// `payload_length` **field** exceeded the max during header validation",
    /// which is identical to the client's condition and must keep the shared
    /// spelling. This one is a pre-parse whole-datagram cap. Same constant, two
    /// checks, two tokens, both implemented.
    OversizeDatagram,
    /// A connection exceeded its per-connection stream **creation-rate** cap.
    ///
    /// **UNREACHABLE UNTIL UNI-STREAM/VIDEO**, in the same sense as
    /// [`Self::PartialFrameDiscard`] and for the same reason: MH opens no
    /// unidirectional-stream accept loop, and audio is one frame per datagram
    /// (ADR-0036 §1), so there is no stream-creation event to rate-limit. A
    /// fixed-window limiter was built for this token at story task 16 and
    /// **removed at review** — it was unreachable enforcement machinery behind a
    /// catalog entry that described it as a live detector, which is a control
    /// that reads as present and cannot fire.
    ///
    /// The token is kept so the author of the accept path inherits the spelling
    /// rather than inventing a second one, and inherits no bound they did not
    /// choose. Owner: whichever task lands the uni-stream accept path (video).
    StreamRateLimited,
    /// No forwarding policy is installed for this meeting. The remedy is the
    /// control plane (ADR-0036 §8), not this handler.
    NoPolicy,
    /// A policy is installed but no egress edge names this sender. The remedy
    /// is MC's assignment.
    NoSubscriber,
    /// An edge names a subscriber with no connection on this handler.
    NoLocalSubscriber,
    /// `rewrite_relay_region` failed on a frame that had already decoded
    /// cleanly. **This is an MH bug**, never a sender fault: it is the
    /// entry-point ambiguity `docs/TODO.md`'s reject-reason entry asks task 16
    /// to resolve, resolved structurally (a distinct token) rather than with an
    /// `entry_point` label. **Should read zero forever.**
    RelayRewriteFailed,
    /// A partially-read stream frame was discarded.
    ///
    /// **UNREACHABLE UNTIL VIDEO.** Audio is one frame per datagram (ADR-0036
    /// §1), so the only producer is the stream-carried path's `Ok(None)`
    /// caller obligation documented at `media_protocol::codec`'s
    /// `decode_stream_frame`. It is defined now because that obligation names
    /// MH as one of its two owners and a counted give-up is the whole point of
    /// it; leaving the token out would make the video path's first author
    /// invent a spelling.
    PartialFrameDiscard,
    /// The QUIC connection received a DATAGRAM frame that MH's ingress loop
    /// never read.
    ///
    /// # Not a respelling of [`Self::IngressQueueOverflow`] — a different layer
    ///
    /// That one is **MH's own bounded ring** shedding a frame it had already
    /// accepted. This one is **quinn's receive buffer** discarding a frame MH
    /// never accepted, or a frame that arrived outside the ingress loop's
    /// lifetime. Different layer, different owner, different remedy. The three
    /// `no_*` tokens are likewise distinct: every one of them presupposes a
    /// live loop that *read* the frame.
    ///
    /// # How it is counted, and why it is not sampled at session start
    ///
    /// Computed **once per connection at teardown**, as
    /// `quinn::Connection::stats().frame_rx.datagram` minus the datagrams
    /// `run_ingress` actually took off the transport. quinn exposes no evicted
    /// count — its only native signal is a bare `debug!` in
    /// `quinn-proto`'s `connection/datagrams.rs` — so this is a difference of
    /// two counts of real events, never an inference from buffer arithmetic.
    ///
    /// Sampling `frame_rx.datagram` at session start instead **would
    /// double-count**, and that shape was written and withdrawn at review:
    /// quinn evicts drop-OLDEST, so the datagrams still buffered when a
    /// session starts are exactly the ones the loop reads moments later and
    /// counts as `forwarded{direction="ingress"}`. On the measurement that
    /// motivated this token — 60 datagrams, 47 accounted, 13 genuinely lost —
    /// the session-start sample would have recorded 20.
    ///
    /// # A UNION of three windows, not just the pre-session one
    ///
    /// Pre-session arrivals, mid-session eviction while the loop was behind,
    /// and post-teardown arrivals. MH cannot split them without more sampling.
    /// The pre-session window is expected to dominate, and the day it does not
    /// is the day mid-session matters most — so do not read a rise as
    /// necessarily pre-session.
    ///
    /// # The lexical pairing with [`Self::TransportSendRefused`] is NOT semantic
    ///
    /// The names pair deliberately — same `transport_*` prefix for "the layer
    /// below us", opposite direction — but that one is an invariant violation
    /// that should read zero forever, and **this one has a legitimately
    /// non-zero tail**. A reader who imports the zero-forever alerting
    /// discipline across the name pair will either alert on noise or dismiss a
    /// real rise as "the usual tail". Saturation-or-input group; see the
    /// catalog.
    ///
    /// **Value is client-influenced and is an upper bound, not a measurement.**
    /// `frame_rx.datagram` counts raw QUIC DATAGRAM frames one layer below
    /// WebTransport session demultiplexing, so datagrams carrying an unmatched
    /// HTTP/3 session-id varint increment it while `wtransport` discards them
    /// (`wtransport-0.7.2/src/driver/mod.rs:172-188`). **Therefore not usable
    /// as an SLI**: a client-inflatable SLI converts an availability attack
    /// into an error-budget attack.
    TransportReceiveDropped,
    /// Datagrams that arrived on a connection whose media session was
    /// **declined**, so no ingress loop was ever spawned.
    ///
    /// Exact rather than an upper bound, unlike [`Self::TransportReceiveDropped`]:
    /// with no loop, every one of these is discarded.
    ///
    /// # Why this is its own token and not the declined arm of that one
    ///
    /// **Neither series is a function of the other, and neither may be deleted
    /// as redundant.** They share a mechanism and differ only by connection
    /// outcome, which is exactly the shape a later reviewer merges as "one
    /// condition, redundant split" — so the reason is recorded here as well as
    /// in the catalog. The vocabulary's discriminator is REMEDY: a decline
    /// sends the responder to MC availability and the binding contract, a
    /// started session sends them to ingest or to a client publishing ahead of
    /// readiness.
    ///
    /// The operational argument is decisive on its own. The MC client's retry
    /// budget means a declined client can hold its admission slot for tens of
    /// seconds while publishing ~50 frames/s, so during an MC outage a shared
    /// token would be dominated by declines across a reconnect herd — burying
    /// the other signal exactly when it is needed, and firing a second page
    /// that merely restates the decline rate.
    ///
    /// Diagnostic only: no alert. It is fully explained one row up by
    /// `mh_media_session_starts_total{outcome=declined_*}`.
    NoMediaSession,
}

impl MediaDropReason {
    /// Every value, in catalog order. The length is written out so a new token
    /// cannot be added without the catalog and the dashboards being revisited —
    /// this array fails to compile first.
    ///
    /// **The ordinal is deliberately not named.** This sentence said "a twelfth
    /// token" while the array held eleven, and story task 26 added two — making
    /// it describe a tripwire two tokens behind where it actually sits. Naming
    /// the next ordinal re-arms that trap at every addition, which is the same
    /// reason `docs/observability/metrics/mh-service.md`'s restated cardinality
    /// integer was DELETED rather than updated in that same change. The written
    /// out length below is compile-checked and is the guard; prose restating it
    /// is an unchecked copy.
    pub const ALL: [Self; 13] = [
        Self::IngressQueueOverflow,
        Self::EgressQueueOverflow,
        Self::TransportSendRefused,
        Self::ConnectionClosed,
        Self::OversizeDatagram,
        Self::StreamRateLimited,
        Self::NoPolicy,
        Self::NoSubscriber,
        Self::NoLocalSubscriber,
        Self::RelayRewriteFailed,
        Self::PartialFrameDiscard,
        Self::TransportReceiveDropped,
        Self::NoMediaSession,
    ];

    /// The wire label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IngressQueueOverflow => "ingress_queue_overflow",
            Self::EgressQueueOverflow => "egress_queue_overflow",
            Self::TransportSendRefused => "transport_send_refused",
            Self::ConnectionClosed => "connection_closed",
            Self::OversizeDatagram => "oversize_datagram",
            Self::StreamRateLimited => "stream_rate_limited",
            Self::NoPolicy => "no_policy",
            Self::NoSubscriber => "no_subscriber",
            Self::NoLocalSubscriber => "no_local_subscriber",
            Self::RelayRewriteFailed => "relay_rewrite_failed",
            Self::PartialFrameDiscard => "partial_frame_discard",
            Self::TransportReceiveDropped => "transport_receive_dropped",
            Self::NoMediaSession => "no_media_session",
        }
    }

    /// The pipeline direction this token can occur in.
    ///
    /// Paired with the token at its definition site, so `reason` and
    /// `direction` cannot disagree: each token has exactly ONE direction, which
    /// is why the counter carries 11 MH-local series rather than 22. A token
    /// that could legitimately occur in both directions would be two conditions
    /// wearing one name.
    #[must_use]
    pub const fn direction(self) -> MediaDirection {
        match self {
            Self::IngressQueueOverflow
            | Self::OversizeDatagram
            | Self::StreamRateLimited
            | Self::NoPolicy
            | Self::TransportReceiveDropped
            | Self::NoMediaSession => MediaDirection::Ingress,
            Self::EgressQueueOverflow
            | Self::TransportSendRefused
            | Self::ConnectionClosed
            | Self::NoSubscriber
            | Self::NoLocalSubscriber
            | Self::RelayRewriteFailed
            | Self::PartialFrameDiscard => MediaDirection::Egress,
        }
    }
}

/// Histogram bucket edges for `mh_media_forward_latency_seconds`, in seconds.
///
/// 50 µs to 100 ms. The bottom of the range has to resolve a forward path whose
/// design target is tens of microseconds of processing, and the top has to
/// still show something when a subscriber stalls; a bucket set that bottoms out
/// at 1 ms would put every healthy observation in the first bucket and make the
/// histogram unable to distinguish "fast" from "instant".
///
/// Named rather than inlined so [`MEDIA_FORWARD_OBJECTIVE_SECONDS`] can be
/// asserted to be one of these edges.
pub const MEDIA_FORWARD_LATENCY_BUCKETS: [f64; 11] = [
    0.000_05, 0.000_1, 0.000_25, 0.000_5, 0.001, 0.002_5, 0.005, 0.010, 0.030, 0.050, 0.100,
];

/// The forwarding latency objective, in seconds — **provisional**.
///
/// ADR-0011 carries a `< 30 ms` figure, but it is **not ratified against this
/// measurement point**: this histogram measures MH-internal
/// ingress-from-network to egress-to-network, not an end-to-end path. Story 8
/// ratifies the real objective. Recorded here as a named constant, and as
/// provisional in `docs/observability/slos.md`, so the figure has one home
/// while it is still moving.
///
/// **No burn-rate alert rests on it** until ratification — `slos.md` forbids
/// one, and an alert on an unratified objective is an alert nobody can act on.
///
/// It is asserted to be exactly one of [`MEDIA_FORWARD_LATENCY_BUCKETS`] (see
/// this module's tests): a histogram quantile at a non-edge value is
/// interpolated between buckets, so an objective that drifted off an edge would
/// silently become an estimate of an estimate, and nothing would fail.
pub const MEDIA_FORWARD_OBJECTIVE_SECONDS: f64 = 0.030;

/// Every metric handle the media forward path needs, resolved once at setup.
///
/// ADR-0036 §11: "Per-stream forwarders own metric handles resolved once at
/// setup, so **no metric macro is reachable from the forward function**." This
/// struct is the mechanism. `crates/mh-service/src/media/**` holds one of these
/// and calls `.increment(1)` / `.record(..)` / `.set(..)` on it; it invokes no
/// macro and performs no registry lookup per frame.
///
/// Handles are resolved through the base `counter!` / `histogram!` / `gauge!`
/// macros, which **return** the handle — never `describe_*` alone, which
/// documents without resolving and leaves `dt-guard metric-coverage` red while
/// `application-metrics` reports green.
///
/// The metric NAME at each site is a string literal, never a hoisted `const`:
/// `dt-guard`'s `MACRO_INVOCATION_WITH_FIRST_ARG_RE` captures a quoted literal
/// first argument, so a `const` name would make `metric-coverage`,
/// `histogram-buckets`, `application-metrics` and `dashboard-panels` all go
/// silently blind to this metric **while reporting clean**. Label values are
/// loop variables, which that regex does not care about.
#[derive(Debug, Clone)]
pub struct MediaMetricHandles {
    /// Indexed by [`MediaDirection::ALL`] order.
    forwarded: [Counter; 2],
    /// Indexed by [`MediaDropReason::ALL`] order.
    dropped: [Counter; 13],
    /// `(reason, handle)` pairs for the codec family, built by iterating
    /// `ALL_REJECT_REASONS` rather than a hand-written token list — that const
    /// is generated by the same macro as the enum, so a ninth codec token added
    /// upstream automatically gets an MH handle instead of silently missing
    /// one.
    codec_dropped: Vec<(RejectReason, Counter)>,
    /// Indexed by [`MediaLatencyPhase::ALL`] order.
    latency: [Histogram; 4],
    /// Published once at setup from the same config field the sampler reads.
    sample_ratio: Gauge,
    /// MH's own application egress queue depth. **Not** quinn's send buffer,
    /// which `wtransport` exposes no accessor for.
    egress_queue_depth: Gauge,
}

impl MediaMetricHandles {
    /// The forwarded counter for one direction.
    ///
    /// Array destructuring rather than indexing: `indexing_slicing` is denied
    /// workspace-wide, and a `match` over the destructured handles is total by
    /// construction — a new [`MediaDirection`] variant fails to compile here
    /// AND in `ALL`, rather than resolving to a fallback handle that quietly
    /// mislabels.
    pub fn forwarded(&self, direction: MediaDirection) -> &Counter {
        let [ingress, egress] = &self.forwarded;
        match direction {
            MediaDirection::Ingress => ingress,
            MediaDirection::Egress => egress,
        }
    }

    /// The drop counter for one MH-local reason.
    pub fn dropped(&self, reason: MediaDropReason) -> &Counter {
        let [ingress_overflow, egress_overflow, send_refused, closed, oversize, rate_limited, no_policy, no_subscriber, no_local, rewrite_failed, partial, receive_dropped, no_session] =
            &self.dropped;
        match reason {
            MediaDropReason::IngressQueueOverflow => ingress_overflow,
            MediaDropReason::EgressQueueOverflow => egress_overflow,
            MediaDropReason::TransportSendRefused => send_refused,
            MediaDropReason::ConnectionClosed => closed,
            MediaDropReason::OversizeDatagram => oversize,
            MediaDropReason::StreamRateLimited => rate_limited,
            MediaDropReason::NoPolicy => no_policy,
            MediaDropReason::NoSubscriber => no_subscriber,
            MediaDropReason::NoLocalSubscriber => no_local,
            MediaDropReason::RelayRewriteFailed => rewrite_failed,
            MediaDropReason::PartialFrameDiscard => partial,
            MediaDropReason::TransportReceiveDropped => receive_dropped,
            MediaDropReason::NoMediaSession => no_session,
        }
    }

    /// The drop counter for one codec reject reason.
    ///
    /// `Option` rather than a fallback handle: the miss is unreachable by
    /// construction (the list is built from `ALL_REJECT_REASONS`, which the
    /// codec's own macro generates alongside the enum) and a fallback handle
    /// would be a silent mislabel if it ever were reachable. A unit test in
    /// this module asserts `Some` for every member, which is what makes the
    /// `None` arm on the hot path provably dead rather than merely believed to
    /// be.
    #[must_use]
    pub fn codec_dropped(&self, reason: RejectReason) -> Option<&Counter> {
        self.codec_dropped
            .iter()
            .find(|(candidate, _)| *candidate == reason)
            .map(|(_, handle)| handle)
    }

    /// The latency histogram for one phase.
    pub fn latency(&self, phase: MediaLatencyPhase) -> &Histogram {
        let [receive_buffer, processing, transmit_buffer, total] = &self.latency;
        match phase {
            MediaLatencyPhase::ReceiveBuffer => receive_buffer,
            MediaLatencyPhase::Processing => processing,
            MediaLatencyPhase::TransmitBuffer => transmit_buffer,
            MediaLatencyPhase::Total => total,
        }
    }

    /// The egress-queue-depth gauge.
    pub const fn egress_queue_depth(&self) -> &Gauge {
        &self.egress_queue_depth
    }

    /// Publish the sampling ratio the sampler was built with.
    ///
    /// Takes the value rather than reading config itself, because the ONE
    /// property worth having here is that the published number and the number
    /// the sampler draws against are the same value from the same field.
    pub fn publish_sample_ratio(&self, ratio: f64) {
        self.sample_ratio.set(ratio);
    }
}

/// Resolve every media forward-path metric handle. Call once, at setup.
///
/// This is a **sibling** of `crates/mh-service/src/media/**` (ADR-0036 §11's
/// layout constraint): setup lives here so the media directory holds only the
/// hot path and the directory boundary is the hot-path boundary.
#[must_use]
pub fn resolve_media_handles() -> MediaMetricHandles {
    // Names are literals; labels are loop variables. See `MediaMetricHandles`.
    let forwarded = MediaDirection::ALL.map(|direction| {
        counter!(
            "mh_media_frames_forwarded_total",
            "direction" => direction.as_str(),
            KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
        )
    });

    let dropped = MediaDropReason::ALL.map(|reason| {
        counter!(
            "mh_media_frames_dropped_total",
            "reason" => reason.as_str(),
            "direction" => reason.direction().as_str(),
            KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
        )
    });

    // The codec family, iterated from the codec's own generated list. Every
    // token MH emits from this family comes from `decode_datagram`, i.e. is a
    // sender-side condition, so the direction is `ingress` for all of them; a
    // failure from `rewrite_relay_region` on an already-decoded frame is not a
    // sender fault at all and carries `MediaDropReason::RelayRewriteFailed`
    // instead. That is what discharges the entry-point ambiguity structurally,
    // with no `entry_point` label.
    let codec_dropped = ALL_REJECT_REASONS
        .iter()
        .map(|reason| {
            (
                *reason,
                counter!(
                    "mh_media_frames_dropped_total",
                    "reason" => reason.as_str(),
                    "direction" => MediaDirection::Ingress.as_str(),
                    KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
                ),
            )
        })
        .collect();

    let latency = MediaLatencyPhase::ALL.map(|phase| {
        histogram!(
            "mh_media_forward_latency_seconds",
            "phase" => phase.as_str(),
            KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
        )
    });

    let sample_ratio = gauge!(
        "mh_media_latency_sample_ratio",
        KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
    );

    let egress_queue_depth = gauge!(
        "mh_media_egress_queue_depth",
        KEY_CUSTODY_LABEL => KEY_CUSTODY_OPERATOR
    );

    MediaMetricHandles {
        forwarded,
        dropped,
        codec_dropped,
        latency,
        sample_ratio,
        egress_queue_depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MhError;

    // Note: These tests execute the metric recording functions to ensure code coverage.
    // The metrics crate will record to a global no-op recorder if none is installed,
    // which is sufficient for coverage testing.
    //
    // Per ADR-0002: These tests do not panic on missing recorder.

    #[test]
    fn test_record_gc_registration() {
        record_gc_registration("success");
        record_gc_registration("error");
    }

    #[test]
    fn test_record_gc_registration_latency() {
        record_gc_registration_latency(Duration::from_millis(50));
        record_gc_registration_latency(Duration::from_millis(500));
        record_gc_registration_latency(Duration::from_secs(2));
    }

    #[test]
    fn test_record_gc_heartbeat() {
        record_gc_heartbeat("success");
        record_gc_heartbeat("error");
    }

    #[test]
    fn test_record_gc_heartbeat_latency() {
        record_gc_heartbeat_latency(Duration::from_millis(5));
        record_gc_heartbeat_latency(Duration::from_millis(50));
        record_gc_heartbeat_latency(Duration::from_millis(500));
    }

    #[test]
    fn test_record_token_refresh() {
        // Success path
        record_token_refresh("success", None, Duration::from_millis(50));

        // Error paths with different error types
        record_token_refresh("error", Some("http"), Duration::from_millis(100));
        record_token_refresh("error", Some("auth_rejected"), Duration::from_millis(200));
        record_token_refresh("error", Some("invalid_response"), Duration::from_millis(30));
        record_token_refresh(
            "error",
            Some("acquisition_failed"),
            Duration::from_millis(10),
        );
    }

    #[test]
    fn test_record_grpc_request() {
        // Both combinations: 1 method x 2 statuses. `MediaHandlerService` has
        // exactly one RPC by design (ADR-0036 §8), and the method label value
        // is now a const inside the recorder rather than a parameter.
        record_grpc_request("success");
        record_grpc_request("error");
    }

    #[test]
    fn test_record_media_policy_apply_covers_every_outcome() {
        for outcome in PolicyApplyOutcome::ALL {
            record_media_policy_apply(outcome);
        }
    }

    #[test]
    fn test_record_media_session_start_covers_every_outcome() {
        for outcome in MediaSessionStartOutcome::ALL {
            record_media_session_start(outcome);
        }
    }

    #[test]
    fn media_session_start_outcome_labels_are_distinct_and_stable() {
        let labels: Vec<&str> = MediaSessionStartOutcome::ALL
            .iter()
            .map(|o| o.as_label())
            .collect();
        // Same split as the policy-apply test above: the expected side is the
        // wire contract and is spelled out, the variant set is iterated. A test
        // that derived both sides from `as_label` would assert nothing.
        assert_eq!(
            labels,
            [
                "started",
                "declined_no_sender_binding",
                "declined_sender_binding_out_of_range",
                "declined_sender_binding_conflict",
                "declined_mc_unavailable",
                "declined_mc_auth_rejected",
                "declined_mc_endpoint_unknown"
            ],
            "outcome label values are wire-visible; a rename silently breaks every dashboard and alert selecting on them"
        );
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(
            unique.len(),
            labels.len(),
            "outcome labels must be distinct"
        );
    }

    /// ADR-0036 §11: no per-participant identity may reach a metric label.
    ///
    /// The token spellings deliberately say `sender_binding` rather than
    /// `sender_id` so an auditor grepping metric code for the identifier gets no
    /// false hit. That is a convention, and a convention nothing checks is a
    /// comment — so it is checked here, over `ALL` rather than a hand-listed set.
    #[test]
    fn media_session_start_outcome_labels_name_no_identity() {
        for outcome in MediaSessionStartOutcome::ALL {
            let label = outcome.as_label();
            for barred in ["sender_id", "participant", "meeting", "connection"] {
                assert!(
                    !label.contains(barred),
                    "outcome label {label:?} names {barred:?}; ADR-0036 §11 keeps participant and \
                     stream identity out of labels, and the decline tokens spell `sender_binding` \
                     precisely so a `sender_id` audit grep stays clean"
                );
            }
        }
    }

    #[test]
    fn test_policy_apply_outcome_labels_are_distinct_and_stable() {
        let labels: Vec<&str> = PolicyApplyOutcome::ALL
            .iter()
            .map(|o| o.as_label())
            .collect();
        // The expected side is spelled out on purpose: it is the wire
        // contract, and a test that derived it from `as_label` would assert
        // nothing. What must NOT be hand-listed is the variant set, which is
        // why the left side iterates `ALL`.
        assert_eq!(
            labels,
            [
                "applied",
                "rejected_stale",
                "no_generation",
                "rejected_invalid",
                "apply_failed"
            ],
            "outcome label values are wire-visible; a rename silently breaks every dashboard and alert selecting on them"
        );
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(
            unique.len(),
            labels.len(),
            "outcome labels must be distinct"
        );
    }

    #[test]
    fn test_record_error() {
        record_error("gc_registration", "grpc", 503);
        record_error("gc_heartbeat", "grpc", 503);
        record_error("token_refresh", "http", 500);
        record_error("grpc_service", "internal", 500);
    }

    #[test]
    fn test_record_webtransport_connection() {
        record_webtransport_connection("accepted");
        record_webtransport_connection("rejected");
        record_webtransport_connection("error");
    }

    #[test]
    fn test_record_webtransport_handshake_duration() {
        record_webtransport_handshake_duration(Duration::from_millis(50));
        record_webtransport_handshake_duration(Duration::from_millis(200));
        record_webtransport_handshake_duration(Duration::from_secs(1));
    }

    #[test]
    fn test_set_active_connections() {
        set_active_connections(0.0);
        set_active_connections(42.0);
        set_active_connections(0.0);
    }

    #[test]
    fn test_record_jwt_validation() {
        record_jwt_validation("success", "meeting", "none");
        record_jwt_validation("failure", "meeting", "validation_failed");
        record_jwt_validation("success", "service", "none");
        record_jwt_validation("failure", "service", "signature_invalid");
        record_jwt_validation("failure", "service", "expired");
        record_jwt_validation("failure", "service", "malformed");
        record_jwt_validation("failure", "service", "scope_mismatch");
    }

    #[test]
    fn test_record_caller_type_rejected() {
        // Test representative label combinations (ADR-0003 Layer 2)
        record_caller_type_rejected(
            "MediaHandlerService",
            "meeting-controller",
            "global-controller",
        );
        record_caller_type_rejected("MediaHandlerService", "meeting-controller", "unknown");
    }

    #[test]
    fn test_record_mc_notification() {
        // All 4 combinations: 2 events x 2 statuses
        record_mc_notification("connected", "success");
        record_mc_notification("connected", "error");
        record_mc_notification("disconnected", "success");
        record_mc_notification("disconnected", "error");
    }

    #[test]
    fn test_cardinality_bounds() {
        // Verify status labels are bounded to 2 values
        let valid_statuses = ["success", "error"];
        for status in &valid_statuses {
            record_gc_registration(status);
            record_gc_heartbeat(status);
        }

        // The `method` label is single-valued by construction — the value is a
        // const inside the recorder, so cardinality is bounded by the 2
        // statuses alone.
        for status in &valid_statuses {
            record_grpc_request(status);
        }

        // The `outcome` label is bounded by the PolicyApplyOutcome enum.
        for outcome in PolicyApplyOutcome::ALL {
            record_media_policy_apply(outcome);
        }

        // Likewise for the media-session-start `outcome` label, bounded by
        // `MediaSessionStartOutcome::ALL`; `key_custody` is a compile-time const
        // with a single permitted value, so it multiplies nothing.
        for outcome in MediaSessionStartOutcome::ALL {
            record_media_session_start(outcome);
        }

        // Verify `error_type` labels are bounded — DERIVED from the enum, not a
        // hand-typed list. The previous hand-list was doubly wrong: it carried
        // `token_timeout`, which `error_type_label` never returns (the real value
        // is `token_acquisition_timeout`), and it omitted six variants — a
        // vacuous assertion under a comment claiming it verified boundedness (F5).
        // One representative of every variant, so the assertion exercises
        // `error_type_label()` itself; the method's own wildcard-free `match` is
        // the forcing function that makes a new variant show up here.
        let all_errors = [
            MhError::Grpc(String::new()),
            MhError::NotRegistered,
            MhError::Config(String::new()),
            MhError::Internal(String::new()),
            MhError::TokenAcquisition(String::new()),
            MhError::TokenAcquisitionTimeout,
            MhError::JwtValidation(String::new()),
            MhError::WebTransportError(String::new()),
            MhError::MeetingNotRegistered(String::new()),
            MhError::McEndpointInvalid(String::new()),
            MhError::OutboundAuthUnavailable(String::new()),
        ];
        let labels: std::collections::HashSet<&str> =
            all_errors.iter().map(MhError::error_type_label).collect();
        assert_eq!(
            labels.len(),
            all_errors.len(),
            "each MhError variant must map to a DISTINCT bounded error_type label; a collision \
             would silently merge two error populations onto one series"
        );
        for err in &all_errors {
            record_error("test_op", err.error_type_label(), err.status_code());
        }
    }

    #[test]
    fn media_forward_objective_is_exactly_a_registered_bucket_edge() {
        // A quantile read at a value that is not a bucket edge is interpolated
        // between the neighbouring edges, so an objective that drifted off an
        // edge would silently become an estimate of an estimate and nothing
        // would fail. When story 8 ratifies a different figure it has to land
        // on an edge or move one.
        assert!(
            MEDIA_FORWARD_LATENCY_BUCKETS.contains(&MEDIA_FORWARD_OBJECTIVE_SECONDS),
            "objective {MEDIA_FORWARD_OBJECTIVE_SECONDS} is not one of \
             {MEDIA_FORWARD_LATENCY_BUCKETS:?}"
        );
    }

    #[test]
    fn every_codec_reject_reason_has_a_resolved_handle() {
        // This is what makes `codec_dropped`'s `None` arm provably dead on the
        // hot path rather than merely believed to be: the list is built by
        // iterating `ALL_REJECT_REASONS`, which the codec's own macro generates
        // alongside the enum, so a ninth token upstream gets a handle
        // automatically — and this test fails if that ever stops being true.
        let handles = resolve_media_handles();
        for reason in ALL_REJECT_REASONS {
            assert!(
                handles.codec_dropped(*reason).is_some(),
                "no handle resolved for codec reject reason '{}'",
                reason.as_str()
            );
        }
        assert!(
            !ALL_REJECT_REASONS.is_empty(),
            "the codec vocabulary is empty; the loop above proves nothing"
        );
    }

    #[test]
    fn every_media_drop_reason_has_exactly_one_direction_and_a_distinct_token() {
        let mut tokens = std::collections::BTreeSet::new();
        for reason in MediaDropReason::ALL {
            assert!(
                tokens.insert(reason.as_str()),
                "duplicate media drop token '{}'",
                reason.as_str()
            );
            // Pairing the direction with the token at the definition site is
            // what keeps `reason` and `direction` from disagreeing; a token
            // legitimately occurring in both directions would be two conditions
            // wearing one name.
            let _: MediaDirection = reason.direction();
        }
        assert_eq!(tokens.len(), MediaDropReason::ALL.len());
    }

    #[test]
    fn test_prometheus_metrics_endpoint_integration() {
        // NOT an ADR-0032 coverage assertion — this test pre-dates ADR-0032
        // and lives in `src/` (outside the guard's `tests/**/*.rs` scan), so
        // the `record_*` calls here are NOT the "test-side reference to a
        // production emission path" the guard is designed to catch. This is
        // an ADR-0011 plumbing test: it exercises every `record_*` wrapper
        // to verify the `counter!`/`histogram!` macro + recorder + exporter
        // wiring captures emissions (the guard's coverage-fidelity
        // requirement is separately met by the per-caller component tests in
        // `crates/mh-service/tests/*.rs` — `gc_integration.rs`,
        // `webtransport_integration.rs`, `webtransport_accept_loop_integration.rs`,
        // etc.). Future readers: do NOT propagate "test-body `record_*`" to
        // `crates/mh-service/tests/`; that DOES dodge the guard.
        //
        // Migrated from hand-rolled `DebuggingRecorder::new() + recorder.install()`
        // to `common::observability::testing::MetricAssertion` purely as DRY
        // cleanup (removes one of two hand-rolled-install sites tracked in
        // `docs/TODO.md`). `MetricAssertion::snapshot()` binds a per-thread
        // `DebuggingRecorder` for this test, dropping the global-install
        // isolation pain the original inline comment called out.
        use common::observability::testing::MetricAssertion;

        let snap = MetricAssertion::snapshot();

        // Record the same set of MH metrics the legacy test exercised so the
        // "every recorded metric actually lands in the recorder" intent is
        // preserved. Histograms are asserted before counters because
        // `Snapshotter::snapshot` drains histogram observations on read
        // (see common::observability::testing §"Delta semantics").
        record_gc_registration("success");
        record_gc_registration("error");
        record_gc_registration_latency(Duration::from_millis(100));
        record_gc_heartbeat("success");
        record_gc_heartbeat("error");
        record_gc_heartbeat_latency(Duration::from_millis(10));
        record_token_refresh("success", None, Duration::from_millis(50));
        record_token_refresh("error", Some("http"), Duration::from_millis(100));
        record_grpc_request("success");
        record_grpc_request("error");
        record_media_policy_apply(PolicyApplyOutcome::Applied);
        record_error("gc_heartbeat", "grpc", 503);

        // Single histogram assertion — `Snapshotter::snapshot()` drains
        // every histogram across all names on read, so asserting multiple
        // histogram names after each other would see zero on the 2nd+ call.
        // A single representative observation proves the recorder captured
        // histogram emissions; the per-histogram-name coverage comes from
        // the individual `record_*_*_seconds` unit tests earlier in this
        // module + the gc_integration.rs assertions on gc-specific names.
        snap.histogram("mh_gc_registration_duration_seconds")
            .assert_observation_count_at_least(1);

        // Counters — one representative per name+label tuple emitted above.
        snap.counter("mh_gc_registration_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mh_gc_registration_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("mh_gc_heartbeats_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mh_gc_heartbeats_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
    }

    // =======================================================================
    // record_token_refresh_metrics (ADR-0032 Category B)
    // =======================================================================
    //
    // These tests cover the `TokenRefreshEvent -> metrics` mapping lifted out
    // of `main.rs` per ADR-0032 Step 2. Each test takes its own per-thread
    // `MetricAssertion::snapshot()`; histograms are asserted first because
    // `Snapshotter::snapshot()` drains histogram observations on read.

    #[test]
    fn record_token_refresh_metrics_success_event_emits_counter_and_histogram() {
        use common::observability::testing::MetricAssertion;
        use common::token_manager::TokenRefreshEvent;

        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: true,
            duration: Duration::from_millis(42),
            error_category: None,
        });

        snap.histogram("mh_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "success")])
            .assert_delta(1);
    }

    // Each failure branch exercises a distinct `error_category` value. The
    // categories are bounded `&'static str`s produced by
    // `common::token_manager::error_category`; enumerating them here guards
    // the mapping against silent regression.

    #[test]
    fn record_token_refresh_metrics_error_http() {
        assert_token_refresh_failure_emits("http");
    }

    #[test]
    fn record_token_refresh_metrics_error_auth_rejected() {
        assert_token_refresh_failure_emits("auth_rejected");
    }

    #[test]
    fn record_token_refresh_metrics_error_invalid_response() {
        assert_token_refresh_failure_emits("invalid_response");
    }

    #[test]
    fn record_token_refresh_metrics_error_acquisition_failed() {
        assert_token_refresh_failure_emits("acquisition_failed");
    }

    #[test]
    fn record_token_refresh_metrics_error_configuration() {
        assert_token_refresh_failure_emits("configuration");
    }

    #[test]
    fn record_token_refresh_metrics_error_channel_closed() {
        assert_token_refresh_failure_emits("channel_closed");
    }

    fn assert_token_refresh_failure_emits(category: &'static str) {
        use common::observability::testing::MetricAssertion;
        use common::token_manager::TokenRefreshEvent;

        let snap = MetricAssertion::snapshot();
        record_token_refresh_metrics(&TokenRefreshEvent {
            success: false,
            duration: Duration::from_millis(10),
            error_category: Some(category),
        });

        snap.histogram("mh_token_refresh_duration_seconds")
            .assert_observation_count_at_least(1);
        snap.counter("mh_token_refresh_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("mh_token_refresh_failures_total")
            .with_labels(&[("error_type", category)])
            .assert_delta(1);
    }
}
