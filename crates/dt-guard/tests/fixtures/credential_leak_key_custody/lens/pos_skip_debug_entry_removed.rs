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
//
// EXTRA CONSTRAINT ON THIS FIXTURE — READ BEFORE RUNNING IT.
//
// The real redaction control this simulates is the live line
//     .skip_debug("dark_tower.signaling.v1.MeetingKekUpdate")
// in crates/proto-gen/build.rs, on the real build path. This fixture is a
// STANDALONE CONSTRUCTION and does NOT edit that file. If a verification run
// ever needs the real file mutated, it is done against a scratch copy, or as an
// ephemeral mutation that is provably reverted with the reverted state asserted
// after the run — never as an in-tree edit. Unlike every other plant here, an
// in-tree version would DELETE A LIVE REDACTION CONTROL from a real file, and
// normalising that diff as "something the fixture does" is itself the hazard.
// build.rs is also an enumerated ADR-0024 §6.4 Guarded Shared Area.

/// A reconstruction of the generated-code configuration, with the redaction
/// entry for the KEK-bearing message REMOVED.
///
/// The real configuration retains `.skip_debug` for the KEK-bearing message;
/// this reconstruction drops it, which is the mutation item 13 names.
pub fn configure_prost() {
    let mut cfg = prost_build::Config::new();
    cfg.skip_debug("dark_tower.signaling.v1.MediaPolicyUpdate");
    // The removal: the KEK-bearing message no longer opts out of the derived
    // Debug, so its key field becomes printable.
    cfg.compile_protos(&["proto/dark_tower/signaling/v1/signaling.proto"], &["proto"])
        .unwrap();
}

// Invariant: FIRE — check item 13, which names "a `skip_debug` entry removed
// from crates/proto-gen/build.rs" as a finding verbatim.
// Expected verdict: FIRE.
