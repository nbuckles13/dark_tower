//! Media forward-path telemetry gates (ADR-0036 §11).
//!
//! Four subjects, deliberately in one binary because they are one obligation:
//! the media path's telemetry must be complete, must be honestly labelled, must
//! not leak, and must not be silently *un*-enforced.
//!
//! 1. **Emission and labels** — every one of the five metric names appears
//!    textually here, which is what `dt-guard metric-coverage` requires;
//! 2. **Bucket fidelity and the sample-ratio gauge**;
//! 3. **The reason-vocabulary collision test** — MH must never spell a
//!    crypto- or key-layer token;
//! 4. **The macro deny** — no log/metric macro form under `media/`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

#[path = "common/mod.rs"]
mod common;

use common::media_frame::audio_datagram;
use common::media_rig::LoopbackRig;

use ::common::observability::testing::MetricAssertion;
use media_protocol::codec::ALL_REJECT_REASONS;
use mh_service::config::{
    DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO, EGRESS_QUEUE_FRAMES, INGRESS_QUEUE_FRAMES,
};
use mh_service::media::forward::{forward_one, EgressQueue, IngressFrame};
use mh_service::media::queue::SharedQueue;
use mh_service::observability::metrics::{
    resolve_media_handles, MediaDirection, MediaDropReason, MediaLatencyPhase,
    MEDIA_FORWARD_LATENCY_BUCKETS, MEDIA_FORWARD_OBJECTIVE_SECONDS,
};
use mh_service::routing::RoutingTable;
use mh_service::session::LocalSubscribers;
use mh_test_utils::media_policy::loopback_egress;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const MEETING: &str = "meeting-metrics";
const PUBLISHER: u32 = 7;

/// The cross-language reject vocabulary, read **as data**.
///
/// **Fourth CRATE-RELATIVE home for this path**, alongside
/// `crates/media-vector-gen/tests/rust_codec_conformance.rs:34`,
/// `crates/media-vector-gen/tests/vectors_are_current.rs:21` and
/// `packages/sdk-core/src/media/frame/__tests__/frameVectors.ts:24`. The
/// duplication is **forced, not tolerated**: `media-vector-gen` is
/// non-production by design ("Nothing in any service links it",
/// `media-vector-gen/src/lib.rs`), so a production-adjacent crate must not take
/// a dependency on it to reach a shared constant. Same shape as the
/// `mc-test-utils`/`env-tests` identity-key boundary note. (A fifth,
/// workspace-relative spelling lives at `media-vector-gen`'s own
/// `VECTOR_FILE_PATH`; it is a different form of the same path, named here so a
/// reader who greps finds no home this comment failed to mention.)
const VECTORS: &str = "../../proto/test-vectors/frame-v2.vectors.json";

/// Forward one frame through a loopback policy, returning the queue it landed
/// in so a test can assert on what was delivered.
fn forward_one_loopback_frame(sample_ratio: f64) -> Arc<EgressQueue> {
    let mut rig = LoopbackRig::new(
        MEETING,
        PUBLISHER,
        vec![loopback_egress(1, PUBLISHER, 0)],
        sample_ratio,
    );
    let queue = rig.subscriber(PUBLISHER);
    rig.handles
        .publish_sample_ratio(rig.forwarder.sample_ratio());

    forward_one(
        &mut rig.forwarder,
        &rig.routing.load(),
        &rig.subscribers.load(),
        IngressFrame {
            payload: audio_datagram(1, 0xC0),
            received_at: Instant::now(),
        },
    );
    queue
}

// ---------------------------------------------------------------------------
// Emission and labels
// ---------------------------------------------------------------------------

/// The healthy path moves the forwarded counter **and leaves the drop counter
/// flat**.
///
/// A forwarded-only assertion passes while the path silently drops and resends,
/// which is exactly the shape of "every signal green and no audio".
#[tokio::test]
async fn a_forwarded_frame_increments_forwarded_and_leaves_every_drop_counter_flat() {
    let snapshot = MetricAssertion::snapshot();
    let _queue = forward_one_loopback_frame(1.0);

    snapshot
        .counter("mh_media_frames_forwarded_total")
        .with_labels(&[("direction", "ingress"), ("key_custody", "operator")])
        .assert_delta(1);

    for reason in MediaDropReason::ALL {
        snapshot
            .counter("mh_media_frames_dropped_total")
            .with_labels(&[
                ("reason", reason.as_str()),
                ("direction", reason.direction().as_str()),
                ("key_custody", "operator"),
            ])
            .assert_delta(0);
    }
}

