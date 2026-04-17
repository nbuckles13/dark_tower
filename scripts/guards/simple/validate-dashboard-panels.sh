#!/bin/bash
#
# Dashboard-Panels Validation Guard (ADR-0031 Prereq #2)
#
# Enforces per-panel invariants for Grafana dashboard JSON files:
#   1. Metric-type classification (ADR-0029):
#        counter   → used only inside rate(...)/increase(...)
#        gauge     → NEVER inside rate(...)/increase(...)
#        histogram → _bucket inside rate() or histogram_quantile(); _sum/_count inside rate()/increase()
#   2. Panel unit declared: fieldConfig.defaults.unit must be set and non-empty.
#        Exempt: panel type 'logs' (Loki log-line viewers) and 'row' (structural).
#   3. $datasource template var: every panel's datasource.uid is either a template
#        reference ($datasource / ${datasource} / $<name>) or a UID listed as a
#        template-variable value. No hard-coded 'prometheus'/'loki' UIDs.
#   4. $__rate_interval: rate()/increase() windows use $__rate_interval, not
#        hard-coded [5m]. SLO dashboards (*-slos.json) and $__range stat-panel
#        windows are exempt — see ADR-0029 §Category C.
#   5. Canonical metric references: every ac_/gc_/mc_/mh_-prefixed metric in a
#        panel expr exists in the relevant crate's metrics.rs AND is documented
#        in the per-service catalog doc.
#
# TEMPLATE FILES
#
# Files matching `_template-*.json` are skipped ONLY during default
# enumeration; the `_template-service-overview.json` file IS scanned normally
# (it lives in the dashboards directory and must pass). The skip applies to
# files whose basenames start with `_template-` and are meant to be copied.
#
# PER-RULE ESCAPE HATCH
#
# Panels may bypass a single check by including `# guard:ignore(<reason>)`
# inside the panel's description field, with a reason ≥10 chars (not
# test/tmp/todo/fixme/wip). The ignore applies to the classification +
# $__rate_interval checks only — never to unit, datasource, or metric-exists.
#
# Exit codes:
#   0 - all pass
#   1 - one or more violations
#   2 - script error (missing python3, bad JSON, missing source files)
#
# Usage:
#   ./validate-dashboard-panels.sh              # scan production dashboard files
#   ./validate-dashboard-panels.sh --self-test  # run fixture-based regression suite

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# shellcheck disable=SC1091
source "$SCRIPT_DIR/../common.sh"

DASHBOARDS_DIR="$REPO_ROOT/infra/grafana/dashboards"
CRATES_DIR="$REPO_ROOT/crates"
CATALOG_DIR="$REPO_ROOT/docs/observability/metrics"
FIXTURES_DIR="$SCRIPT_DIR/fixtures/dashboard-panels"

