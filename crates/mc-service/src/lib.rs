//! Meeting Controller (MC) Service Library
//!
//! This library provides the core functionality for the Dark Tower
//! Meeting Controller - a stateful WebTransport signaling server responsible for:
//!
//! - Real-time meeting coordination and participant state management
//! - Session binding token pattern for secure session recovery (ADR-0023)
//! - WebTransport connection handling for client signaling
//! - Receive-capability management (ADR-0036 §6: the client declares slots it
//!   can decode; MC composes the experience and assigns sources to them)
//! - Media Handler coordination for media routing
//! - Graceful shutdown with meeting migration
//!
//! # Architecture
//!
//! The MC uses an actor model hierarchy (ADR-0023 Section 2):
//!
//! ```text
//! MeetingControllerActor (singleton per MC instance)
//! ├── supervises N MeetingActors
//! │   └── MeetingActor (one per active meeting)
//! │       ├── owns meeting state
//! │       └── supervises N ParticipantActors
//! │           └── ParticipantActor (one per participant)
//! └── MhRegistryActor (tracks MH health via heartbeats)
//! ```
//!
//! # Key Design Decisions
//!
//! - **One connection per meeting**: A user in multiple meetings has multiple connections
//! - **Redis for state**: Live meeting state in Redis with sync writes for critical state
//! - **Fencing tokens**: Generation-based fencing prevents split-brain during failover
//! - **Session binding**: HMAC-SHA256 binding tokens with one-time nonces (30s TTL)
//!
//! # Modules
//!
//! - [`actors`] - Actor model implementation (Phase 6b)
//! - [`config`] - Service configuration from environment
//! - [`errors`] - Error types with appropriate error codes
//!
//! # Reference
//!
//! See ADR-0023 (Meeting Controller Architecture) for the full design specification.

// OPS-10 (ADR-0036 §11): the `test-seams` feature exposes
// `SenderIdAllocator::resuming_from`, which lets a caller seed the allocator
// cursor arbitrarily — that IS the bypass of the fail-closed exhaustion guard.
// "Non-default, not enabled in production" is a property nobody re-checks after
// the next Cargo.toml edit, so the seam fails to COMPILE rather than silently
// shipping the bypass. ADR-0036 §11 rules on exactly this shape for the dev-only
// per-frame tracing feature: "a control that has to notice fails silently for
// anyone building outside the pipeline; a compile error has nothing to notice."
//
// WHAT THIS GATE ACTUALLY COVERS, stated exactly — the predicate is
// `debug_assertions`, which is a PROXY for "this is a test-ish build", not a
// direct test of it. Read it as: **the seam is permitted if and only if
// `debug_assertions` is on.** That is well matched to the need, because test
// targets inherit `[profile.dev]`, where it is on; and it is what makes
// `cargo build --release` reject the seam today, since `[profile.release]` in
// the workspace manifest does not set `debug-assertions` and it therefore
// defaults to off.
//
// WHAT SUPPORTS THE PROXY: the premise "no shipped artifact turns
// `debug_assertions` on" is **checked** — never *concluded* — by
// `dt-guard release-build-profile`
// (`crates/dt-guard/src/release_build_profile.rs`, wired at
// `scripts/guards/simple/validate-release-build-profile.sh`). Cite it by
// `rule_id`, never by count: the guard DERIVES its channel count
// (`enumerated_channel_count()`) precisely so that hand-written encodings of the
// number cannot drift, and that drift has already happened once in this tree.
// The rules bearing on this premise are `dockerfile_build_not_release` and
// `dockerfile_cook_profile_mismatch` (the artifact is built `--release` at all —
// the most likely real failure, since a service Dockerfile that stops passing
// `--release` builds on the dev profile with `debug_assertions` ON and this
// `compile_error!` silently does not fire), `release_profile_debug_assertions`,
// `config_toml_release_profile` and `release_inheriting_profile` (no profile
// re-enables it), and `rustflags_debug_assertions` plus
// `cargo_profile_env_debug_assertions` (no environment channel re-enables it).
//
// CARRY BOTH CAVEATS. (1) **It is a CI check and does nothing for anyone
// building outside the pipeline** — a local `cargo build`, a dev container, a
// fork's CI. (2) **The enumeration is a vocabulary and is incomplete by
// construction**: the guard's own docs refuse the conclusion form, because its
// OK token says *"these enumerated inputs were checked and were clean"*, never
// *"the release premise holds"* — and those differ exactly at the N+1th channel.
// Its RUSTFLAGS scope is service Dockerfiles, `.github/workflows/*.yml` and
// `.cargo/config{,.toml}`; it does NOT scan `infra/devloop/`, `infra/skaffold.yaml`,
// `infra/kind/scripts/`, `scripts/` or `crates/devloop-helper` — which is what
// actually drives image builds on the devloop path. So the full and final
// statement is: *the predicate is `debug_assertions`; the premise that shipped
// artifacts have it off is checked in-pipeline over an enumerated, deliberately
// incomplete channel list; a green means those inputs were clean, not that the
// premise holds; and outside the pipeline nothing checks it at all.*
//
// RESIDUAL, named rather than left for someone to discover: a profile that
// turns `debug-assertions` back ON while optimising — `[profile.release]
// debug-assertions = true`, or a custom profile inheriting release with it set —
// satisfies neither branch and would compile the seam into an optimised binary.
// **No profile in this workspace does that today** (verified: `debug-assertions`
// appears in no `Cargo.toml` or `.cargo/config.toml` in the tree). The first
// spelling is covered by `release_profile_debug_assertions`. The second is
// covered by `release_inheriting_profile` **only when a service Dockerfile
// selects the profile by name** — `profile_reaches_release()` populates its
// candidate set exclusively from Dockerfile scans, so a custom
// `inherits = "release"` profile invoked from a CI workflow or a script is NOT
// covered. Two further conditions stand between that and a shipped bypass: the
// feature is non-default, and it is enabled only by the self dev-dependency, so
// reaching this state also requires explicitly passing `--features test-seams`.
//
// CONSEQUENCE OF THE SELF DEV-DEPENDENCY, so it is not rediscovered as a mystery
// build failure: dev-dependencies are active under `cargo test`, so while
// `cargo build [--release] -p mc-service` compiles none of the seam, both
// `cargo test --release -p mc-service` and `cargo bench` (whose profile inherits
// release) unify `test-seams` onto the lib with `debug_assertions` off and trip
// the `compile_error!` below. Nothing in the pipeline runs either today.
//
// A `build.rs` reading cargo's `PROFILE` was considered and **rejected**
// (@security, 2026-09-02): it duplicates a premise already guarded, changes the
// crate's build graph, and `build.rs` is an enumerated GSA path. The structural
// successor is named in that guard's own docs — `#[cfg(all(feature =
// "release-artifact", debug_assertions))] compile_error!` with service
// Dockerfiles passing `--features release-artifact`, which observes the
// effective cfg rather than its causes. Point at that, do not invent a parallel
// mechanism.
//
// Do not describe this gate as "release builds cannot enable the seam" —
// describe it as the `debug_assertions` predicate it is.
#[cfg(all(feature = "test-seams", not(debug_assertions)))]
compile_error!(
    "the `test-seams` feature exposes the sender-id exhaustion bypass and must never be enabled \
     in a release build"
);

pub mod actors;
pub mod auth;
pub mod config;
pub mod errors;
pub mod grpc;
pub mod media_admission;
pub mod mh_connection_registry;
pub mod observability;
pub mod redis;
pub mod system_info;
pub mod webtransport;
