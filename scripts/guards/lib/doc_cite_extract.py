"""Shared cite-extraction primitives for doc-citation durability guards.

Validated failure modes exercised by the doc-citation guards (per security
review and §Correctness validation of the authoring devloop):
    Guard A (no-line-numbers): bare-cite detection, URL-port allowlist, lazy
        guard:ignore reason rejection, multi-extension coverage, code-block
        cites trip by design (content scan, not AST).
    Guard C (symbol-resolves): file-missing, path-escape (../../ outside repo,
        symlink-escape), symbol-not-found per language, single-segment-only
        symbols, top-level-only YAML keys, word-boundary heading match.
    Basename-fallback (resolve_basename_match): unambiguous single-match resolves
        transparently; ambiguous multi-match returns None and the caller emits
        `file-missing`, forcing the doc author to disambiguate via full path;
        no-match returns None same as above. Walks 4 search roots — `scripts/`,
        `crates/`, `infra/`, `proto/` — excluding `docs/` and `.claude/` so
        doc-to-doc cites are forced to full repo-relative paths.
"""

import os
import re
from dataclasses import dataclass
from typing import List, Optional

# -----------------------------------------------------------------------------
# Constants — single source of truth for Guards A and C
# -----------------------------------------------------------------------------

# Extensions that count as file-shaped path tokens. Excludes URL-domain
# components like `.local`, `.com`, `.cluster` so hostnames with ports
# (e.g. gc-service.dark-tower.svc.cluster.local:5432) do not trip the
# bare-line-cite detector.
EXTENSION_ALLOWLIST = frozenset(
    {"rs", "sh", "toml", "yaml", "yml", "md", "proto", "json", "ts", "tsx", "js"}
)

# Path-token shape. Left-edge negative-lookbehind prevents prefix-collision
# with URL-shaped tokens. Path is anchored by a dotted extension component.
# Single segment basename or dotted/slashed path both match.
_PATH_PREFIX = r"(?<![\w./-])([A-Za-z_][\w./-]*\.[a-z]{1,5})"

# Bare line-number cite: `<path>:NN` or `<path>:NN-NN`. The trailing word
# boundary prevents `<path>:NN0` from matching `<path>:NN`-style cites.
BARE_LINE_CITE_RE = re.compile(_PATH_PREFIX + r":(\d+)(?:-(\d+))?\b")

# Symbol cite: `<path>::<symbol>`. Single-segment symbol only — multi-segment
# (Mod::Type::method) cites must be rewritten to one segment + prose
# disambiguator. The negative-lookahead `(?!:)` ensures we don't slice into
# a `:::` triple-colon sequence.
SYMBOL_CITE_RE = re.compile(_PATH_PREFIX + r"::([A-Za-z_]\w*)\b")

# guard:ignore HTML-comment marker for markdown docs.
GUARD_IGNORE_RE = re.compile(r"<!--\s*guard:ignore\(\s*([^)]+?)\s*\)\s*-->")

# Lazy-reason rejection: same vocabulary as
# validate-alert-rules.sh::load_ignore_lines (≥10 chars, not test/tmp/todo/
# fixme/wip). The kernel is shared; alert-rules-side `load_ignore_lines`
# refactors to call is_lazy_reason() so the vocabulary lives in one place.
_LAZY_REASON_RE = re.compile(r"^(test|tmp|todo|fix ?me|wip)\b", re.IGNORECASE)
_MIN_REASON_LEN = 10


# -----------------------------------------------------------------------------
# Records
# -----------------------------------------------------------------------------

@dataclass(frozen=True)
class Cite:
    """One extracted cite from a doc line.

    kind: "bare-line" or "symbol".
    path: cited filepath as it appears in the doc.
    extra: line-range string (e.g. "36" or "120-126") for "bare-line";
           symbol name for "symbol".
    """
    doc_file: str
    line_no: int
    kind: str
    path: str
    extra: str
    full_match: str
    is_ignored: bool = False


# -----------------------------------------------------------------------------
# Public functions
# -----------------------------------------------------------------------------

def is_lazy_reason(text: str) -> bool:
    """Return True if a guard:ignore reason is too vague to honor.

    Shared kernel — three call sites: this module's docs guards plus
    validate-alert-rules.sh::load_ignore_lines (post-refactor).
    """
    if not isinstance(text, str):
        return True
    text = text.strip()
    if len(text) < _MIN_REASON_LEN:
        return True
    if _LAZY_REASON_RE.match(text):
        return True
    return False


def has_recognized_extension(path: str) -> bool:
    """Return True if path's extension is in EXTENSION_ALLOWLIST.

    A path that matches the regex but lands on a non-allowlisted extension
    (e.g. `foo.bar:42`) is reported by the regex but suppressed here so
    Guard A doesn't trip on arbitrary `.<n-chars>:digit` strings.
    """
    if "." not in path:
        return False
    ext = path.rsplit(".", 1)[1].lower()
    return ext in EXTENSION_ALLOWLIST


