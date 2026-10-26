#!/usr/bin/env bash
# Proto per-kind field-name guard (docs/protocol/CONVENTIONS.md §6; ADR-0036 §5
# amendment 2026-10-25) — dt-guard subcommand. No proto FIELD may encode a media
# kind in its name; per-kind facts are repeated entries keyed by MediaKind.
# Full policy logic (derived vocabulary, synonyms, allow-list) lives in
# `crates/dt-guard/src/proto_per_kind_fields.rs`; lexing in
# `crates/dt-guard/src/common/proto_lex.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" proto-per-kind-fields
