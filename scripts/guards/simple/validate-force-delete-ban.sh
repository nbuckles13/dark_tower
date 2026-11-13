#!/usr/bin/env bash
# Force-Delete Ban Guard (ADR-0039 §9, R-28): no command in infra/, scripts/,
# crates/env-tests/ or packages/*/e2e force-deletes a pod (`--force`, any
# argument order, or a zero grace period through the API) — the one way to
# defeat the MH hostPort slot lock. No allowlist, no suppression. Full policy
# logic lives in `crates/dt-guard/src/force_delete_ban.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" force-delete-ban
