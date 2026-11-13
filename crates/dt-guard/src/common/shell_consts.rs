//! Top-level `NAME="value"` constant extraction from a Bash script.
//!
//! POLICY-FREE. Guards that check what a script DERIVES must read the script's
//! own constants (e.g. `infra/kind/scripts/deploy.sh:MH_TEMPLATE_DEPLOYMENT`,
//! `infra/kind/scripts/mh.sh:MH_WT_PORT_NAME`) rather than retype them — a
//! retyped value is a second home that drifts. This only reads; each caller
//! decides what a missing constant means.
//!
//! Accepted shapes, at column 0 only (a function-local assignment is indented
//! and is not a constant): `NAME=value`, `NAME="value"`, `NAME='value'`, each
//! optionally prefixed by `readonly ` and followed by a `#` comment. A double-
//! quoted value containing `$`, a backtick or a backslash is an expansion, not
//! a constant, and is rejected.

use once_cell::sync::Lazy;
use regex::Regex;

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static SHELL_CONST_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^(?:readonly\s+)?([A-Za-z_][A-Za-z0-9_]*)=(?:"([^"$`\\]*)"|'([^']*)'|([A-Za-z0-9._/:@+-]*))\s*(?:#.*)?$"#,
    )
    .expect("static pattern compiles")
});

/// The value of the ONE top-level constant `name` in `src`.
///
/// # Errors
/// A human-readable reason when `name` is not assigned exactly once at top
/// level as a literal (absent, assigned twice, or assigned an expansion).
pub fn shell_const(src: &str, name: &str) -> Result<String, String> {
    let mut found: Vec<String> = Vec::new();
    let mut expansion = false;
    for line in src.lines() {
        let body = line.strip_prefix("readonly ").unwrap_or(line);
        if !body.starts_with(&format!("{name}=")) {
            continue;
        }
        match SHELL_CONST_RE.captures(line) {
            Some(c) if c.get(1).map(|m| m.as_str()) == Some(name) => {
                let v = c
                    .get(2)
                    .or_else(|| c.get(3))
                    .or_else(|| c.get(4))
                    .map_or("", |m| m.as_str());
                found.push(v.to_string());
            }
            _ => expansion = true,
        }
    }
    match (found.as_slice(), expansion) {
        ([v], false) => Ok(v.clone()),
        (_, true) => Err(format!(
            "`{name}` is assigned a non-literal value at top level (an expansion is not a constant)"
        )),
        ([], false) => Err(format!("no top-level `{name}=` assignment")),
        (_, false) => Err(format!(
            "`{name}` is assigned {} times at top level (expected exactly once)",
            found.len()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_literal_shape() {
        assert_eq!(shell_const("A=\"x-y\"\n", "A"), Ok("x-y".to_string()));
        assert_eq!(shell_const("A='x y'\n", "A"), Ok("x y".to_string()));
        assert_eq!(
            shell_const("readonly A=webtransport # c\n", "A"),
            Ok("webtransport".to_string())
        );
        assert_eq!(
            shell_const("AB=1\nA=\"dark-tower.io/x\"\n", "A"),
            Ok("dark-tower.io/x".to_string())
        );
    }

    #[test]
    fn indented_assignment_is_not_a_constant() {
        assert!(shell_const("f() {\n    A=\"x\"\n}\n", "A").is_err());
    }

    #[test]
    fn rejects_missing_duplicate_and_expansion() {
        assert!(shell_const("B=1\n", "A").is_err());
        assert!(shell_const("A=1\nA=2\n", "A").is_err());
        assert!(shell_const("A=\"${B}\"\n", "A").is_err());
    }
}
