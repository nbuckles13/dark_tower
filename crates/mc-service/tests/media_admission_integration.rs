//! Integration tests for MC's ADR-0036 admission groundwork: the meeting KEK,
//! the roster identity key, and `sender_id` allocation.
//!
//! # The security floor these tests assert, and the one they do NOT
//!
//! MC performs **no `cnf` thumbprint check** binding a presented identity key
//! to the meeting token — deferred to story 2. Every test here that accepts a
//! key is therefore asserting **trust on first use**: that MC recorded and
//! republished what the client sent. A frame signature later verifying against
//! a roster key proves only that all such frames came from **the same
//! keyholder** — *same-keyholder consistency*. None of these tests asserts, and
//! none may be read as asserting, a **verified identity**.
//!
//! Nothing here is named `attested`, `verified` or `trusted`, deliberately.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use mc_service::grpc::MhRegistrationClient;
use mc_service::media_admission::MEETING_KEK_BYTES;
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::jwt_test::make_meeting_claims;
use proto_gen::dark_tower::signaling::v1::{
    self, client_message, server_message, ClientMessage, JoinRequest, JoinResponse, ServerMessage,
};

use test_common::accept_loop_rig::AcceptLoopRig;
// Framing/connect helpers come from `tests/common/mod.rs` — the shared home for
// the 4-byte-length-prefix wire contract. Deliberately NOT redefined here.
use test_common::{
    build_test_stack, connect, encode_framed, read_server_message, sample_identity_public_key,
    seed_meeting_with_mh, TestStackHandles,
};

// ============================================================================
// Rig
// ============================================================================

struct Server {
    rig: AcceptLoopRig,
    stack: TestStackHandles,
}

impl Server {
    async fn start() -> Self {
        let stack = build_test_stack("media-admission-key").await;
        let rig = AcceptLoopRig::start_with(
            Arc::clone(&stack.controller_handle),
            Arc::clone(&stack.jwt_validator),
            Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
            Arc::clone(&stack.mh_reg_client) as Arc<dyn MhRegistrationClient>,
            "mc-test".to_string(),
            "http://mc-test:50052".to_string(),
            32,
        )
        .await;
        Self { rig, stack }
    }

    async fn create_meeting(&self, meeting_id: &str) {
        seed_meeting_with_mh(&self.stack, meeting_id).await;
    }

    /// Create a meeting whose `sender_id` cursor sits at the last issuable id,
    /// so the NEXT admission is the one that hits the wall.
    #[cfg(feature = "test-seams")]
    async fn create_meeting_with_exhausted_sender_ids(&self, meeting_id: &str) {
        self.stack
            .controller_handle
            // `None` cursor == the space is ALREADY exhausted, so the very
            // next admission is the reject. Seeding at 65535 instead would
            // succeed once first, which the allocator unit test already covers.
            .create_meeting_with_sender_id_cursor(meeting_id.to_string(), None)
            .await
            .expect("create meeting");
        // Handler data only — the meeting itself was just created above with
        // the seeded cursor, and `seed_meeting_with_mh` would try to create it
        // a second time.
        test_common::seed_mh_assignment_only(&self.stack, meeting_id);
    }

    fn token(&self, meeting_id: &str) -> String {
        self.stack
            .keypair
            .sign_token(&make_meeting_claims(meeting_id))
    }

    fn url(&self) -> String {
        self.rig.url.clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.rig.controller_handle.cancel();
    }
}

/// Join, holding the session OPEN so the participant stays on the roster.
async fn join_keep_open(
    url: &str,
    meeting_id: &str,
    token: &str,
    identity_public_key: Vec<u8>,
) -> (
    wtransport::Connection,
    wtransport::SendStream,
    wtransport::RecvStream,
    ServerMessage,
) {
    let conn = connect(url).await;
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .expect("open bi")
        .await
        .expect("bi ready");

    let msg = ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: token.to_string(),
            participant_name: "participant".to_string(),
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
            identity_public_key,
        })),
    };
    send.write_all(&encode_framed(&msg))
        .await
        .expect("write join");

    let response = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv))
        .await
        .expect("server responded");
    (conn, send, recv, response)
}

