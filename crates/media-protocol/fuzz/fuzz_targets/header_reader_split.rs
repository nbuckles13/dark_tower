#![no_main]

//! Fuzzes the chunk-fed `HeaderReader` (R-21) with arbitrary chunk splits,
//! asserting the split **never changes the outcome**.
//!
//! Covers the READER, not only decode (ADR-0036 §2: the length-bound defect
//! passes fuzzing of the decode function while being wrong in the reader that
//! calls it). Input layout: the first byte seeds the split schedule, the rest
//! is the stream. The same stream is read three ways — one shot through
//! `parse_header_prefix`, byte-at-a-time, and in fuzzer-chosen chunks — and
//! the three outcomes (header, consumed total, leftover tail, or reject
//! reason) must be identical, and agree with `decode_stream_frame`.

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::{decode_stream_frame, parse_header_prefix, HeaderPrefix};
use media_protocol::frame::{FrameHeader, MAX_HEADER_BYTES};
use media_protocol::reader::{Feed, HeaderReader};

#[derive(Debug, PartialEq, Eq)]
enum Read {
    Complete(FrameHeader, usize, Vec<u8>),
    Rejected(&'static str),
    NeedMore,
}

fn by_reader(stream: &[u8], cuts: &mut dyn Iterator<Item = usize>) -> Read {
    let mut reader = HeaderReader::new();
    let mut at = 0usize;
    let mut consumed_total = 0usize;
    while at < stream.len() || at == 0 {
        let step = cuts.next().unwrap_or(1).max(1);
        let end = (at + step).min(stream.len());
        let chunk = &stream[at..end];
        match reader.feed(chunk) {
            Err(e) => return Read::Rejected(e.reason().as_str()),
            Ok(Feed::NeedMore) => consumed_total += chunk.len(),
            Ok(Feed::Complete { header, consumed }) => {
                consumed_total += consumed;
                assert_eq!(consumed_total, header.header_len());
                assert_eq!(reader.header_bytes(), Some(&stream[..header.header_len()]));
                return Read::Complete(header, consumed_total, stream[at + consumed..].to_vec());
            }
        }
        assert!(reader.buffered_len() <= MAX_HEADER_BYTES);
        if end == stream.len() {
            break;
        }
        at = end;
    }
    Read::NeedMore
}

fuzz_target!(|data: &[u8]| {
    let Some((&seed, stream)) = data.split_first() else {
        return;
    };
    let want = match parse_header_prefix(stream) {
        Err(e) => Read::Rejected(e.reason().as_str()),
        Ok(HeaderPrefix::Incomplete { .. }) => Read::NeedMore,
        Ok(HeaderPrefix::Complete(h)) => {
            Read::Complete(h, h.header_len(), stream[h.header_len()..].to_vec())
        }
    };

    // Byte-at-a-time.
    assert_eq!(by_reader(stream, &mut std::iter::repeat(1)), want);
    // Fuzzer-chosen chunk sizes, derived deterministically from the seed byte
    // and the stream itself so the schedule is arbitrary but reproducible.
    let mut sizes = stream
        .iter()
        .chain(std::iter::once(&seed))
        .cycle()
        .map(|&b| usize::from(b ^ seed) % 17 + 1);
    assert_eq!(by_reader(stream, &mut sizes), want);
    // One single chunk.
    assert_eq!(by_reader(stream, &mut std::iter::repeat(stream.len().max(1))), want);

    // Agreement with the stream decode on the reject reason / incompleteness.
    match (&want, decode_stream_frame(stream)) {
        (Read::Rejected(r), Err(e)) => assert_eq!(*r, e.reason().as_str()),
        (Read::Rejected(_), other) => panic!("reader rejected, stream decode said {other:?}"),
        (Read::NeedMore, Ok(None)) => {}
        (Read::NeedMore, other) => panic!("reader incomplete, stream decode said {other:?}"),
        (Read::Complete(h, _, _), Ok(Some(v))) => assert_eq!(v.encoded_len(), h.frame_len()),
        (Read::Complete(h, _, _), Ok(None)) => assert!(stream.len() < h.frame_len()),
        (Read::Complete(..), Err(e)) => panic!("header valid, stream decode rejected {e:?}"),
    }
});
