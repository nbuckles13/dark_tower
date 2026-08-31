#![no_main]

//! Round-trip correctness for the version-2 frame codec.
//!
//! Asserts **byte identity**, not field equality. Encoding is canonical — one
//! fixed field order, extensions emitted in ascending type order — so
//! `encode(decode(x)) == x` must hold byte-for-byte for every `x` that decodes.
//!
//! That identity is why this target matters beyond "the fields survive": it
//! means decode is **injective** over the accepted language — no two distinct
//! byte strings decode to the same view — which is the no-covert-channel
//! property, established here independently of the byte-coverage test.
//! Weakening this assertion to field equality would silently give that up: a
//! codec that ignored some byte would still pass a field comparison.
//!
//! Note the deliberate absence: this target does **not** call any
//! "re-serialize the view" helper, because none exists. Re-encoding to obtain
//! bytes for verification is the defect the signed/AAD-range invariant
//! forbids — the ranges must be slices of the received buffer. Building
//! `MediaFrameParts` explicitly from public accessors here keeps that
//! distinction visible.

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::{decode_datagram, encode_frame, MediaFrameParts};
use media_protocol::extensions::Extension;

fuzz_target!(|data: &[u8]| {
    let Ok(view) = decode_datagram(data) else {
        return;
    };

    let extensions: Vec<Extension<'_>> =
        view.extensions().iter().map(|ext| Extension { ext_type: ext.ext_type, value: ext.value })
            .collect();

    let parts = MediaFrameParts {
        flags: view.flags(),
        stream_sequence: view.stream_sequence(),
        wrapped_transmit_key: view.wrapped_transmit_key(),
        extensions: &extensions,
        stream_id: view.stream_id(),
        hop_sequence: view.hop_sequence(),
        payload: view.payload(),
        signature: view.signature(),
    };

    let encoded = encode_frame(&parts).expect("a decoded frame must re-encode");
    assert_eq!(encoded.as_ref(), data, "re-encode was not byte-identical");

    let review = decode_datagram(&encoded).expect("a re-encoded frame must decode");
    assert_eq!(view, review, "re-decode produced a different view");
});
