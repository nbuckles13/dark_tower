#!/usr/bin/env bash
# The ONE home for the dev-tool binaries (dt-guard, dt-story): their cargo profile,
# their paths, and how they are built. Every producer (scripts/lang/rust/compile.sh,
# the pre-commit hook, CI) and every consumer (guard wrappers, self-tests, the story
# runner, CI's coverage assertion) reads these values from here. Docs and remediation
# messages name `scripts/lib/tool-bins.sh build` / `scripts/lib/tool-bins.sh dt-story
# <args>` instead of restating a path. `dt-guard tool-bin-paths` fails on a literal
# tool-binary path or a profile-selecting tool build anywhere else, and on drift
# between DT_TOOLS_PROFILE and Cargo.toml's [profile.<name>].
#
# Why a dedicated profile: [profile.release] (fat LTO, 1 codegen unit) is tuned for the
# shipped services; building the tools with it cost ~19-33s per dt-guard edit. The
# devtools profile inherits release and drops LTO (see Cargo.toml).
#
# Two modes:
#   source scripts/lib/tool-bins.sh   — definitions only (variables + functions)
#   scripts/lib/tool-bins.sh build    — build every tool crate (the recovery command;
#                                       `bash scripts/lib/tool-bins.sh build` if +x is lost)
#   scripts/lib/tool-bins.sh dt-story|dt-guard <args...> — exec the built binary
#
# SOURCE-SAFETY CONTRACT (the pre-commit hook sources this; it is #!/bin/bash, set -e,
# bash-3.2-tolerant, and must NOT source scripts/lang/_common.sh):
#   - Definitions only: no file-scope `set`, IFS change, `cd`, or `exit`. Strict mode is
#     enabled only inside the executable-mode branch at the bottom.
#   - EVERY assignment is unconditional — never `${X:-default}`, never `export`. Sourcing
#     must not let an ambient variable redirect a binary path: run-story.sh's DT_STORY is
#     honoured only through its DEVLOOP_TEST=1 seam, and an env-derived default here
#     would bypass that gate. The env overrides (DT_GUARD, DT_STORY) stay at each
#     consumer, under names distinct from the ones defined here.
#   - No `readonly`, `declare -g`, or associative arrays (bash 3.2; sourcing twice in one
#     shell must be harmless).
#   - The root comes from this file's own location, never from an inherited variable, so
#     paths are absolute and a later `cd` (or a run-story seam-redirected tree) cannot
#     change which binary they name.

DT_TOOLS_ROOT="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
# Must match a [profile.<name>] in Cargo.toml (guarded).
DT_TOOLS_PROFILE=devtools
# The dev-tool crate set. dt_build_tool refuses anything else; `dt-guard
# tool-bin-paths` reads this list to know which binary names to police.
DT_TOOLS_CRATES="dt-guard dt-story"
DT_TOOLS_DIR="$DT_TOOLS_ROOT/target/$DT_TOOLS_PROFILE"
DT_GUARD_BIN="$DT_TOOLS_DIR/dt-guard"
DT_STORY_BIN="$DT_TOOLS_DIR/dt-story"
# The one recovery command every missing/stale-binary message names (repo-relative).
DT_TOOLS_BUILD_CMD="scripts/lib/tool-bins.sh build"

# dt_build_tool <crate> [cargo-args...] — the ONE cargo invocation that produces a tool
# binary. --target-dir is explicit: cargo honours an ambient CARGO_TARGET_DIR (the CI
# coverage step exports one via `cargo llvm-cov show-env`), which would write the binary
# somewhere other than DT_TOOLS_DIR while every consumer reads DT_TOOLS_DIR — the same
# producer/consumer split this file exists to remove.
#
# After a successful build, the same crate's binary at the RELEASE-profile path (where
# these tools were built before the devtools profile existed) is removed. Nothing reads
# that path any more, so a leftover copy can only be run by hand and give a silently old
# verdict; removing it turns that into "no such file".
dt_build_tool() {
    # DT_TOOLS_CRATES is space-separated; callers commonly run with IFS=$'\n\t'.
    local IFS=' ' crate="${1:-}" known=0 c stale
    for c in $DT_TOOLS_CRATES; do
        if [ "$c" = "$crate" ]; then known=1; fi
    done
    if [ "$known" -ne 1 ]; then
        echo "tool-bins: '${crate}' is not a dev-tool crate (DT_TOOLS_CRATES: ${DT_TOOLS_CRATES})" >&2
        return 2
    fi
    shift
    ( cd "$DT_TOOLS_ROOT" && cargo build --profile "$DT_TOOLS_PROFILE" --target-dir "$DT_TOOLS_ROOT/target" -p "$crate" "$@" ) || return $?
    if [ "$DT_TOOLS_PROFILE" != release ]; then
        stale="$DT_TOOLS_ROOT/target/release/$crate"
        if [ -e "$stale" ]; then
            rm -f -- "$stale" || {
                echo "tool-bins: could not remove stale ${stale} — remove it by hand; nothing reads it, but running it gives an old verdict" >&2
                return 1
            }
            echo "tool-bins: removed stale ${stale} (${crate} now builds to ${DT_TOOLS_DIR}/${crate})" >&2
        fi
    fi
    return 0
}

# dt_build_tools [cargo-args...] — build every crate in DT_TOOLS_CRATES.
dt_build_tools() {
    local IFS=' ' c
    for c in $DT_TOOLS_CRATES; do
        dt_build_tool "$c" "$@" || return $?
    done
    return 0
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    set -euo pipefail
    __tb_usage() {
        echo "usage: ${DT_TOOLS_BUILD_CMD%% *} build [cargo-args...] | <crate> <args...>   (crates: ${DT_TOOLS_CRATES})" >&2
    }
    __tb_verb="${1:-}"
    if [ $# -gt 0 ]; then shift; fi
    __tb_bin=""
    for __tb_c in $DT_TOOLS_CRATES; do
        if [ "$__tb_c" = "$__tb_verb" ]; then __tb_bin="$DT_TOOLS_DIR/$__tb_c"; fi
    done
    if [ "$__tb_verb" = build ]; then
        dt_build_tools "$@"
    elif [ -n "$__tb_bin" ]; then
        # DT_TOOLS_DIR only — never an ambient DT_GUARD/DT_STORY override.
        if [ ! -x "$__tb_bin" ]; then
            echo "STATUS=FAIL REASON=${__tb_verb}-binary-missing"
            echo "${__tb_verb} not built at ${__tb_bin} — run: ${DT_TOOLS_BUILD_CMD}" >&2
            exit 1
        fi
        exec "$__tb_bin" "$@"
    else
        __tb_usage
        exit 2
    fi
fi
