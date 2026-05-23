//! Path-vs-glob matching — Rust port of
//! `scripts/guards/common.sh::path_matches_glob`.
//!
//! Single SoT for the cross-boundary guard family per ADR-0024 §6.6:
//! * [`crate::cross_boundary_classification`] (Layer B) — GSA detection via
//!   manifest globs.
//! * [`crate::cross_boundary_scope`] (Layer A) — plan-glob expansion against
//!   diff paths.
//!
//! Semantics mirror bash today:
//! * **Literal match**: `path == glob`.
//! * **Trailing `/**`**: glob `prefix/**` matches `prefix/...` (any depth)
//!   AND matches `prefix` itself as a file (rare but possible).
//! * **Simple wildcards** (`*`, `?`): single-segment match — bash uses
//!   `shopt -s extglob` + `[[ ... == $glob ]]`. Rust port handles `*` as
//!   "any chars except `/`" and `?` as "any single char except `/`".
//!
//! No support for `[abc]` character classes — bash uses them rarely and the
//! existing manifest doesn't. Add when a manifest needs them.

/// Does `path` match `glob` per the manifest semantics above?
pub fn path_matches_glob(path: &str, glob: &str) -> bool {
    // Literal match.
    if path == glob {
        return true;
    }
    // Trailing `/**` — match prefix + child path.
    if let Some(prefix) = glob.strip_suffix("/**") {
        return path.starts_with(&format!("{prefix}/")) || path == prefix;
    }
    // Simple wildcards (`*`, `?`).
    if glob.contains('*') || glob.contains('?') {
        return wildcard_match(path, glob);
    }
    false
}

/// Single-segment wildcard match. `*` matches any chars except `/`; `?`
/// matches any single char except `/`. Other chars match literally.
///
/// Iterative two-pointer with backtracking on `*`. Bounded by `glob.len() *
/// path.len()` worst-case; manifest globs are short so this is fine.
#[expect(
    clippy::indexing_slicing,
    reason = "every `g[gi]` / `p[pi]` is bounds-checked one line above"
)]
fn wildcard_match(path: &str, glob: &str) -> bool {
    let p: Vec<char> = path.chars().collect();
    let g: Vec<char> = glob.chars().collect();

    let mut pi = 0usize;
    let mut gi = 0usize;
    let mut star_p: Option<usize> = None;
    let mut star_g: Option<usize> = None;

    while pi < p.len() {
        if gi < g.len() && (g[gi] == p[pi] || (g[gi] == '?' && p[pi] != '/')) {
            pi += 1;
            gi += 1;
        } else if gi < g.len() && g[gi] == '*' {
            star_g = Some(gi);
            star_p = Some(pi);
            gi += 1;
        } else if let (Some(sg), Some(sp)) = (star_g, star_p) {
            // Backtrack: extend the previous `*` by one char, unless that
            // would consume a `/` (single-segment wildcard).
            if p[sp] == '/' {
                return false;
            }
            gi = sg + 1;
            star_p = Some(sp + 1);
            pi = sp + 1;
        } else {
            return false;
        }
    }

    // Trailing `*`s in the glob consume the empty tail.
    while gi < g.len() && g[gi] == '*' {
        gi += 1;
    }
    gi == g.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_match() {
        assert!(path_matches_glob(
            "crates/common/src/jwt.rs",
            "crates/common/src/jwt.rs"
        ));
        assert!(!path_matches_glob(
            "crates/common/src/jwt.rs",
            "crates/common/src/jwks.rs"
        ));
    }

    #[test]
    fn trailing_starstar_matches_subpaths() {
        assert!(path_matches_glob("proto/foo.proto", "proto/**"));
        assert!(path_matches_glob("proto/sub/dir/foo.proto", "proto/**"));
        // `prefix` itself as a file (per bash semantics).
        assert!(path_matches_glob("proto", "proto/**"));
        // Different prefix shouldn't match.
        assert!(!path_matches_glob("protocol/foo", "proto/**"));
    }

    #[test]
    fn trailing_starstar_anchors_at_separator() {
        // Regression: `proto/**` must NOT match `protocol/...` because the
        // separator anchors the prefix.
        assert!(!path_matches_glob("protocol/foo.rs", "proto/**"));
    }

    #[test]
    fn single_star_matches_within_segment() {
        assert!(path_matches_glob("foo.rs", "*.rs"));
        assert!(path_matches_glob("foo_bar.rs", "*.rs"));
        // `*` does NOT cross `/`.
        assert!(!path_matches_glob("crates/foo.rs", "*.rs"));
    }

    #[test]
    fn question_mark_single_char() {
        assert!(path_matches_glob("a.rs", "?.rs"));
        assert!(!path_matches_glob("ab.rs", "?.rs"));
        // `?` does NOT cross `/`.
        assert!(path_matches_glob("a/b.rs", "?/b.rs"));
    }

    #[test]
    fn empty_path_vs_empty_glob() {
        assert!(path_matches_glob("", ""));
        assert!(!path_matches_glob("a", ""));
        assert!(path_matches_glob("", "*"));
    }

    #[test]
    fn wave1_canon_path_shapes() {
        // Real manifest globs from cross-boundary-ownership.yaml.
        assert!(path_matches_glob(
            "crates/common/src/jwt.rs",
            "crates/common/src/jwt.rs"
        ));
        assert!(path_matches_glob(
            "crates/ac-service/src/jwks/keys.rs",
            "crates/ac-service/src/jwks/**"
        ));
        assert!(path_matches_glob(
            "db/migrations/001_init.sql",
            "db/migrations/**"
        ));
        assert!(!path_matches_glob(
            "crates/gc-service/src/jwt.rs",
            "crates/common/src/jwt.rs"
        ));
    }
}
