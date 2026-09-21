#!/usr/bin/env bash
# Rust compile (per @team-lead 2026-05-19, ADR-0034 Bundle 1):
#   1. Workspace debug build — catches link-time errors `cargo check` misses.
#   2. dt-guard release build — produces `target/release/dt-guard` for the
#      ADR §3 wrappers at `scripts/guards/simple/*.sh` which invoke
#      `${DT_GUARD:-$REPO_ROOT/target/release/dt-guard}`.
#   3. dt-story release build — produces `target/release/dt-story` for the
#      Layer-3 `validate-story-manifest.sh` guard (story-runner manifest
#      schema check). Same build-once-before-guards pattern as dt-guard.
#   4. ADR-0036 §11 release-build feature gate — a NEGATIVE compile assertion.
#      Not a build; see the block above its invocation for why it lives here.
# Release is incremental on top of debug (~5-15s cold, ~0s warm with sccache
# per ADR-0034 §Negative).
set -euo pipefail
IFS=$'\n\t'
source "$(dirname "${BASH_SOURCE[0]}")/../_common.sh"
install_wrapper_exit_trap  # task #50: emit STATUS=FAIL if we abort before emitting
run_and_emit "cargo-build" cargo build --workspace "${CARGO_LOCKED[@]}" --quiet "$@"
run_and_emit "cargo-build-dt-guard" cargo build --release -p dt-guard "${CARGO_LOCKED[@]}" --quiet "$@"
run_and_emit "cargo-build-dt-story" cargo build --release -p dt-story "${CARGO_LOCKED[@]}" --quiet "$@"

# ADR-0036 §11 release-build feature gate — asserts mh-service's `per-frame-trace`
# `compile_error!` (crates/mh-service/src/lib.rs) BOTH fires under the release profile with
# the feature on AND stays clean with it off. A negative compile assertion, not a build.
#
# WHY LAYER 1 AND NOT scripts/layer3.sh, where the eleven *.test.sh self-tests live.
# "There is no *.test.sh auto-runner, so an unwired self-test never runs" is true, but it
# picks the wiring MECHANISM (an explicit invocation) — not the layer. The layer is decided
# by ADR-0033 §3's amendment clause, which asks of any new validation tool "whether its cost
# belongs in the guard+audit fast-tier budget (like layers 3/6) or outside it (like the
# language layers and Layer 7)". §4 answers it: the language layers are excluded because
# they carry "inherently large/variable cost". A cargo invocation is variable cost by
# construction, so it belongs out here regardless of how cheap it measures warm.
#
# The warm number is what makes Layer 3 look safe, and it is the wrong number. Warm this is
# ~1s (measured: clean arm 0s, fire arm 1s at rc 101, alternating 0s). On a Cargo.lock
# change — i.e. a Swatinem/rust-cache key miss in CI — it is a COLD release-profile build of
# mh-service's graph (quinn/rustls/h3/prost/tokio), which nothing upstream warms: the two
# --release builds above are dt-guard/dt-story and share almost none of it. In Layer 3 that
# would make layer-all.sh's `WARN BUDGET_TOTAL_BREACH` fire for "Cargo.lock changed" rather
# than "the fast floor regressed" — a dead token. ADR-0033 §4 uses that exact reasoning to
# exclude Layer 7 from the per-layer warn; adopting the structure with the opposite
# conclusion would be incoherent.
#
# Layer 3 invoking cargo ZERO times is structural, not incidental. Layer 1 PRODUCES
# target/release/{dt-guard,dt-story}; Layer 3 CONSUMES them. .github/workflows/ci.yml
# documents that split in prose, and guards/simple/validate-story-manifest.sh treats a
# missing binary as a PRECONDITION naming this file as the producer. Do not move this there.
#
# Placed AFTER the workspace build deliberately: if the tree does not compile, that line
# reds first, so this gate's "clean arm failed for an unrelated reason" symptom announces
# itself upstream instead of needing to be distinguished here. Triage:
# docs/runbooks/devloop-validation.md §6.1.
#
# No "$@" — cargo args are meaningless to a test harness that picks its own profile and
# feature set, and forwarding them would let a caller's flag silently change what is
# asserted. The release profile is invoked BY NAME inside the script, never reproduced via
# a RUSTFLAGS debug-assertions override, so the premise stays bound to the profile
# infra/docker/mh-service/Dockerfile ships.
run_and_emit "release-feature-gate" "$(dirname "${BASH_SOURCE[0]}")/../../release-feature-gate.test.sh"
