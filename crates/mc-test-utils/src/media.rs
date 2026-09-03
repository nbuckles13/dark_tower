//! Media-path test fixtures (ADR-0036).

/// A syntactically valid Ed25519 identity public key for tests.
///
/// **Not real key material and not a real curve point.** A fixed, obviously
/// synthetic byte pattern: MC's join path performs an exact-length check on an
/// opaque 32-byte blob and no curve validation, so a pattern is sufficient and
/// no keypair generation is needed.
///
/// One home for the value rather than eleven inline literals across the MC test
/// suite. env-tests keeps its own copy in `crates/env-tests/src/fixtures/media.rs`
/// — it must not take a dev-dependency on this crate, which depends on
/// `mc-service`, because linking the service into a black-box cluster-test suite
/// inverts the ADR-0028 layering.
///
/// Using this in a test asserts nothing about identity: MC performs no `cnf`
/// binding, so a key accepted at join yields same-keyholder consistency only,
/// never a verified identity.
pub fn sample_identity_public_key() -> Vec<u8> {
    vec![0x42; 32]
}

// ---------------------------------------------------------------------------
// Forwarding-policy fixtures (ADR-0036 §7/§8/§9)
// ---------------------------------------------------------------------------

/// The handler id MC test fixtures dial and expect echoed back.
///
/// One value for both halves of the §8 `handler_id` comparison, so a test
/// asserting `match` cannot pass because two independent literals happened to
/// agree, and a test asserting `handler_id_mismatch` has to differ from it
/// deliberately.
pub const TEST_HANDLER_ID: &str = "mh-test-0";

/// The N=1 loopback policy for one handler: one participant, one audio egress
/// stream, one candidate — themselves.
///
/// Produced by calling MC's real [`mc_service::media_routing::compute_assignment`]
/// rather than hand-building a `HandlerAssignment`. That matters: a hand-built
/// literal would let the fixture and the production computation drift, and every
/// test consuming it would keep passing against a shape MC no longer emits.
///
/// # Panics
///
/// Panics if the assignment cannot be computed or does not cover
/// [`TEST_HANDLER_ID`] — a fixture failure that must fail the test loudly.
#[must_use]
pub fn loopback_assignment() -> mc_service::media_routing::HandlerAssignment {
    loopback_assignment_for(TEST_HANDLER_ID, 1)
}

/// [`loopback_assignment`] with an explicit handler id and participant count,
/// for tests that need two distinguishable handlers or more than one subscriber.
///
/// Sender ids come from a real [`mc_service::media_admission::SenderIdAllocator`]
/// rather than a test-only constructor, so this fixture needs no `test-seams`
/// feature — which matters, because enabling that feature here would spread the
/// sender-id exhaustion bypass to every crate that links these fixtures.
///
/// # Panics
///
/// Panics if `participants` is 0, or if the assignment cannot be computed or
/// does not cover `handler_id`.
#[must_use]
pub fn loopback_assignment_for(
    handler_id: &str,
    participants: usize,
) -> mc_service::media_routing::HandlerAssignment {
    use mc_service::media_admission::SenderIdAllocator;
    use mc_service::media_routing::{
        compute_assignment, HandlerId, MeetingRoutingInput, RoutingParticipant,
    };

    assert!(participants > 0, "a meeting fixture needs a participant");

    let handler = HandlerId::new(handler_id);
    let mut allocator = SenderIdAllocator::new();
    let participants: Vec<RoutingParticipant> = (0..participants)
        .map(|_| RoutingParticipant {
            sender_id: allocator
                .allocate()
                .expect("fixture sender-id allocation")
                .sender_id,
            handlers: vec![handler.clone()],
        })
        .collect();

    compute_assignment(&MeetingRoutingInput {
        participants,
        handlers: vec![handler.clone()],
    })
    .expect("loopback assignment must compute")
    .for_handler(&handler)
    .cloned()
    .expect("loopback assignment must cover its own handler")
}
