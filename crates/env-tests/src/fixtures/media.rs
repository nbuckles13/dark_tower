//! Media-path test fixtures (ADR-0036).

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

/// A syntactically valid Ed25519 identity public key for join requests.
///
/// **Not real key material and not a real curve point.** MC's join path checks
/// length only — exactly 32 bytes, no curve validation — so a fixed synthetic
/// pattern is sufficient and no keypair generation is needed.
///
/// ANCHOR (DRY): the source of this value is
/// `mc_test_utils::media::sample_identity_public_key`. env-tests deliberately
/// does NOT take a dev-dependency on `mc-test-utils` to share it: that crate
/// depends on `mc-service`, and linking a service crate into a suite whose whole
/// premise is black-box validation against a deployed cluster inverts the
/// ADR-0028 layering. env-tests links exactly one local crate (`proto-gen`) and
/// zero service crates, and that thinness is the point. Two homes is the
/// accepted cost; keep them equal in shape, not by import.
///
/// Passing this asserts nothing about identity: MC performs no `cnf` binding,
/// so an accepted key gives same-keyholder consistency and never a verified
/// identity.
pub fn sample_identity_public_key() -> Vec<u8> {
    vec![0x42; 32]
}

/// Bound on the whole binding phase's send-until-received loop. A rig
/// precondition for unreliable QUIC datagrams, not a performance assertion.
pub const BINDING_DEADLINE: Duration = Duration::from_secs(30);

/// A frame-v2 audio datagram through the production codec, with publisher-side
/// relay values a correct relay must rewrite and a payload marker naming the
/// sender (MH is keyless: payload and signature must survive byte-identically).
///
/// Hoisted from `tests/27_mc_slot_placement.rs` at story 2 task 10, when
/// `26_mh_quic.rs` became its second consumer.
///
/// ANCHOR (DRY): this is a deliberate TWIN of
/// `crates/mh-service/tests/common/media_frame.rs` (the mh-service forward-path
/// suites' frame builder). env-tests does NOT share it by import: that file is a
/// SERVICE crate's test-support module, and ADR-0028 layering forbids this
/// black-box cluster suite from depending on a service's test utilities — the
/// same rule `sample_identity_public_key` in `fixtures/media.rs` records for its
/// own twin. Both are built through `media_protocol::codec::encode_frame` (a
/// wire-format crate, not a service crate), so neither hand-rolls the wire
/// layout; two homes is the accepted cost, kept equal in shape, not by import.
pub fn audio_datagram(stream_sequence: u32, marker: u8) -> bytes::Bytes {
    use media_protocol::codec::{encode_frame, MediaFrameParts};
    use media_protocol::frame::{FrameFlags, SIGNATURE_SIZE};
    let payload = vec![marker; 160];
    let mut signature = [0_u8; SIGNATURE_SIZE];
    for (i, b) in signature.iter_mut().enumerate() {
        *b = u8::try_from(i % 251).unwrap_or(0);
    }
    encode_frame(&MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: false,
        },
        stream_sequence,
        wrapped_transmit_key: None,
        extensions: &[],
        stream_id: 0xFFFF,
        hop_sequence: 0xDEAD_BEEF,
        payload: &payload,
        signature: &signature,
    })
    .expect("fixture frame must encode")
}

/// Send marked datagrams from every sender each round until `receiver` has
/// received one bearing each sender's marker; return the raw received datagram
/// per marker. Shared by both binding arms, so mh-0 and mh-1 are proven with the
/// SAME loop. Bounded by [`BINDING_DEADLINE`]; `phase` names the failure.
///
/// Ordering is send-until-received under the deadline: any arrival reflects a
/// post-declaration policy (the receiver's edges exist only after it declares) —
/// never a sleep, never a Prometheus gate.
///
/// `also_held` names markers of senders this receiver legitimately holds on the
/// same transport but that this call is not waiting for: frames from an earlier
/// phase can still be in flight, so they are tolerated and ignored — never
/// counted toward completion. A frame bearing ANY other marker is a sender the
/// receiver must not hear on this transport (e.g. a peer it shares no handler
/// with), and fails at once: a positive observation of a leak, never a timed
/// negative.
pub async fn bind_until_received(
    phase: &str,
    receiver: &wtransport::Connection,
    senders: &[(&wtransport::Connection, u8)],
    also_held: &[u8],
) -> HashMap<u8, bytes::Bytes> {
    bind_until_received_each(phase, receiver, senders, also_held, 1, |_, _| {}).await
}

