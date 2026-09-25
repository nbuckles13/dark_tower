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

/// The two-party policy for one handler: two participants who hear each
/// other, each declaring one audio slot, each slot pinned to the other.
///
/// Produced by calling MC's real [`mc_service::media_routing::SlotTable`]
/// render rather than hand-building a `HandlerAssignment`. That matters: a
/// hand-built literal would let the fixture and the production computation
/// drift, and every test consuming it would keep passing against a shape MC no
/// longer emits. (Story 1's single-participant loopback fixture is gone with
/// loopback itself — story 2 R-3.)
///
/// # Panics
///
/// Panics if the assignment cannot be computed or does not cover
/// [`TEST_HANDLER_ID`] — a fixture failure that must fail the test loudly.
#[must_use]
pub fn two_party_assignment() -> mc_service::media_routing::HandlerAssignment {
    meeting_assignment_for(TEST_HANDLER_ID, 2)
}

/// A full-mesh policy for `participants` participants on one handler: each
/// declares `participants - 1` audio slots, so everyone hears everyone.
///
/// Sender ids come from a real [`mc_service::media_admission::SenderIdAllocator`]
/// rather than a test-only constructor, so this fixture needs no `test-seams`
/// feature — which matters, because enabling that feature here would spread the
/// sender-id exhaustion bypass to every crate that links these fixtures.
///
/// Everyone is connected to the one handler through the production
/// connectivity path (`ConnectedHandlers` built from a resolved endpoint), so
/// the fixture exercises the real edge rule rather than bypassing it.
///
/// # Panics
///
/// Panics if the render fails or does not cover `handler_id`. A single
/// participant yields an EMPTY policy (a solo participant hears nothing).
#[must_use]
pub fn meeting_assignment_for(
    handler_id: &str,
    participants: usize,
) -> mc_service::media_routing::HandlerAssignment {
    use mc_service::media_admission::SenderIdAllocator;
    use mc_service::media_routing::{
        ConnectedHandlers, HandlerEndpoint, HandlerId, MeetingHandlers, SlotTable,
    };

    let handler = HandlerId::new(handler_id);
    let handlers = MeetingHandlers::new([HandlerEndpoint {
        id: handler.clone(),
        webtransport_url: format!("https://{handler_id}.fixture:4434"),
        grpc_endpoint: format!("http://{handler_id}.fixture:50053"),
    }])
    .expect("fixture handler set");
    let mut connected = ConnectedHandlers::new();
    connected.insert(
        handlers
            .resolve(handler_id)
            .expect("fixture handler resolves"),
    );
    let mut allocator = SenderIdAllocator::new();
    // The production constructor (the seed-0 `SlotTable::new` is `#[cfg(test)]`
    // and unreachable from here). With ONE fixture handler the per-meeting
    // tiebreak rotation has nothing to choose, so the id is arbitrary.
    let mut table = SlotTable::for_meeting("mc-test-utils-fixture");
    let senders: Vec<_> = (0..participants)
        .map(|_| {
            let sender = allocator
                .allocate()
                .expect("fixture sender-id allocation")
                .sender_id;
            table.admit(sender).expect("fixture admission");
            sender
        })
        .collect();
    let slots: Vec<u16> = (0..participants.saturating_sub(1))
        .map(|i| u16::try_from(i).expect("fixture slot id"))
        .collect();
    for sender in &senders {
        table.set_demand(*sender, slots.clone());
    }
    for sender in senders {
        table.set_connectivity(sender, Some(connected.clone()));
    }

    table
        .render([&handler])
        .expect("fixture assignment must render")
        .for_handler(&handler)
        .cloned()
        .expect("fixture assignment must cover its own handler")
}