fn expect_join_response(msg: ServerMessage) -> JoinResponse {
    match msg.message {
        Some(server_message::Message::JoinResponse(r)) => r,
        other => panic!("expected JoinResponse, got {other:?}"),
    }
}

fn expect_error(msg: ServerMessage) -> v1::ErrorMessage {
    match msg.message {
        Some(server_message::Message::Error(e)) => e,
        other => panic!("expected ErrorMessage, got {other:?}"),
    }
}

// ============================================================================
// 1. KEK generated at meeting create, held in memory, returned in the response
// ============================================================================

/// The KEK is generated when the meeting actor is created and handed to every
/// participant MC admits (ADR-0036 §4 step 3).
///
/// Asserts the *properties* that matter — exact AES-256 length, not all-zero,
/// and distinct per meeting — rather than a fixed value. There is deliberately
/// no seeded-RNG test seam: a deterministic KEK in any build configuration
/// would defeat every confidentiality property §4 claims.
#[tokio::test]
async fn kek_is_generated_at_meeting_create_and_returned_in_join_response() {
    let server = Server::start().await;
    server.create_meeting("kek-meeting-a").await;
    server.create_meeting("kek-meeting-b").await;

    let (_c1, _s1, _r1, msg_a) = join_keep_open(
        &server.url(),
        "kek-meeting-a",
        &server.token("kek-meeting-a"),
        sample_identity_public_key(),
    )
    .await;
    let a = expect_join_response(msg_a);

    assert_eq!(
        a.meeting_kek.len(),
        MEETING_KEK_BYTES,
        "the KEK must be exactly AES-256; consumers fail closed on any other length"
    );
    assert!(
        a.meeting_kek.iter().any(|&b| b != 0),
        "an all-zero KEK is never a usable key"
    );
    assert_eq!(
        a.kek_generation, 0,
        "first generation is 0, and 0 is legal rather than a not-provisioned sentinel"
    );

    let (_c2, _s2, _r2, msg_b) = join_keep_open(
        &server.url(),
        "kek-meeting-b",
        &server.token("kek-meeting-b"),
        sample_identity_public_key(),
    )
    .await;
    let b = expect_join_response(msg_b);

    assert_ne!(
        a.meeting_kek, b.meeting_kek,
        "the KEK is per-meeting and randomly generated, never derived from a shared secret — \
         a master-secret-derived design would let one compromise reach backward across meetings"
    );
}

/// The KEK-issuance counter fires once per meeting-actor creation and carries
/// `key_custody=operator`.
///
/// Asserts the LABEL as much as the count: `key_custody` is a constraint, not a
/// snapshot, and it exists in place of an end-to-end or zero-trust boolean —
/// which no metric, log, dashboard or document may carry, because the default
/// deployment is neither (ADR-0036 §4, §11).
#[tokio::test(flavor = "current_thread")]
async fn meeting_kek_generation_is_counted_with_operator_key_custody() {
    let snap = MetricAssertion::snapshot();
    let server = Server::start().await;
    server.create_meeting("kek-metric-meeting").await;

    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("key_custody", "operator")])
        .assert_delta(1);
}

// ============================================================================
// 2. Malformed identity key rejected, with a generic error
// ============================================================================

