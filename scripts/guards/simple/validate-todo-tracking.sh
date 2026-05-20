#!/bin/bash
# Validates devloop tech-debt tracking conventions:
#   (1) docs/TODO.md is the only TODO.md in the tree. Any newly-added TODO.md
#       at any other path (root, crate, etc.) fails the guard.
#   (2) Each docs/devloop-outputs/*/main.md's "Accepted Deferrals" section
#       (or older "Tech Debt Pointers" heading — both accepted) contains only
#       one-line pointer bullets, never inlined multi-line debt bodies. A line
#       is acceptable in the section if it is blank, starts with `- ` (a
#       bullet), is inside a fenced code block (examples), or is template-shape
#       prose (Example/Examples/or). Multi-line bodies are the failure shape.
#
# The original "Tech Debt References" heading (the first iteration) is
# intentionally NOT matched so historical devloops don't retroactively fail.
# The "Accepted Deferrals" rename (Revision 8 follow-up) is a wording change
# that surfaces the cost shift; mechanical rules are identical to the prior
# "Tech Debt Pointers" enforcement.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../common.sh"

# Allow callers to override the search root (used by tests against fixtures)
ROOT="${TODO_TRACKING_ROOT:-.}"

ERRORS=""

# ─── Rule 1: only docs/TODO.md is allowed ─────────────────────────────

# Search the whole tree, not just diff, so this catches stray files even
# if the offending commit was pushed without the diff including the rename.
while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    [[ "$path" == "${ROOT%/}/docs/TODO.md" ]] && continue
    [[ "$path" == "${ROOT%/}/TODO.md" && "$ROOT" == "." ]] && : # fall through
    # Normalize: when ROOT is ".", compare against bare paths
    if [[ "$ROOT" == "." ]]; then
        [[ "$path" == "docs/TODO.md" ]] && continue
    fi
    ERRORS="${ERRORS}STRAY TODO.md: ${path}\n  -> remove this file; all tech debt lives in docs/TODO.md.\n"
done < <(
    if [[ "$ROOT" == "." ]]; then
        git ls-files '*TODO.md' 2>/dev/null
    else
        # Fixture mode: just list any TODO.md under the fixture root
        find "$ROOT" -name 'TODO.md' -type f 2>/dev/null
    fi
)

# Also catch untracked stray TODO.md files surfaced in the current diff
# (covers the case where a devloop hasn't yet committed the stray file).
# Only meaningful in real-repo mode.
if [[ "$ROOT" == "." ]]; then
    while IFS= read -r path; do
        [[ -z "$path" ]] && continue
        [[ "$path" == "docs/TODO.md" ]] && continue
        ERRORS="${ERRORS}STRAY TODO.md (untracked): ${path}\n  -> remove this file; all tech debt lives in docs/TODO.md.\n"
    done < <(git ls-files --others --exclude-standard '*TODO.md' 2>/dev/null)
fi

# ─── Rule 2: Accepted Deferrals / Tech Debt Pointers sections must be pointer-only ───

for main_md in "$ROOT"/docs/devloop-outputs/*/main.md; do
    [ -f "$main_md" ] || continue

    # Skip the template itself — it intentionally documents both shapes
    case "$main_md" in
        */_template/main.md) continue ;;
    esac

    # Extract the section under either accepted heading. The original "Tech
    # Debt References" heading is intentionally ignored so historical devloops
    # don't retroactively fail.
    section=$(awk '
        /^## (Accepted Deferrals|Tech Debt Pointers)[[:space:]]*$/ { in_section=1; next }
        in_section && /^## / { in_section=0 }
        in_section { print }
    ' "$main_md")

    [ -z "$section" ] && continue

    # Walk the section line-by-line. A "body" line is one that is non-blank,
    # not a bullet, not inside a fenced code block, and not template-shape
    # prose (Example/Examples/or). Two or more consecutive body lines indicate
    # an inlined debt body rather than pointer-shape content.
    line_no=0
    in_fence=0
    body_run=0
    section_start_line=$(grep -nE '^## (Accepted Deferrals|Tech Debt Pointers)' "$main_md" | head -1 | cut -d: -f1)
    section_start_line=${section_start_line:-0}

    while IFS= read -r line; do
        line_no=$((line_no + 1))
        abs_line=$((section_start_line + line_no))

        # Toggle code-fence state
        if [[ "$line" =~ ^[[:space:]]*\`\`\` ]]; then
            in_fence=$(( 1 - in_fence ))
            body_run=0
            continue
        fi

        # Lines inside a code fence are example/illustrative — not flagged
        if [[ "$in_fence" -eq 1 ]]; then
            continue
        fi

        # Blank lines reset the body-run counter
        if [[ -z "${line// }" ]]; then
            body_run=0
            continue
        fi

        # Pointer bullets are fine
        if [[ "$line" =~ ^[[:space:]]*[-*+][[:space:]] ]]; then
            body_run=0
            continue
        fi

        # Template-shape prose words used between example blocks
        if [[ "$line" =~ ^(Example|Examples|or)[[:space:]]*:?[[:space:]]*$ ]]; then
            body_run=0
            continue
        fi

        # Anything else is body prose; ≥2 consecutive triggers the finding.
        body_run=$((body_run + 1))
        if [[ "$body_run" -ge 2 ]]; then
            ERRORS="${ERRORS}INLINE TECH DEBT BODY in ${main_md}:~${abs_line}\n  -> Tech debt bodies belong in docs/TODO.md, not main.md.\n  -> Replace with one-line pointer bullets: '- \`docs/TODO.md\` §SECTION — hook'\n"
            break
        fi
    done <<< "$section"
done

# ─── Report ───────────────────────────────────────────────────────────

if [ -n "$ERRORS" ]; then
    echo -e "$ERRORS"
    exit 1
fi

exit 0
