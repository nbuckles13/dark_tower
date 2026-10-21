//! The cut-through header API (R-21): [`parse_header_prefix`], the chunk-fed
//! [`HeaderReader`] and [`rewrite_relay_region_header`].
//!
//! The deterministic counterpart of the `header_prefix` and
//! `header_reader_split` fuzz targets, which run only nightly. Four claims:
//!
//! 1. **Split invariance.** For every corpus seed and every way of splitting
//!    it — every single cut, and byte-at-a-time — the reader's outcome (the
//!    header, the bytes consumed, the leftover tail, or the reject reason)
//!    equals the one-shot parse. Full outcomes are compared, not `is_ok()`.
//! 2. **Coherence between entry points.** The one-shot parse, the reader and
//!    `decode_stream_frame` agree on every prefix of every seed: same reject
//!    reason, same total length, same incomplete-versus-terminal decision.
//! 3. **`needed` is exact and bounded.** Supplying one fewer byte than asked
//!    stays incomplete; the parse never asks past `MAX_HEADER_BYTES`.
//! 4. **The header-slice rewrite** touches only the relay bytes, writes
//!    nothing into an invalid header, and a rewritten header followed by the
//!    untouched tail decodes and keeps its signed ranges.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

mod common;

use common::seeds::{seeds, Outcome};
use common::{build, canonical_frame, set_byte, truncated_to, ALL_SHAPES, SENTINEL_PAYLOAD};
use media_protocol::codec::{
    decode_datagram, decode_stream_frame, parse_header_prefix, rewrite_relay_region_header,
    HeaderPrefix, RejectReason,
};
use media_protocol::frame::{
    FrameHeader, FLAGS_OFFSET, FLAG_DISCARDABLE, MAX_HEADER_BYTES, MAX_PAYLOAD_BYTES,
    PAYLOAD_LENGTH_OFFSET, PUBLISHER_FIXED_PREFIX_SIZE,
};
use media_protocol::reader::{Feed, HeaderReader};

/// A full, comparable outcome of reading one header.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Read {
    /// Header complete: the header, the total bytes consumed into it, and the
    /// bytes left over after it (the tail prefix).
    Complete(FrameHeader, usize, Vec<u8>),
    /// Terminal rejection.
    Rejected(RejectReason),
    /// All input consumed, header still incomplete.
    NeedMore,
}

/// The one-shot answer, in the same shape.
fn one_shot(bytes: &[u8]) -> Read {
    match parse_header_prefix(bytes) {
        Err(e) => Read::Rejected(e.reason()),
        Ok(HeaderPrefix::Incomplete { .. }) => Read::NeedMore,
        Ok(HeaderPrefix::Complete(h)) => {
            Read::Complete(h, h.header_len(), bytes[h.header_len()..].to_vec())
        }
    }
}

/// Drive a fresh reader over `chunks`.
fn by_reader(chunks: &[&[u8]]) -> Read {
    let mut reader = HeaderReader::new();
    let mut consumed_total = 0usize;
    for (i, chunk) in chunks.iter().enumerate() {
        match reader.feed(chunk) {
            Err(e) => return Read::Rejected(e.reason()),
            Ok(Feed::NeedMore) => consumed_total += chunk.len(),
            Ok(Feed::Complete { header, consumed }) => {
                consumed_total += consumed;
                let mut leftover = chunk[consumed..].to_vec();
                for rest in &chunks[i + 1..] {
                    leftover.extend_from_slice(rest);
                }
                assert_eq!(
                    reader.header_bytes().map(<[u8]>::len),
                    Some(header.header_len()),
                    "header_bytes must be exactly the header"
                );
                return Read::Complete(header, consumed_total, leftover);
            }
        }
    }
    Read::NeedMore
}

fn assert_seed_population() -> Vec<common::seeds::Seed> {
    let all = seeds();
    assert!(!all.is_empty(), "empty corpus would pass vacuously");
    assert!(
        all.iter().any(|s| s.outcome == Outcome::Accept),
        "at least one accepting seed must be iterated"
    );
    assert!(
        all.iter()
            .any(|s| matches!(s.outcome, Outcome::Reject(r) if r != RejectReason::Truncated)),
        "at least one terminally rejecting seed must be iterated"
    );
    all
}

