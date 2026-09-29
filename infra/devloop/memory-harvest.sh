#!/usr/bin/env bash
# memory-harvest.sh — copy a devloop container's Claude memories to the host.
#
# Claude in the dev container saves memories under its home directory, which is
# not mounted, so removing the container loses them. devloop.sh calls
# harvest_container_memory after each session and BEFORE every removal of the dev
# container; the /absorb-devloop skill triages the inbox (promote / drop).
#
# Sourced by devloop.sh (functions only). Tested by memory-harvest.test.sh.

# Claude Code keys a project's memory by its working directory with `/` -> `-`;
# every devloop session runs in /work.
DEVLOOP_CONTAINER_MEMORY_DIR="/home/dev/.claude/projects/-work/memory"
DEVLOOP_MEMORY_INBOX="${DEVLOOP_MEMORY_INBOX:-${HOME}/.cache/devloop/memory-inbox}"

# harvest_container_memory <container> <slug>
# Copies the container's memory directory into $DEVLOOP_MEMORY_INBOX/<slug>/,
# merging with earlier harvests (files updated in place overwrite). Returns 0 when
# the container or its memory directory does not exist (nothing to lose), non-zero
# when a copy that should have worked failed.
harvest_container_memory() {
    local container="$1" slug="$2" tmp err dest n
    podman container exists "$container" 2>/dev/null || return 0
    tmp="$(mktemp -d)" || return 1
    if ! err="$(podman cp "${container}:${DEVLOOP_CONTAINER_MEMORY_DIR}/." "$tmp/" 2>&1)"; then
        rm -rf "$tmp"
        if [[ "$err" == *"no such file or directory"* || "$err" == *"No such file or directory"* ]]; then
            return 0
        fi
        echo "ERROR: could not copy Claude memories out of ${container}: ${err}" >&2
        return 1
    fi
    n="$(find "$tmp" -type f -name '*.md' | wc -l)"
    if [[ "$n" -eq 0 ]]; then
        rm -rf "$tmp"
        return 0
    fi
    dest="${DEVLOOP_MEMORY_INBOX}/${slug}"
    mkdir -p "$dest" && cp -a "$tmp/." "$dest/" || { rm -rf "$tmp"; echo "ERROR: could not write ${dest}" >&2; return 1; }
    rm -rf "$tmp"
    echo "Harvested ${n} memory file(s) from ${container} -> ${dest}"
}

# harvest_before_removal <container> <slug> — the gate in front of every removal.
# A failed harvest stops the removal; DEVLOOP_SKIP_MEMORY_HARVEST=1 discards them.
harvest_before_removal() {
    harvest_container_memory "$1" "$2" && return 0
    if [[ "${DEVLOOP_SKIP_MEMORY_HARVEST:-}" == "1" ]]; then
        echo "WARN: DEVLOOP_SKIP_MEMORY_HARVEST=1 — removing $1 without its Claude memories." >&2
        return 0
    fi
    echo "Refusing to remove $1: its Claude memories were not harvested (see above)." >&2
    echo "Fix the cause and rerun, or rerun with DEVLOOP_SKIP_MEMORY_HARVEST=1 to discard them." >&2
    return 1
}
