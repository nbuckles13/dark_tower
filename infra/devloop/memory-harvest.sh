#!/usr/bin/env bash
# memory-harvest.sh — move Claude memories between the host and devloop containers.
#
# Seed: a new dev container starts with a copy of the host project's memory (the
# one shared store; /absorb-devloop's triage promotes into it).
# Harvest: Claude in the container saves memories under its home directory, which
# is not mounted, so removing the container loses them. devloop.sh calls
# harvest_container_memory after each session and BEFORE every removal of the dev
# container. Files identical to their seed copy are skipped, so the inbox holds
# only what the devloop added or changed; /absorb-devloop triages it.
#
# Sourced by devloop.sh (functions only). Tested by memory-harvest.test.sh.

# Claude Code keys a project's memory by its working directory with `/` -> `-`;
# every devloop session runs in /work.
DEVLOOP_CONTAINER_MEMORY_DIR="/home/dev/.claude/projects/-work/memory"
DEVLOOP_MEMORY_INBOX="${DEVLOOP_MEMORY_INBOX:-${HOME}/.cache/devloop/memory-inbox}"

# host_memory_dir <repo-root> — the host project's memory dir. Claude Code keys a
# project by its path with every non-alphanumeric character replaced by `-`.
# DEVLOOP_MEMORY_SEED_DIR overrides it.
host_memory_dir() {
    if [[ -n "${DEVLOOP_MEMORY_SEED_DIR:-}" ]]; then
        printf '%s' "$DEVLOOP_MEMORY_SEED_DIR"
        return
    fi
    printf '%s/.claude/projects/%s/memory' "$HOME" "$(printf '%s' "$1" | sed 's/[^A-Za-z0-9]/-/g')"
}

# seed_container_memory <container> <repo-root> — copy the host memory into a new
# container. A missing host dir is a loud WARN (the key derivation may be wrong),
# not an error: a devloop can run without memories.
seed_container_memory() {
    local container="$1" src
    src="$(host_memory_dir "$2")"
    if [[ ! -d "$src" ]]; then
        echo "WARN: no host Claude memory at ${src}; ${container} starts without seeded memories (set DEVLOOP_MEMORY_SEED_DIR if the path is wrong)." >&2
        return 0
    fi
    podman exec "$container" mkdir -p "$DEVLOOP_CONTAINER_MEMORY_DIR"         && podman cp "${src}/." "${container}:${DEVLOOP_CONTAINER_MEMORY_DIR}/"         || { echo "ERROR: could not seed Claude memories into ${container}" >&2; return 1; }
    echo "Seeded $(find "$src" -maxdepth 1 -type f -name '*.md' | wc -l) memory file(s) into ${container}"
}

# harvest_container_memory <container> <slug> [repo-root]
# Copies the container's memory directory into $DEVLOOP_MEMORY_INBOX/<slug>/,
# merging with earlier harvests (files updated in place overwrite). With a
# repo-root, files byte-identical to the host copy (unchanged seeds) are skipped.
# Returns 0 when the container or its memory directory does not exist (nothing to
# lose), non-zero when a copy that should have worked failed.
harvest_container_memory() {
    local container="$1" slug="$2" repo_root="${3:-}" tmp err dest n seed f
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
    if [[ -n "$repo_root" ]]; then
        seed="$(host_memory_dir "$repo_root")"
        for f in "$tmp"/*.md; do
            [[ -f "$f" && -f "${seed}/${f##*/}" ]] && cmp -s "$f" "${seed}/${f##*/}" && rm -f "$f"
        done
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

# harvest_before_removal <container> <slug> [repo-root] — the gate in front of every removal.
# A failed harvest stops the removal; DEVLOOP_SKIP_MEMORY_HARVEST=1 discards them.
harvest_before_removal() {
    harvest_container_memory "$1" "$2" "${3:-}" && return 0
    if [[ "${DEVLOOP_SKIP_MEMORY_HARVEST:-}" == "1" ]]; then
        echo "WARN: DEVLOOP_SKIP_MEMORY_HARVEST=1 — removing $1 without its Claude memories." >&2
        return 0
    fi
    echo "Refusing to remove $1: its Claude memories were not harvested (see above)." >&2
    echo "Fix the cause and rerun, or rerun with DEVLOOP_SKIP_MEMORY_HARVEST=1 to discard them." >&2
    return 1
}
