#!/usr/bin/env bash
# release-feature-gate.test.sh — real-build demonstration that the dev-only
# per-frame tracing feature is a COMPILE ERROR under the release profile.
#
# ADR-0036 §11 "A control's coverage must be demonstrated, not asserted"
# (story task 23). This is the FIRE half for the `compile_error!` gate at
# `crates/mh-service/src/lib.rs`: inject the adverse condition — the release
# profile with the feature enabled — and require a real compiler to reject it.
#
# THE REASONING FOR THE GATE ITSELF IS NOT RESTATED HERE. The predicate, what
# supports it, both caveats, the named residual and the rejected `build.rs`
# alternative are stated once at `crates/mc-service/src/lib.rs` above its own
# `compile_error!`. Read that block. `crates/mh-service/src/lib.rs` already
# points at it and forbids a second copy; this file is the third site and would
# be the one that drifts. The complementarity with `dt-guard
# release-build-profile` — that guard asserts the PREMISE, this asserts the
# control FIRES — is stated once in that guard's own module doc
# (`crates/dt-guard/src/release_build_profile.rs` §Boundary against story task
# 23). Cross-referenced, not re-derived.
#
# WHY `cargo check` AND NOT `cargo build` (Lead ruling, @operations measured).
# `compile_error!` fires during macro EXPANSION, long before codegen, so the
# fire arm cannot distinguish the two verbs — `build` would add only linking,
# which is not what this control is about. `cargo build --release -p mh-service`
# measures 50-57s whenever mh-service source changed, i.e. on exactly the
# devloops where this control matters; both `check` arms together are ~2s warm.
# This is a real compiler invocation under the real profile, which is what the
# task's "a real build, not a CI string check" contrasts against.
#
# WHY THE PROFILE BY NAME AND NOT A RUSTFLAGS OVERRIDE. The gate keys on
# `debug_assertions`, not on the profile name. Invoking `--release` binds the
# assertion to the profile that governs the deployed image
# (`infra/docker/mh-service/Dockerfile` builds `--release`); reproducing the
# predicate by hand with `-C debug-assertions=off` would test something that
# RESEMBLES the artifact instead of the artifact's own profile. This also buys a
# forcing function: if `[profile.release]` ever gains `debug-assertions = true`,
# the control silently dies and this self-test goes RED.
#
# LAYER 1, NOT LAYER 3 (@infrastructure ESCALATE, Lead ruled). ADR-0033 §4
# excludes the language layers from the fast tier because they carry inherently
# large/variable cost. Cargo is variable cost; Layer 3 invokes cargo zero times
# by design. Wired from `scripts/lang/rust/compile.sh`.

set -euo pipefail
IFS=$'\n\t'

__here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lang/_test_helpers.sh
source "${__here}/lang/_test_helpers.sh"

REPO_ROOT="$(cd "${__here}/.." && pwd)"

# ---------------------------------------------------------------------------
# The (crate, feature) table.
#
# Table-driven rather than hardcoded to MH so the MC sibling is one row rather
# than a copy-paste. `crates/mc-service`'s `test-seams` gate is the SAME
# mechanism and has no automated real-build self-test either, but MC is
# meeting-controller's crate and adding it was ruled a residual for this
# devloop (tracked by @dry-reviewer under docs/TODO.md §Cross-Service
# Duplication). The shape admits it; the row is deliberately absent.
#
# Fields: <crate-name>|<feature>|<lib.rs path relative to repo root>
# ---------------------------------------------------------------------------
GATES=(
  "mh-service|per-frame-trace|crates/mh-service/src/lib.rs"
)

# The pinned phrase. Chosen because it lies wholly within ONE source line of the
# `compile_error!` literal, so the `\`-continuation hazard cannot reach it. It is
# also specific enough that no unrelated compile failure would carry it.
#
# The EMPTY-extraction hole this closes: `output contains ""` is vacuously true,
# which would silently degrade the fire arm's text assertion to rc-only and
# reopen the typo'd-feature-name / unrelated-break / offline-registry holes. That
# is why an empty grep result is a hard PRECONDITION_FAILURE and never a pass.
readonly NEEDLE_PHRASE='must never be enabled in a release build'

# Operator-lane exit. The environment is broken, not the diff — do not consume a
# devloop attempt. `run_and_emit` in the calling wrapper will ALSO emit a
# STATUS=FAIL line for the non-zero rc; worst-status aggregation still lands the
# layer on PRECONDITION_FAILURE and exit 2, so the lane is correct, but the log
# carries both lines. The §6.1 runbook row says so.
precondition_fail() {
  printf 'STATUS=PRECONDITION_FAILURE REASON=%s\n' "$1" >&2
  printf 'release-feature-gate: %s\n' "$2" >&2
  exit 2
}

