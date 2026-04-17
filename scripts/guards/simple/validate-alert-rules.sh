#!/bin/bash
#
# Alert-Rules Validation Guard (ADR-0031 Prereq #1)
#
# Enforces per-alert invariants for Prometheus alert-rule files:
#   1. annotations.runbook_url present
#   2. runbook_url repo-relative under docs/runbooks/ AND target exists on disk
#   3. labels.severity in {page, warning, info}
#   4. for: duration >= 30s
#   5. annotation text free of hostnames, credentials, IPs, secrets
#   6. per-rule `# guard:ignore(<reason>)` exempts rule from check 5 only
#
# LEGACY ALLOWLIST
#
# Files named in scripts/guards/simple/alert-rules.legacy-allowlist run in
# lenient mode: checks 2 and 3 are relaxed (absolute URLs allowed; severity
# value-set extended to include "critical"). Presence/duration/hygiene checks
# remain enforced. Non-expansion enforced via count-pin + approval marker.
#
# TEMPLATE FILES
#
# Files matching `_template-*.yaml` are skipped entirely.
#
# Exit codes:
#   0 - all pass
#   1 - one or more violations
#   2 - script error (missing python3/pyyaml, bad yaml, etc.)
#
# Usage:
#   ./validate-alert-rules.sh              # scan production alert-rule files
#   ./validate-alert-rules.sh --self-test  # run fixture-based regression suite

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

ALERTS_DIR="$REPO_ROOT/infra/docker/prometheus/rules"
RUNBOOKS_DIR="$REPO_ROOT/docs/runbooks"
ALLOWLIST_FILE="$SCRIPT_DIR/alert-rules.legacy-allowlist"
FIXTURES_DIR="$SCRIPT_DIR/fixtures/alert-rules"
MIGRATION_DEADLINE="2026-06-30"

# Non-expansion pin. Bump when adding a legitimate legacy entry (with an
# ALLOWLIST_EXPANSION_APPROVED_BY marker in the same PR). Decrement when
# removing (migration complete).
readonly EXPECTED_ALLOWLIST_COUNT=2

# -----------------------------------------------------------------------------
# Mode detection: is a given absolute file path on the legacy allowlist?
# -----------------------------------------------------------------------------
is_legacy_file() {
    local abs_path="$1"
    local rel_path="${abs_path#"$REPO_ROOT/"}"
    [[ -f "$ALLOWLIST_FILE" ]] || return 1
    grep -vE '^\s*(#|$)' "$ALLOWLIST_FILE" | grep -Fxq -- "$rel_path"
}

# -----------------------------------------------------------------------------
# Non-expansion check: allowlist active-line count must match the pin, unless
# an ALLOWLIST_EXPANSION_APPROVED_BY marker appears in the committed tree.
# -----------------------------------------------------------------------------
check_allowlist_integrity() {
    [[ -f "$ALLOWLIST_FILE" ]] || {
        echo -e "${RED}FAIL: allowlist file missing: $ALLOWLIST_FILE${NC}" >&2
        return 1
    }

    local actual_count
    actual_count=$(grep -cvE '^\s*(#|$)' "$ALLOWLIST_FILE" || true)

    if [[ "$actual_count" -ne "$EXPECTED_ALLOWLIST_COUNT" ]]; then
        local marker_present=0
        if git -C "$REPO_ROOT" grep -l '# ALLOWLIST_EXPANSION_APPROVED_BY:' >/dev/null 2>&1; then
            marker_present=1
        fi

        if [[ "$actual_count" -gt "$EXPECTED_ALLOWLIST_COUNT" && "$marker_present" -eq 0 ]]; then
            echo -e "${RED}FAIL: legacy allowlist grew from $EXPECTED_ALLOWLIST_COUNT to $actual_count without approval marker.${NC}" >&2
            echo "      To add a legacy entry, include in the PR a committed line:" >&2
            echo "          # ALLOWLIST_EXPANSION_APPROVED_BY: <specialist-name>" >&2
            echo "      Then bump EXPECTED_ALLOWLIST_COUNT in $0 to $actual_count." >&2
            return 1
        fi

        if [[ "$actual_count" -lt "$EXPECTED_ALLOWLIST_COUNT" ]]; then
            echo -e "${RED}FAIL: legacy allowlist shrunk from $EXPECTED_ALLOWLIST_COUNT to $actual_count.${NC}" >&2
            echo "      Migration complete? Decrement EXPECTED_ALLOWLIST_COUNT in $0 to $actual_count." >&2
            return 1
        fi
    fi

    return 0
}

