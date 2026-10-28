#!/usr/bin/env bash
# Docker build-context guard: `.dockerignore` is an allowlist and the context it
# admits holds no key material or secret files — dt-guard subcommand.
# Full policy logic lives in `crates/dt-guard/src/docker_context.rs`.
# shellcheck source=./_dt_guard_wrapper.sh
source "$(dirname "${BASH_SOURCE[0]}")/_dt_guard_wrapper.sh" docker-context