// ---------------------------------------------------------------------------
// 1. Split invariance
// ---------------------------------------------------------------------------

#[test]
fn every_single_cut_of_every_seed_matches_the_one_shot_parse() {
    for seed in assert_seed_population() {
        let want = one_shot(&seed.bytes);
        for cut in 0..=seed.bytes.len() {
            let (a, b) = seed.bytes.split_at(cut);
            assert_eq!(
                by_reader(&[a, b]),
                want,
                "seed {} split at {cut} diverged from the one-shot parse",
                seed.name
            );
        }
    }
}

#[test]
fn byte_at_a_time_matches_the_one_shot_parse() {
    for seed in assert_seed_population() {
        let chunks: Vec<&[u8]> = seed.bytes.chunks(1).collect();
        assert_eq!(
            by_reader(&chunks),
            one_shot(&seed.bytes),
            "seed {} fed byte-at-a-time diverged",
            seed.name
        );
    }
}

#[test]
fn consumed_totals_equal_the_header_and_the_leftover_is_the_tail() {
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let view = decode_datagram(&frame).unwrap();
        for cut in 0..=frame.len() {
            let (a, b) = frame.split_at(cut);
            let Read::Complete(header, consumed, leftover) = by_reader(&[a, b]) else {
                panic!("a whole valid frame must complete its header");
            };
            assert_eq!(consumed, header.header_len());
            assert_eq!(header.header_len(), view.payload_range().start);
            assert_eq!(leftover.as_slice(), &frame[header.header_len()..]);
            assert_eq!(leftover.len(), header.tail_len());
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Coherence between entry points
// ---------------------------------------------------------------------------

#[test]
fn header_parse_reader_and_stream_decode_agree_on_every_prefix() {
    for seed in assert_seed_population() {
        for len in 0..=seed.bytes.len() {
            let data = &seed.bytes[..len];
            let parsed = parse_header_prefix(data);
            let stream = decode_stream_frame(data);
            let read = by_reader(&[data]);
            match parsed {
                Err(e) => {
                    assert_eq!(
                        stream
                            .as_ref()
                            .err()
                            .map(media_protocol::codec::DecodeError::reason),
                        Some(e.reason()),
                        "seed {} len {len}: stream decode disagrees on the reject",
                        seed.name
                    );
                    assert_eq!(read, Read::Rejected(e.reason()));
                }
                Ok(HeaderPrefix::Incomplete { .. }) => {
                    assert!(
                        matches!(stream, Ok(None)),
                        "seed {} len {len}: header incomplete but stream decode says {stream:?}",
                        seed.name
                    );
                    assert_eq!(read, Read::NeedMore);
                }
                Ok(HeaderPrefix::Complete(h)) => {
                    match stream {
                        Ok(Some(view)) => {
                            assert_eq!(view.encoded_len(), h.frame_len());
                            assert_eq!(view.flags(), h.flags());
                            assert_eq!(view.payload_length(), h.payload_length());
                            assert_eq!(view.stream_sequence(), h.stream_sequence());
                            assert_eq!(view.gop_counter(), h.gop_counter());
                            assert_eq!(view.stream_id(), h.stream_id());
                            assert_eq!(view.hop_sequence(), h.hop_sequence());
                            assert_eq!(view.payload_range().start, h.header_len());
                        }
                        Ok(None) => assert!(
                            len < h.frame_len(),
                            "stream decode incomplete on a buffer holding the whole frame"
                        ),
                        Err(e) => panic!(
                            "seed {} len {len}: header valid but stream decode rejected {e}",
                            seed.name
                        ),
                    }
                    assert!(matches!(read, Read::Complete(rh, _, _) if rh == h));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. `needed` is exact and bounded
// ---------------------------------------------------------------------------

#[test]
fn needed_is_exact_and_never_past_max_header_bytes() {
    let mut checked_boundaries = 0usize;
    for seed in assert_seed_population() {
        for len in 0..=seed.bytes.len() {
            let Ok(HeaderPrefix::Incomplete { needed }) = parse_header_prefix(&seed.bytes[..len])
            else {
                continue;
            };
            assert!(needed >= 1);
            assert!(
                len + needed <= MAX_HEADER_BYTES,
                "seed {} asked past MAX_HEADER_BYTES at len {len} (needed {needed})",
                seed.name
            );
            // One fewer byte than asked, at every step, stays incomplete and
            // asks for exactly the remainder.
            for d in 1..needed {
                if len + d > seed.bytes.len() {
                    break;
                }
                assert_eq!(
                    parse_header_prefix(&seed.bytes[..len + d]),
                    Ok(HeaderPrefix::Incomplete { needed: needed - d }),
                    "seed {} len {len}: needed is not exact",
                    seed.name
                );
            }
            // Exactly `needed` more lands ON a region boundary — in both
            // directions: an under-report would stop inside the region, an
            // over-report would skip past its end. The boundaries come from
            // the decoded artifact, not from the parser under test.
            if seed.outcome == Outcome::Accept {
                let view = decode_datagram(&seed.bytes).expect("accept seed decodes");
                let boundaries = [
                    1, // the version byte
                    PUBLISHER_FIXED_PREFIX_SIZE,
                    view.ext_length_field_range().start, // end of the wrapped key, if any
                    view.extensions_range().start,       // end of ext_length
                    view.extensions_range().end,         // end of the extensions
                    view.payload_range().start,          // end of the relay region
                ];
                assert!(
                    boundaries.contains(&(len + needed)),
                    "seed {} len {len}: len + needed = {} is not a region boundary {boundaries:?}",
                    seed.name,
                    len + needed
                );
                checked_boundaries += 1;
            }
        }
    }
    assert!(
        checked_boundaries > 0,
        "no accepting seed reached the boundary check"
    );
}

#[test]
fn an_over_max_length_rejects_from_the_fixed_prefix_alone() {
    for declared in [u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap(), u32::MAX] {
        let mut frame = canonical_frame();
        frame[PAYLOAD_LENGTH_OFFSET..PAYLOAD_LENGTH_OFFSET + 4]
            .copy_from_slice(&declared.to_be_bytes());
        let prefix = &frame[..PUBLISHER_FIXED_PREFIX_SIZE];
        assert_eq!(
            parse_header_prefix(prefix).err().map(|e| e.reason()),
            Some(RejectReason::PayloadLengthExceedsMax)
        );
        // Byte-at-a-time: the reader never asks for byte 15.
        let mut reader = HeaderReader::new();
        let mut outcome = Ok(Feed::NeedMore);
        for byte in prefix.chunks(1) {
            outcome = reader.feed(byte);
            if outcome.is_err() {
                break;
            }
        }
        assert_eq!(
            outcome.err().map(|e| e.reason()),
            Some(RejectReason::PayloadLengthExceedsMax)
        );
        assert_eq!(reader.buffered_len(), PUBLISHER_FIXED_PREFIX_SIZE);
    }
}

// ---------------------------------------------------------------------------
// Reader lifecycle
// ---------------------------------------------------------------------------

#[test]
fn errors_are_sticky_until_reset_and_reset_reads_a_fresh_frame() {
    let frame = canonical_frame();
    let bad = set_byte(&frame, FLAGS_OFFSET, frame[FLAGS_OFFSET] | FLAG_DISCARDABLE);
    let mut reader = HeaderReader::new();
    let first = reader.feed(&bad).unwrap_err().reason();
    assert_eq!(first, RejectReason::KeyBearingDiscardable);
    for _ in 0..3 {
        assert_eq!(reader.feed(&frame).unwrap_err().reason(), first);
        assert_eq!(reader.feed(&[]).unwrap_err().reason(), first);
    }
    assert!(!reader.is_mid_header());
    assert!(reader.header().is_none());
    reader.reset();
    assert_eq!(reader.buffered_len(), 0);
    let Ok(Feed::Complete { header, consumed }) = reader.feed(&frame) else {
        panic!("a reset reader must read a valid frame cleanly");
    };
    assert_eq!(consumed, header.header_len());
}

#[test]
fn complete_is_sticky_until_reset() {
    let frame = canonical_frame();
    let mut reader = HeaderReader::new();
    let Ok(Feed::Complete { header, .. }) = reader.feed(&frame) else {
        panic!("valid frame");
    };
    // The tail must never be mistaken for a new header.
    assert_eq!(
        reader.feed(&frame[header.header_len()..]),
        Ok(Feed::Complete {
            header,
            consumed: 0
        })
    );
    assert_eq!(reader.header(), Some(header));
    assert_eq!(reader.header_bytes(), Some(&frame[..header.header_len()]));
}

#[test]
fn is_mid_header_at_the_boundaries() {
    let frame = canonical_frame();
    let header_len = match parse_header_prefix(&frame).unwrap() {
        HeaderPrefix::Complete(h) => h.header_len(),
        HeaderPrefix::Incomplete { .. } => panic!("complete"),
    };
    let mut reader = HeaderReader::new();
    assert!(!reader.is_mid_header(), "0 bytes: a clean frame boundary");
    assert_eq!(reader.feed(&frame[..1]), Ok(Feed::NeedMore));
    assert!(reader.is_mid_header(), "1 byte");
    assert_eq!(reader.feed(&frame[1..header_len - 1]), Ok(Feed::NeedMore));
    assert!(reader.is_mid_header(), "header_len - 1 bytes");
    assert!(matches!(
        reader.feed(&frame[header_len - 1..]),
        Ok(Feed::Complete { consumed: 1, .. })
    ));
    assert!(!reader.is_mid_header(), "complete");
}

// ---------------------------------------------------------------------------
// 4. The header-slice relay rewrite
// ---------------------------------------------------------------------------

#[test]
fn header_rewrite_touches_only_relay_bytes_and_the_result_still_decodes() {
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let original = decode_datagram(&frame).unwrap();
        let header_len = original.payload_range().start;
        let relay = original.relay_region_range();

        let mut header = frame[..header_len].to_vec();
        let rewritten = rewrite_relay_region_header(&mut header, 0x0102, 0x0304_0506).unwrap();
        assert_eq!(rewritten.stream_id(), 0x0102);
        assert_eq!(rewritten.hop_sequence(), 0x0304_0506);
        for (i, (&a, &b)) in frame[..header_len].iter().zip(&header).enumerate() {
            if !relay.contains(&i) {
                assert_eq!(a, b, "{shape:?}: byte {i} outside the relay region changed");
            }
        }

        // Header plus the untouched tail is a valid frame with the same signed
        // input: the rewrite never needed the tail and never altered it.
        let mut reassembled = header.clone();
        reassembled.extend_from_slice(&frame[header_len..]);
        let view = decode_datagram(&reassembled).expect("rewritten frame decodes");
        assert_eq!(view.signed_ranges(), original.signed_ranges());
        assert_eq!(view.stream_id(), 0x0102);
        assert_eq!(view.hop_sequence(), 0x0304_0506);
    }
}

#[test]
fn header_rewrite_refuses_an_invalid_or_short_header_without_writing() {
    let frame = canonical_frame();
    let header_len = decode_datagram(&frame).unwrap().payload_range().start;

    let bad = set_byte(&frame, FLAGS_OFFSET, frame[FLAGS_OFFSET] | FLAG_DISCARDABLE);
    let mut bad_header = bad[..header_len].to_vec();
    let before = bad_header.clone();
    assert_eq!(
        rewrite_relay_region_header(&mut bad_header, 9, 9)
            .err()
            .map(|e| e.reason()),
        Some(RejectReason::KeyBearingDiscardable)
    );
    assert_eq!(bad_header, before);

    let mut short = truncated_to(&frame, header_len - 1);
    let before = short.clone();
    assert_eq!(
        rewrite_relay_region_header(&mut short, 9, 9)
            .err()
            .map(|e| e.reason()),
        Some(RejectReason::Truncated)
    );
    assert_eq!(short, before);
}
