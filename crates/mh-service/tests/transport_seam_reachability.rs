//! Reachability suite for the ADR-0036 §10 transport seam.
//!
//! # What this proves, and what it deliberately does not
//!
//! ADR-0036 §10 justifies the transport seam partly on "deterministic
//! reachability of drop paths for metric coverage". This suite is the
//! discharge of that claim: for every drop path the design asks a counter to
//! observe, here is the line of test code that fires it.
//!
//! Assertions are on **shim-observable outcomes, not counter deltas.** Neither
//! MH counter exists yet — the uplink hop-sequence gap counter and the egress
//! queue overflow counter both belong to the forward-path task — and nothing
//! here names them as though they did. When those counters land, the tests
//! that assert on them attach a `MetricAssertion` to these same knobs.
//!
//! # Zero syscalls, proved rather than asserted
//!
//! [`forward_path_needs_no_io_driver`] builds its runtime by hand with
//! `enable_time()` and **no I/O driver**. Any *tokio* network or file I/O on
//! such a runtime panics with "there is no reactor running", so that test
//! cannot pass if the driven path reaches the network through tokio — which is
//! the path that matters, since production goes through wtransport and quinn
//! and both are tokio-based. It would not trap a blocking `std::net` call or a
//! raw syscall; nothing here makes one, but the control is narrower than "no
//! syscalls are possible" and is described as what it is. The remaining tests
//! use `#[tokio::test(start_paused = true)]` for convenience.
//!
//! # Deterministic time
//!
//! Delay is `tokio::time::sleep_until` released by `tokio::time::advance`
//! under a `start_paused` runtime. There is no `Clock` trait in this
//! workspace and the shim takes no clock argument.
//! [`added_delay_is_released_by_virtual_time`] is auto-advance-tolerant and
//! asserts exact virtual-time equality; [`added_delay_requires_explicit_advance`]
//! drives `advance` explicitly with the receive parked in a spawned task.
//! Both ship so neither mechanism is mistaken for the other.

use bytes::Bytes;
use mh_service::transport::{DatagramSendError, MediaSendStream, MediaTransport, TransportError};
use mh_test_utils::transport_shim::{DatagramLoss, LossDelayTransport, ShimConfig};
use std::time::Duration;

/// Distinct, recognisable payloads. Content is arbitrary; only identity
/// matters to these assertions.
fn payload(n: u8) -> Bytes {
    Bytes::from(vec![n; 8])
}

/// Drive the seam through a GENERIC parameter, never a concrete type and never
/// `dyn`. This is what exercises monomorphisation, and it is the shape the
/// forward-path task's loops will have. (`&dyn MediaTransport` would not even
/// compile: the trait's RPITIT methods make it dyn-incompatible, so
/// "generics, not dyn" is enforced by the compiler rather than by review.)
fn send_all<T: MediaTransport>(transport: &T, count: u8) -> Vec<Result<(), DatagramSendError>> {
    (1..=count)
        .map(|n| transport.send_datagram(payload(n)))
        .collect()
}

// =====================================================================
// 1. Egress in-flight loss
// =====================================================================

#[tokio::test(start_paused = true)]
async fn datagram_send_drop_is_deterministically_reachable() {
    let shim = LossDelayTransport::new();
    shim.set_egress_datagram_loss(DatagramLoss::EveryNth(2));

    let results = send_all(&shim, 4);

    // Every send reports success: an in-flight drop is invisible to the
    // sender, which is precisely why the far end's hop-sequence gap is the
    // only signal for it.
    assert!(
        results.iter().all(Result::is_ok),
        "in-flight loss must be invisible to the sender, got {results:?}"
    );

    let delivered = shim.delivered_datagrams();
    assert!(!shim.capture_truncated(), "capture must not be truncated");
    assert_eq!(
        delivered,
        vec![payload(1), payload(3)],
        "EveryNth(2) must drop offers 2 and 4 and deliver 1 and 3, in order"
    );
    assert_eq!(shim.egress_dropped_in_flight(), 2);
    assert_eq!(shim.delivered_count(), 2);
    assert_eq!(
        shim.refused_for_capacity(),
        0,
        "loss must not be implemented as back-pressure"
    );
}

