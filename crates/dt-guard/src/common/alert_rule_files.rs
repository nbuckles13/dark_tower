//! Loaded Prometheus rule files — which files load, and the `expr` of every
//! rule in them.
//!
//! Hoisted from `alert_rules.rs` (`loadable_rules_files`) and
//! `application_metrics.rs` (`yaml_value_to_json` and the `groups[].rules[].expr`
//! walk) so every guard that reasons about "what Prometheus evaluates" reads the
//! SAME predicate and the SAME parsed exprs. Parsed through serde, never raw
//! lines: a YAML comment that mentions a metric is not a rule.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

/// **The** predicate for *which files in the rules directory Prometheus loads*.
///
/// Returns bare filenames, sorted. Two exclusions, both deliberate:
///
/// * **not `_`-prefixed** — `_template-service-alerts.yaml` is a starter of
///   `<svc>` placeholders. Its exprs are not parseable PromQL, and Prometheus
///   validates rule files at config load and **exits non-zero**, which
///   in-cluster is CrashLoopBackOff and takes whole-cluster bring-up with it.
///   It is additionally guard-*exempt* by design, so loading it would evaluate
///   and publish annotation text nothing has ever scanned.
/// * **`-alerts.yaml` suffix** — a recording-rules file would want a different
///   glob and a different review; nothing has needed one yet, and admitting it
///   silently is how the set stops meaning anything.
///
/// This is intentionally **narrower** than the set [`crate::alert_rules::run`] lints, which is
/// every `*.yaml`/`*.yml` minus `_template-`. The gap is real and it is
/// reported: `alert_rules::check_rule_file_loading` compares the **linted** set against
/// this one and flags every file that falls in between — `Foo-alerts.yaml`,
/// `mc-recording-rules.yaml`, `mh-media-alerts.yml`. Each of those is a file
/// full of live alert rules that this guard lints, passes clean, and Prometheus
/// never loads: the "alive, never applied" defect, occurring inside the guard
/// built to close it.
///
/// **An earlier version of this comment claimed that asymmetry was reported
/// when only half of it was.** `Foo-alerts.yaml` was caught (an uppercase name
/// is outside the glob *and* outside this predicate, so the glob-coverage loop
/// saw it); `mc-recording-rules.yaml` was not, because a file that is not in
/// this predicate is not in `expected` and was therefore never asked about.
/// The comment named the uncaught example. Found at review, and it is the same
/// class as every other overclaim in this changeset — a sentence asserting a
/// protection that did not exist.
pub fn loadable_rules_files(alerts_dir: &Path) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    if !alerts_dir.is_dir() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(alerts_dir).context("read alerts dir for loading coverage")? {
        let entry = entry.context("read alerts dir entry")?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('_') {
            continue;
        }
        if !name.ends_with("-alerts.yaml") {
            continue;
        }
        out.push(name);
    }
    out.sort();
    Ok(out)
}

/// Convert serde_norway::Value → serde_json::Value (shallow, lossy for tags/aliases).
pub fn yaml_value_to_json(v: serde_norway::Value) -> Value {
    match v {
        serde_norway::Value::Null => Value::Null,
        serde_norway::Value::Bool(b) => Value::Bool(b),
        serde_norway::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        serde_norway::Value::String(s) => Value::String(s),
        serde_norway::Value::Sequence(seq) => {
            Value::Array(seq.into_iter().map(yaml_value_to_json).collect())
        }
        serde_norway::Value::Mapping(map) => {
            let mut o = serde_json::Map::new();
            for (k, val) in map {
                let key = match k {
                    serde_norway::Value::String(s) => s,
                    other => match serde_norway::to_string(&other) {
                        Ok(s) => s.trim().to_string(),
                        Err(_) => continue,
                    },
                };
                o.insert(key, yaml_value_to_json(val));
            }
            Value::Object(o)
        }
        serde_norway::Value::Tagged(_) => Value::Null,
    }
}

/// One rule's `expr`, as parsed from a rule file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleExpr {
    /// The `alert:` name, or `None` for a recording rule.
    pub alert: Option<String>,
    pub expr: String,
}

/// Every string `expr` under `groups[].rules[]` of one parsed rule document.
#[must_use]
pub fn rule_exprs(doc: &Value) -> Vec<RuleExpr> {
    let mut out = Vec::new();
    let Some(groups) = doc.get("groups").and_then(Value::as_array) else {
        return out;
    };
    for g in groups {
        let Some(rules) = g.get("rules").and_then(Value::as_array) else {
            continue;
        };
        for r in rules {
            if let Some(expr) = r.get("expr").and_then(Value::as_str) {
                out.push(RuleExpr {
                    alert: r.get("alert").and_then(Value::as_str).map(str::to_owned),
                    expr: expr.to_owned(),
                });
            }
        }
    }
    out
}

/// A [`RuleExpr`] tagged with the rule file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedRuleExpr {
    /// Bare filename within the rules directory.
    pub file: String,
    pub rule: RuleExpr,
}

/// Read and parse every [`loadable_rules_files`] file in `alerts_dir` and
/// return all their rule exprs. I/O and YAML errors propagate with the file
/// path attached — a rule file Prometheus would refuse to load is not "no
/// rules".
pub fn load_rule_exprs(alerts_dir: &Path) -> Result<Vec<LoadedRuleExpr>> {
    load_rule_exprs_from(alerts_dir, &loadable_rules_files(alerts_dir)?)
}

/// As [`load_rule_exprs`], over an already-computed [`loadable_rules_files`]
/// list (so a caller that also needs the list reads the directory once).
pub fn load_rule_exprs_from(alerts_dir: &Path, files: &[String]) -> Result<Vec<LoadedRuleExpr>> {
    let mut out = Vec::new();
    for name in files {
        let path = alerts_dir.join(name);
        let src = std::fs::read_to_string(&path)
            .with_context(|| format!("read rule file {}", path.display()))?;
        let doc: serde_norway::Value = serde_norway::from_str(&src)
            .with_context(|| format!("parse rule file {}", path.display()))?;
        for rule in rule_exprs(&yaml_value_to_json(doc)) {
            out.push(LoadedRuleExpr {
                file: name.clone(),
                rule,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_exprs_and_ignores_yaml_comments() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("x-alerts.yaml"),
            "# dt_client_in_a_comment\ngroups:\n  - name: g\n    rules:\n      - alert: A\n        expr: up == 0\n      - record: r\n        expr: sum(up)\n",
        )
        .unwrap();
        std::fs::write(d.path().join("_template-alerts.yaml"), "not: [valid").unwrap();
        let got = load_rule_exprs(d.path()).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].rule.alert.as_deref(), Some("A"));
        assert_eq!(got[1].rule.alert, None);
        assert!(got.iter().all(|e| !e.rule.expr.contains("dt_client")));
    }

    #[test]
    fn a_malformed_loadable_file_is_an_error_naming_it() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("bad-alerts.yaml"), "groups: [unclosed").unwrap();
        let err = format!("{:#}", load_rule_exprs(d.path()).unwrap_err());
        assert!(err.contains("bad-alerts.yaml"), "{err}");
    }
}
