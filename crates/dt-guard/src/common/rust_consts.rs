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

/// `pub const NAME: <unsigned int> = <decimal>;` in a Rust source file (any
/// indentation, so associated consts in an `impl` block count).
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static RUST_UINT_CONST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*pub\s+const\s+([A-Z][A-Z0-9_]*)\s*:\s*(?:u8|u16|u32|u64|usize)\s*=\s*([0-9][0-9_]*)\s*;")
        .expect("static pattern compiles")
});

/// `pub const NAME: u16 = 6;`-shaped pairs (unsigned integer literals only; a
/// const computed from an expression is not read). A name defined twice keeps
/// neither, so a caller sees it as missing rather than picking one silently.
#[must_use]
pub fn rust_uint_consts(src: &str) -> HashMap<String, u64> {
    let mut out: HashMap<String, Option<u64>> = HashMap::new();
    for c in RUST_UINT_CONST_RE.captures_iter(src) {
        let (Some(name), Some(val)) = (c.get(1), c.get(2)) else {
            continue;
        };
        let parsed = val.as_str().replace('_', "").parse::<u64>().ok();
        out.entry(name.as_str().to_string())
            .and_modify(|v| *v = None)
            .or_insert(parsed);
    }
    out.into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uint_consts_read_associated_consts_and_drop_duplicates() {
        let src = "impl P {\n    pub const K: u16 = 6;\n    pub const B: u16 = 2_0;\n}\npub const D: u8 = 1;\npub const D: u8 = 2;\npub const S: &str = \"x\";\n";
        let c = rust_uint_consts(src);
        assert_eq!(c.get("K"), Some(&6));
        assert_eq!(c.get("B"), Some(&20));
        assert_eq!(c.get("D"), None, "a duplicated name is not read");
        assert_eq!(c.get("S"), None);
    }
}
