#!/usr/bin/env bash
# No key material crosses the MC->MH contract (ADR-0036 §4), for
# proto/dark_tower/internal/v1/internal.proto — dt-guard subcommand, ported from
# bash 2026-10-26 (ADR-0034). Policy (allow-lists: @security) and tests live in
# `crates/dt-guard/src/internal_proto_no_key_material.rs`; lexing in
# `common/proto_lex.rs`. No suppression path.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" internal-proto-no-key-material
