#!/bin/bash
#
# Doc-Citation Symbol-Resolves Guard (Guard C)
#
# For each `<path>::<symbol>` cite in long-lived doc trees (docs/runbooks/**,
# .claude/skills/**), verifies the symbol resolves in the named file.
#
# Per-language resolution patterns (see scripts/guards/lib/doc_cite_extract.py
# for canonical regex builders):
#   .rs         fn|struct|enum|trait|impl|const|static|type <symbol>
#   .sh         ^<symbol>() OR ^function <symbol>   (top-level only — by design)
#   .toml       ^[<section>] OR ^<key> = …
#   .yaml/.yml  ^<key>:   (top-level only)
#   .md         ^#+ … <heading-text>   (word-boundary, substring-permissive at start)
#   .proto      message|service|enum|rpc <symbol>
#
# Extensions outside the table → cite is silently allowed (unverified). See
# `supported_resolution_extensions()` in the shared module.
#
# Failure reasons (greppable tokens in VIOLATION lines):
#   file-missing       — cited <path> doesn't exist under repo root
#   path-escape        — cited <path> resolves outside repo root (traversal/symlink)
#   symbol-not-found   — file exists, in-scope extension, but no matching definition
#
# Output: simple-guard convention via common.sh helpers. Layer 3 wrapper emits STATUS=.
#
# Exit codes:
#   0 - all symbol cites resolve OR no in-scope cites found
#   1 - one or more symbol cites failed to resolve
#   2 - script error

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

from doc_cite_extract import (  # noqa: E402
    extract_cites, walk_in_scope_docs,
    resolve_cited_path, resolve_basename_match, symbol_resolves_in_file,
    supported_resolution_extensions,
)

SUPPORTED = set(supported_resolution_extensions())

docs = walk_in_scope_docs(REPO_ROOT)

violations = 0
cites_seen = 0
for rel, abs_path in docs:
    try:
        with open(abs_path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError as exc:
        sys.stderr.write(f"ERROR: could not read {rel}: {exc}\n")
        sys.exit(2)
    for cite in extract_cites(rel, text):
        if cite.kind != "symbol":
            continue
        if cite.is_ignored:
            continue
        cites_seen += 1
        # Extension gate: outside SUPPORTED → silently allow (unverified).
        ext = cite.path.rsplit(".", 1)[-1].lower() if "." in cite.path else ""
        if ext not in SUPPORTED:
            continue

        # Path safety: resolve under repo root, reject path-escape.
        resolved = resolve_cited_path(REPO_ROOT, cite.path)
        if resolved is None:
            print(
                f"VIOLATION: {cite.doc_file} — doc-citation-symbol-unresolved — "
                f"{cite.full_match} — path-escape"
            )
            violations += 1
            continue
        if not os.path.isfile(resolved):
            # Basename fallback — runbook convention cites by basename for
            # brevity (e.g. `_common.sh::aggregate_worst_status` resolves to
            # `scripts/lang/_common.sh`). Unambiguous match required; multi-
            # match falls through to file-missing so the doc author has to
            # disambiguate.
            fallback = resolve_basename_match(REPO_ROOT, cite.path)
            if fallback is None:
                print(
                    f"VIOLATION: {cite.doc_file} — doc-citation-symbol-unresolved — "
                    f"{cite.full_match} — file-missing"
                )
                violations += 1
                continue
            resolved = fallback

        # Symbol resolution.
        if not symbol_resolves_in_file(resolved, cite.extra):
            print(
                f"VIOLATION: {cite.doc_file} — doc-citation-symbol-unresolved — "
                f"{cite.full_match} — symbol-not-found"
            )
            violations += 1

if violations > 0:
    print(f"\nFound {violations} unresolved symbol cite(s) across {cites_seen} cite(s) in {len(docs)} doc(s).")
    sys.exit(1)
print(f"OK - All doc-symbol cites resolve ({cites_seen} cite(s) checked in {len(docs)} doc(s))")
sys.exit(0)
PYEOF
}

main() {
    init_violations
    start_timer

    print_header "Doc-Citation Symbol-Resolves Guard (Guard C)"
    echo "Scanning: docs/runbooks/**, .claude/skills/**"
    echo ""

    local rc=0
    run_scan || rc=$?

    echo ""
    print_elapsed_time
    exit "$rc"
}

main "$@"
