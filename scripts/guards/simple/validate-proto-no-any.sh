#!/usr/bin/env bash
# Proto no-Any guard (docs/protocol/CONVENTIONS.md §Enforcement: the premise of
# proto/buf.yaml's MESSAGE_NO_DELETE exception) — dt-guard subcommand, ported
# from bash 2026-10-26 (ADR-0034). Policy (@protocol) and tests live in
# `crates/dt-guard/src/proto_no_any.rs`; lexing in `common/proto_lex.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" proto-no-any