// =====================================================================
// 2. Ingress in-flight loss — the only firing path for MH's own gap counter
// =====================================================================

#[tokio::test(start_paused = true)]
async fn ingress_datagram_loss_is_deterministically_reachable() {
    let shim = LossDelayTransport::new();
    shim.set_ingress_datagram_loss(DatagramLoss::Next(1));

    for n in 1..=3 {
        assert!(
            shim.push_inbound(payload(n)).is_ok(),
            "injection must be accepted; a lost datagram is accepted-not-delivered"
        );
    }

    // The first injected datagram was accepted and never queued, so the
    // receiver sees payloads 2 and 3 — a gap, not a swallowed item.
    assert_eq!(shim.recv_datagram().await, Ok(payload(2)));
    assert_eq!(shim.recv_datagram().await, Ok(payload(3)));

    assert_eq!(shim.ingress_dropped(), 1);
    assert_eq!(
        shim.egress_dropped_in_flight(),
        0,
        "ingress loss must not move an egress counter"
    );
    assert_eq!(shim.refused_for_capacity(), 0);
    assert_eq!(
        shim.egress_accepted(),
        0,
        "nothing was sent; only the receive direction was exercised"
    );
}

// =====================================================================
// 3. Back-pressure refusal, and the payload comes back UNCOPIED
// =====================================================================

#[tokio::test(start_paused = true)]
async fn backpressure_refusal_hands_back_the_same_allocation() {
    let shim = LossDelayTransport::new();
    shim.set_send_capacity(2);

    assert!(shim.send_datagram(payload(1)).is_ok());
    assert!(shim.send_datagram(payload(2)).is_ok());

    // Keep a clone so the original allocation's address is observable. Cloning
    // `Bytes` bumps a refcount and copies nothing, so both handles report the
    // same pointer.
    let original = payload(3);
    let probe = original.clone();
    let refused = shim.send_datagram(original);
    assert!(
        matches!(refused, Err(DatagramSendError::WouldBlock(_))),
        "a send past capacity must refuse with WouldBlock, got {refused:?}"
    );

    // Routed through the seam's own accessor rather than an inline match, so
    // this suite exercises `into_payload` across the crate boundary the way a
    // real bounded-queue requeue would.
    let handed_back = refused
        .err()
        .and_then(DatagramSendError::into_payload)
        .expect("WouldBlock must carry the payload back");

    // POINTER identity, not byte equality. Byte equality would pass against an
    // implementation that copied, which is the whole property under test:
    // MH's bounded queue must be able to requeue without copying.
    assert_eq!(
        handed_back.as_ptr(),
        probe.as_ptr(),
        "WouldBlock must hand back the same allocation, not a copy"
    );
    assert_eq!(handed_back.len(), probe.len());

    assert_eq!(shim.refused_for_capacity(), 1);
    assert_eq!(shim.delivered_count(), 2);
    assert_eq!(
        shim.egress_dropped_in_flight(),
        0,
        "a refusal is not a drop: MH still owns the payload"
    );
    assert_eq!(
        shim.delivered_datagrams(),
        vec![payload(1), payload(2)],
        "the refused payload must not reach the sink"
    );
}

/// A refusal must consume no capacity, or a caller draining and retrying would
/// see the budget silently erode. Sibling of the test above; separated because
/// it is a different claim.
#[tokio::test(start_paused = true)]
async fn refusal_consumes_no_capacity_so_a_retry_after_drain_succeeds() {
    let shim = LossDelayTransport::new();
    shim.set_send_capacity(1);

    assert!(shim.send_datagram(payload(1)).is_ok());
    assert!(matches!(
        shim.send_datagram(payload(2)),
        Err(DatagramSendError::WouldBlock(_))
    ));
    assert!(matches!(
        shim.send_datagram(payload(3)),
        Err(DatagramSendError::WouldBlock(_))
    ));
    assert_eq!(shim.refused_for_capacity(), 2);

    // Simulate the drain the forward path would perform.
    shim.set_send_capacity(1);
    assert!(shim.send_datagram(payload(4)).is_ok());
    assert_eq!(shim.delivered_count(), 2);
}

