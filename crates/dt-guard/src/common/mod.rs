//! Shared kernel — STATUS emission, duration parsing, path safety.
//!
//! Per ADR-0034 §1, the `common/` directory hosts cross-subcommand helpers.
//! Each submodule is a single-responsibility unit.

pub mod duration;
pub mod explain;
pub mod git_changes;
pub mod grafana;
pub mod manifest_match;
pub mod markdown_table;
pub mod metric_catalog;
pub mod path_safety;
pub mod pii_vocabulary;
pub mod scan;
pub mod services;
pub mod status;
pub mod test_code_filter;