def extract_cites(doc_file: str, doc_text: str) -> List[Cite]:
    """Extract bare-line and symbol cites from a doc's text.

    Returns Cite records with is_ignored set per the line's
    `<!-- guard:ignore(<reason>) -->` annotation, with lazy reasons rejected.

    doc_file is relative-to-repo-root (caller's responsibility).
    """
    out: List[Cite] = []
    for lineno, line in enumerate(doc_text.splitlines(), start=1):
        # Determine ignore status once per line.
        is_ignored = False
        m = GUARD_IGNORE_RE.search(line)
        if m and not is_lazy_reason(m.group(1)):
            is_ignored = True

        # Symbol cites first — we want `foo.rs::bar` to NOT also be flagged
        # by the bare-line regex (it won't, since `::` precludes `:NN`, but
        # this ordering keeps the search deterministic for tests).
        for sm in SYMBOL_CITE_RE.finditer(line):
            path, sym = sm.group(1), sm.group(2)
            if not has_recognized_extension(path):
                continue
            out.append(Cite(
                doc_file=doc_file, line_no=lineno,
                kind="symbol", path=path, extra=sym,
                full_match=sm.group(0), is_ignored=is_ignored,
            ))

        for bm in BARE_LINE_CITE_RE.finditer(line):
            path, start, end = bm.group(1), bm.group(2), bm.group(3)
            if not has_recognized_extension(path):
                continue
            extra = start if end is None else f"{start}-{end}"
            out.append(Cite(
                doc_file=doc_file, line_no=lineno,
                kind="bare-line", path=path, extra=extra,
                full_match=bm.group(0), is_ignored=is_ignored,
            ))
    return out


# -----------------------------------------------------------------------------
# Symbol-resolution per-language patterns (Guard C)
# -----------------------------------------------------------------------------

def _build_rs_pattern(sym: str) -> re.Pattern:
    return re.compile(
        r"\b(?:fn|struct|enum|trait|impl|const|static|type)\s+" + re.escape(sym) + r"\b"
    )


def _build_sh_pattern(sym: str) -> re.Pattern:
    # Top-level only by design (left-anchored). Local/nested defs not supported.
    e = re.escape(sym)
    return re.compile(rf"^{e}\s*\(\s*\)|^function\s+{e}\b", re.MULTILINE)


def _build_toml_pattern(sym: str) -> re.Pattern:
    e = re.escape(sym)
    return re.compile(rf"^\[{e}\]|^{e}\s*=", re.MULTILINE)


def _build_yaml_pattern(sym: str) -> re.Pattern:
    # Top-level only — line-start, no leading whitespace.
    return re.compile(rf"^{re.escape(sym)}\s*:", re.MULTILINE)


def _build_md_pattern(sym: str) -> re.Pattern:
    # Heading text with word-boundary on the end of <sym> (not full-equality).
    # F3(c): `foo.md::Test` must NOT match `## Testing Setup`.
    return re.compile(rf"^#+\s+.*\b{re.escape(sym)}\b", re.MULTILINE | re.IGNORECASE)


def _build_proto_pattern(sym: str) -> re.Pattern:
    return re.compile(
        r"\b(?:message|service|enum|rpc)\s+" + re.escape(sym) + r"\b"
    )


_PATTERN_BUILDERS = {
    "rs":    _build_rs_pattern,
    "sh":    _build_sh_pattern,
    "toml":  _build_toml_pattern,
    "yaml":  _build_yaml_pattern,
    "yml":   _build_yaml_pattern,
    "md":    _build_md_pattern,
    "proto": _build_proto_pattern,
}


def supported_resolution_extensions() -> List[str]:
    """Extensions Guard C knows how to resolve. Others skip silently."""
    return sorted(_PATTERN_BUILDERS.keys())


