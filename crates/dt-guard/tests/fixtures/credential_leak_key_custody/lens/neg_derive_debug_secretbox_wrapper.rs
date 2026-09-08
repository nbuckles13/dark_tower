// FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// A must-NOT-fire counterpart for the credential-leak semantic check
// (scripts/guards/semantic/checks.md, items 11-13). It carries key-adjacent
// vocabulary in shapes that are LEGITIMATE, so a check that name-matches rather
// than judging the value is caught reporting a finding here.
//
// Not a member of any cargo target; excluded from the Rust scanners twice over
// by common::test_code_filter::is_scan_exempt. See ../README.md.

use common::secret::SecretBox;

/// The in-tree redacting wrapper. Its own `Debug` prints REDACTED, never bytes.
#[derive(Debug)]
pub struct MeetingKek(SecretBox<[u8; 32]>);

/// Identical to pos_derive_debug_raw_kek_bytes.rs except for the field TYPE.
#[derive(Debug)]
pub struct MeetingAdmission {
    pub meeting_id: String,
    pub meeting_kek: MeetingKek,
}

// Invariant: CLEAR — must NOT fire.
//
// checks.md item 13: "A `#[derive(Debug)]` is SAFE when every field on the path
// to key bytes is a redacting secret wrapper whose own `Debug` redacts."
//
// THE PAIRING IS THE TEST. This file and pos_derive_debug_raw_kek_bytes.rs
// differ in exactly ONE thing — `MeetingKek` here versus `[u8; 32]` there — and
// the field is spelled `meeting_kek` in both. That is deliberate and was fixed
// in the poles for this reason: if the two differed in NAME as well as type, a
// check that merely name-matched would pass both poles while never exercising
// the wrapper logic at all. Do not "tidy" the names apart.
//
// checks.md also requires verifying the wrapper actually redacts rather than
// trusting a Secret-shaped name: `common::secret::SecretBox` is the in-tree one,
// and crates/mc-service/src/media_admission/kek.rs has a test asserting it.
// Expected verdict: CLEAR.
