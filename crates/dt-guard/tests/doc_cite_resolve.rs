//! 7-case `resolve_cited_path` security veto-blocking suite.
//!
//! Per ADR-0034 §5 + ADR-0024 §5.7 (security veto-blocking). Real
//! filesystem behavior — `#[cfg(unix)]` for the 3 symlink cases. Case 6
//! is a REAL dangling symlink (no `unittest.mock` equivalent) per @test
//! R3 retraction: reliability dominates ergonomics.

use dt_guard::common::path_safety::resolve_cited_path;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Case 1 — happy path: cited file exists inside the repo.
#[test]
fn containment_positive() {
    let tmp = TempDir::new().expect("tmpdir");
    let inside = tmp.path().join("inside.rs");
    fs::write(&inside, "fn foo() {}").expect("write");
    let resolved = resolve_cited_path(tmp.path(), "inside.rs");
    assert!(resolved.is_some(), "real file should resolve");
    assert!(
        resolved
            .unwrap()
            .starts_with(tmp.path().canonicalize().unwrap()),
        "resolved path must stay inside repo"
    );
}

/// Case 2 — `../etc/passwd` traversal escapes the repo root → None.
#[test]
fn traversal_escape_returns_none() {
    let tmp = TempDir::new().expect("tmpdir");
    let result = resolve_cited_path(tmp.path(), "../etc/passwd");
    assert!(
        result.is_none(),
        "traversal must be rejected (got {:?})",
        result
    );
}

/// Case 3 — absolute paths skip the join-relative-to-root step and then
/// face containment. `/etc/passwd` canonicalizes outside repo → None.
/// Per @security commitment #17, document the semantic (NOT
/// "absolute paths always rejected").
#[test]
fn absolute_path_escape_returns_none() {
    let tmp = TempDir::new().expect("tmpdir");
    // Literal /etc/passwd — almost certainly outside the repo_root tmpdir.
    let result = resolve_cited_path(tmp.path(), "/etc/passwd");
    assert!(
        result.is_none(),
        "absolute path outside repo must be rejected (got {:?})",
        result
    );

    // Per @security spec: also test a path inside repo_root.parent().
    // Path::join with an absolute target replaces the relative root, so the
    // cited path canonicalizes to /tmp/foo/x; if /tmp/foo/x is NOT under
    // repo_root, containment rejects.
    if let Some(parent) = tmp.path().parent() {
        let outside = parent.join("escape-target.txt");
        // No need to create it — even existence isn't required for the
        // semantic we're pinning; canonicalize fails on missing → None
        // (which is also "rejected", just via the dangling-symlink-style
        // path). For a more thorough check, create a sibling file:
        let _ = fs::write(&outside, "x");
        let absolute_str = outside.to_string_lossy().into_owned();
        let result = resolve_cited_path(tmp.path(), &absolute_str);
        assert!(
            result.is_none(),
            "absolute path in parent dir must be rejected (got {:?})",
            result
        );
        let _ = fs::remove_file(&outside);
    }
}

/// Case 4 — symlink whose target is outside the repo → None.
#[cfg(unix)]
#[test]
fn symlink_escape_returns_none() {
    let tmp = TempDir::new().expect("tmpdir");
    let outside_dir = TempDir::new().expect("outside tmpdir");
    let outside_file = outside_dir.path().join("secret.txt");
    fs::write(&outside_file, "secret").expect("write outside");

    let link_path = tmp.path().join("escape-link");
    std::os::unix::fs::symlink(&outside_file, &link_path).expect("create symlink");

    let result = resolve_cited_path(tmp.path(), "escape-link");
    assert!(
        result.is_none(),
        "symlink to outside must be rejected (got {:?})",
        result
    );
}

/// Case 5 — symlink whose target is inside the repo → resolves.
#[cfg(unix)]
#[test]
fn symlink_inside_resolves() {
    let tmp = TempDir::new().expect("tmpdir");
    let real = tmp.path().join("real.rs");
    fs::write(&real, "fn foo() {}").expect("write real");

    let link_path = tmp.path().join("inside-link");
    std::os::unix::fs::symlink(&real, &link_path).expect("create symlink");

    let result = resolve_cited_path(tmp.path(), "inside-link");
    assert!(
        result.is_some(),
        "intra-repo symlink must resolve (got None)"
    );
    let resolved = result.unwrap();
    assert!(
        resolved.starts_with(tmp.path().canonicalize().unwrap()),
        "resolved must be inside repo (got {:?})",
        resolved
    );
}

/// Case 6 — dangling symlink → `canonicalize` errors → None.
/// REAL filesystem test (no mock) per @test R3 retraction.
#[cfg(unix)]
#[test]
fn dangling_symlink_returns_none() {
    let tmp = TempDir::new().expect("tmpdir");
    let nonexistent_target = Path::new("/nonexistent/path/that/cannot/exist");
    let link_path = tmp.path().join("dangling");
    std::os::unix::fs::symlink(nonexistent_target, &link_path).expect("create dangling symlink");

    let result = resolve_cited_path(tmp.path(), "dangling");
    assert!(
        result.is_none(),
        "dangling symlink must return None (got {:?})",
        result
    );
}

/// Case 7 — `"."` resolves to repo_root.
#[test]
fn cited_path_dot_resolves_to_repo_root() {
    let tmp = TempDir::new().expect("tmpdir");
    let result = resolve_cited_path(tmp.path(), ".");
    assert!(result.is_some(), "'.' should resolve");
    assert_eq!(
        result.unwrap(),
        tmp.path().canonicalize().unwrap(),
        "'.' must resolve to canonicalized repo_root"
    );
}

/// Bonus regression test for semantic-guard watch-point #3: violation
/// messages never leak absolute paths from canonicalize().
#[test]
fn no_absolute_path_leak_on_containment_violation() {
    let tmp = TempDir::new().expect("tmpdir");
    let outside_dir = TempDir::new().expect("outside");
    let outside_file = outside_dir.path().join("escape.txt");
    fs::write(&outside_file, "").expect("write");
    let absolute_str = outside_file.to_string_lossy().into_owned();
    let result = resolve_cited_path(tmp.path(), &absolute_str);
    // The function itself returns None on a containment violation — the
    // caller is the one responsible for not leaking, but we assert here
    // that resolve_cited_path doesn't return Some-with-absolute-path on
    // an out-of-repo target. (The "no-leak in error messages" invariant
    // is enforced at the caller level; this is the upstream gate.)
    assert!(
        result.is_none(),
        "must not return Some for out-of-repo target (got {:?})",
        result
    );
}