// =====================================================================
// 4. The drop paths are structurally distinguishable
// =====================================================================

/// The assertion that makes the knobs usable as independent counter triggers.
/// Each scenario must move its own counter and leave the other two at zero —
/// otherwise a downstream test could not tell which condition it reproduced.
#[tokio::test(start_paused = true)]
async fn the_three_drop_paths_are_distinguishable() {
    // Egress loss only.
    let loss = LossDelayTransport::new();
    loss.set_egress_datagram_loss(DatagramLoss::All);
    let _ = send_all(&loss, 3);
    assert_eq!(loss.egress_dropped_in_flight(), 3);
    assert_eq!(loss.refused_for_capacity(), 0);
    assert_eq!(loss.ingress_dropped(), 0);

    // Capacity refusal only.
    let capacity = LossDelayTransport::new();
    capacity.set_send_capacity(0);
    let _ = send_all(&capacity, 3);
    assert_eq!(capacity.refused_for_capacity(), 3);
    assert_eq!(capacity.egress_dropped_in_flight(), 0);
    assert_eq!(capacity.ingress_dropped(), 0);

    // Ingress loss only.
    let ingress = LossDelayTransport::new();
    ingress.set_ingress_datagram_loss(DatagramLoss::All);
    for n in 1..=3 {
        assert!(ingress.push_inbound(payload(n)).is_ok());
    }
    assert_eq!(ingress.ingress_dropped(), 3);
    assert_eq!(ingress.egress_dropped_in_flight(), 0);
    assert_eq!(ingress.refused_for_capacity(), 0);
}

// =====================================================================
// 5-6. Deterministic delay
// =====================================================================

/// Exact virtual-time equality. Impossible under a real clock, so this
/// assertion doubles as proof that the clock under test is virtual. Tolerant
/// of the runtime's own auto-advance.
#[tokio::test(start_paused = true)]
async fn added_delay_is_released_by_virtual_time() {
    let shim = LossDelayTransport::new();
    let delay = Duration::from_millis(250);
    shim.set_added_delay(delay);

    assert!(shim.push_inbound(payload(1)).is_ok());

    let start = tokio::time::Instant::now();
    let received = shim.recv_datagram().await;
    let elapsed = tokio::time::Instant::now() - start;

    assert_eq!(received, Ok(payload(1)));
    assert_eq!(
        elapsed, delay,
        "virtual elapsed time must equal the configured delay exactly"
    );
}

/// The explicit-advance half. The receive is parked in a spawned task, the
/// main task yields (so the runtime is never idle and cannot auto-advance),
/// the handle is asserted still pending, and only then does `advance` release
/// it.
#[tokio::test(start_paused = true)]
async fn added_delay_requires_explicit_advance() {
    let shim = LossDelayTransport::new();
    let delay = Duration::from_secs(5);
    shim.set_added_delay(delay);
    assert!(shim.push_inbound(payload(7)).is_ok());

    let receiver = shim.clone();
    let handle = tokio::spawn(async move { receiver.recv_datagram().await });

    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert!(
        !handle.is_finished(),
        "the receive must remain parked until virtual time advances"
    );

    tokio::time::advance(delay).await;

    let received = handle.await.expect("receive task must not panic");
    assert_eq!(received, Ok(payload(7)));
}

/// Delay is PATH LATENCY, not a per-call cost, on the ingress path: N
/// datagrams injected at one virtual instant all become readable at one
/// virtual instant. Pinned because the alternative semantics (per-recv sleep)
/// is a plausible reimplementation that would silently change what a
/// slow-path test measures.
#[tokio::test(start_paused = true)]
async fn ingress_delay_is_path_latency_not_per_call_cost() {
    let shim = LossDelayTransport::new();
    let delay = Duration::from_millis(100);
    shim.set_added_delay(delay);

    for n in 1..=3 {
        assert!(shim.push_inbound(payload(n)).is_ok());
    }

    let start = tokio::time::Instant::now();
    for n in 1..=3 {
        assert_eq!(shim.recv_datagram().await, Ok(payload(n)));
    }
    let elapsed = tokio::time::Instant::now() - start;

    assert_eq!(
        elapsed, delay,
        "three datagrams injected together must all become readable together"
    );
}