# Confirm the `compile_error!` message needle against the crate's own lib.rs.
#
# THE CONTINUATION HAZARD IS FORECLOSED STRUCTURALLY, NOT CHECKED. @security and
# @infrastructure both found that the literal at `crates/mh-service/src/lib.rs`
# spans two source lines joined by a `\` continuation with leading indentation,
# which rustc renders as ONE line — so an extractor that concatenated the source
# lines would build a needle carrying a stray backslash and a run of spaces that
# can never match, and it would be LONG, so a minimum-length floor would wave it
# through. They proposed asserting the needle contains no backslash and no
# double space.
#
# I did not take those two assertions, and the reason is this task's own
# subject. Grepping for ONE FIXED PHRASE that lies wholly within a single source
# line cannot produce a backslash, a double space, or a short needle — so those
# checks are STRUCTURALLY UNREACHABLE here. Keeping them would ship three
# controls that can never fire, dressed as defence in depth, inside the devloop
# whose whole point is that a control which cannot fire reads as coverage. The
# ADR's own remedy applies: "prefer structural impossibility over a control that
# has to notice." The single-phrase grep IS the fix; a checker for a malformation
# it cannot produce is theatre. Raised to @security rather than dropped quietly.
#
# WHAT THIS IS, STATED HONESTLY: a two-way PIN, not a derivation. The phrase is
# written once, here, and must appear BOTH in the gate's source AND in the
# compiler's output. It is not "derived" in the sense of being read out of the
# source without being named — naming it is what lets a single-line grep dodge
# the continuation. Drift is therefore LOUD, not silent: rewording the
# `compile_error!` message makes this grep find nothing, which is a
# PRECONDITION_FAILURE with its own reason token, not a silent pass. That is the
# property that matters, and it is the one a retyped-literal-in-an-assertion
# would NOT have had, because a retyped needle simply stops matching the
# compiler output while the source moves on.
#
# IF YOU CHANGE THIS TO A DERIVATION, READ THIS FIRST (@security). This is a
# fixed-phrase PIN, not a derivation. If you rewrite it to reconstruct the
# literal out of the source, you MUST add malformation checks — a derivation can
# produce a needle that is long, non-empty and still incapable of matching, and
# the `\` continuation at `crates/mh-service/src/lib.rs:106-107` is exactly how:
# Rust's `\`-newline escape strips the newline AND the next line's leading
# whitespace, so concatenating the two source lines yields a backslash and a run
# of spaces that rustc's rendered output never contains. A minimum-length floor
# does NOT catch that, because the bad needle is long. The reason this file has
# no such checks is that a fixed-phrase grep cannot produce that input at all.
#
# Args: $1=absolute path to the crate's lib.rs
# Emits the needle on stdout; exits via precondition_fail if the pin is broken.
derive_needle() {
  local lib="$1" needle

  needle="$(grep -o "$NEEDLE_PHRASE" "$lib" | head -n 1 || true)"

  if [[ -z "$needle" ]]; then
    precondition_fail "release-feature-gate-needle-not-found" \
      "could not confirm the compile_error! needle in ${lib} — the marker text is gone or reshaped. This is a COULD-NOT-EVALUATE, not a control failure: do not 'fix' it by relaxing the pin, and do not retype the new wording into the fire-arm assertion without also confirming it here. IF YOU ONLY REFLOWED THE MESSAGE (moved where the \\ continuation falls, without changing the rendered text), the compiler output still contains the phrase but this source grep can fail: move NEEDLE_PHRASE to a phrase lying wholly within ONE source line. Do NOT delete this check — it is the half that makes a reworded message loud instead of silent."
  fi

  printf '%s' "$needle"
}

# ---------------------------------------------------------------------------
# Preconditions. Every one is an OPERATOR-lane failure and every one is LOUD.
# There is deliberately no skip path: a control that green-skips is dead, and
# this file exists to demonstrate a control, so it must never pass quietly.
# ---------------------------------------------------------------------------
command -v cargo >/dev/null 2>&1 || precondition_fail \
  "release-feature-gate-cargo-missing" \
  "cargo is not on PATH — cannot run a real build."