# -----------------------------------------------------------------------------
# Python validator — one dashboard at a time, emits JSON-lines on stdout.
# -----------------------------------------------------------------------------
run_python_validator() {
    local json_path="$1"

    python3 - "$json_path" "$CRATES_DIR" "$CATALOG_DIR" "$REPO_ROOT" <<'PYEOF'
import json
import os
import re
import sys

JSON_PATH = sys.argv[1]
CRATES_DIR = sys.argv[2]
CATALOG_DIR = sys.argv[3]
REPO_ROOT = sys.argv[4]

# Service prefixes we classify. Kept in sync with CANONICAL_SERVICES in
# scripts/guards/common.sh (observability owns this guard, not the canonical
# mapping — duplicating the short list here keeps us free of the bash->python
# export dance).
SERVICE_PREFIXES = ("ac", "gc", "mc", "mh")

# Histograms expose _bucket, _sum, _count derived series.
HIST_SUFFIXES = ("_bucket", "_sum", "_count")

# Exempt panel types from unit + metric-type classification.
#   row: structural; no data.
#   logs: Loki log-line viewer; log lines, not numbers.
PANEL_TYPE_EXEMPT_UNIT = {"row", "logs"}
PANEL_TYPE_EXEMPT_CLASSIFY = {"row", "logs"}

# SLO dashboards intentionally keep hard-coded rate windows aligned with alert
# burn-rate math (ADR-0029 §Category C).
SLO_DASHBOARD_SUFFIX = "-slos.json"

# Rate windows that are semantically bound to the dashboard's time range (not
# a scrape interval) and are therefore NOT $__rate_interval candidates.
TIME_RANGE_WINDOWS = {"$__range", "$__interval"}

LAZY_REASON_RE = re.compile(r"^(test|tmp|todo|fix ?me|wip)\b", re.IGNORECASE)
IGNORE_MARKER_RE = re.compile(r"#\s*guard:ignore\(\s*([^)]+?)\s*\)")


def extract_metric_types(crates_dir):
    """Parse crates/*/src/observability/metrics.rs for counter!/gauge!/histogram!.

    Returns dict: metric_name -> 'counter' | 'gauge' | 'histogram'.
    """
    types = {}
    macro_re = re.compile(
        r"(counter|gauge|histogram)!\s*\(\s*\"([a-z_][a-z0-9_]*)\""
    )
    for prefix in SERVICE_PREFIXES:
        path = os.path.join(crates_dir, f"{prefix}-service/src/observability/metrics.rs")
        if not os.path.isfile(path):
            continue
        with open(path, "r", encoding="utf-8") as f:
            src = f.read()
        for m in macro_re.finditer(src):
            types[m.group(2)] = m.group(1)
    return types


def extract_catalog_metrics(catalog_dir):
    """Parse docs/observability/metrics/*.md for documented metric names.

    Heuristic: `### \`metric_name\`` heading. Matches existing
    validate-application-metrics.sh convention.
    """
    documented = set()
    head_re = re.compile(r"^###\s+`([a-z_][a-z0-9_]*)`", re.MULTILINE)
    if not os.path.isdir(catalog_dir):
        return documented
    for name in sorted(os.listdir(catalog_dir)):
        if not name.endswith(".md"):
            continue
        path = os.path.join(catalog_dir, name)
        try:
            with open(path, "r", encoding="utf-8") as f:
                src = f.read()
        except OSError:
            continue
        for m in head_re.finditer(src):
            documented.add(m.group(1))
    return documented


def walk_panels(panels, out=None):
    """Flatten dashboard panels, descending into row-panel .panels."""
    if out is None:
        out = []
    for p in panels or []:
        if p.get("type") == "row":
            # Row may itself be rendered (as a collapsed header) but we don't
            # subject rows to data-panel rules. Recurse into nested panels.
            out.append(p)
            walk_panels(p.get("panels") or [], out)
            continue
        out.append(p)
    return out


def is_metric_prefix_of_interest(metric_name):
    return any(metric_name.startswith(p + "_") for p in SERVICE_PREFIXES)


def strip_hist_suffix(metric):
    for suf in HIST_SUFFIXES:
        if metric.endswith(suf):
            return metric[: -len(suf)], suf
    return metric, None


def metric_inside_fn(expr, metric, fn_names):
    """Return True iff `metric` appears inside any fn in fn_names.

    Implementation note: we locate each `fn(` opener and track paren depth
    until the matching close; `metric` must appear inside that span. This is
    simple and robust to nested templates because PromQL function arguments
    don't use '{}' for nested calls.
    """
    for fn in fn_names:
        for m in re.finditer(rf"\b{re.escape(fn)}\s*\(", expr):
            start = m.end()
            depth = 1
            i = start
            while i < len(expr) and depth > 0:
                c = expr[i]
                if c == "(":
                    depth += 1
                elif c == ")":
                    depth -= 1
                i += 1
            span = expr[start : i - 1]
            # Match metric as a bare identifier — avoid substring collisions
            # (e.g., foo_total matching inside foo_total_errors).
            if re.search(rf"(?<![A-Za-z0-9_]){re.escape(metric)}(?![A-Za-z0-9_])", span):
                return True
    return False


def find_rate_windows(expr):
    """Yield (fn, window_str, start_idx) for each rate()/increase() call.

    `window_str` is the literal bracket contents, e.g. `5m`, `$__rate_interval`.
    """
    for m in re.finditer(
        r"\b(rate|increase|irate)\s*\(\s*[^)]*?\[([^\]]+)\]\s*\)",
        expr,
    ):
        yield m.group(1), m.group(2).strip(), m.start()


def datasource_uid_from_obj(ds):
    """Pull UID string from a datasource field (may be string or dict)."""
    if isinstance(ds, str):
        return ds
    if isinstance(ds, dict):
        return ds.get("uid")
    return None


def datasource_type_from_obj(ds):
    if isinstance(ds, dict):
        return ds.get("type")
    return None


def is_templated_datasource(uid):
    """Accept `$var`, `${var}`, `${var:raw}` as template references."""
    if not isinstance(uid, str):
        return False
    return bool(re.match(r"^\$\{?[a-zA-Z_][a-zA-Z0-9_:]*\}?$", uid))


def collect_template_var_values(dashboard):
    """Return set of datasource template-var names declared in the dashboard.

    Grafana stores datasource template vars with type='datasource'. We record
    their `name` so that `$name`/`${name}` references validate.
    """
    tvars = set()
    for v in (dashboard.get("templating") or {}).get("list", []) or []:
        if v.get("type") == "datasource":
            name = v.get("name")
            if isinstance(name, str) and name:
                tvars.add(name)
    return tvars


def extract_ignore_reason(panel):
    """Return (ignore_reason, lazy_ignore_diag).

    Returns (reason, None) on a valid ignore; (None, diagnostic) when a marker
    is present but the reason is too short or lazy; (None, None) otherwise.
    """
    desc = panel.get("description") or ""
    m = IGNORE_MARKER_RE.search(desc)
    if not m:
        return None, None
    reason = m.group(1).strip()
    if len(reason) < 10 or LAZY_REASON_RE.match(reason):
        return (
            None,
            f"guard:ignore reason too short or too vague: {reason!r} "
            f"(require >=10 chars, not test/tmp/todo/fixme/wip)",
        )
    return reason, None


def emit(kind, file, panel_id, panel_title, message):
    sys.stdout.write(
        json.dumps(
            {
                "file": file,
                "panel_id": panel_id,
                "panel_title": panel_title,
                "kind": kind,
                "message": message,
            }
        )
        + "\n"
    )


def main():
    try:
        with open(JSON_PATH, "r", encoding="utf-8") as f:
            dashboard = json.load(f)
    except (json.JSONDecodeError, OSError) as exc:
        sys.stdout.write(json.dumps({"error": f"cannot parse {JSON_PATH}: {exc}"}) + "\n")
        sys.exit(2)

    rel_path = os.path.relpath(JSON_PATH, REPO_ROOT)
    base_name = os.path.basename(JSON_PATH)
    is_slo_dashboard = base_name.endswith(SLO_DASHBOARD_SUFFIX)

    metric_types = extract_metric_types(CRATES_DIR)
    catalog_metrics = extract_catalog_metrics(CATALOG_DIR)
    template_vars = collect_template_var_values(dashboard)

    panels = walk_panels(dashboard.get("panels"))

    for p in panels:
        ptype = p.get("type", "")
        pid = p.get("id", 0)
        ptitle = p.get("title", "")

        if ptype == "row":
            continue

        ignore_reason, lazy_diag = extract_ignore_reason(p)
        if lazy_diag is not None:
            emit("lazy_ignore_reason", rel_path, pid, ptitle, lazy_diag)

        # ---- Rule 2: unit declared ----
        if ptype not in PANEL_TYPE_EXEMPT_UNIT:
            fc = p.get("fieldConfig") or {}
            defaults = fc.get("defaults") or {}
            unit = defaults.get("unit")
            if not isinstance(unit, str) or not unit.strip():
                emit(
                    "panel_unit",
                    rel_path,
                    pid,
                    ptitle,
                    "fieldConfig.defaults.unit is missing or empty",
                )

        # ---- Rule 3: datasource is a template reference ----
        # A panel-level datasource is a structural requirement: without one,
        # targets inherit nothing. But a panel may omit it and rely on each
        # target's datasource — so we check target-level references too.
        p_ds = p.get("datasource")
        p_ds_uid = datasource_uid_from_obj(p_ds)
        p_ds_type = datasource_type_from_obj(p_ds)
        if p_ds_uid is not None and not is_templated_datasource(p_ds_uid):
            emit(
                "hardcoded_datasource",
                rel_path,
                pid,
                ptitle,
                f"panel datasource.uid is hard-coded ({p_ds_uid!r}); "
                f"use $datasource template variable",
            )

        targets = p.get("targets") or []
        for t in targets:
            t_ds = t.get("datasource")
            t_ds_uid = datasource_uid_from_obj(t_ds)
            if t_ds_uid is not None and not is_templated_datasource(t_ds_uid):
                emit(
                    "hardcoded_datasource",
                    rel_path,
                    pid,
                    ptitle,
                    f"target refId={t.get('refId','?')} datasource.uid is "
                    f"hard-coded ({t_ds_uid!r}); use $datasource template variable",
                )

        # Only data panels with Prometheus (or inherited) targets need
        # metric-type / rate-window / metric-exists checks.
        effective_ds_type = p_ds_type
        # If panel-level type is absent, try first target's type.
        for t in targets:
            if effective_ds_type:
                break
            effective_ds_type = datasource_type_from_obj(t.get("datasource"))

        if ptype in PANEL_TYPE_EXEMPT_CLASSIFY:
            continue
        if effective_ds_type and effective_ds_type.lower() == "loki":
            continue

        # ---- Rules 1, 4, 5: per-expression checks ----
        for t in targets:
            expr = t.get("expr") or ""
            if not expr:
                continue

            # Rule 4: rate window must be $__rate_interval (non-SLO dashboards).
            if not is_slo_dashboard and ignore_reason is None:
                for fn, window, _ in find_rate_windows(expr):
                    # $__rate_interval is the canonical choice; $__range is
                    # accepted (used in stat panels); $__interval discouraged
                    # but not flagged here (reviewer-only). Anything else is
                    # a hard-coded window.
                    if window == "$__rate_interval":
                        continue
                    if window in TIME_RANGE_WINDOWS:
                        continue
                    # Hard-coded duration like 5m, 1h, 30s:
                    emit(
                        "rate_window",
                        rel_path,
                        pid,
                        ptitle,
                        f"{fn}() uses hard-coded window [{window}]; use "
                        f"[$__rate_interval] per ADR-0029",
                    )

            # Rule 5 + Rule 1 prep: collect all service-metric refs.
            metric_refs = set(
                re.findall(
                    r"\b((?:ac|gc|mc|mh)_[a-z][a-z0-9_]*)",
                    expr,
                )
            )

            for mref in metric_refs:
                base, suf = strip_hist_suffix(mref)
                declared_type = metric_types.get(mref) or metric_types.get(base)

                # Rule 5: metric must exist in code AND catalog.
                canonical_name = base if suf else mref
                if canonical_name not in metric_types:
                    emit(
                        "metric_not_in_code",
                        rel_path,
                        pid,
                        ptitle,
                        f"metric {mref!r} not defined in "
                        f"crates/*/src/observability/metrics.rs",
                    )
                    continue
                if canonical_name not in catalog_metrics:
                    emit(
                        "metric_not_in_catalog",
                        rel_path,
                        pid,
                        ptitle,
                        f"metric {mref!r} not documented in "
                        f"docs/observability/metrics/",
                    )

                if ignore_reason is not None:
                    continue

                # Rule 1: metric-type classification.
                if declared_type == "counter":
                    # Counter must be inside rate() or increase(); a bare
                    # reference (sum(metric), metric{...}) violates ADR-0029.
                    if not metric_inside_fn(expr, mref, ("rate", "increase", "irate")):
                        emit(
                            "counter_misuse",
                            rel_path,
                            pid,
                            ptitle,
                            f"counter metric {mref!r} used without "
                            f"rate()/increase() — violates ADR-0029 §Category A",
                        )
                elif declared_type == "gauge":
                    # Gauge must NEVER be inside rate()/increase(). Valid use
                    # is bare reference or inside sum()/avg()/etc.
                    if metric_inside_fn(expr, mref, ("rate", "increase", "irate")):
                        emit(
                            "gauge_misuse",
                            rel_path,
                            pid,
                            ptitle,
                            f"gauge metric {mref!r} wrapped in rate()/"
                            f"increase() — gauges represent current value, "
                            f"not a counting process",
                        )
                elif declared_type == "histogram":
                    # Histograms expose _bucket/_sum/_count. Enforce:
                    #   _bucket → inside rate() AND (inside histogram_quantile()
                    #             when that fn appears) — any rate() is fine.
                    #   _sum, _count → inside rate() or increase().
                    if suf == "_bucket":
                        if not metric_inside_fn(expr, mref, ("rate", "irate")):
                            emit(
                                "histogram_misuse",
                                rel_path,
                                pid,
                                ptitle,
                                f"histogram bucket {mref!r} must be inside "
                                f"rate() — use histogram_quantile(..., "
                                f"rate({base}_bucket[...]))",
                            )
                    elif suf in ("_sum", "_count"):
                        if not metric_inside_fn(
                            expr, mref, ("rate", "increase", "irate")
                        ):
                            emit(
                                "histogram_misuse",
                                rel_path,
                                pid,
                                ptitle,
                                f"histogram series {mref!r} must be inside "
                                f"rate()/increase()",
                            )
                    # suf is None: bare histogram base (no suffix) isn't a
                    # real Prometheus series — if the user wrote it, it's
                    # likely a typo; treat as metric_not_in_code above.


if __name__ == "__main__":
    main()
PYEOF
}