/// Latency is observed per phase, with the labels the catalog names.
///
/// The sampler is forced to 1.0: a random sampler must never gate an
/// assertion. Only emission, labels and phase are asserted — **never a numeric
/// magnitude**, because the measurement clock is real elapsed time and no test
/// may depend on how fast a machine is.
#[tokio::test]
async fn latency_is_observed_per_phase_with_the_catalogued_labels() {
    // One snapshot per phase, deliberately: histogram assertions DRAIN the
    // captured entries, so asserting two phases against one snapshot would
    // read the second as unobserved for reasons unrelated to the code.
    for phase in [
        MediaLatencyPhase::ReceiveBuffer,
        MediaLatencyPhase::Processing,
    ] {
        let snapshot = MetricAssertion::snapshot();
        let _queue = forward_one_loopback_frame(1.0);
        snapshot
            .histogram("mh_media_forward_latency_seconds")
            .with_labels(&[("phase", phase.as_str()), ("key_custody", "operator")])
            .assert_observation_count_at_least(1);
    }
}

/// The two phases only the egress loop can observe.
///
/// `transmit_buffer` and `total` close at the moment the transport send call
/// returns, which is task C's — which is precisely why C is a separate task:
/// with the forward loop draining its own pushes there would be no egress-queue
/// residency to measure, and no firing path for the overflow counter either.
#[tokio::test]
async fn the_egress_loop_observes_the_transmit_buffer_and_total_phases() {
    for phase in [MediaLatencyPhase::TransmitBuffer, MediaLatencyPhase::Total] {
        let snapshot = MetricAssertion::snapshot();
        let handles = Arc::new(resolve_media_handles());
        let transport = Arc::new(mh_test_utils::transport_shim::LossDelayTransport::new());
        let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
        let context = Arc::new(mh_service::media::MediaTaskContext {
            routing: Arc::new(RoutingTable::new()),
            subscribers: Arc::new(LocalSubscribers::new()),
            handles,
        });
        let now = Instant::now();
        queue.push(mh_service::media::forward::EgressFrame {
            payload: audio_datagram(1, 0xE0),
            received_at: now,
            queued_at: now,
            // Forced, never drawn: a random sampler must not gate an assertion.
            sampled: true,
        });

        let cancel = tokio_util::sync::CancellationToken::new();
        let egress = tokio::spawn(mh_service::media::ingress::run_egress(
            Arc::clone(&transport),
            Arc::clone(&queue),
            context,
            cancel.clone(),
        ));
        while transport.delivered_count() == 0 {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
        let _ = egress.await;

        snapshot
            .histogram("mh_media_forward_latency_seconds")
            .with_labels(&[("phase", phase.as_str()), ("key_custody", "operator")])
            .assert_observation_count_at_least(1);
        snapshot
            .counter("mh_media_frames_forwarded_total")
            .with_labels(&[("direction", "egress"), ("key_custody", "operator")])
            .assert_delta(1);
    }
}

/// The published gauge **is** the field the sampler draws against.
///
/// A gauge computed separately from the value in force is a gauge that lies
/// exactly when someone is using it to interpret a histogram.
#[tokio::test]
async fn the_sample_ratio_gauge_publishes_the_value_the_sampler_uses() {
    let snapshot = MetricAssertion::snapshot();
    let _queue = forward_one_loopback_frame(DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO);

    snapshot
        .gauge("mh_media_latency_sample_ratio")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO);
}

/// The egress-queue-depth gauge tracks MH's own application queue.
///
/// It is **not** quinn's send-buffer occupancy: `wtransport` exposes no
/// accessor for that (its `Connection` surface is `send_datagram`, `close`,
/// `session_id`, `remote_address`, `stable_id`, `max_datagram_size`, `rtt`,
/// `export_keying_material`, `peer_identity`, `handshake_data`), and quinn's
/// own `datagram_send_buffer_space` is reachable only through the raw-QUIC
/// escape hatch MH deliberately does not take.
#[tokio::test]
async fn the_egress_queue_depth_gauge_reports_mh_s_own_queue() {
    let snapshot = MetricAssertion::snapshot();
    let queue = forward_one_loopback_frame(1.0);

    assert_eq!(queue.depth(), 1);
    snapshot
        .gauge("mh_media_egress_queue_depth")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(1.0);
}

