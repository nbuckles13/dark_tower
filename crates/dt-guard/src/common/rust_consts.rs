//! `pub const NAME: &str = "value";` extraction from Rust source.
//!
//! POLICY-FREE. Hoisted 2026-10-26 from `client_metrics_export` when it gained
//! its second consumer (`common::kind_labels`, which resolves metric-label key
//! constants such as `MEDIA_KIND_LABEL` through
//! `crates/common/src/observability/labels.rs`). Each caller decides what a
//! missing constant means; this only reads.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// `pub const NAME: &str = "value";` in a Rust source file.
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RUST_STR_CONST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?m)^\s*pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"([^"]*)"\s*;"#,
    )
    .expect("static pattern compiles")
});

/// `pub const NAME: &str = "value";` pairs in a Rust source file.
#[must_use]
pub fn rust_str_consts(src: &str) -> HashMap<String, String> {
    RUST_STR_CONST_RE
        .captures_iter(src)
        .filter_map(|c| {
            Some((
                c.get(1)?.as_str().to_string(),
                c.get(2)?.as_str().to_string(),
            ))
        })
        .collect()
}
