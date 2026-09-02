//! Test utilities for the Dark Tower Media Handler.
//!
//! # What is here
//!
//! [`transport_shim`] — a deterministic test double for `mh_service::transport`'s
//! per-connection I/O seam (ADR-0036 §10). It injects datagram loss in either
//! direction, added path delay, and send-capacity back-pressure, and it drives
//! the forward path with **zero syscalls**.
//!
//! # This crate is different in kind from its three siblings, and the
//! difference is why its dependency edge matters
//!
//! `ac-test-utils`, `gc-test-utils` and `mc-test-utils` are assertion helpers
//! and mocks of *external* services. None of them implements a production
//! trait that production code is generic over.
//!
//! This one does. [`transport_shim::LossDelayTransport`] is a drop-in for a
//! production hot-path type — a lossy, delaying, capacity-refusing
//! implementation of `MediaTransport`. Substituting it for the real transport
//! would not fail to compile; it would silently degrade media.
//!
//! # The control on that, stated at its true strength and no higher
//!
//! A separate crate does **not** make substitution impossible. What is true:
//!
//! * While the edge stays in `[dev-dependencies]`, the compiler forbids
//!   production code in `mh-service` from importing `mh_test_utils` at all.
//!   Substituting the shim therefore requires a visible one-line manifest move
//!   out of `[dev-dependencies]` — a reviewable source edit, not a build-flag
//!   flip. That is strictly stronger than a cargo feature, where
//!   `--all-features` re-enables the capability with no source edit at all.
//! * It is one line away, not impossible. Say so rather than claiming a
//!   control the arrangement does not provide.
//!
//! Three further structural barriers sit on the release-artifact path, so the
//! caveat above does not understate the control either. Traced against
//! `infra/docker/mh-service/Dockerfile`: the builder stage runs
//! `cargo chef cook --release … --package mh-service` (line 53) and
//! `cargo build --release --package mh-service` (line 59) — both
//! **package-scoped, not `--workspace`**; cargo does not build
//! dev-dependencies for a non-test profile; and the distroless stage copies
//! only the stripped `/build/target/release/mh-service` binary (lines 78 and
//! 135). Adding this crate to `[workspace] members` therefore does not put it
//! in the release image even incidentally. **Substitution needs the manifest
//! move plus a production `use mh_test_utils::…` — two visible source edits in
//! the same diff, not one.**
//!
//! ANCHOR (DRY): the release-profile half of the argument above — that the
//! builder stage builds `--release` — is enforced by `dt-guard
//! release-build-profile` (`crates/dt-guard/src/release_build_profile.rs`).
//! Cross-reference only; the claims in this comment are unchanged.
//!
//! A mechanical guard on that edge is tracked in `docs/TODO.md` §Test Debt.
//! Its second class member is already in the tree: wtransport's
//! `dangerous-configuration` (certificate-validation bypass) is dev-scoped in
//! both `crates/mh-service/Cargo.toml` and `crates/mc-service/Cargo.toml` — a
//! nearer analogue than the three test-utils crates, because like the shim it
//! is a *capability* rather than a helper.
//!
//! # Why `forbid(unsafe_code)` and not merely `deny`
//!
//! This crate opts out of workspace lints (see `Cargo.toml`), and
//! `unsafe_code` is not in `[workspace.lints.rust]` either, so opting out
//! would otherwise leave it with no `unsafe` bar at all. A test double whose
//! whole job is to certify a *memory* property — pointer identity on a
//! no-copy hand-back — is the likeliest place in the tree for someone to reach
//! for `unsafe` to make a zero-copy assertion pass. `forbid` makes the shim
//! structurally unable to fake the property it exists to demonstrate.

#![forbid(unsafe_code)]

pub mod media_policy;
pub mod transport_shim;
