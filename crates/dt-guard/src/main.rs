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
use dt_guard::application_metrics;
use dt_guard::cite_extract;
use dt_guard::common::status::{emit_fail, reason_token};
use dt_guard::dashboard_panels;
use dt_guard::grafana_datasources;
use dt_guard::infrastructure_metrics;
use dt_guard::metric_labels;
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
    }
}
