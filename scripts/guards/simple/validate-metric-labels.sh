#!/bin/bash
#
# Metric-Labels Validation Guard (ADR-0031 Prereq #3)
#
# Enforces per-metric-label invariants for Rust files invoking the `metrics`
# crate's macros (`counter!`, `gauge!`, `histogram!`, `describe_counter!`,
# `describe_gauge!`, `describe_histogram!`):
#
#   1. PII denylist on label KEYS (co-owned with security):
#        email, phone, display_name, user_id (raw), name, address,
#        ip / ip_addr, device_id — plus common prefixes/suffixes (`*_email`,
#        `raw_*`, etc.). Hashed forms (`user_id_hash`, `ip_hash`) are allowed.
#
#   2. Cardinality budget (source-level, per ADR-0011):
#        - String literal label VALUES > 64 chars are flagged. Runtime-bound
#          label values (`.to_string()` of an identifier) are allowed but the
#          author MUST ensure the source is bounded; see taxonomy.md
#          §Bounded-label pattern.
#        - Obviously-unbounded patterns (`Uuid::to_string()`, `request_path`,
#          raw `{email}`-like variable names inside a label value) are flagged.
#
#   3. Label-name hygiene (syntactic only):
#        - Snake_case, no uppercase, no dashes, no dots.
#        - Valid identifier shape (`^[a-z_][a-z0-9_]*$`).
#
# Canonical-alias enforcement (e.g., `svc_type` vs `service_type`) is
# [reviewer-only] per Lead ruling 2026-04-17 — documenting the canonical
# form without machine-enforcing it avoids a grandfather-allowlist scenario
# when the current fleet has drift. See docs/observability/label-taxonomy.md
# §Shared Label Names and the TODO.md entries for the coordinated rename
# migrations.
#
# PER-INVOCATION ESCAPE HATCH
#
#   A macro invocation may bypass check #1 by adding a `# pii-safe: <reason>`
#   comment on the same line as the macro OR on the line immediately
#   preceding the macro call. Reason must be >= 10 chars and must NOT match
#   the lazy set {test, tmp, todo, fixme, wip}.
#
#   The escape hatch does NOT suppress cardinality-budget violations (rule 2)
#   — those are an independent concern and reviewers should scrutinize every
#   literal value length regardless of PII safety.
#
# Exit codes:
#   0 - all pass
#   1 - one or more violations
#   2 - script error (missing python3, unparseable source, etc.)
#
# Usage:
#   ./validate-metric-labels.sh              # scan production metrics.rs files
#   ./validate-metric-labels.sh --self-test  # run fixture-based regression suite

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

CRATES_DIR="$REPO_ROOT/crates"
FIXTURES_DIR="$SCRIPT_DIR/fixtures/metric-labels"

