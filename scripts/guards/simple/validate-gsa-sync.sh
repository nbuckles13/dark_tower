#!/bin/bash
#
# GSA Enumeration Sync Guard (ADR-0024 §6.8 item #2)
#
# The Guarded Shared Areas enumerated list (ADR-0024 §6.4) is mirrored in
# five locations. This guard holds a canonical copy, then per canon path
# greps each mirror for a full or basename match (handles the shorthand
# notation in the markdown mirrors without requiring an expander), and
# cross-checks backticked-token / key counts to catch extra-path drift.
#
# Mirrors:
#   1. docs/decisions/adr-0024-agent-teams-workflow.md §6.4
#   2. .claude/skills/devloop/SKILL.md
#   3. .claude/skills/devloop/review-protocol.md
#   4. scripts/guards/simple/cross-boundary-ownership.yaml
#   5. this file (CANON array below)
#
# Scope boundary: the enumeration section in each markdown mirror runs from
# the anchor comment through the first contiguous `- ` bullet list; the
# YAML enumeration is every `"path":` key line. Intersection-rule sub-paths
# (e.g. `proto/internal.proto`) are YAML-only by design — so YAML count is
# allowed to exceed canon, but markdown counts must equal canon.
#
# Exit codes:
#   0 - all mirrors in sync
#   1 - divergence detected
#   2 - one or more mirror files missing

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

# Canonical GSA path list. Fully expanded. Update all five mirrors together.
CANON=(
    "proto/**"
    "proto-gen/**"
    "build.rs"
    "crates/media-protocol/**"
    "crates/common/src/jwt.rs"
    "crates/common/src/meeting_token.rs"
    "crates/common/src/token_manager.rs"
    "crates/common/src/secret.rs"
    "crates/common/src/webtransport/**"
    "crates/ac-service/src/jwks/**"
    "crates/ac-service/src/token/**"
    "crates/ac-service/src/crypto/**"
    "crates/ac-service/src/audit/**"
    "db/migrations/**"
)

# YAML-only intersection-rule sub-paths. Sub-paths of a CANON entry that
# get their own YAML key because they require a broader owner union
# (e.g., proto/internal.proto needs protocol + auth-controller + security
# per ADR-0003 §5.7). Extending this list is a micro-debate trigger — the
# explicit allowlist keeps the "YAML may have more paths than canon"
# exception bounded so typo'd stray keys are still caught.
INTERSECTION_SUBPATHS=(
    "proto/internal.proto"
)

ADR="$REPO_ROOT/docs/decisions/adr-0024-agent-teams-workflow.md"
SKILL="$REPO_ROOT/.claude/skills/devloop/SKILL.md"
PROTOCOL="$REPO_ROOT/.claude/skills/devloop/review-protocol.md"
YAML="$REPO_ROOT/scripts/guards/simple/cross-boundary-ownership.yaml"

for f in "$ADR" "$SKILL" "$PROTOCOL" "$YAML"; do
    if [[ ! -f "$f" ]]; then
        print_violation "GSA-sync mirror file missing: ${f#"$REPO_ROOT/"}"
        exit 2
    fi
done

# Slice a markdown mirror's enumeration section: starts at the anchor
# comment, enters "list mode" on the first `- ` bullet, exits at the first
# non-bullet non-blank line. Works for all three markdown mirrors because
# prose paragraphs below the list don't start with `- `.
slice_markdown_section() {
    local file="$1"
    awk '
        /Mirror of ADR-0024 §6.4|Source of truth for GSA enumeration/ { armed = 1; next }
        !armed { next }
        /^[[:space:]]*-[[:space:]]/ { in_list = 1; print; next }
        in_list && /^[[:space:]]*$/ { next }
        in_list { exit }
    ' "$file"
}

# Shorthand forms a canon path may appear as in markdown:
#   full path           (always)
#   basename            (e.g., `meeting_token.rs` after `crates/common/src/jwt.rs`)
#   last-dir/**         (e.g., `src/token/**` after `crates/ac-service/src/jwks/**`)
# The second and third forms are alternatives — only one applies per canon
# path depending on whether it ends in `/**`.
shorthand_forms() {
    local p="$1"
    echo "$p"
    if [[ "$p" == *"/**" ]]; then
        # `foo/bar/baz/**` → `baz/**`, and also `bar/baz/**` to tolerate
        # two-segment shorthand if authors use it.
        local stem="${p%/**}"
        local last="${stem##*/}"
        [[ -n "$last" && "$last" != "$stem" ]] && echo "$last/**"
        local rest="${stem%/*}"
        local second="${rest##*/}"
        [[ -n "$second" && "$second" != "$rest" && "$second" != "$last" ]] \
            && echo "$second/$last/**"
    else
        echo "${p##*/}"
    fi
}

violations=0
report() {
    print_violation "GSA-sync: $1"
    violations=$((violations + 1))
}

# Per-canon grep on each markdown mirror's enumeration slice: accept either
# `full-path` or `basename` (handles shorthand siblings like `meeting_token.rs`).
check_markdown_mirror() {
    local label="$1" file="$2"
    local slice
    slice=$(slice_markdown_section "$file")
    if [[ -z "$slice" ]]; then
        report "$label enumeration section empty — anchor comment missing or structure changed"
        return
    fi
    local p form forms matched
    for p in "${CANON[@]}"; do
        matched=0
        while IFS= read -r form; do
            [[ -z "$form" ]] && continue
            if grep -qF -- "\`$form\`" <<< "$slice"; then
                matched=1
                break
            fi
        done < <(shorthand_forms "$p")
        if [[ "$matched" -eq 0 ]]; then
            forms=$(shorthand_forms "$p" | paste -sd',' -)
            report "canon path \`$p\` missing from $label (tried: $forms)"
        fi
    done
    # Count check: markdown should have exactly len(CANON) backticked tokens.
    local count
    count=$(grep -oE '`[^`]+`' <<< "$slice" | wc -l)
    if [[ "$count" -ne "${#CANON[@]}" ]]; then
        report "$label has $count backticked paths, canon has ${#CANON[@]} — mirror drift (add/remove path without canon update)"
    fi
}

check_markdown_mirror "ADR-0024 §6.4"       "$ADR"
check_markdown_mirror "SKILL.md"            "$SKILL"
check_markdown_mirror "review-protocol.md"  "$PROTOCOL"

# YAML check: every canon path must appear as a YAML key, and every YAML
# key must be either a canon path or an allowed intersection sub-path. The
# subset check catches typo'd stray keys that the canon-only scan would
# miss (e.g., `crates/typod/**` silently passing because all canon paths
# are still present).
yaml_keys=$(awk -F'"' '/^[[:space:]]*"[^"]+":/ { print $2 }' "$YAML")
allowed=$(printf '%s\n' "${CANON[@]}" "${INTERSECTION_SUBPATHS[@]}" | sort -u)

for p in "${CANON[@]}"; do
    if ! grep -qxF -- "$p" <<< "$yaml_keys"; then
        report "canon path \`$p\` missing from cross-boundary-ownership.yaml"
    fi
done
while IFS= read -r k; do
    [[ -z "$k" ]] && continue
    if ! grep -qxF -- "$k" <<< "$allowed"; then
        report "cross-boundary-ownership.yaml has stray key \`$k\` — not in CANON or INTERSECTION_SUBPATHS (possible typo; intentional additions require canon/intersection-subpath update)"
    fi
done <<< "$yaml_keys"

if [[ "$violations" -gt 0 ]]; then
    exit 1
fi

print_ok "GSA enumeration in sync across all five mirrors"
exit 0