/// Every malformed key is refused, and refused *identically*.
///
/// The byte-equality assertion across all four cases IS the non-oracle check:
/// it is stronger than asserting the message omits particular words, because it
/// fails on any future divergence rather than on a word list.
///
/// "Malformed" means **any length other than 0 or 32**. Length 0 is a defined
/// state, not a malformation — see the absent-key test below.
#[tokio::test]
async fn malformed_identity_key_is_rejected_with_a_generic_error() {
    let server = Server::start().await;
    server.create_meeting("malformed-key-meeting").await;

    // NOTE: the empty key is deliberately NOT in this table. Length 0 is the
    // contract's NO KEY PUBLISHED state and is ADMITTED — covered by
    // `absent_identity_key_is_admitted_and_publishes_an_empty_roster_entry`.
    // Only lengths other than 0 and 32 are malformed.
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("one byte", vec![0x42; 1]),
        ("one byte short", vec![0x42; MEETING_KEK_BYTES - 1]),
        ("one byte long", vec![0x42; MEETING_KEK_BYTES + 1]),
        ("oversized", vec![0x42; MEETING_KEK_BYTES * 2]),
    ];

    let mut messages = Vec::new();
    for (label, key) in cases {
        let (_conn, _send, _recv, msg) = join_keep_open(
            &server.url(),
            "malformed-key-meeting",
            &server.token("malformed-key-meeting"),
            key,
        )
        .await;
        let err = expect_error(msg);
        assert_eq!(
            err.code,
            v1::ErrorCode::InvalidRequest as i32,
            "{label}: a malformed key is a client error"
        );
        messages.push((label, err.message));
    }

    let first = &messages[0].1;
    for (label, message) in &messages {
        assert_eq!(
            message, first,
            "{label}: every rejection cause must produce a BYTE-IDENTICAL message. A caller must \
             not be able to distinguish absent from short from long — that distinction is an \
             oracle for which validation failed."
        );
    }
}

// ============================================================================
// 2b. Absent key is ADMITTED and publishes an EMPTY roster entry
// ============================================================================

/// Length 0 is the contract's NO KEY PUBLISHED state: the joiner is **admitted**
/// and its roster entry carries an **empty** key.
///
/// # What this test protects, and it is not the happy path
///
/// The assertion that matters is `len() == 0`, **not** `is_empty()` framing and
/// emphatically not "the join succeeded". The failure mode it catches is a
/// zero-filled 32-byte placeholder reaching the wire — an `unwrap_or_default()`
/// or `[0u8; 32]` anywhere on the `Option<IdentityPublicKey>` → bytes path.
/// That is **strictly worse than empty**: a consumer sees a populated key, takes
/// the verify path, and never receives the "no key published" signal the
/// contract defines. It would fail *closed* (all-zeros is not a valid Ed25519
/// point), so no test of verification outcome would catch it — it destroys the
/// distinction accept-absent rests on, silently. Hence a length assertion, and
/// an explicit not-32 assertion alongside it.
///
/// # What is NOT asserted here
///
/// That anything downstream fails closed on this entry. MC's job ends at
/// publishing the empty state honestly; the obligation to DROP frames from a
/// keyless participant — never "skip verification", which fails open — is the
/// consumer's, is owned by client, and is tracked in `docs/TODO.md` with a
/// trigger. Nothing in MC can enforce it, and this test must not be read as
/// evidence that it holds.
#[tokio::test(flavor = "current_thread")]
async fn absent_identity_key_is_admitted_and_publishes_an_empty_roster_entry() {
    let snap = MetricAssertion::snapshot();
    let server = Server::start().await;
    server.create_meeting("absent-key-meeting").await;

    // First participant publishes NO key.
    let (_c1, _s1, _r1, first_msg) = join_keep_open(
        &server.url(),
        "absent-key-meeting",
        &server.token("absent-key-meeting"),
        Vec::new(),
    )
    .await;
    let first = expect_join_response(first_msg);
    assert!(
        !first.participant_id.is_empty(),
        "a joiner that publishes no identity key is ADMITTED — length 0 is a defined state, and \
         rejecting it would exclude only honest clients that have not implemented the field yet"
    );
    assert_eq!(
        first.meeting_kek.len(),
        MEETING_KEK_BYTES,
        "an admitted keyless participant still receives the KEK — admission IS the authorization"
    );

    // Second participant sees the first on its roster, keyless.
    let (_c2, _s2, _r2, second_msg) = join_keep_open(
        &server.url(),
        "absent-key-meeting",
        &server.token("absent-key-meeting"),
        sample_identity_public_key(),
    )
    .await;
    let second = expect_join_response(second_msg);
    let entry = second
        .existing_participants
        .iter()
        .find(|p| p.participant_id == first.participant_id)
        .expect("the keyless participant IS on the roster, not silently omitted");

    assert_eq!(
        entry.identity_public_key.len(),
        0,
        "a keyless roster entry must be EMPTY BYTES. A zero-filled 32-byte placeholder would read \
         as present, send a consumer down the verify path, and destroy the no-key-published \
         signal — silently, because it still fails closed"
    );
    assert_ne!(
        entry.identity_public_key.len(),
        32,
        "explicitly not a 32-byte all-zero array — the placeholder this design must never emit"
    );
    assert!(
        entry.sender_id.is_some(),
        "a keyless participant still gets a sender_id: the key-id namespace is independent of \
         whether attribution is possible"
    );

    // The absent case is observable, both arms, so the ratio is computable.
    snap.counter("mc_join_identity_key_presence_total")
        .with_labels(&[("presence", "absent")])
        .assert_delta(1);
    snap.counter("mc_join_identity_key_presence_total")
        .with_labels(&[("presence", "present")])
        .assert_delta(1);
}