# -----------------------------------------------------------------------------
# Python validator — one file at a time, emits JSON-lines on stdout.
# -----------------------------------------------------------------------------
run_python_validator() {
    local rs_path="$1"

    python3 - "$rs_path" "$REPO_ROOT" <<'PYEOF'
import json
import os
import re
import sys

RS_PATH = sys.argv[1]
REPO_ROOT = sys.argv[2]

# Macros we parse. `describe_*` carries metric name only (labels are declared at
# call sites); we include them so a mistyped metric-name literal still gets
# length-checked, but they don't emit label-key checks.
LABEL_MACROS = ("counter", "gauge", "histogram")
DESCRIBE_MACROS = ("describe_counter", "describe_gauge", "describe_histogram")
ALL_MACROS = LABEL_MACROS + DESCRIBE_MACROS

# --- PII / secret denylist ---------------------------------------------------
# Raw tokens: case-insensitive match; a label key is a hit when it EQUALS one
# of these OR contains one as a component (split on `_`).
#
# NOTE: this list is co-owned with @security. Security may extend at review
# time; any extension should land via an update to taxonomy.md and here.
#
# Category A (secrets): non-negotiable per ADR-0011 §PII & Cardinality
# (lines 156–161). Placing a credential into a metric label exfiltrates it
# to the metrics backend (which typically has weaker access controls than
# the secret store). These MUST be in the denylist; `# pii-safe` cannot
# whitelist a raw secret — if you see a Category A flag, fix the code.
PII_TOKENS_CATEGORY_A = {
    "password",
    "passwd",
    "api_key",
    "apikey",
    "secret",
    # Bare `token` is Category A per Lead ruling 2026-04-17 — strictest
    # safety wins: a bare `token` label key almost always means "the actual
    # token value" which is catastrophic. Legitimate `token_*` labels
    # describing token flavors or subsystems are allowlisted via
    # CATEGORY_A_ALLOWLIST below. New `token_*` uses require an allowlist
    # addition + security/observability co-owner review.
    "token",
    "bearer_token",
    "access_token",
    "refresh_token",
    "session_token",
    "id_token",
    "private_key",
    "privkey",
    "signing_key",
    "jwt",
    "auth_header",
    "authorization",
}

# Category A allowlist: label keys that share a Category A component but are
# semantically safe (describe the token FLAVOR or SUBSYSTEM, not the token
# value). Narrower than LABEL_ALLOWLIST (which covers Category B substring
# false-positives). Per Lead ruling 2026-04-17, additions require security
# co-owner sign-off — default posture is "rename the label" before "extend
# the allowlist."
CATEGORY_A_ALLOWLIST = {
    # Only label keys with actual production usage belong here. Speculative
    # entries invert the "rename before extend" posture documented in
    # taxonomy.md — new `token_*` labels should justify their allowlist
    # entry on first use, not before.
    "token_type",      # bounded enum: meeting / guest / service.
                       # Used by record_jwt_validation() in GC, MC, MH.
}

# Category B (user-PII): personal identifiers. Hashed forms (`_hash`,
# `_sha256`, etc.) are allowed per HASHED_SUFFIXES below.
PII_TOKENS_CATEGORY_B = {
    "email",
    "phone",
    "phone_number",
    "display_name",
    "user_id",
    "name",
    "username",
    "nickname",
    "handle",
    "address",
    "postal_code",
    "zip",
    "zipcode",
    "ip",
    "ip_addr",
    "ipv4",
    "ipv6",
    "device_id",
    "user_agent",
    "fingerprint",
    "ssn",
    "dob",
    "passport",
    "driver_license",
    "credit_card",
    "card_number",
    "cvv",
    "latitude",
    "longitude",
    "geolocation",
    "geoip",
}

PII_TOKENS = PII_TOKENS_CATEGORY_A | PII_TOKENS_CATEGORY_B

# Prefix denylist: labels starting with these are flagged regardless of the
# suffix. `raw_*` is a common anti-pattern ("raw_email", "raw_user_id", etc.)
# — the prefix signals the author knew the value was sensitive but opted out
# of sanitization. Case-insensitive.
PII_PREFIX_DENYLIST = (
    "raw_",
)

# Suffix `_hash` or `_hashed` marks a value as opaque — allowed for
# Category B only. Category A tokens remain denied regardless of suffix
# (you should never hash a credential into a label; just don't include it).
HASHED_SUFFIXES = ("_hash", "_hashed", "_id_hash", "_sha256", "_digest")

# Allowlist: label keys that SUBSTRING-match a PII token but are semantically safe.
# `hostname` contains `name` but is not a PII identifier; keep deliberately
# narrow — prefer renaming labels over extending this list.
LABEL_ALLOWLIST = {
    "hostname",     # contains "name" substring but is not identity-PII
    "filename",     # contains "name" substring
    "pathname",     # contains "name" substring
    "typename",     # contains "name" substring
    "nameservice",  # contains "name" substring
}

# --- Cardinality heuristics --------------------------------------------------
MAX_LITERAL_VALUE_LENGTH = 64

# Literal tokens that indicate obviously-unbounded label values when they
# appear inside a label-value expression. Match as whole-word tokens.
UNBOUNDED_VALUE_PATTERNS = [
    (re.compile(r"\bUuid\s*::\s*(?:new_v4|from_u128|parse_str)\b"), "Uuid::* — per-request UUIDs blow cardinality"),
    (re.compile(r"\bUuid\s*\(\s*\)\s*\.\s*to_string\b"), "Uuid::to_string() — per-request UUIDs blow cardinality"),
    (re.compile(r"\brequest_path\b"), "raw request_path — use a normalizer to bounded set"),
    (re.compile(r"\buser_email\b"), "user_email in label value — PII and unbounded"),
    (re.compile(r"\bSystemTime\s*::\s*now\b"), "SystemTime::now() — unbounded time values"),
]

# --- Escape hatch parsing ----------------------------------------------------
PII_SAFE_RE = re.compile(r"#\s*pii-safe\s*:\s*(.+?)\s*$")
LAZY_REASON_RE = re.compile(r"^(test|tmp|todo|fix ?me|wip)\b", re.IGNORECASE)


# =============================================================================
# Source-span extraction
# =============================================================================

def strip_comments_preserve_layout(src):
    """Replace Rust comment contents with spaces, preserving line numbers
    and column offsets. This is a pre-pass so the macro-opener regex and
    paren walker don't false-positive on `//` lines that contain something
    like `counter!(...`.

    Handles:
      - line comments  `// ... \n`
      - block comments `/* ... */` (non-nested — Rust permits nesting but our
        metric files don't use nested block comments, and the guard is
        conservative-false-positive anyway)

    String literals are respected: `"// not a comment"` stays intact.
    """
    out = []
    i = 0
    n = len(src)
    in_str = False
    str_delim = None
    while i < n:
        c = src[i]
        if in_str:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(src[i + 1])
                i += 2
                continue
            if c == str_delim:
                in_str = False
                str_delim = None
            i += 1
            continue
        # String literal start.
        if c == '"':
            in_str = True
            str_delim = '"'
            out.append(c)
            i += 1
            continue
        # Line comment: replace contents up to (but not including) the
        # newline with spaces. Also strip the leading `//` to whitespace.
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = i
            while j < n and src[j] != "\n":
                j += 1
            out.append(" " * (j - i))
            i = j
            continue
        # Block comment: replace contents with spaces/newlines. Newlines
        # inside the block preserve line numbering.
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            j = i + 2
            while j < n - 1 and not (src[j] == "*" and src[j + 1] == "/"):
                j += 1
            # Include the closing `*/` in the replacement span.
            end = min(j + 2, n)
            span = src[i:end]
            # Keep newlines; replace everything else with spaces.
            replaced = "".join("\n" if ch == "\n" else " " for ch in span)
            out.append(replaced)
            i = end
            continue
        out.append(c)
        i += 1
    return "".join(out)


def find_macro_invocations(src):
    """Yield (macro_name, start_lineno, end_lineno, body) for each macro call.

    `body` is the substring INSIDE the outer `()`; start_lineno is the 1-based
    line of the `<macro>!` token; end_lineno is the 1-based line of the
    closing `)`. This respects nested parens so that `.to_string()` inside a
    label value doesn't close the span early.

    String literals are minimally-respected: we track `"..."` spans and skip
    parens inside them. We don't need to handle `r#"..."#` raw strings because
    metric labels are plain string literals in practice.
    """
    macro_alt = "|".join(re.escape(m) for m in ALL_MACROS)
    # Match either `counter!(` or `metrics::counter!(` — we only care about
    # the macro name and the open paren position.
    opener_re = re.compile(
        rf"(?:\bmetrics\s*::\s*)?\b({macro_alt})\s*!\s*\("
    )

    for m in opener_re.finditer(src):
        macro = m.group(1)
        paren_open_idx = m.end() - 1  # index of the '(' we just matched
        start_lineno = src.count("\n", 0, m.start()) + 1

        depth = 1
        i = paren_open_idx + 1
        in_str = False
        str_delim = None
        while i < len(src) and depth > 0:
            c = src[i]
            if in_str:
                if c == "\\" and i + 1 < len(src):
                    i += 2
                    continue
                if c == str_delim:
                    in_str = False
                    str_delim = None
                i += 1
                continue
            if c == '"':
                in_str = True
                str_delim = '"'
                i += 1
                continue
            if c == "/" and i + 1 < len(src) and src[i + 1] == "/":
                # Line comment — skip to EOL. Inside a macro body this is rare
                # but `# pii-safe` uses `//` not `#` in some codebases; we use
                # `//` markers for Rust-native style and also accept `#` for
                # parity with guard-family convention. Handled by the marker
                # scanner, not here. Just skip the comment content.
                nl = src.find("\n", i)
                if nl == -1:
                    i = len(src)
                else:
                    i = nl
                continue
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            i += 1

        if depth != 0:
            # Unterminated macro — emit a parse-error signal.
            yield (macro, start_lineno, start_lineno, None)
            continue

        body = src[paren_open_idx + 1 : i]
        end_lineno = src.count("\n", 0, i) + 1
        yield (macro, start_lineno, end_lineno, body)


# =============================================================================
# Body parsing: extract label keys + value expressions
# =============================================================================

# We parse the body as comma-separated args at the TOP LEVEL (paren/bracket/
# brace/string aware). Each arg is either:
#   - a string literal (metric name, expected as the first arg)
#   - `"key" => <value-expr>` (a label pair)
#
# We don't fully tokenize Rust — we just need key names and the textual form
# of each value expression.

def split_top_level_args(body):
    """Split `body` on top-level commas. Return list of arg strings (trimmed)."""
    args = []
    depth_p = depth_b = depth_br = 0
    in_str = False
    str_delim = None
    cur = []
    i = 0
    while i < len(body):
        c = body[i]
        if in_str:
            cur.append(c)
            if c == "\\" and i + 1 < len(body):
                cur.append(body[i + 1])
                i += 2
                continue
            if c == str_delim:
                in_str = False
                str_delim = None
            i += 1
            continue
        if c == '"':
            in_str = True
            str_delim = '"'
            cur.append(c)
            i += 1
            continue
        if c == "(":
            depth_p += 1
        elif c == ")":
            depth_p -= 1
        elif c == "[":
            depth_br += 1
        elif c == "]":
            depth_br -= 1
        elif c == "{":
            depth_b += 1
        elif c == "}":
            depth_b -= 1
        elif c == "," and depth_p == depth_b == depth_br == 0:
            args.append("".join(cur).strip())
            cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    tail = "".join(cur).strip()
    if tail:
        args.append(tail)
    return args


STRING_LITERAL_RE = re.compile(r'^"((?:[^"\\]|\\.)*)"$')
# A literal wrapped in a common cheap conversion: "...".to_string(),
# "...".into(), String::from("...") — treat as equivalent to the literal for
# length-checking purposes. Fuller expressions (format!, concat!) defeat this
# heuristic, which is fine — we fall through to the runtime-bound case.
STRING_LITERAL_TO_STRING_RE = re.compile(
    r'^"((?:[^"\\]|\\.)*)"\s*\.\s*(?:to_string|to_owned|into)\s*\(\s*\)\s*$'
)
STRING_FROM_LITERAL_RE = re.compile(
    r'^String\s*::\s*from\s*\(\s*"((?:[^"\\]|\\.)*)"\s*\)\s*$'
)


def parse_string_literal(expr):
    """If `expr` is a simple `"..."` literal (or `"...".to_string()` /
    `"...".into()` / `String::from("...")`), return the decoded contents;
    else return None."""
    s = expr.strip()
    for rx in (STRING_LITERAL_RE, STRING_LITERAL_TO_STRING_RE, STRING_FROM_LITERAL_RE):
        m = rx.match(s)
        if m:
            raw = m.group(1)
            return raw.encode().decode("unicode_escape", errors="replace")
    return None


def extract_label_pairs(body):
    """Yield (key_literal_str_or_None, key_raw_expr, value_raw_expr) tuples.

    First top-level arg is treated as the metric name and skipped. Each
    subsequent arg must look like `<key-expr> => <value-expr>`. Args that
    don't match the `=>` shape are yielded with key_literal_str=None and a
    `parse_error` signal returned via the key_raw_expr==None convention.
    """
    args = split_top_level_args(body)
    if not args:
        return
    # First arg = metric name; yield it as metadata so the caller can
    # length-check it.
    yield ("__metric_name__", args[0], None)

    for arg in args[1:]:
        # Split on `=>` at top level. Most label args have the form
        # `"key" => value`. We only split on the first `=>`.
        idx = _find_top_level_fatarrow(arg)
        if idx < 0:
            yield ("__parse_error__", arg, None)
            continue
        key_expr = arg[:idx].strip()
        val_expr = arg[idx + 2 :].strip()
        key_literal = parse_string_literal(key_expr)
        yield (key_literal, key_expr, val_expr)


def _find_top_level_fatarrow(arg):
    depth_p = depth_b = depth_br = 0
    in_str = False
    str_delim = None
    i = 0
    while i < len(arg) - 1:
        c = arg[i]
        if in_str:
            if c == "\\" and i + 1 < len(arg):
                i += 2
                continue
            if c == str_delim:
                in_str = False
                str_delim = None
            i += 1
            continue
        if c == '"':
            in_str = True
            str_delim = '"'
            i += 1
            continue
        if c == "(":
            depth_p += 1
        elif c == ")":
            depth_p -= 1
        elif c == "[":
            depth_br += 1
        elif c == "]":
            depth_br -= 1
        elif c == "{":
            depth_b += 1
        elif c == "}":
            depth_b -= 1
        elif (
            c == "="
            and arg[i + 1] == ">"
            and depth_p == depth_b == depth_br == 0
        ):
            return i
        i += 1
    return -1


# =============================================================================
# Checks
# =============================================================================

def is_hashed_label(label):
    return any(label.endswith(suf) for suf in HASHED_SUFFIXES)


def pii_token_hit(label):
    """Return (token, category) where category is 'A' (secret),
    'B' (user-PII), or 'prefix' (raw_* prefix); or None for no match.

    Match semantics: case-insensitive exact match, `_`-component match, or
    multi-word-substring match. Catches `user_id`, `user_email`,
    `customer_email`, `client_ip`, `source_ip_addr`, `bearer_token`, etc.

    Rules:
      - Category A (secrets): match always wins; hashed-suffix exemption
        does NOT apply (never hash a credential into a label — just omit it).
      - Category B (user-PII): hashed-suffix exemption applies.
      - Prefix denylist (`raw_*`): fires regardless of suffix.
      - Allowlist (`hostname`, `filename`, ...): suppresses Category B only.
    """
    if not label:
        return None
    lower = label.lower()

    # Prefix denylist runs first — `raw_*` is always a smell even if the rest
    # of the label is a non-PII token.
    for prefix in PII_PREFIX_DENYLIST:
        if lower.startswith(prefix):
            return (prefix, "prefix")

    # Category A: the CATEGORY_A_ALLOWLIST narrowly exempts specific
    # `token_*` flavor/subsystem names that are semantically safe. Anything
    # else matching a Category A token fires — `# pii-safe` cannot bypass.
    if lower not in CATEGORY_A_ALLOWLIST:
        a_hit = _token_hit_in_set(lower, PII_TOKENS_CATEGORY_A)
        if a_hit is not None:
            return (a_hit, "A")

    # Category B: allowlist + hashed-suffix exemptions apply.
    if label in LABEL_ALLOWLIST:
        return None
    if is_hashed_label(label):
        return None
    b_hit = _token_hit_in_set(lower, PII_TOKENS_CATEGORY_B)
    if b_hit is not None:
        return (b_hit, "B")
    return None


def _token_hit_in_set(lower, token_set):
    """Shared token-match helper used by both categories.

    Returns the matching token or None.
    """
    if lower in token_set:
        return lower
    # Multi-word tokens (`ip_addr`, `display_name`, `private_key`, ...):
    # whole-substring match since `_`-split won't keep them together.
    for tok in token_set:
        if "_" in tok and tok in lower:
            return tok
    parts = set(lower.split("_"))
    for tok in token_set:
        if "_" in tok:
            continue
        if tok in parts:
            return tok
    return None


def naming_hygiene_issues(label):
    """Return a list of (kind, message) for non-canonical label-name issues."""
    out = []
    if not label:
        return out
    if label != label.lower():
        out.append((
            "label_naming",
            f"label key {label!r} contains uppercase; must be snake_case lowercase",
        ))
    if "-" in label or "." in label or " " in label:
        out.append((
            "label_naming",
            f"label key {label!r} contains disallowed character(s); "
            f"use snake_case [a-z0-9_]",
        ))
    if label and not re.match(r"^[a-z_][a-z0-9_]*$", label):
        # Covers leading digits, punctuation other than above.
        out.append((
            "label_naming",
            f"label key {label!r} is not a valid snake_case identifier",
        ))
    return out


def unbounded_value_hit(value_expr):
    """Return (pattern_label, match_text) for first obviously-unbounded match."""
    if not value_expr:
        return None
    for rx, label in UNBOUNDED_VALUE_PATTERNS:
        m = rx.search(value_expr)
        if m:
            return (label, m.group(0))
    return None


def literal_value_too_long(value_expr):
    """If value_expr is a `"..."` literal longer than MAX_LITERAL_VALUE_LENGTH,
    return its decoded length. Else return None."""
    lit = parse_string_literal(value_expr)
    if lit is None:
        return None
    if len(lit) > MAX_LITERAL_VALUE_LENGTH:
        return len(lit)
    return None


# =============================================================================
# Escape-hatch marker scan
# =============================================================================

def load_pii_safe_markers(lines):
    """Return {lineno: reason} for each `# pii-safe: <reason>` or
    `// pii-safe: <reason>` comment. Lazy reasons are emitted as diagnostics
    and NOT honored."""
    markers = {}
    diagnostics = []
    # Accept both `#` and `//` prefixes — Rust source normally uses `//` but
    # the guard-family convention is `#`. We accept both for ergonomics.
    marker_re = re.compile(
        r"(?:#|//)\s*pii-safe\s*:\s*(.+?)\s*$"
    )
    for idx, line in enumerate(lines, start=1):
        m = marker_re.search(line)
        if not m:
            continue
        reason = m.group(1).strip()
        # Trim trailing */ or similar. Not bulletproof; rare case.
        if reason.endswith("*/"):
            reason = reason[:-2].strip()
        if len(reason) < 10 or LAZY_REASON_RE.match(reason):
            diagnostics.append((
                idx,
                f"pii-safe reason too short or too vague: {reason!r} "
                f"(require >=10 chars, not test/tmp/todo/fixme/wip)",
            ))
            continue
        markers[idx] = reason
    return markers, diagnostics


def invocation_is_pii_safe(markers, start_lineno):
    """A marker on the macro line OR the line immediately above applies."""
    if start_lineno in markers:
        return markers[start_lineno]
    if (start_lineno - 1) in markers:
        return markers[start_lineno - 1]
    return None


# =============================================================================
# Main
# =============================================================================

def emit(kind, file, line, message):
    sys.stdout.write(json.dumps({
        "file": file,
        "line": line,
        "kind": kind,
        "message": message,
    }) + "\n")


def main():
    try:
        with open(RS_PATH, "r", encoding="utf-8") as f:
            src = f.read()
    except OSError as exc:
        sys.stdout.write(json.dumps({"error": f"cannot read {RS_PATH}: {exc}"}) + "\n")
        sys.exit(2)

    rel_path = os.path.relpath(RS_PATH, REPO_ROOT)
    lines = src.splitlines()
    markers, lazy_diags = load_pii_safe_markers(lines)

    for lineno, msg in lazy_diags:
        emit("lazy_pii_safe_reason", rel_path, lineno, msg)

    # Strip comments BEFORE scanning for macro openers: a line comment like
    # `// counter!(...` must not be treated as a real invocation. Layout
    # (line numbers, column offsets) is preserved so violation line numbers
    # still correspond to the original source.
    stripped_src = strip_comments_preserve_layout(src)

    for macro, start_ln, end_ln, body in find_macro_invocations(stripped_src):
        if body is None:
            emit(
                "parse_error",
                rel_path,
                start_ln,
                f"could not find matching ')' for {macro}! invocation",
            )
            continue

        pii_safe_reason = invocation_is_pii_safe(markers, start_ln)

        is_describe = macro in DESCRIBE_MACROS

        for key_literal, key_raw, val_raw in extract_label_pairs(body):
            if key_literal == "__metric_name__":
                # Metric-name literal: we don't run PII checks, but we DO
                # length-check it via the same literal-value limit (metric
                # names are part of cardinality). Also enforce snake_case on
                # metric names.
                lit = parse_string_literal(key_raw)
                if lit is None:
                    continue
                if len(lit) > MAX_LITERAL_VALUE_LENGTH:
                    emit(
                        "metric_name_length",
                        rel_path,
                        start_ln,
                        f"metric name {lit!r} is {len(lit)} chars "
                        f"(> {MAX_LITERAL_VALUE_LENGTH} char limit)",
                    )
                if lit and not re.match(r"^[a-z_][a-z0-9_]*$", lit):
                    emit(
                        "metric_name_naming",
                        rel_path,
                        start_ln,
                        f"metric name {lit!r} is not snake_case "
                        f"(must match ^[a-z_][a-z0-9_]*$)",
                    )
                continue

            if key_literal == "__parse_error__":
                # Could not find `=>` — either a trailing argument like
                # `"name"` (no labels) or malformed source. Describe macros
                # allow no-label forms; `counter!("name")` is also valid.
                # Treat as parse-warning only when this isn't describe/short.
                if not is_describe and key_raw and not STRING_LITERAL_RE.match(key_raw.strip()):
                    emit(
                        "parse_error",
                        rel_path,
                        start_ln,
                        f"could not find '=>' in label arg {key_raw!r}",
                    )
                continue

            if is_describe:
                # describe_* takes (name, description) — no labels. If we've
                # already emitted the metric-name check above, we're done.
                # Anything else after the name is a description string, not a
                # label. Skip further processing.
                continue

            # Rule 3: naming hygiene (always runs, even when pii-safe applies
            # — pii-safe covers PII keys and canonical aliases; it doesn't
            # license uppercase or punctuation).
            if key_literal is not None:
                for kind, msg in naming_hygiene_issues(key_literal):
                    emit(kind, rel_path, start_ln, msg)

                # Rule 1: PII / secret denylist.
                # - Category A (secrets): ALWAYS fires — `# pii-safe` cannot
                #   whitelist a credential-in-label. Non-negotiable per
                #   ADR-0011 §PII & Cardinality.
                # - Category B (user-PII) and prefix (`raw_*`): suppressed by
                #   a valid `# pii-safe: <reason>`.
                hit = pii_token_hit(key_literal)
                if hit is not None:
                    tok, category = hit
                    if category == "A":
                        emit(
                            "label_secret",
                            rel_path,
                            start_ln,
                            f"label key {key_literal!r} matches secret "
                            f"denylist token {tok!r} (Category A, "
                            f"non-bypassable per ADR-0011) — never place "
                            f"credentials in metric labels; remove the label",
                        )
                    elif pii_safe_reason is None:
                        if category == "prefix":
                            emit(
                                "label_pii",
                                rel_path,
                                start_ln,
                                f"label key {key_literal!r} has denylisted "
                                f"prefix {tok!r} — the `raw_` prefix "
                                f"signals an unsanitized identifier; rename "
                                f"or add `# pii-safe: <reason>`",
                            )
                        else:
                            emit(
                                "label_pii",
                                rel_path,
                                start_ln,
                                f"label key {key_literal!r} matches PII "
                                f"denylist token {tok!r} — use a hashed/"
                                f"opaque form (e.g., {key_literal}_hash) "
                                f"or add `# pii-safe: <reason>` on or "
                                f"above this line",
                            )

            # Rule 2: cardinality — literal value too long (always runs;
            # pii-safe does NOT suppress this).
            if val_raw is not None:
                too_long = literal_value_too_long(val_raw)
                if too_long is not None:
                    emit(
                        "literal_value_length",
                        rel_path,
                        start_ln,
                        f"label value literal is {too_long} chars "
                        f"(> {MAX_LITERAL_VALUE_LENGTH}) — "
                        f"label values are series dimensions; keep short",
                    )

                # Rule 2: obviously-unbounded source (always runs; pii-safe
                # does NOT suppress — an unbounded cardinality source is a
                # fleet-wide hazard independent of PII).
                ub = unbounded_value_hit(val_raw)
                if ub is not None:
                    pat_label, match_text = ub
                    emit(
                        "unbounded_value",
                        rel_path,
                        start_ln,
                        f"label value expression contains {match_text!r}: "
                        f"{pat_label}",
                    )


if __name__ == "__main__":
    main()
PYEOF
}

