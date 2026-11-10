// DELIBERATE-LEAK FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// This file plants a key-custody leak on purpose so the credential-leak
// semantic check (scripts/guards/semantic/checks.md, items 11-13) can be shown
// to catch it. Every byte value here is synthetic — `vec![0xC3u8; 32]`, the
// in-tree convention from crates/proto-gen/tests/signaling_roundtrip.rs — and
// no value is derived from any real KDF, capture, or deployment.
//
// COPYING ANY LINE OF THIS FILE INTO PRODUCTION IS THE DEFECT THIS FILE EXISTS
// TO CATCH. It is not a template.
//
// Not a member of any cargo target: unreferenced .rs under tests/ is compiled
// by nothing, so fmt and clippy never see it. Excluded from the Rust scanners
// twice over by common::test_code_filter::is_scan_exempt (the /fixtures/ path
// segment, and crates/dt-guard/**). See ../README.md before adding a fixture.

use crate::actors::messages::{ParticipantMessage, SignalingPayload, RAW_SERVER_MESSAGE_TYPE};

/// Hands a freshly minted dial token to the participant actor as raw bytes.
pub async fn deliver_token(
    participant: &tokio::sync::mpsc::Sender<ParticipantMessage>,
    media_dial_token: String,
) {
    let _ = participant
        .send(ParticipantMessage::Send {
            message: SignalingPayload::Raw {
                message_type: RAW_SERVER_MESSAGE_TYPE,
                data: media_dial_token.into_bytes(),
            },
        })
        .await;
}

// Invariant: FIRE — check item 12, story-4 mailbox-hop rule (checks.md
// "Emission sites ... The MediaHandlerSet that carries tokens"): a minted
// `media_dial` token crosses the meeting-actor -> participant-actor mailbox as
// an untyped `SignalingPayload::Raw` `Vec<u8>`, whose derived `Debug` prints
// the ASCII JWS. A finding whether or not it is ever Debug-printed. The SAFE
// form is the typed `ParticipantMessage::HandlerSet` carrying the generated
// `MediaHandlerSet` (redacting `Debug`, `crates/proto-gen` `skip_debug`).
// Expected verdict: FIRE.