# -----------------------------------------------------------------------------
# Scan a single JSON file. Returns 0 on clean, 1 on violations, 2 on script error.
# -----------------------------------------------------------------------------
validate_file() {
    local json_path="$1"

    local output
    if ! output=$(run_python_validator "$json_path" 2>&1); then
        echo -e "${RED}ERROR:${NC} python validator failed on $json_path" >&2
        echo "$output" >&2
        return 2
    fi

    # Any `"error":` record is a script-level error, not a violation.
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
        str(d.get("panel_id", 0)),
        d.get("panel_title", ""),
        d.get("kind", ""),
        d.get("message", ""),
    ]))
' 2>&1); then
        echo -e "${RED}ERROR:${NC} failed to format violations" >&2
        echo "$formatted" >&2
        return 2
    fi

    while IFS=$'\t' read -r file panel_id panel_title kind message; do
        [[ -z "$file$panel_id$panel_title$kind$message" ]] && continue
        echo -e "${RED}VIOLATION:${NC} ${file} panel=${panel_id} [${panel_title}] (${kind}) ${message}"
        file_violations=$((file_violations + 1))
        increment_violations
    done <<< "$formatted"

    if [[ "$file_violations" -eq 0 ]]; then
        local rel_path="${json_path#"$REPO_ROOT/"}"
        print_ok "$rel_path"
    fi
}

