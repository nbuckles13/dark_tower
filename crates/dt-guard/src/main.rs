//! dt-guard binary entry point.
//!
//! Per ADR-0034 §1: pure clap dispatch + STATUS emission with no business
//! logic. Per semantic-guard watch-point #2: every error path emits a
//! single-line `STATUS=FAIL REASON=<token>` to stdout BEFORE exiting
//! non-zero. The 5-line wrapper only handles missing-binary; main.rs
//! handles every other failure shape.

use anyhow::Result;
use clap::{Parser, Subcommand};
use dt_guard::alert_rules;
use dt_guard::api_version;
use dt_guard::application_metrics;
use dt_guard::cite_extract;
use dt_guard::common::status::{emit_fail, reason_token};
use dt_guard::cross_boundary_classification;
use dt_guard::cross_boundary_scope;
use dt_guard::dashboard_panels;
use dt_guard::env_config;
use dt_guard::grafana_datasources;
use dt_guard::gsa_sync;
use dt_guard::histogram_buckets;
use dt_guard::infrastructure_metrics;
use dt_guard::instrument_skip_all;
use dt_guard::knowledge_index;
use dt_guard::kustomize;
use dt_guard::metric_coverage;
use dt_guard::metric_labels;
use dt_guard::rust_log_secrets;
use dt_guard::rust_pii;
use dt_guard::rust_secrets;
use dt_guard::test_coverage;
use dt_guard::test_registration;
use dt_guard::test_rigidity;
use dt_guard::todo_tracking;
use dt_guard::ts_dev_trust;
use dt_guard::ts_exports_map;
use dt_guard::ts_metric_naming;
use dt_guard::ts_pii;
use dt_guard::ts_secrets;
use dt_guard::ts_test_removal;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "dt-guard",
    about = "Dark Tower guard pipeline policies as a single Rust binary (ADR-0034).",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Guard A: forbid bare `<path>:<line>` cites in long-lived doc trees.
    CiteNoLineNumbers {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Guard C: verify each `<path>::<symbol>` cite resolves in its target file.
    CiteSymbolResolves {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Alert-rules policy: runbook_url, severity, for-floor, hygiene checks.
    AlertRulesPolicy {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Dashboard-panels policy: unit, datasource, rate-window, metric-type, metric-exists.
    DashboardPanels {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Metric-labels policy: PII denylist + cardinality heuristic + naming hygiene.
    MetricLabels {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Application metrics: source/dashboard/alert/catalog coverage + target query mode.
    ApplicationMetrics {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Infrastructure metrics: Docker-label patterns + Prometheus-schema label refs.
    InfrastructureMetrics {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Grafana datasources: UID-dedup + Loki-label consistency.
    GrafanaDatasources {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TS hardcoded-secrets scan: 5 checks against `.ts`/`.tsx`/`.svelte`.
    TsNoSecrets {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TS PII-in-logs scan: log-sink + structured-object + error-message checks.
    TsNoPiiInLogs {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TS test-removal check (v1 file-deletion-only).
    TsNoTestRemoval {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TS client metric-name convention (R-26 `dt_client_*`).
    TsNameGuardDtClient {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TS no-dev-trust-path-in-prod-bundle (R-14 forcing function).
    TsNoDevTrustPathInProdBundle {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TS exports-map closed-world (ADR-0028 §5 supply chain).
    TsExportsMapClosed {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
        /// Promote "missing exports" from WARN to FAIL.
        #[arg(long)]
        strict: bool,
    },
    /// Rust hardcoded-secrets scan (HYGIENE_PATTERNS consumer).
    RustNoHardcodedSecrets {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Rust PII-in-logs scan (CATEGORY_B vocabulary).
    RustNoPiiInLogs {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Rust secrets-in-logs scan (CATEGORY_A identifier vocabulary).
    RustNoSecretsInLogs {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Rust `#[instrument]` allowlist discipline (skip_all not skip(...)).
    RustInstrumentSkipAll {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// API route pattern check (versioned /api/v{N}/ + unversioned ops).
    ApiVersionCheck {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Knowledge-index validation: stale pointers, ADR refs, size cap.
    KnowledgeIndex {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Histogram-buckets co-location: `histogram!()` ↔ `Matcher::Prefix()`.
    HistogramBuckets {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Metric test-coverage check (ADR-0032).
    MetricCoverage {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// TODO tracking discipline (single canonical TODO.md + pointer-only deferrals).
    TodoTracking {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Env-config consistency between Rust config.rs and K8s manifests.
    EnvConfig {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Kustomize infrastructure validation (R-15..R-20).
    Kustomize {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Test-coverage quick-check (warning-only; --full mode deferred to Wave 3).
    TestCoverage {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Test-registration: `#[path]` discovery in `crates/*/tests/*_tests.rs`.
    TestRegistration {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Test-rigidity: env-tests escape-clause detection (6 checks).
    TestRigidity {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// GSA enumeration sync across 5 mirrors (ADR-0024 §6.8 item #2).
    GsaSync {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
    /// Cross-Boundary Classification (Layer B): GSA-not-Mechanical + Owner-in-manifest.
    CrossBoundaryClassification {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
        /// Explicit `docs/devloop-outputs/.../main.md` to check (Gate 1 mode).
        /// Default: scan modified main.md files via diff (Gate 2 mode).
        #[arg(long)]
        main_md: Option<PathBuf>,
    },
    /// Cross-Boundary Scope (Layer A): plan-vs-diff drift detection with
    /// row-level user-story exemption (Wave-2 tightening #1).
    CrossBoundaryScope {
        /// Repository root for path resolution.
        #[arg(long)]
        root: PathBuf,
        /// Emit single-line `EXPLAIN:` records per finding (ADR §7).
        #[arg(long)]
        explain: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Print the full error chain to stderr for human triage; emit the
            // single-line STATUS=FAIL to stdout for wrapper parsing.
            eprintln!("{e:#}");
            emit_fail(reason_token(&e));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::CiteNoLineNumbers { root, explain } => {
            cite_extract::run_no_line_numbers(&root, explain)
        }
        Command::CiteSymbolResolves { root, explain } => {
            cite_extract::run_symbol_resolves(&root, explain)
        }
        Command::AlertRulesPolicy { root, explain } => alert_rules::run(&root, explain),
        Command::DashboardPanels { root, explain } => dashboard_panels::run(&root, explain),
        Command::MetricLabels { root, explain } => metric_labels::run(&root, explain),
        Command::ApplicationMetrics { root, explain } => application_metrics::run(&root, explain),
        Command::InfrastructureMetrics { root, explain } => {
            infrastructure_metrics::run(&root, explain)
        }
        Command::GrafanaDatasources { root, explain } => grafana_datasources::run(&root, explain),
        Command::TsNoSecrets { root, explain } => ts_secrets::run(&root, explain),
        Command::TsNoPiiInLogs { root, explain } => ts_pii::run(&root, explain),
        Command::TsNoTestRemoval { root, explain } => ts_test_removal::run(&root, explain),
        Command::TsNameGuardDtClient { root, explain } => ts_metric_naming::run(&root, explain),
        Command::TsNoDevTrustPathInProdBundle { root, explain } => {
            ts_dev_trust::run(&root, explain)
        }
        Command::TsExportsMapClosed {
            root,
            explain,
            strict,
        } => {
            // `--strict` flag wins over the env var. When neither is set,
            // default to the env-var semantics (literal-`"1"` only).
            let effective_strict = strict
                || std::env::var("STRICT_EXPORTS_MAP")
                    .map(|v| v == "1")
                    .unwrap_or(false);
            ts_exports_map::run_with_strict(&root, explain, effective_strict)
        }
        Command::RustNoHardcodedSecrets { root, explain } => rust_secrets::run(&root, explain),
        Command::RustNoPiiInLogs { root, explain } => rust_pii::run(&root, explain),
        Command::RustNoSecretsInLogs { root, explain } => rust_log_secrets::run(&root, explain),
        Command::RustInstrumentSkipAll { root, explain } => {
            instrument_skip_all::run(&root, explain)
        }
        Command::ApiVersionCheck { root, explain } => api_version::run(&root, explain),
        Command::KnowledgeIndex { root, explain } => knowledge_index::run(&root, explain),
        Command::HistogramBuckets { root, explain } => histogram_buckets::run(&root, explain),
        Command::MetricCoverage { root, explain } => metric_coverage::run(&root, explain),
        Command::TodoTracking { root, explain } => todo_tracking::run(&root, explain),
        Command::EnvConfig { root, explain } => env_config::run(&root, explain),
        Command::Kustomize { root, explain } => kustomize::run(&root, explain),
        Command::TestCoverage { root, explain } => test_coverage::run(&root, explain),
        Command::TestRegistration { root, explain } => test_registration::run(&root, explain),
        Command::TestRigidity { root, explain } => test_rigidity::run(&root, explain),
        Command::GsaSync { root, explain } => gsa_sync::run(&root, explain),
        Command::CrossBoundaryClassification {
            root,
            explain,
            main_md,
        } => cross_boundary_classification::run_with_arg(&root, explain, main_md.as_deref()),
        Command::CrossBoundaryScope { root, explain } => cross_boundary_scope::run(&root, explain),
    }
}