// ============================================================================
// 2c. Identity-key validation runs AFTER authentication
// ============================================================================

/// A malformed identity key presented with an INVALID token reports
/// `jwt_validation`, never `identity_key_invalid`.
///
/// Ordering is a security property, not a style preference. An earlier revision
/// parsed the identity key *before* `validate_meeting_token`, which let an
/// unauthenticated caller probe a validation surface and get a distinguishable
/// response without presenting a valid token, and made a bad-token join surface
/// as an input-validation error — hiding an auth failure behind it. Authenticate
/// first, then validate input.
#[tokio::test(flavor = "current_thread")]
async fn identity_key_validation_runs_after_authentication() {
    let snap = MetricAssertion::snapshot();
    let server = Server::start().await;
    server.create_meeting("order-meeting").await;

    // Both wrong at once: a garbage token AND a malformed key.
    let (_conn, _send, _recv, msg) = join_keep_open(
        &server.url(),
        "order-meeting",
        "not-a-valid-jwt",
        vec![0x42; 7],
    )
    .await;

    let err = expect_error(msg);
    assert_eq!(
        err.code,
        v1::ErrorCode::Unauthorized as i32,
        "auth failure must win: the identity-key check runs only after the token is validated"
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "jwt_validation")])
        .assert_delta(1);
    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "identity_key_invalid")])
        .assert_delta(0);
}

// ============================================================================
// 3. sender_id is monotonic and never recycled across a leave and a later join
// ============================================================================

/// Invariant R-35: a key id is never reused under one KEK, so a departed
/// participant's `sender_id` is never reissued.
#[tokio::test]
async fn sender_id_is_not_recycled_across_a_leave_and_a_later_join() {
    let server = Server::start().await;
    server.create_meeting("recycle-meeting").await;

    let first_id = {
        let (conn, _s, _r, msg) = join_keep_open(
            &server.url(),
            "recycle-meeting",
            &server.token("recycle-meeting"),
            sample_identity_public_key(),
        )
        .await;
        let id = expect_join_response(msg)
            .sender_id
            .expect("sender_id assigned");
        // Clean close: post-task-#64 this removes the participant from the
        // roster immediately rather than lingering through the grace period.
        drop(conn);
        id
    };
    assert_ne!(first_id, 0, "0 is reserved-invalid and never allocated");

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (_conn, _s, _r, msg) = join_keep_open(
        &server.url(),
        "recycle-meeting",
        &server.token("recycle-meeting"),
        sample_identity_public_key(),
    )
    .await;
    let second = expect_join_response(msg);
    let second_id = second.sender_id.expect("sender_id assigned");

    assert_ne!(
        second_id, first_id,
        "a departed participant's sender_id must NEVER be reissued to a later joiner: reuse under \
         one KEK collides two senders on one key id, and since the wrap nonce derives from the \
         key id, on one AES-GCM nonce — authentication-key recovery, not merely a \
         confidentiality loss"
    );
    assert!(
        second_id > first_id,
        "allocation is monotonic, so the namespace is consumed by cumulative lifetime admissions \
         rather than by concurrent participants"
    );
    assert!(
        second
            .existing_participants
            .iter()
            .all(|p| p.sender_id != Some(first_id)),
        "the departed participant is off the roster, and its id is retired with it"
    );
}