// =====================================================================
// 7. Zero syscalls, proved structurally
// =====================================================================

/// The zero-syscall gate. The runtime has `enable_time()` and **no I/O
/// driver**, so any attempt at tokio-driven network or file I/O panics with
/// "there is no reactor running" — verified by deliberately inserting a
/// `tokio::net::UdpSocket::bind` here, which panics at `tokio/src/net/udp.rs`.
/// Production reaches the network through wtransport and quinn, both
/// tokio-based, so an accidental syscall on the forward path would trip this.
/// A raw `std::net` call would not; see the module docs.
#[test]
fn forward_path_needs_no_io_driver() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("current-thread runtime with only the time driver must build");

    runtime.block_on(async {
        let shim = LossDelayTransport::new();

        for n in 1..=4 {
            assert!(shim.push_inbound(payload(n)).is_ok());
        }

        // A miniature forward path: receive, rewrite a fixed-offset prefix the
        // way a relay-region rewrite would, send onward.
        for n in 1..=4_u8 {
            let received = shim
                .recv_datagram()
                .await
                .expect("injected datagram must arrive");
            assert_eq!(received, payload(n));

            let mut rewritten = Vec::with_capacity(received.len());
            rewritten.extend_from_slice(&received);
            if let Some(first) = rewritten.first_mut() {
                *first = first.wrapping_add(100);
            }
            shim.send_datagram(Bytes::from(rewritten))
                .expect("forwarding must succeed with no knobs armed");
        }

        assert_eq!(shim.delivered_count(), 4);
        assert_eq!(shim.egress_dropped_in_flight(), 0);
        assert_eq!(shim.refused_for_capacity(), 0);
    });
}

// =====================================================================
// 8. Unidirectional streams
// =====================================================================

#[tokio::test(start_paused = true)]
async fn uni_stream_open_write_finish_round_trips() {
    let shim = LossDelayTransport::new();

    let mut stream = shim.open_uni().await.expect("open_uni must succeed");
    assert_eq!(shim.open_uni_streams(), 1);
    assert_eq!(shim.max_concurrent_open_uni(), 1);

    stream
        .write_all(b"first")
        .await
        .expect("write must succeed");
    stream
        .write_all(b"second")
        .await
        .expect("write must succeed");
    stream.finish().await.expect("finish must succeed");

    assert_eq!(shim.open_uni_streams(), 0);
    let finished = shim.finished_uni_streams();
    assert_eq!(finished.len(), 1);
    let recorded = finished.first().expect("one finished stream");
    assert_eq!(recorded.open_ordinal, 0);
    assert_eq!(
        recorded.chunks,
        vec![b"first".to_vec(), b"second".to_vec()],
        "chunks must be captured in write order"
    );
}

/// The high-water mark ADR-0036 §10's "concurrent unfinished groups stay at or
/// below the application cap" gate reads. Recorded even after the streams
/// close, so a gate can assert the peak rather than a sampled instant.
#[tokio::test(start_paused = true)]
async fn concurrent_unfinished_stream_high_water_mark_is_observable() {
    let shim = LossDelayTransport::new();

    let mut a = shim.open_uni().await.expect("open a");
    let mut b = shim.open_uni().await.expect("open b");
    let c = shim.open_uni().await.expect("open c");
    assert_eq!(shim.open_uni_streams(), 3);

    a.finish().await.expect("finish a");
    b.finish().await.expect("finish b");
    // Dropped without finishing: an abandoned group of pictures. It must
    // release its slot but must NOT be recorded as delivered.
    drop(c);

    assert_eq!(shim.open_uni_streams(), 0);
    assert_eq!(
        shim.max_concurrent_open_uni(),
        3,
        "the peak must survive the streams closing"
    );
    assert_eq!(
        shim.finished_uni_streams().len(),
        2,
        "an abandoned stream must not read as a finished one"
    );
}

