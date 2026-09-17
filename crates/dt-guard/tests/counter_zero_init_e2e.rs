//! Proof-of-trap fixtures for `dt-guard counter-zero-init` — the FOUR rule IDs
//! the bash suite (`scripts/guards/counter-zero-init.test.sh`) leaves
//! unasserted at their emission sites: `slot_witness_cfg_test`,
//! `catalog_but_no_entrypoint`, `non_literal_metric_name_in_entrypoint`,
//! `empty_scan_no_counters`.
//!
//! The other six rule IDs are exercised by that bash suite and are EXCLUDED
//! here (named in `BASH_COVERED`, machine-checked against source). The parity
//! test asserts all 10 source consts = {4 cargo-fixtured} ∪ {6 bash-covered};
//! an 11th unclassified const reds. (@paired-test, @code-reviewer, @team-lead
//! #6.)
//!
//! Synthetic roots use ONE `mc-service` (a CANONICAL_SERVICES member); the
//! other three service dirs are absent, so `run()` `continue`s past them and
//! scans only `mc-service` — the `counter-zero-init.test.sh` `make_root` shape.

mod common;

use common::{assert_clean, assert_parity, run_all, Case, Expect};
use std::path::Path;

/// Bash-covered IDs, deliberately not cargo-fixtured here. Each is asserted by
/// `scripts/guards/counter-zero-init.test.sh` (case numbers noted).
const BASH_COVERED: &[&str] = &[
    "missing_zero_init",               // case 1 positive control
    "exempt_and_zero_init",            // case 5 mutual exclusion
    "malformed_exempt_reason",         // case 4 fail-closed
    "slot_witness_wildcard",           // case 6 catch-all arm
    "entrypoint_not_called_from_main", // case 7 uncalled entrypoint
    "scan_root_absent",                // case 9 renamed metrics.rs
];

const CATALOG_ONE: &str = "# MC Metrics\n### `mc_widget_total`\n- **Type**: Counter\n";
const CATALOG_NONE: &str = "# MC Metrics\n\nNo counters catalogued.\n";
const MAIN_CALLS: &str = "fn main() { zero_initialize_counters(); }\n";
const MAIN_NOCALL: &str = "fn main() { let _ = 1; }\n";

/// Lay one `mc-service` into `root`: metrics.rs + catalog + main.rs.
fn make_mc(root: &Path, metrics: &str, catalog: &str, main: &str) {
    common::write(
        root,
        "crates/mc-service/src/observability/metrics.rs",
        metrics,
    );
    common::write(root, "docs/observability/metrics/mc-service.md", catalog);
    common::write(root, "crates/mc-service/src/main.rs", main);
}

fn negatives() -> Vec<Case> {
    vec![
        // empty_scan_no_counters: a canonical service is present but the
        // catalog + emissions yield ZERO `_total` counters (required + exempt
        // both 0) and there are no findings → the vacuity bail. Surfaces via
        // `anyhow::bail!`, not a Finding, so assert on the bail token.
        Case {
            name: "empty_scan_no_counters",
            build: |root| {
                make_mc(
                    root,
                    "pub fn noop() { let _ = 1; }\n",
                    CATALOG_NONE,
                    MAIN_NOCALL,
                );
            },
            expect: Expect::Bail("empty_scan_no_counters"),
        },
        // catalog_but_no_entrypoint: catalog lists a `_total`, metrics.rs emits
        // it, but there is NO `// dt-guard:zero-init-entrypoint`-marked fn.
        // Structurally co-fires `missing_zero_init` (the same counter is R\Z),
        // which is bash-covered — so assert INCLUDING, not exact.
        Case {
            name: "catalog_but_no_entrypoint",
            build: |root| {
                make_mc(
                    root,
                    "pub fn record_widget() { counter!(\"mc_widget_total\", \"k\" => \"v\").increment(1); }\n",
                    CATALOG_ONE,
                    MAIN_NOCALL,
                );
            },
            expect: Expect::Including {
                fire: &["catalog_but_no_entrypoint"],
                absent: &[],
            },
        },
        // non_literal_metric_name_in_entrypoint: a marked entrypoint whose body
        // holds a `counter!` with a NON-literal first arg. The required counter
        // is also touched with a literal in the same body, so `missing_zero_init`
        // stays silent and this is single-fire.
        Case {
            name: "non_literal_metric_name_in_entrypoint",
            build: |root| {
                make_mc(
                    root,
                    "pub fn record_widget() { counter!(\"mc_widget_total\", \"k\" => \"v\").increment(1); }\n\
                     // dt-guard:zero-init-entrypoint\n\
                     pub fn zero_initialize_counters() {\n\
                     \x20   counter!(\"mc_widget_total\", \"k\" => \"v\").increment(0);\n\
                     \x20   counter!(dynamic_name, \"k\" => \"v\").increment(0);\n\
                     }\n",
                    CATALOG_ONE,
                    MAIN_CALLS,
                );
            },
            expect: Expect::Exact(&["non_literal_metric_name_in_entrypoint"]),
        },
        // slot_witness_cfg_test: a `slot_*` witness inside a `#[cfg(test)]`
        // block (a release build never compiles it). Catalog carries no
        // `_total`, so this is the only finding — and because there IS a
        // finding, the empty-scan bail does not pre-empt it.
        Case {
            name: "slot_witness_cfg_test",
            build: |root| {
                make_mc(
                    root,
                    "#[cfg(test)]\n\
                     mod tests {\n\
                     \x20   const fn slot_widget(v: T) -> usize { match v { A => 0, B => 1 } }\n\
                     }\n",
                    CATALOG_NONE,
                    MAIN_NOCALL,
                );
            },
            expect: Expect::Exact(&["slot_witness_cfg_test"]),
        },
    ]
}

#[test]
fn negatives_trap_their_rule_ids() {
    run_all("counter-zero-init", &negatives());
}

/// The clean happy-path: a real, correctly zero-inited counter → STATUS=OK.
/// NOT ok-by-emptiness (an empty scan would bail `empty_scan_no_counters`);
/// the OK reason names ≥1 service scanned. (@code-reviewer #3.)
#[test]
fn clean_root_passes_non_vacuously() {
    let root = common::new_root();
    make_mc(
        root.path(),
        "pub fn record_widget() { counter!(\"mc_widget_total\", \"k\" => \"v\").increment(1); }\n\
         // dt-guard:zero-init-entrypoint\n\
         pub fn zero_initialize_counters() { counter!(\"mc_widget_total\", \"k\" => \"v\").increment(0); }\n",
        CATALOG_ONE,
        MAIN_CALLS,
    );
    assert_clean(
        root.path(),
        "counter-zero-init",
        "counter-zero-init-clean-1-services",
    );
}

/// Source-derived SSoT: all 10 `*_RULE_ID` consts = {4 cargo-fixtured here} ∪
/// {6 bash-covered}. A new 11th const with no fixture and no bash entry reds.
#[test]
fn rule_id_inventory_is_fully_partitioned() {
    assert_parity("counter_zero_init.rs", &negatives(), BASH_COVERED);
}