# -----------------------------------------------------------------------------
# Self-test: iterate fixture files, assert exit from filename (pass-*/fail-*).
# -----------------------------------------------------------------------------
self_test() {
    echo ""
    echo "========================================="
    echo "Dashboard-Panels Guard — Self-Test"
    echo "========================================="
    echo ""

    if [[ ! -d "$FIXTURES_DIR" ]]; then
        echo -e "${RED}FAIL: fixtures dir missing: $FIXTURES_DIR${NC}" >&2
        exit 2
    fi

    local passed=0 failed=0
    shopt -s nullglob
    for fixture in "$FIXTURES_DIR"/*.json; do
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

    print_header "Dashboard-Panels Validation Guard"
    echo "Scanning: $DASHBOARDS_DIR"
    echo ""

    if [[ ! -d "$DASHBOARDS_DIR" ]]; then
        echo -e "${YELLOW}Dashboards directory not found: $DASHBOARDS_DIR${NC}"
        print_elapsed_time
        exit 0
    fi

    local json_files=()
    shopt -s nullglob
    for f in "$DASHBOARDS_DIR"/*.json; do
        local name
        name=$(basename "$f")
        # Skip copy-me template stubs at enumeration time. The overview
        # template (_template-service-overview.json) is a starter that must
        # still pass, so it's NOT skipped — its filename matches the general
        # skip pattern but it's explicitly allow-listed here.
        case "$name" in
            _template-service-overview.json) ;;
            _template-*.json) continue ;;
        esac
        json_files+=("$f")
    done
    shopt -u nullglob

    if [[ "${#json_files[@]}" -eq 0 ]]; then
        echo -e "${YELLOW}No dashboard files found in $DASHBOARDS_DIR${NC}"
        print_elapsed_time
        exit 0
    fi

    local file_error=0
    for json_path in "${json_files[@]}"; do
        if ! validate_file "$json_path"; then
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
        echo "See docs/observability/dashboard-conventions.md for rule rationale and migration guidance."
        exit 1
    fi

    echo -e "${GREEN}All dashboard panels pass.${NC}"
    exit 0
}

main "$@"