/// A malformed datagram is counted under the **shared codec vocabulary**,
/// verbatim, never under an MH-local spelling.
#[tokio::test]
async fn a_codec_reject_is_counted_under_the_shared_codec_token() {
    let snapshot = MetricAssertion::snapshot();
    let mut rig = LoopbackRig::new(
        MEETING,
        PUBLISHER,
        vec![loopback_egress(1, PUBLISHER, 0)],
        1.0,
    );
    let _queue = rig.subscriber(PUBLISHER);

    let mut tampered = audio_datagram(1, 0xD0).to_vec();
    tampered.extend_from_slice(b"tail");
    forward_one(
        &mut rig.forwarder,
        &rig.routing.load(),
        &rig.subscribers.load(),
        IngressFrame {
            payload: bytes::Bytes::from(tampered),
            received_at: Instant::now(),
        },
    );

    snapshot
        .counter("mh_media_frames_dropped_total")
        .with_labels(&[
            ("reason", "trailing_bytes"),
            ("direction", "ingress"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

// ---------------------------------------------------------------------------
// Buckets and the objective
// ---------------------------------------------------------------------------

/// The objective is EXACTLY a bucket edge.
///
/// A quantile at a non-edge value is interpolated between buckets, so an
/// objective that drifted off an edge would silently become an estimate of an
/// estimate and nothing would fail. Written as membership in the same slice the
/// recorder registers, so story 8's ratified figure cannot land off-edge.
#[test]
fn the_forwarding_objective_is_exactly_one_of_the_registered_bucket_edges() {
    assert!(
        MEDIA_FORWARD_LATENCY_BUCKETS.contains(&MEDIA_FORWARD_OBJECTIVE_SECONDS),
        "MEDIA_FORWARD_OBJECTIVE_SECONDS ({MEDIA_FORWARD_OBJECTIVE_SECONDS}) must be one of \
         {MEDIA_FORWARD_LATENCY_BUCKETS:?} — a quantile read at a non-edge value is \
         interpolated, and the SLO would silently become an approximation"
    );
    // Anti-false-green: a `contains` over an empty or degenerate slice passes
    // vacuously for the wrong reason.
    assert!(
        MEDIA_FORWARD_LATENCY_BUCKETS.len() > 5,
        "the bucket slice is degenerate; the membership assertion above proves nothing"
    );
    assert!(
        MEDIA_FORWARD_LATENCY_BUCKETS
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "bucket edges must be strictly ascending"
    );
}

// ---------------------------------------------------------------------------
// The reason-vocabulary collision test
// ---------------------------------------------------------------------------

/// Every cross-language reject token, read from the vector file as data.
fn vector_tokens() -> BTreeSet<String> {
    let path = repo_relative(VECTORS);
    let raw = std::fs::read_to_string(&path).expect(
        "the cross-language vector file must be readable; if it moved, this test's relative \
         path is stale and the collision bar has silently disappeared",
    );
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("vector file must be JSON");
    parsed["reject_reasons"]
        .as_array()
        .expect("reject_reasons must be an array")
        .iter()
        .filter_map(|entry| entry["token"].as_str().map(str::to_string))
        .collect()
}

fn repo_relative(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// MH's own drop vocabulary must not collide with **any** cross-language reject
/// token.
///
/// # Why this reads the vector file and not `ALL_REJECT_REASONS`
///
/// `ALL_REJECT_REASONS` is blind precisely where the risk is. The crypto and
/// key tokens have **no Rust home at all** — they exist only in this file and
/// in `packages/sdk-core/src/media/frame/rejectReason.ts` — so a test against
/// the Rust eight would let an MH-local enum define `replay_detected` and pass
/// cleanly. `no_transmit_key` is the sharpest trap: tagged `layer: "codec"`,
/// not a `RejectReason` variant, and it *reads* like a plain routing failure.
/// `no_roster_entry` reads like a sibling of `no_subscriber` and is not.
///
/// An MH series carrying one of these asserts MH performed a verification it is
/// structurally incapable of performing — it is keyless and never opens a frame
/// — and an operator reads it as "MH validates frames", after which someone
/// relies on a control that does not exist.
///
/// # The deliberate exception, named so nobody "fixes" it into a violation
///
/// `signature_invalid` already appears in this crate as a JWT `failure_reason`
/// value (`grpc/auth_interceptor.rs`, `observability/metrics.rs`). That is
/// correct and unrelated: different metric, different label **key**, a JWT
/// signature rather than a frame signature. The vector file records it as
/// `"fleet_spelling": "failure_reason"`. It is also exactly how the bad version
/// arrives — an author adding media drop reasons finds `signature_invalid`
/// already in `metrics.rs` and reads it as house vocabulary.
#[test]
fn no_mh_local_drop_reason_collides_with_a_cross_language_reject_token() {
    let tokens = vector_tokens();

    // ANTI-FALSE-GREEN, both halves. Naming alone survives a token being
    // *added* that MH must now avoid; counting alone survives a *rename*, which
    // keeps the count while silently removing the bar on the old spelling — and
    // `wrap_key_id_mismatch` has a rename already filed as a protocol
    // follow-up, so that is live rather than hypothetical. Without this, a file
    // move or a renamed field yields an empty barred set and a vacuously green
    // assertion.
    // The eight CODEC tokens are DERIVED from `ALL_REJECT_REASONS`; the eight
    // crypto and key tokens are literals because they have no Rust home at all
    // — they exist only in the vector file and in the SDK's
    // `rejectReason.ts`, which is the entire reason this test reads the fixture
    // rather than the Rust enum. Deriving the half that can be derived removes
    // eight hand-typed spellings without weakening the pin: a Rust-side rename
    // still moves both the derived list and MH's emission together, and the
    // count floor below is what catches a fixture that lost entries.
    let required: Vec<&str> = ALL_REJECT_REASONS
        .iter()
        .map(|reason| reason.as_str())
        .chain([
            "no_transmit_key",
            "signature_invalid",
            "decrypt_failed",
            "unwrap_failed",
            "replay_detected",
            "wrap_key_id_mismatch",
            "no_kek_for_generation",
            "no_roster_entry",
        ])
        .collect();
    assert!(
        tokens.len() >= required.len(),
        "expected at least {} reject tokens in the vector file, found {} — a shortfall means \
         the fixture moved or its shape changed, and the collision assertion below would pass \
         vacuously",
        required.len(),
        tokens.len()
    );
    for token in &required {
        assert!(
            tokens.contains(*token),
            "the vector file no longer carries '{token}'; the bar on that spelling has silently \
             disappeared"
        );
    }

    for reason in MediaDropReason::ALL {
        assert!(
            !tokens.contains(reason.as_str()),
            "MH-local drop reason '{}' collides with a cross-language reject token. If it is a \
             codec condition, emit `RejectReason::as_str()` instead of re-spelling it; if it is \
             a crypto- or key-layer condition, MH cannot observe it at all.",
            reason.as_str()
        );
    }
}

/// No MH media label value says `budget` or `capacity`.
///
/// This is a §1 transport-parameter queue — a latency ceiling — and **not** the
/// deferred §11 egress-budget chain. A label borrowing that vocabulary would
/// make a future reader believe the budget mechanism shipped.
#[test]
fn no_media_label_value_claims_to_be_a_budget_or_a_capacity() {
    for value in MediaDropReason::ALL.iter().map(|r| r.as_str()) {
        assert!(
            !value.contains("budget") && !value.contains("capacity"),
            "'{value}' borrows egress-budget vocabulary for a transport-parameter queue"
        );
    }
    for value in MediaDirection::ALL.iter().map(|d| d.as_str()) {
        assert!(!value.contains("budget") && !value.contains("capacity"));
    }
    for value in MediaLatencyPhase::ALL.iter().map(|p| p.as_str()) {
        assert!(!value.contains("budget") && !value.contains("capacity"));
    }
}

/// No media label value carries a participant, stream-identity or meeting
/// dimension.
#[test]
fn no_media_label_value_carries_stream_or_participant_identity() {
    const BARRED: [&str; 7] = [
        "stream_id",
        "slot_id",
        "sender_id",
        "egress_stream_id",
        "stream_number",
        "participant_id",
        "meeting_id",
    ];
    let values: Vec<&str> = MediaDropReason::ALL
        .iter()
        .map(|r| r.as_str())
        .chain(MediaDirection::ALL.iter().map(|d| d.as_str()))
        .chain(MediaLatencyPhase::ALL.iter().map(|p| p.as_str()))
        .collect();
    assert!(
        !values.is_empty(),
        "the value set is empty; this proves nothing"
    );
    for value in values {
        for barred in BARRED {
            assert!(
                !value.contains(barred),
                "media label value '{value}' carries the identity dimension '{barred}'"
            );
        }
    }
}

/// `direction` is pipeline-relative, never participant-relative.
///
/// Both readings are 2-valued and indistinguishable in a catalog entry; only
/// the pipeline-relative one is structurally incapable of growing a third value
/// that individuates a participant.
#[test]
fn direction_is_pipeline_relative() {
    let values: Vec<&str> = MediaDirection::ALL.iter().map(|d| d.as_str()).collect();
    assert_eq!(values, vec!["ingress", "egress"]);
    for participant_relative in ["uplink", "downlink", "send", "receive"] {
        assert!(
            !values.contains(&participant_relative),
            "'{participant_relative}' is participant-relative and is barred"
        );
    }
}

// ---------------------------------------------------------------------------
// The macro deny
// ---------------------------------------------------------------------------

/// Denied macro FORMS. Handle methods (`.increment`, `.record`, `.set`) are
/// deliberately absent: the deny is about macros, and a guard that banned
/// cached-handle calls would ban the pattern it exists to enforce
/// (ADR-0036 §11).
///
/// # Two things about this list that are not obvious from reading it
///
/// **The level macros are covered DIRECTLY as of 2026-09-08.** They used to be
/// covered only transitively — there was no `info!` / `warn!` / `error!` entry,
/// on the reasoning that a file calling one must first import it and the import
/// line carries `tracing::` or `log::`. That indirection was load-bearing and
/// fragile in two specific ways, and the trigger this comment used to name
/// ("if such a re-export appears, add the level macros directly") had in fact
/// already fired twice without anyone noticing: a `use crate::telemetry::warn;`
/// re-export carries neither prefix, and a `#[macro_use] extern crate tracing;`
/// at the crate root puts the only qualifying line in `lib.rs`, which this
/// walker does not scan.
///
/// What made the indirection necessary was the old substring matcher, which
/// could not tell `error` the identifier from `error!` the macro.
/// [`contains_macro_invocation`] can, so the names are listed directly and the
/// import entries are now belt-and-braces rather than the only coverage.
///
/// **`describe_*` is denied even though it does not emit.** It is the form that
/// documents a metric without resolving a handle — the shape that leaves
/// `dt-guard application-metrics` green while `metric-coverage` goes red — and a
/// `describe_` call under `media/` would mean handle resolution had migrated
/// into the hot path.
/// **There is deliberately no bare `metrics::` entry**, and that is the third
/// non-obvious thing. `media/` legitimately imports types from
/// `crate::observability::metrics` — that is the whole injected-handle design —
/// and a bare `metrics::` matches that import at a token boundary (`::` is not
/// an identifier character), so it fired on `use
/// crate::observability::metrics::MediaMetricHandles`. Denying it would ban the
/// pattern the deny exists to enforce, which is the same mistake as denying
/// `.increment`. Nothing is lost: the fully-qualified `metrics::counter!(…)`
/// still contains `counter!(` at a token boundary and is caught by that entry.
/// Non-invocation needles: import prefixes and the attribute form.
///
/// These are matched as plain token-boundary substrings because they are not
/// macro *invocations* and so have no `!` or delimiter to shape-match.
const DENIED_PREFIXES: [&str; 3] = ["tracing::", "log::", "#[instrument"];

/// Macro NAMES, matched by shape rather than as `name!(` literals.
///
/// # Why names and not `name!(` substrings (corrected 2026-09-08)
///
/// Every entry here used to be spelled `name!(`, which hardcoded two
/// assumptions Rust does not make — the same two this repo's `dt-guard`
/// `media-telemetry-deny` guard was widened to stop making in the same commit:
/// whitespace may sit between the name and the `!` (`counter !("m")`), and the
/// delimiter may be `[` or `{` (`counter!{…}`). Both compile and both ran
/// straight through this walker.
///
/// That mattered more here than it would in an ordinary test, because this
/// walker is the **only** coverage for `webtransport/media_transport.rs` (see
/// the DO-NOT-RETIRE banner below). A complement that is materially narrower
/// than the control it complements reads as coverage while covering less —
/// which is the precise failure ADR-0036 §11's coverage section is about, and
/// which the dt-guard side of this commit spends itself closing. Matching on
/// shape via [`contains_macro_invocation`] means the list cannot re-acquire
/// that class one entry at a time.
///
/// # Ordering and prefixes
///
/// Names that are prefixes of other names (`print`/`println`,
/// `counter`/`describe_counter`, `span`/`info_span`) are safe in any order:
/// the shape matcher requires the character after the name to be whitespace or
/// `!`, so `print` cannot match inside `println!(`, and the token-boundary
/// check rejects `counter` inside `describe_counter!(` because `_` is an
/// identifier character.
const DENIED_MACRO_NAMES: [&str; 24] = [
    // tracing/log level macros, denied DIRECTLY as of 2026-09-08.
    //
    // These were previously covered only TRANSITIVELY, via the `tracing::` /
    // `log::` import-prefix entries, because a substring list could not tell
    // `error` the identifier from `error!` the macro. `contains_macro_invocation`
    // can, so the indirection is no longer necessary — and it never covered
    // two live spellings: a `#[macro_use] extern crate tracing;` at the crate
    // root (that line lives in `lib.rs`, which this walker does not scan) and
    // a `use crate::telemetry::warn;` re-export.
    //
    // `compile_error!` is safe under this: the leading `_` makes `error` fail
    // the token-boundary check, which is the same argument `dt-guard` relies on.
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    // The `log` crate's GENERIC macro, `log!(Level::Info, ...)`. Denied by
    // dt-guard as `TelemetryGroup::LogMacro` and missed here until 2026-09-08,
    // where it was covered only by the `"log::"` import prefix -- i.e. exactly
    // the transitive coverage the paragraph above retires, with the same two
    // uncovered spellings. `catalog!(` and `dialog !(` cannot match it: the
    // token-boundary check rejects both on the preceding `a`.
    "log",
    // metrics
    "counter",
    "histogram",
    "gauge",
    "describe_counter",
    "describe_histogram",
    "describe_gauge",
    // std print family — `print!`/`eprint!` were MISSING before 2026-09-08.
    // Unlike the level macros there is no import line to catch them: they are
    // std-prelude, so nothing else in this list would ever have fired.
    "print",
    "println",
    "eprint",
    "eprintln",
    "dbg",
    // tracing event + span family. `warn_span`/`error_span`/`trace_span` were
    // MISSING before 2026-09-08; only `span`/`info_span`/`debug_span` were
    // listed, so three of the six span spellings walked through.
    "event",
    "span",
    "trace_span",
    "debug_span",
    "info_span",
    "warn_span",
    "error_span",
];

/// Whether `line` contains `needle` at a token boundary — not preceded by an
/// identifier character.
///
/// Without this, `"log::"` matches `catalog::` and `dialog::`, and `"span!("`
/// matches `info_span!(`. Those trip in the safe direction — they cannot go
/// false-green — but a spurious trip on legitimate code teaches the next author
/// to loosen the guard, which is the real cost.
fn contains_at_token_boundary(line: &str, needle: &str) -> bool {
    let bytes = line.as_bytes();
    line.match_indices(needle).any(|(index, _)| {
        index == 0
            || !bytes
                .get(index.wrapping_sub(1))
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    })
}

/// Whether `line` invokes the macro `name`, in ANY legal Rust spelling.
///
/// Shape: `name` at a token boundary, then optional whitespace, then `!`, then
/// optional whitespace, then one of `(`, `[`, `{`. This is deliberately the
/// same shape as `dt_guard::media_telemetry_deny`'s `DENIED_MACRO_RE`, so the
/// two encodings of ADR-0036 §11 cannot drift into denying different things.
///
/// Replaces a set of `name!(` literal substrings, which missed `counter !("m")`
/// and `counter!{…}` — both legal, both compiling, both logging. See
/// [`DENIED_MACRO_NAMES`] for why a narrower complement was the dangerous kind
/// of narrow.
///
/// The trailing-character check after `name` is what makes prefix names safe:
/// in `println!(`, the character after `print` is `l`, which is neither
/// whitespace nor `!`, so `print` does not match there.
fn contains_macro_invocation(line: &str, name: &str) -> bool {
    let bytes = line.as_bytes();
    line.match_indices(name).any(|(index, _)| {
        let preceded_by_ident = index != 0
            && bytes
                .get(index.wrapping_sub(1))
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if preceded_by_ident {
            return false;
        }
        let mut cursor = index + name.len();
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'!') {
            return false;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        matches!(bytes.get(cursor), Some(b'(' | b'[' | b'{'))
    })
}

/// Every `.rs` file under `dir`, RECURSIVELY.
///
/// A single non-recursive `read_dir` scans `media/*.rs` and misses
/// `media/<subdir>/*.rs`, which compiles into the hot path just the same. That
/// is ADR-0036 §11's own stated failure — "a file list narrows silently on
/// refactor while the guard keeps passing" — one level down, and the file-count
/// floor below does not catch it: adding a subdirectory leaves the top-level
/// count unchanged. Whoever ports this to `dt-guard` inherits the walk.
fn rust_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        // `expect`, not a silent `continue`: a subdirectory that fails to read
        // would be skipped while the file-count floor below still passed —
        // the same vacuous-scan shape this walk exists to close, one level in.
        let entries = std::fs::read_dir(&current).unwrap_or_else(|e| {
            panic!(
                "hot-path subdirectory {} must be readable: {e}",
                current.display()
            )
        });
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

/// No log or metric macro form appears anywhere under the hot-path directory,
/// nor in the transport-seam adapter.
///
/// # DO NOT RETIRE THIS TEST — it is the dt-guard scope's COMPLEMENT
///
/// `dt-guard media-telemetry-deny` now exists and enforces ADR-0036 §11 over
/// `crates/mh-service/src/media/`. That makes this walker look redundant. **It
/// is not.** The guard's scope is a directory list, and
/// `src/webtransport/media_transport.rs` — the per-frame transport-seam
/// adapter — structurally cannot enter it: naming one file is the file-list
/// shape §11 rules out, and its directory cannot be denied wholesale because
/// `connection.rs` sits beside it and legitimately needs telemetry.
///
/// So the seam-adapter arm below is that file's **only** coverage. Deleting
/// this test on redundancy grounds drops it silently, with every guard still
/// reporting clean. Reasoning lives once, in
/// `scripts/guards/simple/media-telemetry-deny.yaml` §INCOMPLETE BY DESIGN.
///
/// # In-crate stand-in for the directory half, which dt-guard now covers
///
/// ADR-0036 §11 requires enforcement as "a deny of log and metric macros scoped
/// to a directory". Building the `dt-guard` subcommand was infrastructure's,
/// not media-handler's (CLAUDE.md §Guard-crate ownership); it landed in
/// `c0538b56`. The directory half of this walker is therefore now belt-and-
/// braces, and the seam-adapter half is load-bearing — see `docs/TODO.md`.
///
/// It is a walker rather than `macro_rules!` shadowing because shadowing cannot
/// intercept the `#[instrument]` **attribute** at all: it would cover less
/// while reading as if it covered more.
#[test]
fn no_log_or_metric_macro_is_reachable_from_the_hot_path() {
    let media_dir = repo_relative("src/media");
    let seam_adapter = repo_relative("src/webtransport/media_transport.rs");

    // ANTI-FALSE-GREEN (story R-23): a deny over nothing is the vacuous pass
    // this arrangement exists to avoid. If the directory is renamed, moved or
    // emptied, this test fails rather than reporting a clean scan of zero
    // files.
    assert!(
        media_dir.is_dir(),
        "the hot-path directory {} does not exist — the deny below would scan nothing and pass",
        media_dir.display()
    );
    assert!(
        seam_adapter.is_file(),
        "the transport-seam adapter {} does not resolve; @observability's ruling scopes the \
         deny to it explicitly because it is a SIBLING of the hot path, not a child",
        seam_adapter.display()
    );

    let mut scanned = 0_usize;
    let mut files: Vec<PathBuf> = rust_files_recursive(&media_dir);
    assert!(
        !files.is_empty(),
        "the hot-path directory holds no .rs files — the deny below would scan nothing and pass"
    );
    files.push(seam_adapter);
    assert!(
        files.len() >= 8,
        "expected the hot-path directory plus the seam adapter to hold at least 8 files, found \
         {} — a shrunken scan set is how this deny goes quietly blind",
        files.len()
    );

    for path in files {
        let source = std::fs::read_to_string(&path).expect("a scanned file must be readable");
        scanned += 1;
        for (number, line) in source.lines().enumerate() {
            // Comments and doc comments legitimately NAME the denied macros —
            // this file's own subject is that they must not be invoked.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            for denied in DENIED_PREFIXES {
                assert!(
                    !contains_at_token_boundary(line, denied),
                    "{}:{} references '{denied}'. ADR-0036 §11: the media directory holds ONLY \
                     the hot path, and no log or metric macro may be reachable from it — observe \
                     through a handle resolved at setup instead. If this is setup or teardown \
                     code, move it to a SIBLING module rather than adding an exemption.",
                    path.display(),
                    number + 1
                );
            }
            for denied in DENIED_MACRO_NAMES {
                assert!(
                    !contains_macro_invocation(line, denied),
                    "{}:{} invokes '{denied}!'. ADR-0036 §11: the media directory holds ONLY the \
                     hot path, and no log or metric macro may be reachable from it — observe \
                     through a handle resolved at setup instead. If this is setup or teardown \
                     code, move it to a SIBLING module rather than adding an exemption. \
                     (Matched by SHAPE, so `{denied} !(`, `{denied}!{{…}}` and `{denied}![…]` \
                     are caught too — all three are legal Rust.)",
                    path.display(),
                    number + 1
                );
            }
        }
    }
    assert!(scanned >= 8, "scanned only {scanned} files");
}

/// The walker's matcher covers every legal invocation spelling — pinned,
/// because an unpinned matcher is how this walker drifted narrow in the first
/// place.
///
/// Until 2026-09-08 the deny list held `name!(` literal substrings, so all of
/// the `MUST MATCH` cases below walked through. This test is what stops the
/// list being "simplified" back to substrings, and it is deliberately colocated
/// with the walker rather than living in `dt-guard`: the two are independent
/// encodings of ADR-0036 §11 and each has to stand up on its own.
#[test]
fn the_walker_matcher_catches_every_legal_invocation_spelling() {
    for line in [
        "counter!(\"m\", 1);",
        "counter !(\"m\", 1);",          // whitespace before `!`
        "counter  !(\"m\", 1);",         // more of it
        "counter!{\"m\", 1}",            // brace delimiter
        "counter![\"m\", 1]",            // bracket delimiter
        "log!(Level::Info, \"x\");",     // the log crate's generic macro
        "print!(\"x\");",                // was absent from the list entirely
        "eprint!(\"x\");",               // ditto
        "warn_span!(\"s\");",            // was absent from the span list
        "error_span!(\"s\");",           // ditto
        "trace_span!(\"s\");",           // ditto
        "    metrics::counter!(\"m\");", // qualified still contains the name
    ] {
        assert!(
            DENIED_MACRO_NAMES
                .iter()
                .any(|name| contains_macro_invocation(line, name)),
            "MUST MATCH but did not: {line}"
        );
    }

    for line in [
        // Handle calls — the allow side. `.increment(` cannot match `name!`.
        "self.handles.frames_forwarded.increment(1);",
        "self.handles.bytes.record(len as f64);",
        // A denied NAME in a non-invocation position.
        "let counter = 0;",
        "struct Gauge { span: u32 }",
        // Compound names must not fire: the token boundary still holds.
        "my_counter!(\"m\");",
        // `log` must not fire inside these — token boundary on the `a`.
        "catalog!(\"x\");",
        "dialog !(\"x\");",
        "frame_counter !(\"m\");",
        // `!=` is not an invocation — the char after `!` must be a delimiter.
        "if counter != (0) { }",
        // `print` must not match inside `println` twice over, nor `span`
        // inside `info_span` — asserted as "exactly one name matches".
        "let x = 1;",
    ] {
        assert!(
            !DENIED_MACRO_NAMES
                .iter()
                .any(|name| contains_macro_invocation(line, name)),
            "MUST NOT MATCH but did: {line}"
        );
    }

    // Prefix names must not double-count: `println!(` is `println`, never also
    // `print`; `describe_counter!(` is never also `counter`.
    for (line, want) in [("println!(\"x\");", 1), ("describe_counter!(\"m\");", 1)] {
        let hits = DENIED_MACRO_NAMES
            .iter()
            .filter(|name| contains_macro_invocation(line, name))
            .count();
        assert_eq!(hits, want, "wrong match count for {line}");
    }
}

/// The queue bounds the forward path is built against are the declared
/// constants, not literals.
#[test]
fn the_queue_bounds_are_the_declared_constants() {
    let ingress: SharedQueue<u8> = SharedQueue::new(INGRESS_QUEUE_FRAMES);
    let egress: SharedQueue<u8> = SharedQueue::new(EGRESS_QUEUE_FRAMES);
    assert_eq!(ingress.capacity(), INGRESS_QUEUE_FRAMES);
    assert_eq!(egress.capacity(), EGRESS_QUEUE_FRAMES);
    assert_ne!(
        INGRESS_QUEUE_FRAMES, EGRESS_QUEUE_FRAMES,
        "the two bounds answer different questions and are deliberately not shared; if they are \
         ever equal by coincidence, this assertion is the place to record that it is a \
         coincidence"
    );
}
