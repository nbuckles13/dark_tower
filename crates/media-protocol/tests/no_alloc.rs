//! Behavioural proof that the header parse and the chunk-fed reader allocate
//! nothing — in particular nothing sized from the attacker-controlled
//! `payload_length` (ADR-0036 §2: the trust boundary that passes fuzzing of a
//! whole-frame decode while being wrong in the reader).
//!
//! A counting `#[global_allocator]`, in its own test binary so it affects no
//! other test. libtest runs tests on parallel threads, so a process-global
//! count would pick up the harness's and other tests' allocations; the count
//! here is **thread-local and armed only inside a measured window** on the
//! measuring thread. The positive control runs inside the same window kind, so
//! a counter that was not installed (or never armed) fails the control rather
//! than passing every assertion vacuously.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

mod common;

use common::canonical_frame;
use media_protocol::codec::{
    parse_header_prefix, rewrite_relay_region_header, HeaderPrefix, RejectReason,
};
use media_protocol::frame::{
    MAX_PAYLOAD_BYTES, PAYLOAD_LENGTH_OFFSET, PUBLISHER_FIXED_PREFIX_SIZE,
};
use media_protocol::reader::{Feed, HeaderReader};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn note() {
    // `try_with`: the allocator can be entered during thread teardown, after
    // the thread-locals are gone.
    let _ = ARMED.try_with(|armed| {
        if armed.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

// SAFETY: delegates every operation to `System` unchanged; the only addition
// is a thread-local counter increment, which does not allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded verbatim to the system allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Allocations made on this thread while `f` runs.
fn allocations_during<R>(f: impl FnOnce() -> R) -> (usize, R) {
    COUNT.with(|c| c.set(0));
    ARMED.with(|a| a.set(true));
    let out = f();
    ARMED.with(|a| a.set(false));
    (COUNT.with(Cell::get), out)
}

fn with_declared_length(declared: u32) -> Vec<u8> {
    let mut frame = canonical_frame();
    frame[PAYLOAD_LENGTH_OFFSET..PAYLOAD_LENGTH_OFFSET + 4]
        .copy_from_slice(&declared.to_be_bytes());
    frame
}

#[test]
fn positive_control_the_counter_sees_an_allocation() {
    let (n, v) = allocations_during(|| std::hint::black_box(Vec::<u8>::with_capacity(4096)));
    drop(v);
    assert!(
        n >= 1,
        "the counting allocator is not installed or not armed"
    );
}

#[test]
fn over_max_and_u32_max_lengths_reject_without_allocating() {
    for declared in [u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap(), u32::MAX] {
        let frame = with_declared_length(declared);
        let mut reader = HeaderReader::new();
        let (n, (one_shot, fed)) = allocations_during(|| {
            let one_shot = parse_header_prefix(std::hint::black_box(&frame))
                .err()
                .map(|e| e.reason());
            let mut fed = None;
            for byte in frame.chunks(1) {
                if let Err(e) = reader.feed(byte) {
                    fed = Some(e.reason());
                    break;
                }
            }
            (one_shot, fed)
        });
        // The zero-alloc claim is about THIS rejection, made from the fixed
        // prefix alone — not some earlier, unrelated one.
        assert_eq!(one_shot, Some(RejectReason::PayloadLengthExceedsMax));
        assert_eq!(fed, Some(RejectReason::PayloadLengthExceedsMax));
        assert_eq!(reader.buffered_len(), PUBLISHER_FIXED_PREFIX_SIZE);
        assert_eq!(n, 0, "rejecting declared length {declared} allocated");
    }
}

#[test]
fn the_accept_path_does_not_allocate() {
    let frame = canonical_frame();
    let mut header_copy = [0u8; media_protocol::frame::MAX_HEADER_BYTES];
    let mut reader = HeaderReader::new();
    let (n, ok) = allocations_during(|| {
        let parsed = matches!(
            parse_header_prefix(std::hint::black_box(&frame)),
            Ok(HeaderPrefix::Complete(_))
        );
        let mut done = false;
        for byte in frame.chunks(3) {
            if let Ok(Feed::Complete { header, .. }) = reader.feed(byte) {
                let len = header.header_len();
                header_copy[..len].copy_from_slice(reader.header_bytes().unwrap_or(&[]));
                done = rewrite_relay_region_header(&mut header_copy[..len], 7, 8).is_ok();
                break;
            }
        }
        parsed && done
    });
    assert!(ok);
    assert_eq!(n, 0, "the header accept path allocated");
}