# -----------------------------------------------------------------------------
# Python validator — one file at a time, emits JSON-lines on stdout.
# -----------------------------------------------------------------------------
run_python_validator() {
    local yaml_path="$1"
    local mode="$2"

    python3 - "$yaml_path" "$mode" "$RUNBOOKS_DIR" "$REPO_ROOT" <<'PYEOF'
import json
import os
import re
import sys

try:
    import yaml
except ImportError:
    print(json.dumps({"error": "PyYAML not available. Install with: pip install pyyaml"}))
    sys.exit(2)

YAML_PATH = sys.argv[1]
MODE = sys.argv[2]
RUNBOOKS_DIR = sys.argv[3]
REPO_ROOT = sys.argv[4]

STRICT_SEVERITIES = {"page", "warning", "info"}
LENIENT_SEVERITIES = STRICT_SEVERITIES | {"critical"}
MIN_FOR_SECONDS = 30

# IPv4 addresses allowed as doc/example references, not real targets.
IPV4_ALLOWLIST = {
    "0.0.0.0", "127.0.0.1", "255.255.255.255", "1.2.3.4",
    "169.254.169.254",  # link-local/metadata — reserved, safe as doc example
}

# Ordered list of hygiene heuristics. Keys are human-readable; values are compiled regexes.
# Go-template expressions (`{{ ... }}`) are redacted BEFORE these patterns run — this
# matters for the bearer/auth patterns that would otherwise flag `Bearer {{ $labels.x }}`.
HYGIENE_PATTERNS = [
    ("bearer token", re.compile(r"Bearer\s+[A-Za-z0-9._-]{10,}")),
    ("authorization header", re.compile(r"Authorization:\s*[A-Za-z]+\s+\S{10,}")),
    ("AWS access key", re.compile(r"AKIA[0-9A-Z]{16}")),
    ("AWS secret marker", re.compile(r"(?i)aws_secret_access_key|aws_access_key_id")),
    ("generic secret=value", re.compile(r"(?i)\bsecret[_-]?key\s*[:=]\s*[\"']?[A-Za-z0-9/+=]{16,}")),
    ("OpenAI/Stripe-style key", re.compile(r"\bsk-[A-Za-z0-9]{20,}\b|\bpk-[A-Za-z0-9]{20,}\b")),
    ("GitHub PAT", re.compile(r"\bgh[pous]_[A-Za-z0-9]{36,}\b")),
    ("Slack token", re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b")),
    ("JWT", re.compile(r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+")),
    ("PEM private key", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
    ("internal DNS suffix", re.compile(
        r"\.svc\.cluster\.local\b|\.cluster\.local\b|\.internal\b"
        r"|\.amazonaws\.com\b|\.ec2\.internal\b|\.compute\.internal\b")),
    # Prod/stage hostname — require a dot in the token so bare identifiers like
    # `MeetingProd-staging` in alert names don't false-positive.
    ("prod/stage hostname", re.compile(
        r"\b[a-z0-9]+(?:-prod-|-stage-|-prd-|-stg-)[a-z0-9-]+\.[a-z]")),
]

IPV4_REGEX = re.compile(r"\b(?:\d{1,3}\.){3}\d{1,3}\b")

TEMPLATE_EXPR = re.compile(r"\{\{[^}]*\}\}")

DURATION_RE = re.compile(r"(\d+)([smhdwy])")


def parse_prometheus_duration(s):
    """Parse '30s', '5m', '1h30m', etc. to seconds. Returns None if unparseable."""
    if not isinstance(s, str):
        return None
    s = s.strip()
    if not s:
        return None
    multipliers = {"s": 1, "m": 60, "h": 3600, "d": 86400, "w": 604800, "y": 31536000}
    total = 0
    matched = ""
    for num, unit in DURATION_RE.findall(s):
        total += int(num) * multipliers[unit]
        matched += num + unit
    if matched != s:
        return None
    return total


LAZY_REASON_RE = re.compile(r"^(test|tmp|todo|fix ?me|wip)\b", re.IGNORECASE)


def load_ignore_lines(path):
    """Return {lineno: reason} for each `# guard:ignore(<reason>)` in the file.

    A lazy reason (too short, or matching test/tmp/todo/fixme/wip) is rejected —
    the ignore is treated as absent so the hygiene check still fires.
    """
    ignored = {}
    marker = re.compile(r"#\s*guard:ignore\(\s*([^)]+?)\s*\)")
    try:
        with open(path, "r", encoding="utf-8") as f:
            for lineno, line in enumerate(f, start=1):
                m = marker.search(line)
                if not m:
                    continue
                reason = m.group(1).strip()
                if len(reason) < 10 or LAZY_REASON_RE.match(reason):
                    # Emit a diagnostic so the reviewer sees it, but don't honor the ignore.
                    print(json.dumps({
                        "file": os.path.relpath(path),
                        "alert": "",
                        "line": lineno,
                        "kind": "lazy_ignore_reason",
                        "message": f"guard:ignore reason too short or too vague: {reason!r} (require >=10 chars, not test/tmp/todo/fixme/wip)",
                    }))
                    continue
                ignored[lineno] = reason
    except OSError:
        pass
    return ignored


def rule_is_ignored(rule_line, ignore_lines):
    """Return reason string if rule is exempt from hygiene, else None.
    An ignore marker on the `- alert:` line or the line immediately above applies."""
    if rule_line in ignore_lines:
        return ignore_lines[rule_line]
    if (rule_line - 1) in ignore_lines:
        return ignore_lines[rule_line - 1]
    return None


def check_hygiene(text):
    """Return (pattern_name, match) for first hygiene hit, or None."""
    if not isinstance(text, str):
        return None
    # Redact Go-template expressions FIRST. This matters for `Bearer {{ $labels.x }}`
    # which is legitimate templating, not a bearer-token leak.
    scrub = TEMPLATE_EXPR.sub("<<TEMPLATED>>", text)

    # IPv4 scan with allowlist for documentation-reference IPs.
    for m in IPV4_REGEX.finditer(scrub):
        if m.group(0) not in IPV4_ALLOWLIST:
            return "public-or-private IPv4", m.group(0)

    for name, regex in HYGIENE_PATTERNS:
        m = regex.search(scrub)
        if m:
            return name, m.group(0)
    return None


def emit(violation_kind, file, alert_name, line, message):
    print(json.dumps({
        "file": file,
        "alert": alert_name,
        "line": line,
        "kind": violation_kind,
        "message": message,
    }))


def approximate_rule_line(raw_lines, alert_name):
    """Find the line `- alert: <alert_name>` in the raw file. Returns 0 if not found."""
    pattern = re.compile(r"^\s*-\s*alert:\s*['\"]?" + re.escape(alert_name) + r"['\"]?\s*$")
    for idx, line in enumerate(raw_lines, start=1):
        if pattern.match(line):
            return idx
    return 0


def validate_runbook_url(url, mode, runbooks_dir):
    """Returns (ok, message). Mode-aware."""
    if not url or not isinstance(url, str) or not url.strip():
        return False, "annotations.runbook_url is missing or empty"

    if mode == "lenient":
        # Lenient: accept any non-empty string. No shape or existence check.
        return True, None

    # Strict: must be repo-relative under docs/runbooks/ AND target must exist
    # AND the resolved real path must stay within docs/runbooks/ (no traversal).
    if re.match(r"^(https?:|//|file:)", url):
        return False, f"runbook_url must be repo-relative docs/runbooks/... (got: {url})"
    if not url.startswith("docs/runbooks/"):
        return False, f"runbook_url must start with docs/runbooks/ (got: {url})"

    path_part = url.split("#", 1)[0]
    abs_target = os.path.normpath(os.path.join(REPO_ROOT, path_part))
    try:
        resolved = os.path.realpath(abs_target)
        runbooks_real = os.path.realpath(runbooks_dir)
    except OSError:
        return False, f"runbook_url target cannot be resolved: {path_part}"

    # Path-traversal / symlink-escape check: resolved path must stay under docs/runbooks/.
    if not (resolved == runbooks_real or resolved.startswith(runbooks_real + os.sep)):
        return False, f"runbook_url target escapes docs/runbooks/ via traversal or symlink: {path_part}"

    if not os.path.isfile(resolved):
        return False, f"runbook_url target does not exist on disk: {path_part}"
    return True, None


def validate_severity(severity, mode):
    if severity is None or severity == "":
        return False, "labels.severity is missing or empty"
    allowed = LENIENT_SEVERITIES if mode == "lenient" else STRICT_SEVERITIES
    if severity not in allowed:
        allowed_str = "{" + ", ".join(sorted(allowed)) + "}"
        return False, f"labels.severity must be in {allowed_str} (got: {severity})"
    return True, None


def validate_for(for_val):
    if for_val is None:
        return False, "for: is missing"
    secs = parse_prometheus_duration(str(for_val))
    if secs is None:
        return False, f"for: is not a valid Prometheus duration (got: {for_val})"
    if secs < MIN_FOR_SECONDS:
        return False, f"for: must be >= {MIN_FOR_SECONDS}s (got: {for_val} = {secs}s)"
    return True, None


def main():
    try:
        with open(YAML_PATH, "r", encoding="utf-8") as f:
            raw = f.read()
        doc = yaml.safe_load(raw)
    except (yaml.YAMLError, OSError) as exc:
        print(json.dumps({"error": f"cannot parse {YAML_PATH}: {exc}"}))
        sys.exit(2)

    if not isinstance(doc, dict):
        print(json.dumps({"error": f"{YAML_PATH}: top-level must be a mapping"}))
        sys.exit(2)

    raw_lines = raw.splitlines()
    ignore_lines = load_ignore_lines(YAML_PATH)
    rel_path = os.path.relpath(YAML_PATH)

    for group in doc.get("groups", []) or []:
        for rule in group.get("rules", []) or []:
            alert_name = rule.get("alert")
            if not alert_name:
                # Recording rule or malformed — not our concern here.
                continue
            rule_line = approximate_rule_line(raw_lines, alert_name)

            annotations = rule.get("annotations") or {}
            labels = rule.get("labels") or {}

            # Check 1-2: runbook_url
            ok, msg = validate_runbook_url(annotations.get("runbook_url"), MODE, RUNBOOKS_DIR)
            if not ok:
                emit("runbook_url", rel_path, alert_name, rule_line, msg)

            # Check 3: severity
            ok, msg = validate_severity(labels.get("severity"), MODE)
            if not ok:
                emit("severity", rel_path, alert_name, rule_line, msg)

            # Check 4: for: duration (same in both modes)
            ok, msg = validate_for(rule.get("for"))
            if not ok:
                emit("for_duration", rel_path, alert_name, rule_line, msg)

            # Check 5: annotation hygiene (same in both modes; ignore-hatch scoped here)
            ignore_reason = rule_is_ignored(rule_line, ignore_lines)
            if ignore_reason is not None:
                # Log the bypass so reviewers see it even on a passing run.
                sys.stderr.write(
                    f"WARN: alert {alert_name} bypassed annotation_hygiene check "
                    f"— reason: {ignore_reason}\n"
                )
            else:
                for field in ("summary", "description", "impact"):
                    text = annotations.get(field)
                    hit = check_hygiene(text)
                    if hit:
                        kind, match = hit
                        emit(
                            "annotation_hygiene",
                            rel_path,
                            alert_name,
                            rule_line,
                            f"annotations.{field} contains suspected {kind}: {match!r}",
                        )


if __name__ == "__main__":
    main()
PYEOF
}

# -----------------------------------------------------------------------------
# Scan a single YAML file.
# -----------------------------------------------------------------------------
validate_file() {
    local yaml_path="$1"
    local mode="strict"

    if is_legacy_file "$yaml_path"; then
        mode="lenient"
        local rel_path="${yaml_path#"$REPO_ROOT/"}"
        echo -e "${YELLOW}[LEGACY]${NC} $rel_path — lenient mode (strict checks: severity present, runbook_url present, for:>=30s, annotation hygiene). Migration deadline $MIGRATION_DEADLINE. Tracked in TODO.md#adr-0031-alert-migration."
    fi

    local output
    if ! output=$(run_python_validator "$yaml_path" "$mode" 2>&1); then
        echo -e "${RED}ERROR:${NC} python validator failed on $yaml_path" >&2
        echo "$output" >&2
        return 2
    fi

    local file_violations=0
    # Pretty-print violations. Transform JSON-lines to tab-separated `file\talert\tline\tmessage`
    # via one python3 invocation rather than four-per-line, so a run with many violations
    # doesn't spawn 4N subprocesses.
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
    print("\t".join([d.get("file",""), d.get("alert",""), str(d.get("line",0)), d.get("message","")]))
' 2>&1); then
        echo -e "${RED}ERROR:${NC} failed to format violations" >&2
        echo "$formatted" >&2
        return 2
    fi

    # Check for any error records first (guard-level failures, not rule violations).
    if echo "$output" | grep -q '"error"'; then
        echo -e "${RED}ERROR:${NC} $output" >&2
        return 2
    fi

    while IFS=$'\t' read -r file alert line_num message; do
        [[ -z "$file$alert$line_num$message" ]] && continue
        echo -e "${RED}VIOLATION:${NC} ${file}:${line_num} [${alert}] ${message}"
        file_violations=$((file_violations + 1))
        increment_violations
    done <<< "$formatted"

    if [[ "$file_violations" -eq 0 ]]; then
        local rel_path="${yaml_path#"$REPO_ROOT/"}"
        print_ok "$rel_path"
    fi
}

# -----------------------------------------------------------------------------
# Self-test: run each fixture under strict or lenient, assert exit code.
# Fixtures are named pass-*.yaml / fail-*.yaml. Names starting `fail-lenient-`
# or `pass-lenient-` are treated as legacy. Others use strict mode.
# -----------------------------------------------------------------------------
self_test() {
    echo ""
    echo "========================================="
    echo "Alert-Rules Guard — Self-Test"
    echo "========================================="
    echo ""

    if [[ ! -d "$FIXTURES_DIR" ]]; then
        echo -e "${RED}FAIL: fixtures dir missing: $FIXTURES_DIR${NC}" >&2
        exit 2
    fi

    local passed=0 failed=0
    shopt -s nullglob
    for fixture in "$FIXTURES_DIR"/*.yaml; do
        local name
        name=$(basename "$fixture")
        local expected_exit
        local mode="strict"

        case "$name" in
            pass-*)  expected_exit=0 ;;
            fail-*)  expected_exit=1 ;;
            *) continue ;;
        esac

        case "$name" in
            *lenient*|*legacy*) mode="lenient" ;;
        esac

        local output
        local violation_count=0
        if output=$(run_python_validator "$fixture" "$mode" 2>&1); then
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
            echo -e "  ${GREEN}PASS${NC} $name (mode=$mode, violations=$violation_count)"
            passed=$((passed + 1))
        else
            echo -e "  ${RED}FAIL${NC} $name (mode=$mode, expected_exit=$expected_exit, got=$actual_exit, violations=$violation_count)"
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

    print_header "Alert-Rules Validation Guard"
    echo "Scanning: $ALERTS_DIR"
    echo ""

    if ! check_allowlist_integrity; then
        exit 1
    fi

    local yaml_files=()
    shopt -s nullglob
    for f in "$ALERTS_DIR"/*.yaml "$ALERTS_DIR"/*.yml; do
        local name
        name=$(basename "$f")
        case "$name" in
            _template-*.yaml|_template-*.yml) continue ;;
        esac
        yaml_files+=("$f")
    done
    shopt -u nullglob

    if [[ "${#yaml_files[@]}" -eq 0 ]]; then
        echo -e "${YELLOW}No alert-rule files found in $ALERTS_DIR${NC}"
        print_elapsed_time
        exit 0
    fi

    local file_error=0
    for yaml_path in "${yaml_files[@]}"; do
        if ! validate_file "$yaml_path"; then
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
        echo "See docs/observability/alert-conventions.md for rule rationale and migration guidance."
        exit 1
    fi

    echo -e "${GREEN}All alert rules pass.${NC}"
    exit 0
}

main "$@"