// ============================================================================
// 6. The roster carries both attribution fields
// ============================================================================

/// A receiver resolves an arriving frame's key id to `sender_id`, `sender_id`
/// to a roster entry, and that entry to `identity_public_key` (ADR-0036 §2,
/// §3). This asserts the roster half of that chain is present and correct.
///
/// It asserts **same-keyholder consistency only**: MC republished the bytes the
/// client sent. With no `cnf` binding this is trust on first use and says
/// nothing about *whose* key it is.
#[tokio::test]
async fn roster_carries_the_joiner_identity_public_key_and_sender_id() {
    let server = Server::start().await;
    server.create_meeting("roster-meeting").await;

    let first_key = sample_identity_public_key();
    let (_c1, _s1, _r1, first_msg) = join_keep_open(
        &server.url(),
        "roster-meeting",
        &server.token("roster-meeting"),
        first_key.clone(),
    )
    .await;
    let first = expect_join_response(first_msg);
    let first_sender_id = first.sender_id.expect("sender_id assigned");
    assert!(
        first.existing_participants.is_empty(),
        "first joiner sees an empty roster"
    );

    let (_c2, _s2, _r2, second_msg) = join_keep_open(
        &server.url(),
        "roster-meeting",
        &server.token("roster-meeting"),
        sample_identity_public_key(),
    )
    .await;
    let second = expect_join_response(second_msg);

    let entry = second
        .existing_participants
        .iter()
        .find(|p| p.participant_id == first.participant_id)
        .expect("the first participant is on the second joiner's roster");

    assert_eq!(
        entry.identity_public_key, first_key,
        "the roster republishes the key the client presented, byte for byte"
    );
    assert_eq!(
        entry.sender_id,
        Some(first_sender_id),
        "the roster's sender_id matches what its owner was told, or the key-id lookup resolves \
         to the wrong participant's key and attribution silently corrupts"
    );
    assert_eq!(
        entry.identity_public_key.len(),
        32,
        "a roster key is always exactly 32 bytes; MC refuses the join otherwise, so a keyless \
         roster entry is structurally unproducible"
    );
}

// ============================================================================
// 5. Exhaustion rejects the admission
// ============================================================================

/// At the wall MC refuses the admission rather than allocating.
///
/// A warn-and-continue that still wrapped would silently reissue a live
/// `sender_id` and produce exactly the key-id collision R-35 exists to prevent.
///
/// Uses the `test-seams` cursor seed rather than performing 65535 joins. The
/// seam is compiled only under a non-default feature and a release build with it
/// enabled fails to compile.
#[cfg(feature = "test-seams")]
#[tokio::test(flavor = "current_thread")]
async fn sender_id_exhaustion_rejects_the_admission() {
    let snap = MetricAssertion::snapshot();
    let server = Server::start().await;
    server
        .create_meeting_with_exhausted_sender_ids("exhausted-meeting")
        .await;

    let (_conn, _send, _recv, msg) = join_keep_open(
        &server.url(),
        "exhausted-meeting",
        &server.token("exhausted-meeting"),
        sample_identity_public_key(),
    )
    .await;

    let err = expect_error(msg);
    assert_eq!(
        err.code,
        v1::ErrorCode::CapacityExceeded as i32,
        "exhaustion is refused, never wrapped onto a live id"
    );
    assert_eq!(
        err.message, "Meeting is at capacity",
        "byte-identical to an ordinary capacity refusal: a client must not be able to \
         distinguish namespace exhaustion from a participant cap"
    );

    // `record_session_join` fires AFTER the error frame is written, so reading
    // the response does not guarantee the metric has landed. Bounded window on
    // `current_thread` (the same pattern as
    // `webtransport_accept_loop_integration.rs`), not a race-y bare assert.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Pinned to the FULL label set, not a broader match, so a sibling test
    // incrementing this counter under `identity_key_invalid` cannot perturb it.
    snap.counter("mc_session_join_failures_total")
        .with_labels(&[("error_type", "sender_id_space_exhausted")])
        .assert_delta(1);
}