for gate in "${GATES[@]}"; do
  IFS='|' read -r crate feature libpath <<<"$gate"

  lib_abs="${REPO_ROOT}/${libpath}"
  [[ -f "$lib_abs" ]] || precondition_fail \
    "release-feature-gate-libsrc-missing" \
    "${libpath} does not exist — the gate's source of truth moved."

  needle="$(derive_needle "$lib_abs")"

  # -- Arm 1: CLEAN. Release profile, feature OFF. Must succeed. --------------
  #
  # RUNS FIRST, for two reasons. (a) It is the predicate control: without it, an
  # arm asserting only a non-zero exit still passes when the crate stops
  # compiling for any unrelated reason — the "fires for the wrong reason"
  # version of a dead control. (b) It warms the dependency graph the fire arm
  # reuses, so the feature toggle invalidates only the crate's own unit.
  #
  # --color=never: rustc colorizes on TTY detection and a substring match
  # against ANSI-wrapped output is a free flake to eliminate.
  clean_out=""
  clean_rc=0
  clean_out="$(cargo check --release -p "$crate" --color=never 2>&1)" || clean_rc=$?

  if (( clean_rc != 0 )); then
    # Distinguish a broken toolchain/registry (operator) from a broken tree
    # (implementer). A dep-fetch failure is not this diff's fault and must not
    # consume an attempt.
    if [[ "$clean_out" == *"failed to download"* || "$clean_out" == *"failed to get"* \
       || "$clean_out" == *"network failure"* || "$clean_out" == *"no space left"* ]]; then
      precondition_fail "release-feature-gate-toolchain-unavailable" \
        "the clean arm failed on a registry/network/disk condition, not on source: ${clean_out}"
    fi
    FAIL=$((FAIL + 1))
    FAILURES+=("[${crate}/clean-arm] release-profile build WITHOUT the feature failed (rc=${clean_rc}); the tree is broken for an unrelated reason and the fire arm proves nothing until it is fixed")
    continue
  fi
  assert_rc "${crate}/clean-arm-succeeds-without-feature" 0 "$clean_rc"

  # -- Arm 2: FIRE. Release profile, feature ON. Must fail, for the RIGHT reason.
  #
  # THE CONJUNCTION IS THE POINT, and it is stronger than either half alone.
  # Reviewers split on this: @observability argued for exit code and NOT text
  # (a text-only assertion would miss `[profile.release]` gaining
  # `debug-assertions = true`, which makes the build SUCCEED); @security, @test
  # and @operations argued for text (a non-zero exit alone false-passes on a
  # typo'd feature name, an unrelated compile break, or an offline registry).
  # Requiring BOTH kills both failure modes. And matching text against the
  # output of a real compiler run is not the "CI string check" §11 rules out —
  # that phrase means scanning SOURCE instead of compiling.
  fire_out=""
  fire_rc=0
  fire_out="$(cargo check --release -p "$crate" --features "$feature" --color=never 2>&1)" || fire_rc=$?

  assert_rc "${crate}/fire-arm-exits-non-zero" 101 "$fire_rc"

  # The needle actually matched. THIS is the assertion that closes the
  # malformed-needle hole — a needle can be non-empty, long, backslash-free and
  # still be the wrong text. Length answers "did we extract something"; this
  # answers "did we extract the right thing".
  assert_status "${crate}/fire-arm-cites-the-compile_error-message" "$needle" "$fire_out"

  # Attribution: the diagnostic came from THIS crate's lib.rs, not from a
  # dependency that happened to break.
  assert_status "${crate}/fire-arm-originates-in-the-gate-source" "$libpath" "$fire_out"

  # Not a cargo argument error masquerading as the control firing. The exact
  # wording was taken from cargo, not guessed: a mistyped feature yields
  # "the package 'X' does not contain this feature: Y". Verified by running it —
  # an earlier draft of this assertion used the plural `--features` wording from
  # a different cargo path and was silently DEAD, passing on the very input it
  # was written to catch.
  #
  # WHICH CONTROL ACTUALLY CARRIES THE TYPO CASE — do not harden the wrong one
  # (@infrastructure, measured). With a mistyped feature, cargo fails at feature
  # resolution BEFORE compiling, so `compile_error!` never expands and the needle
  # occurs zero times: the `fire-arm-cites-the-compile_error-message` assertion
  # above already reds on this case by itself. THAT is the load-bearing typo
  # control, and its expectation is derived from an in-repo artifact
  # (`crates/mh-service/src/lib.rs`), so a reword makes it loudly fail.
  #
  # This `assert_absent` is a REDUNDANT SECONDARY that buys a clearer diagnostic.
  # Its expectation is cargo's wording, which is NOT an artifact in this tree and
  # cannot be pinned to one — so no input-side guard can protect it, and a future
  # toolchain reword will silently kill it again by the exact mechanism that
  # killed it once. That is an accepted, bounded residual, recorded here rather
  # than left as a latent surprise. If you find this assertion stale, fix it
  # against real cargo output; do not treat the needle assertion as decoration.
  assert_absent "${crate}/fire-arm-is-not-a-feature-name-typo" \
    "does not contain this feature" "$fire_out"
done

report_results "scripts/release-feature-gate.test.sh"
