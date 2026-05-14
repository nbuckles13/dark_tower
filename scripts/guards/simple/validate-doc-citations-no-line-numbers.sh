#!/bin/bash
#
# Doc-Citation No-Line-Numbers Guard (Guard A)
#
# Forbids bare `<path>:<NN>` and `<path>:<NN>-<NN>` cites in long-lived doc
# trees (docs/runbooks/**, .claude/skills/**). Line numbers drift; symbol-name
# anchors + section references + prose are durable.
#
# Allowed forms (NOT flagged):
#   - `<path>::<symbol>` — Guard C verifies the symbol resolves.
#   - `<path> § "<header>"` — markdown section reference.
#   - Prose mentions ("the foo helper in bar.rs").
#   - Lines with `<!-- guard:ignore(<reason>) -->` where reason >= 10 chars
#     and not test/tmp/todo/fixme/wip (lazy-reason rejection per common kernel).
#
# Output: simple-guard convention via common.sh helpers — `print_violation` per
# finding, `print_ok` on pass, exit 0/1. The Layer 3 wrapper emits STATUS=.
#
# Exit codes:
#   0 - all in-scope docs clean OR no in-scope docs found
#   1 - one or more bare line cites detected
#   2 - script error (python3 missing, etc.)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LIB_DIR="$REPO_ROOT/scripts/guards/lib"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

run_scan() {
    python3 - "$REPO_ROOT" "$LIB_DIR" <<'PYEOF'
import os
import sys

REPO_ROOT = sys.argv[1]
LIB_DIR   = sys.argv[2]
sys.path.insert(0, LIB_DIR)

from doc_cite_extract import extract_cites, is_in_scope_doc  # noqa: E402

# Walk the in-scope doc trees identically to common.sh::doc_citation_in_scope_files.
docs = []
for sub in ("docs/runbooks", ".claude/skills"):
    abs_sub = os.path.join(REPO_ROOT, sub)
    if not os.path.isdir(abs_sub):
        continue
    for dirpath, _dirnames, filenames in os.walk(abs_sub):
        for fn in filenames:
            if not fn.endswith(".md"):
                continue
            abs_path = os.path.join(dirpath, fn)
            rel = os.path.relpath(abs_path, REPO_ROOT)
            if is_in_scope_doc(rel):
                docs.append((rel, abs_path))
docs.sort()

violations = 0
for rel, abs_path in docs:
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError as exc:
        sys.stderr.write(f"ERROR: could not read {rel}: {exc}\n")
        sys.exit(2)
    for cite in extract_cites(rel, text):
        if cite.kind != "bare-line":
            continue
        if cite.is_ignored:
            continue
        # Greppable single line: "VIOLATION: <doc-file> — <token> — <doc-file>:<line>: <offending-cite>"
        print(
            f"VIOLATION: {cite.doc_file} — doc-citations-line-numbers-found — "
            f"{cite.doc_file}:{cite.line_no}: {cite.full_match}"
        )
        violations += 1

if violations > 0:
    print(f"\nFound {violations} bare-line cite violation(s) in {len(docs)} in-scope doc(s).")
    sys.exit(1)
print(f"OK - No doc-citation line-numbers found ({len(docs)} doc(s) scanned)")
sys.exit(0)
PYEOF
}

main() {
    init_violations
    start_timer

    print_header "Doc-Citation No-Line-Numbers Guard (Guard A)"
    echo "Scanning: docs/runbooks/**, .claude/skills/**"
    echo ""

    local rc=0
    run_scan || rc=$?

    echo ""
    print_elapsed_time
    exit "$rc"
}

main "$@"