# -----------------------------------------------------------------------------
# Scan a single .rs file. Returns 0/1/2 per guard family convention.
# -----------------------------------------------------------------------------
validate_file() {
    local rs_path="$1"

    local output
    if ! output=$(run_python_validator "$rs_path" 2>&1); then
        echo -e "${RED}ERROR:${NC} python validator failed on $rs_path" >&2
        echo "$output" >&2
        return 2
    fi

    if echo "$output" | grep -q '"error"'; then
        echo -e "${RED}ERROR:${NC} $output" >&2
        return 2
    fi

    local file_violations=0
    local formatted
    if ! formatted=$(printf '%s\n' "$output" | python3 -c '
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    try:
        d = json.loads(line)
    except json.JSONDecodeError:
        continue
    print("\t".join([
        d.get("file", ""),
        str(d.get("line", 0)),
        d.get("kind", ""),
        d.get("message", ""),
    ]))
' 2>&1); then
        echo -e "${RED}ERROR:${NC} failed to format violations" >&2
        echo "$formatted" >&2
        return 2
    fi

    while IFS=$'\t' read -r file line_num kind message; do
        [[ -z "$file$line_num$kind$message" ]] && continue
        echo -e "${RED}VIOLATION:${NC} ${file}:${line_num} (${kind}) ${message}"
        file_violations=$((file_violations + 1))
        increment_violations
    done <<< "$formatted"

    if [[ "$file_violations" -eq 0 ]]; then
        local rel_path="${rs_path#"$REPO_ROOT/"}"
        print_ok "$rel_path"
    fi
}

# -----------------------------------------------------------------------------
# Enumerate Rust files that invoke metrics macros.
# -----------------------------------------------------------------------------
find_metric_files() {
    # Primary source: crates/*/src/observability/metrics.rs
    shopt -s nullglob
    local primary=()
    for f in "$CRATES_DIR"/*/src/observability/metrics.rs; do
        primary+=("$f")
    done
    shopt -u nullglob

    # Secondary: any other Rust file invoking the macros. Grep is cheap and
    # keeps us honest if instrumentation leaks into non-observability files.
    # We include crates/ only; skip vendored / target directories.
    local secondary=()
    if command -v grep >/dev/null 2>&1; then
        local extra
        extra=$(grep -rEl --include="*.rs" \
            --exclude-dir=target --exclude-dir=vendor \
            -e "\b(counter|gauge|histogram|describe_counter|describe_gauge|describe_histogram)!\s*\(" \
            "$CRATES_DIR" 2>/dev/null || true)
        while IFS= read -r line; do
            [[ -z "$line" ]] && continue
            # Skip primary files (already in list).
            local skip=0
            for p in "${primary[@]}"; do
                if [[ "$p" == "$line" ]]; then
                    skip=1
                    break
                fi
            done
            [[ "$skip" -eq 0 ]] && secondary+=("$line")
        done <<< "$extra"
    fi

    printf '%s\n' "${primary[@]}" "${secondary[@]}"
}

# -----------------------------------------------------------------------------
# Self-test: iterate fixtures, assert exit from filename (pass-*/fail-*).
# -----------------------------------------------------------------------------
self_test() {
    echo ""
    echo "========================================="
    echo "Metric-Labels Guard — Self-Test"
    echo "========================================="
    echo ""

    if [[ ! -d "$FIXTURES_DIR" ]]; then
        echo -e "${RED}FAIL: fixtures dir missing: $FIXTURES_DIR${NC}" >&2
        exit 2
    fi

    local passed=0 failed=0
    shopt -s nullglob
    for fixture in "$FIXTURES_DIR"/*.rs; do
        local name
        name=$(basename "$fixture")
        local expected_exit

        case "$name" in
            pass-*) expected_exit=0 ;;
            fail-*) expected_exit=1 ;;
            *) continue ;;
        esac

        local output
        local violation_count=0
        if output=$(run_python_validator "$fixture" 2>&1); then
            violation_count=$(echo "$output" | grep -c '"kind"' || true)
        else
            echo -e "${RED}SELF-TEST ERROR:${NC} python validator blew up on $name" >&2
            echo "$output" >&2
            failed=$((failed + 1))
            continue
        fi

        local actual_exit=0
        if [[ "$violation_count" -gt 0 ]]; then
            actual_exit=1
        fi

        if [[ "$actual_exit" -eq "$expected_exit" ]]; then
            echo -e "  ${GREEN}PASS${NC} $name (violations=$violation_count)"
            passed=$((passed + 1))
        else
            echo -e "  ${RED}FAIL${NC} $name (expected_exit=$expected_exit, got=$actual_exit, violations=$violation_count)"
            echo "$output" | sed 's/^/    /'
            failed=$((failed + 1))
        fi
    done
    shopt -u nullglob

    echo ""
    echo -e "Self-test: ${GREEN}${passed} passed${NC}, ${RED}${failed} failed${NC}"
    if [[ "$failed" -gt 0 ]]; then
        exit 1
    fi
    exit 0
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
main() {
    init_violations
    start_timer

    if [[ "${1:-}" == "--self-test" ]]; then
        self_test
    fi

    print_header "Metric-Labels Validation Guard"
    echo "Scanning: $CRATES_DIR"
    echo ""

    local rs_files=()
    while IFS= read -r f; do
        [[ -z "$f" ]] && continue
        rs_files+=("$f")
    done < <(find_metric_files)

    if [[ "${#rs_files[@]}" -eq 0 ]]; then
        echo -e "${YELLOW}No metrics.rs files found under $CRATES_DIR${NC}"
        print_elapsed_time
        exit 0
    fi

    local file_error=0
    for rs_path in "${rs_files[@]}"; do
        if ! validate_file "$rs_path"; then
            file_error=1
        fi
    done

    echo ""
    print_elapsed_time
    local total
    total=$(get_violations)

    if [[ "$file_error" -ne 0 ]]; then
        exit 2
    fi

    if [[ "$total" -gt 0 ]]; then
        echo -e "${RED}Found $total violation(s)${NC}"
        echo ""
        echo "See docs/observability/label-taxonomy.md for rule rationale and migration guidance."
        echo "Fleet-wide series budget: 5M (runtime-enforced, not source-checked)."
        exit 1
    fi

    echo -e "${GREEN}All metric labels pass.${NC}"
    exit 0
}

main "$@"