/// The slow-subscriber mechanism the forward-path gate needs: stream writes
/// pay the added delay per call, so a stalled subscriber is deterministic.
#[tokio::test(start_paused = true)]
async fn stream_writes_pay_the_added_delay_so_a_slow_subscriber_is_drivable() {
    let shim = LossDelayTransport::new();
    let delay = Duration::from_millis(40);
    shim.set_added_delay(delay);

    let mut stream = shim.open_uni().await.expect("open_uni must succeed");
    let start = tokio::time::Instant::now();
    stream.write_all(b"one").await.expect("write must succeed");
    stream.write_all(b"two").await.expect("write must succeed");
    stream.finish().await.expect("finish must succeed");
    let elapsed = tokio::time::Instant::now() - start;

    assert_eq!(
        elapsed,
        delay * 3,
        "two writes and a finish must each pay the delay"
    );
}

// =====================================================================
// 9. Every error variant is reachable through the shim
// =====================================================================

/// Exhaustive match over both seam error enums, with every arm reached by a
/// shim knob.
///
/// This is the seam's own ADR-0036 §10 justification applied to itself: a
/// variant added later without a firing path fails to COMPILE here rather than
/// shipping as a state no test can reproduce. It also depends on neither enum
/// being `#[non_exhaustive]` — that attribute would force a wildcard arm and
/// silently retire the property.
#[tokio::test(start_paused = true)]
async fn every_transport_error_variant_is_shim_reachable() {
    fn classify_send(e: &DatagramSendError) -> &'static str {
        match e {
            DatagramSendError::WouldBlock(_) => "would_block",
            DatagramSendError::TooLarge => "too_large",
            DatagramSendError::ConnectionClosed => "connection_closed",
            DatagramSendError::DatagramsUnsupported => "datagrams_unsupported",
        }
    }
    fn classify_transport(e: &TransportError) -> &'static str {
        match e {
            TransportError::ConnectionClosed => "connection_closed",
            TransportError::StreamClosed => "stream_closed",
        }
    }

    let mut send_seen: Vec<&'static str> = Vec::new();

    // WouldBlock — capacity knob.
    let capacity = LossDelayTransport::new();
    capacity.set_send_capacity(0);
    if let Err(e) = capacity.send_datagram(payload(1)) {
        send_seen.push(classify_send(&e));
    }

    // TooLarge — max-datagram-size knob.
    let sizing = LossDelayTransport::new();
    sizing.set_max_datagram_size(Some(4));
    if let Err(e) = sizing.send_datagram(payload(2)) {
        send_seen.push(classify_send(&e));
    }

    // ConnectionClosed — close knob.
    let closed = LossDelayTransport::new();
    closed.set_closed();
    if let Err(e) = closed.send_datagram(payload(3)) {
        send_seen.push(classify_send(&e));
    }

    // DatagramsUnsupported — peer-support knob.
    let unsupported = LossDelayTransport::new();
    unsupported.set_datagrams_unsupported(true);
    if let Err(e) = unsupported.send_datagram(payload(4)) {
        send_seen.push(classify_send(&e));
    }

    send_seen.sort_unstable();
    assert_eq!(
        send_seen,
        vec![
            "connection_closed",
            "datagrams_unsupported",
            "too_large",
            "would_block"
        ],
        "every DatagramSendError variant must be reachable through a shim knob"
    );

    let mut transport_seen: Vec<&'static str> = Vec::new();

    // ConnectionClosed — close knob, observed on the receive path.
    let closed_rx = LossDelayTransport::new();
    closed_rx.set_closed();
    if let Err(e) = closed_rx.recv_datagram().await {
        transport_seen.push(classify_transport(&e));
    }

    // StreamClosed — write after finish, no knob needed.
    let streams = LossDelayTransport::new();
    let mut stream = streams.open_uni().await.expect("open_uni must succeed");
    stream.finish().await.expect("finish must succeed");
    if let Err(e) = stream.write_all(b"after finish").await {
        transport_seen.push(classify_transport(&e));
    }

    transport_seen.sort_unstable();
    assert_eq!(
        transport_seen,
        vec!["connection_closed", "stream_closed"],
        "every TransportError variant must be reachable through the shim"
    );
}