/// [`bind_until_received`], but completing only once `per_sender` frames from
/// EACH sender have arrived, and handing EVERY such frame to `each(marker,
/// raw)` — so a caller can assert a property of every received frame (e.g.
/// that a sender's frames are never relayed onto another sender's slot) rather
/// than of the first one only. Returns the first raw datagram per marker.
pub async fn bind_until_received_each(
    phase: &str,
    receiver: &wtransport::Connection,
    senders: &[(&wtransport::Connection, u8)],
    also_held: &[u8],
    per_sender: usize,
    mut each: impl FnMut(u8, &bytes::Bytes),
) -> HashMap<u8, bytes::Bytes> {
    let mut seen: HashMap<u8, bytes::Bytes> = HashMap::new();
    let mut counts: HashMap<u8, usize> = HashMap::new();
    let deadline = Instant::now() + BINDING_DEADLINE;
    let mut seq = 0u32;
    while senders
        .iter()
        .any(|(_, m)| counts.get(m).copied().unwrap_or(0) < per_sender)
    {
        assert!(
            Instant::now() < deadline,
            "{phase}: within {BINDING_DEADLINE:?} the receiver did not receive from every sender \
             (per-marker counts: {:?}, {} each expected from {} senders). Check mh_media_frames_dropped_total{{reason}} \
             (no_subscriber = MC pushed no edge; no_local_subscriber = receiver not connected; \
             no_media_session = binding not yet made)",
            counts.iter().map(|(m, c)| (*m, *c)).collect::<BTreeMap<u8, usize>>(),
            per_sender,
            senders.len(),
        );
        seq += 1;
        for (conn, marker) in senders {
            // The send error is NOT discarded. A dropped `Err` here makes the
            // deadline message above ambiguous between "MH forwarded nothing"
            // and "the client never got a frame onto the wire" (datagram too
            // large for the negotiated size, unsupported by the peer, or the
            // session already closed) — and it would send triage to MH's drop
            // counters for a fault on this side of the link.
            conn.send_datagram(audio_datagram(seq, *marker))
                .unwrap_or_else(|e| {
                    panic!("{phase}: sender {marker:#04x} could not send its datagram: {e}")
                });
        }
        match tokio::time::timeout(Duration::from_millis(250), receiver.receive_datagram()).await {
            // Nothing this round: the expected case while the policy is still
            // being installed. Keep sending until the deadline.
            Err(_elapsed) => {}
            // The receiver's SESSION is gone — MH closed or reset it. Terminal
            // now rather than at the deadline: waiting cannot un-close a
            // connection, and reporting it as "did not receive from every
            // sender" would name the wrong subject.
            Ok(Err(e)) => panic!(
                "{phase}: the receiver's MH connection ended while waiting for forwarded \
                 datagrams: {e}. Check MH's logs for this session — a close here is MH \
                 refusing or dropping the connection, not a forwarding gap"
            ),
            Ok(Ok(d)) => {
                let payload = d.payload();
                let view = media_protocol::codec::decode_datagram(&payload)
                    .unwrap_or_else(|_| panic!("{phase}: receiver got a malformed datagram"));
                if let Some(&marker) = view.payload().first() {
                    if senders.iter().any(|(_, m)| *m == marker) {
                        each(marker, &payload);
                        *counts.entry(marker).or_insert(0) += 1;
                        seen.entry(marker).or_insert(payload.clone());
                    } else {
                        assert!(
                            also_held.contains(&marker),
                            "{phase}: the receiver got a frame from sender {marker:#04x}, which it \
                             must not hear on this transport"
                        );
                    }
                }
            }
        }
    }
    seen
}

/// Assert one received datagram `raw` was relayed onto `expected_slot` (read off
/// the RECEIVER's own `StreamAssignments`, never a literal), with the relay
/// region rewritten and the signed publisher region, payload and signature
/// relayed byte-identically (MH is keyless). `marker`/`who` name the sender.
pub fn assert_relayed_on_slot(
    phase: &str,
    raw: &bytes::Bytes,
    expected_slot: u32,
    marker: u8,
    who: &str,
) {
    let view = media_protocol::codec::decode_datagram(raw).expect("received datagram decodes");
    assert_eq!(
        u32::from(view.stream_id()),
        expected_slot,
        "{phase}: {who}'s frames must arrive on the slot the receiver's own StreamAssignments \
         named for {who}"
    );
    assert_ne!(
        view.hop_sequence(),
        0xDEAD_BEEF,
        "{phase}: MH writes its own hop sequence"
    );
    // Re-encode exactly what was sent for this sequence number.
    let sent_bytes = audio_datagram(view.stream_sequence(), marker);
    let sent = media_protocol::codec::decode_datagram(&sent_bytes).expect("fixture decodes");
    assert_eq!(
        view.publisher_region(),
        sent.publisher_region(),
        "{phase}: the signed publisher region is relayed byte-identically"
    );
    assert_eq!(
        view.payload(),
        sent.payload(),
        "{phase}: the payload is opaque to MH"
    );
    assert_eq!(
        view.signature(),
        sent.signature(),
        "{phase}: the signature is untouched"
    );
}

/// A marked datagram's `(marker, stream_sequence, relay slot)`.
///
/// Hoisted from `tests/26_mh_quic.rs` at story 2 task 12, when
/// `tests/35_mc_server_mute_teardown.rs` became its second consumer.
///
/// # Panics
///
/// Panics on a malformed datagram.
#[must_use]
pub fn read_frame(raw: &[u8]) -> (u8, u32, u32) {
    let view = media_protocol::codec::decode_datagram(raw)
        .unwrap_or_else(|_| panic!("received a malformed datagram"));
    (
        view.payload().first().copied().unwrap_or(0),
        view.stream_sequence(),
        u32::from(view.stream_id()),
    )
}

/// Drain whatever is queued on `conn` (non-blocking in effect: one short read
/// per call), handing every marked frame to `each` as `(marker, seq, slot)`.
///
/// Hoisted with [`read_frame`].
pub async fn drain(conn: &wtransport::Connection, mut each: impl FnMut(u8, u32, u32)) {
    while let Ok(Ok(d)) =
        tokio::time::timeout(Duration::from_millis(50), conn.receive_datagram()).await
    {
        let (marker, seq, slot) = read_frame(&d.payload());
        each(marker, seq, slot);
    }
}
