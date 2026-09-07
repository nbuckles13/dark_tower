//! The print family inside the media path.
//!
//! `crates/mh-service/src/media/mod.rs` names `println!` / `eprintln!` /
//! `dbg!` explicitly. `print!` and `eprint!` are the same shape and the same
//! leak with one fewer character, so they are in the group too.

pub fn on_frame(frame: &[u8]) {
    print!("frame ");
    println!("len={}", frame.len());
    eprint!("drop ");
    eprintln!("len={}", frame.len());
    dbg!(frame.len());
}

// Invariant: five hits, all `media-telemetry-deny-macro-in-media-path`.
//
// `dbg!` earns its place: it is the cheapest one-token way to dump a whole
// frame to stderr via `Debug`, which is precisely §11's voice-activity-trace
// exposure. It also writes unconditionally — no level gate — which is §11's
// "a log level is not an acceptable gate" with the gate removed entirely.