/// `set_closed()` must WAKE a parked receiver rather than hang it. Without
/// this, a shutdown-path test would deadlock instead of failing, and a
/// deadlocked test reads as a hung suite rather than a bug.
#[tokio::test(start_paused = true)]
async fn closing_wakes_a_parked_receiver() {
    let shim = LossDelayTransport::new();
    let receiver = shim.clone();
    let handle = tokio::spawn(async move { receiver.recv_datagram().await });

    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert!(!handle.is_finished(), "receive parks on an empty queue");

    shim.set_closed();
    let result = handle.await.expect("receive task must not panic");
    assert_eq!(result, Err(TransportError::ConnectionClosed));
}

// =====================================================================
// Bounded capture
// =====================================================================

/// Past the capture limit the shim keeps COUNTING but stops RETAINING, and
/// says so. A test that asserted on a silently truncated capture would pass or
/// fail for reasons unrelated to its subject.
#[tokio::test(start_paused = true)]
async fn capture_is_bounded_and_truncation_is_loud() {
    let shim = LossDelayTransport::with_config(ShimConfig {
        inbound_capacity: 8,
        capture_limit: 2,
        ..ShimConfig::default()
    });

    let _ = send_all(&shim, 5);

    assert_eq!(shim.delivered_count(), 5, "counting must continue");
    assert_eq!(shim.delivered_datagrams().len(), 2, "retention is bounded");
    assert!(
        shim.capture_truncated(),
        "truncation must be observable, never silent"
    );
}

/// The finished-stream history is bounded on the same terms as the datagram
/// capture: keep counting, stop retaining, make the truncation loud.
///
/// Each retained entry owns every byte written to its stream, so under
/// ADR-0036 §1 (one video stream per group of pictures) an unbounded history
/// would leak monotonically across a Tier-2 benchmark run and pollute the very
/// deltas that benchmark exists to compare. Regression test for exactly that.
#[tokio::test(start_paused = true)]
async fn finished_stream_history_is_bounded_and_truncation_is_loud() {
    let shim = LossDelayTransport::with_config(ShimConfig {
        finished_stream_limit: 2,
        ..ShimConfig::default()
    });

    for _ in 0..5 {
        let mut stream = shim.open_uni().await.expect("open_uni must succeed");
        stream.write_all(b"gop").await.expect("write must succeed");
        stream.finish().await.expect("finish must succeed");
    }

    assert_eq!(
        shim.finished_uni_streams().len(),
        2,
        "retention must stop at the configured limit"
    );
    assert!(
        shim.finished_streams_truncated(),
        "truncation must be observable, never silent"
    );
    assert_eq!(
        shim.open_uni_streams(),
        0,
        "bounding retention must not disturb the open-stream accounting"
    );
    assert_eq!(
        shim.max_concurrent_open_uni(),
        1,
        "streams were opened and finished one at a time"
    );
}

/// A full ingress queue hands the payload back rather than dropping it, so
/// saturation is distinguishable from the loss this shim injects deliberately.
#[tokio::test(start_paused = true)]
async fn full_ingress_queue_hands_the_payload_back() {
    let shim = LossDelayTransport::with_config(ShimConfig {
        inbound_capacity: 2,
        capture_limit: 8,
        ..ShimConfig::default()
    });

    assert!(shim.push_inbound(payload(1)).is_ok());
    assert!(shim.push_inbound(payload(2)).is_ok());

    let rejected = shim.push_inbound(payload(3));
    assert_eq!(
        rejected,
        Err(payload(3)),
        "a full queue must fail loudly and return the payload"
    );
    assert_eq!(
        shim.ingress_dropped(),
        0,
        "queue saturation is not in-flight loss and must not move that counter"
    );
}