def symbol_resolves_in_file(file_path: str, symbol: str) -> bool:
    """Return True if `symbol` appears as a definable construct in file_path.

    The caller is responsible for path-safety (existence + traversal check) —
    this function only does the regex match on file contents.
    """
    ext = file_path.rsplit(".", 1)[-1].lower() if "." in file_path else ""
    builder = _PATTERN_BUILDERS.get(ext)
    if builder is None:
        # Extensions outside the resolution table are silently allowed at
        # the caller level — they should not reach this function.
        return True
    pat = builder(symbol)
    try:
        with open(file_path, "r", encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError:
        return False
    return bool(pat.search(text))


def resolve_cited_path(repo_root: str, cited_path: str) -> Optional[str]:
    """Resolve cited_path under repo_root, enforcing in-repo containment.

    Returns the resolved absolute path on success, or None if the path
    escapes repo_root (traversal or symlink-escape). Caller distinguishes
    file-missing from path-escape by checking os.path.isfile on the result.
    """
    abs_target = os.path.normpath(os.path.join(repo_root, cited_path))
    try:
        resolved = os.path.realpath(abs_target)
        root_real = os.path.realpath(repo_root)
    except OSError:
        return None
    if resolved == root_real:
        return resolved
    if not resolved.startswith(root_real + os.sep):
        return None
    return resolved


# Cache for basename search: {basename: [abs_paths...]} populated lazily.
_BASENAME_INDEX: Optional[dict] = None

# Search roots for basename-only cites. Runbook convention is to cite by
# basename (e.g. `_common.sh::aggregate_worst_status`) — the basename
# fallback walks these roots and resolves an unambiguous match.
_BASENAME_SEARCH_ROOTS = ("scripts", "crates", "infra", "proto")


def _build_basename_index(repo_root: str) -> dict:
    """Walk _BASENAME_SEARCH_ROOTS once; populate {basename: [abs_paths...]}.

    Side effect: callers cache the returned dict in module-level _BASENAME_INDEX
    (see resolve_basename_match). First call walks 4 trees from repo_root
    (scripts/, crates/, infra/, proto/); subsequent calls reuse the cached
    index. Cost ~40ms one-shot on the current repo.
    """
    index: dict = {}
    for root in _BASENAME_SEARCH_ROOTS:
        abs_root = os.path.join(repo_root, root)
        if not os.path.isdir(abs_root):
            continue
        for dirpath, _dirnames, filenames in os.walk(abs_root):
            for fn in filenames:
                index.setdefault(fn, []).append(os.path.join(dirpath, fn))
    return index


def resolve_basename_match(repo_root: str, cited_path: str) -> Optional[str]:
    """If cited_path is a basename-only token, find unambiguous match.

    Returns the absolute path if exactly one match exists across the search
    roots; None otherwise (no match OR ambiguous multi-match). Multi-match
    returns None so doc authors must disambiguate via a full repo-relative
    path rather than getting a silent wrong-file resolution.

    Search roots are `scripts/`, `crates/`, `infra/`, `proto/` — the union
    of places source-of-truth symbols live (build tooling, services, infra
    manifests, wire format). `docs/` and `.claude/` are intentionally
    excluded so doc-to-doc cites are forced to full paths.

    A cited path containing `/` is NOT basename-only and returns None;
    callers should treat such cites as full repo-relative paths.

    Side effect: lazily builds module-level `_BASENAME_INDEX` on first
    call. Subsequent calls reuse the index — if the filesystem changes
    between calls in a long-lived process, results may stale. Fine for
    one-shot guard invocations.
    """
    global _BASENAME_INDEX
    if "/" in cited_path:
        return None
    if _BASENAME_INDEX is None:
        _BASENAME_INDEX = _build_basename_index(repo_root)
    matches = _BASENAME_INDEX.get(cited_path, [])
    if len(matches) != 1:
        return None
    return matches[0]


# -----------------------------------------------------------------------------
# Scope (in-scope file walk)
# -----------------------------------------------------------------------------

# In-scope doc trees the new guards walk. Defined here (not shell-side) so
# both guards consume identical scope; matches §Planning §Shared internals.
IN_SCOPE_DIRS = ("docs/runbooks", ".claude/skills")


def is_in_scope_doc(rel_path: str) -> bool:
    """Return True if rel_path is one of the in-scope markdown docs."""
    if not rel_path.endswith(".md"):
        return False
    for d in IN_SCOPE_DIRS:
        if rel_path == d or rel_path.startswith(d + "/"):
            return True
    return False


def walk_in_scope_docs(repo_root: str):
    """Yield (rel_path, abs_path) for each in-scope markdown doc under repo_root.

    Mechanically derived from IN_SCOPE_DIRS — single source of truth. Both
    Guards A and C consume this; expanding scope is a one-line edit to
    IN_SCOPE_DIRS above. The shell helper `common.sh::doc_citation_in_scope_files`
    is kept for parity but currently has no Bash-side consumer.
    """
    seen = []
    for sub in IN_SCOPE_DIRS:
        abs_sub = os.path.join(repo_root, sub)
        if not os.path.isdir(abs_sub):
            continue
        for dirpath, _dirnames, filenames in os.walk(abs_sub):
            for fn in filenames:
                if not fn.endswith(".md"):
                    continue
                abs_path = os.path.join(dirpath, fn)
                rel = os.path.relpath(abs_path, repo_root)
                if is_in_scope_doc(rel):
                    seen.append((rel, abs_path))
    seen.sort()
    return seen
