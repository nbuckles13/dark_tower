//! `vendor-patches` subcommand — a vendored crate is EXACTLY its published
//! upstream source plus a recorded, additive-only patch, and the workspace
//! actually links it.
//!
//! # Why
//!
//! `vendor/<crate>/` replaces a crates.io dependency through a root
//! `[patch.<registry>]` path (ADR-0039 §7: wtransport cannot send a session
//! close). Vendored code sits outside the registry's integrity checks and
//! outside workspace lints, so without this guard it can drift silently from
//! what was reviewed, and a version bump can silently UNPATCH it (cargo only
//! warns "patch was not used") and relink upstream into every service.
//!
//! # The vendored set — derived, never listed
//!
//! Every root `[patch.*]` entry with a `path`
//! (`common::cargo_manifest::patch_path_targets`, the ONE derivation shared with
//! `workspace-deps` and `docker-context`). Its metadata lives OUTSIDE the crate
//! directory, so the crate tree itself stays byte-identical to upstream plus the
//! patch: `vendor/patches/<dir name>/PATCH.md` (with a machine-readable block)
//! and `PATCH.diff` (unified diff, `a/` → `b/` relative to the crate root).
//!
//! # Checks (each its own REASON token)
//!
//! | Token | Fires when |
//! |---|---|
//! | `vendor-patches-manifest-unreadable` | root `Cargo.toml` / `[patch.*]` / `Cargo.lock` unreadable (PRECONDITION) |
//! | `vendor-patch-target-outside-vendor` | a `[patch.*]` path target is not exactly `vendor/<name>` (one level, not `patches`): the vendored set and the content scans' `vendor/` prefix (`common::git_changes::is_vendored`) must be one definition, and `vendor/patches/<name>` must not collide |
//! | `vendor-crate-missing` | a patch target has no `Cargo.toml` |
//! | `vendor-patch-metadata-missing` | `PATCH.md` / `PATCH.diff` / a PATCH.md field missing |
//! | `vendor-not-excluded` | the crate is not in root `[workspace] exclude` |
//! | `vendor-dir-not-patch-target` | a directory under `vendor/` (other than `patches/`) is not a patch target |
//! | `vendor-patch-not-in-lock` | `Cargo.lock` does not resolve the upstream name+version to the path (no `source`), or still holds a registry copy |
//! | `vendor-patch-not-additive` | `PATCH.diff` removes or changes an upstream line |
//! | `vendor-patch-diff-invalid` | unparseable diff, hunk counts that disagree with the body, a context or added line that does not match the tree, a hunk for an absent file |
//! | `vendor-patch-diff-empty` | `PATCH.diff` adds nothing (positive control: a vendored crate with no patch should not be vendored) |
//! | `vendor-tree-symlink` | a symlink anywhere under the crate dir |
//! | `vendor-tree-drift` | reverse-applying `PATCH.diff` to the tree does not reproduce `pristine-tree-digest` (an edit outside the diff, an extra or missing file) |
//!
//! Hermetic: no network, no `cargo`, no external `patch`. Because the diff is
//! additive-only, reverse-applying it is exactly "remove each hunk's `+` lines
//! after checking every line against the tree", done here natively.
//!
//! # Other modes
//!
//! * `--list` — the derived vendored paths, one per line (for `audit.sh`'s
//!   discovery skip; bash parses neither `Cargo.toml` nor `Cargo.lock`).
//! * `--pristine-digest <dir> --vendored <path>` — the Layer-6 crates.io anchor:
//!   digest an unpacked upstream `.crate` with the SAME function the guard uses
//!   and compare it to that crate's recorded `pristine-tree-digest`.
//! * `--attributed-lock <out>` — a copy of `Cargo.lock` in which every vendored
//!   package carries its upstream registry `source`, so `cargo audit` matches
//!   advisories against it (a path package has none and is never matched).

use crate::common::cargo_manifest::{
    patch_path_targets, read_lock, read_root_manifest, LockPackage, PatchPath,
};
use crate::common::status::{emit_ok, emit_scope};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const PATCHES_DIR: &str = "vendor/patches";
const VENDOR_DIR: &str = "vendor";
const BLOCK_BEGIN: &str = "<!-- vendor-patch:begin -->";
const BLOCK_END: &str = "<!-- vendor-patch:end -->";
const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// The machine-readable fields of a `PATCH.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchMeta {
    pub upstream_crate: String,
    pub upstream_version: String,
    pub upstream_crate_sha256: String,
    pub pristine_tree_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub token: &'static str,
    pub path: String,
    pub detail: String,
}

fn violation(token: &'static str, path: impl Into<String>, detail: impl Into<String>) -> Violation {
    Violation {
        token,
        path: path.into(),
        detail: detail.into(),
    }
}

/// `vendor/patches/<last path component>`.
pub fn metadata_dir(vendored_path: &str) -> String {
    let name = vendored_path.rsplit('/').next().unwrap_or(vendored_path);
    format!("{PATCHES_DIR}/{name}")
}

/// Parse the `PATCH.md` block. Every field is required.
pub fn parse_patch_md(text: &str) -> Result<PatchMeta, String> {
    let begin = text
        .find(BLOCK_BEGIN)
        .ok_or_else(|| format!("no `{BLOCK_BEGIN}` block"))?;
    let rest = &text[begin + BLOCK_BEGIN.len()..];
    let end = rest
        .find(BLOCK_END)
        .ok_or_else(|| format!("no `{BLOCK_END}` after the block start"))?;
    let mut fields = BTreeMap::new();
    for line in rest[..end].lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| format!("block line is not `key: value`: {line}"))?;
        fields.insert(k.trim().to_string(), v.trim().to_string());
    }
    let mut take = |k: &str| {
        fields
            .remove(k)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("missing field `{k}`"))
    };
    Ok(PatchMeta {
        upstream_crate: take("upstream-crate")?,
        upstream_version: take("upstream-version")?,
        upstream_crate_sha256: take("upstream-crate-sha256")?,
        pristine_tree_digest: take("pristine-tree-digest")?,
    })
}

// ---------------------------------------------------------------------------
// Diff parsing and reverse application
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk {
    old_start: usize,
    old_len: usize,
    new_start: usize,
    new_len: usize,
    /// (`'+'` or `' '`, line text without terminator).
    body: Vec<(char, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilePatch {
    /// Path relative to the crate root.
    path: String,
    /// `--- /dev/null`: the whole file is added.
    new_file: bool,
    hunks: Vec<Hunk>,
}

/// Why a diff could not be applied in reverse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    /// A `-` line: the patch changes upstream.
    NotAdditive(String),
    /// Anything else the guard cannot apply exactly.
    Invalid(String),
}

fn parse_range(s: &str) -> Option<(usize, usize)> {
    match s.split_once(',') {
        Some((start, len)) => Some((start.parse().ok()?, len.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

fn strip_side<'a>(line: &'a str, marker: &str, side: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(marker)?;
    let rest = rest.split('\t').next().unwrap_or(rest).trim_end();
    if rest == "/dev/null" {
        return Some(rest);
    }
    rest.strip_prefix(side)
}

fn parse_diff(text: &str) -> Result<Vec<FilePatch>, DiffError> {
    let invalid = |m: String| DiffError::Invalid(m);
    let mut files: Vec<FilePatch> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("diff ") || line.starts_with("index ") || line.is_empty() {
            continue;
        }
        if let Some(old) = strip_side(line, "--- ", "a/") {
            let next = lines
                .next()
                .ok_or_else(|| invalid("`---` header without `+++`".into()))?;
            let new = strip_side(next, "+++ ", "b/")
                .ok_or_else(|| invalid(format!("expected `+++ b/<path>` after `{line}`")))?;
            if new == "/dev/null" {
                return Err(DiffError::NotAdditive(format!("{old}: file deleted")));
            }
            if old != "/dev/null" && old != new {
                return Err(DiffError::NotAdditive(format!("{old} renamed to {new}")));
            }
            files.push(FilePatch {
                path: new.to_string(),
                new_file: old == "/dev/null",
                hunks: Vec::new(),
            });
            continue;
        }
        if let Some(header) = line.strip_prefix("@@ ") {
            let file = files
                .last_mut()
                .ok_or_else(|| invalid("hunk before any file header".into()))?;
            let header = header
                .split(" @@")
                .next()
                .ok_or_else(|| invalid(format!("bad hunk header `{line}`")))?;
            let mut parts = header.split(' ');
            let (old_start, old_len) = parts
                .next()
                .and_then(|p| p.strip_prefix('-'))
                .and_then(parse_range)
                .ok_or_else(|| invalid(format!("bad hunk header `{line}`")))?;
            let (new_start, new_len) = parts
                .next()
                .and_then(|p| p.strip_prefix('+'))
                .and_then(parse_range)
                .ok_or_else(|| invalid(format!("bad hunk header `{line}`")))?;
            let mut body = Vec::new();
            let (mut old_seen, mut new_seen) = (0, 0);
            while old_seen < old_len || new_seen < new_len {
                let Some(b) = lines.next() else {
                    return Err(invalid(format!(
                        "{}: hunk `{line}` ends early (counts disagree with the body)",
                        file.path
                    )));
                };
                let (tag, text) = b.split_at(b.chars().next().map_or(0, char::len_utf8));
                match tag {
                    " " => {
                        old_seen += 1;
                        new_seen += 1;
                        body.push((' ', text.to_string()));
                    }
                    "+" => {
                        new_seen += 1;
                        body.push(('+', text.to_string()));
                    }
                    "-" => {
                        return Err(DiffError::NotAdditive(format!(
                            "{}: removes upstream line `{text}`",
                            file.path
                        )))
                    }
                    "\\" => {
                        return Err(invalid(format!(
                            "{}: `\\ No newline at end of file` is not supported",
                            file.path
                        )))
                    }
                    _ => {
                        return Err(invalid(format!(
                            "{}: unexpected hunk line `{b}`",
                            file.path
                        )))
                    }
                }
            }
            if old_seen != old_len || new_seen != new_len {
                return Err(invalid(format!(
                    "{}: hunk `{line}` counts disagree with its body",
                    file.path
                )));
            }
            if let Some(next) = lines.peek() {
                if next.starts_with('+') && !next.starts_with("+++ ") || next.starts_with(' ') {
                    return Err(invalid(format!(
                        "{}: hunk `{line}` has more body lines than its header counts",
                        file.path
                    )));
                }
                if next.starts_with('-') && !next.starts_with("--- ") {
                    return Err(DiffError::NotAdditive(format!(
                        "{}: removes upstream line `{}`",
                        file.path,
                        &next[1..]
                    )));
                }
            }
            file.hunks.push(Hunk {
                old_start,
                old_len,
                new_start,
                new_len,
                body,
            });
            continue;
        }
        if line.starts_with('-') {
            return Err(DiffError::NotAdditive(format!(
                "removed line outside a hunk: {line}"
            )));
        }
        return Err(invalid(format!("unparseable diff line `{line}`")));
    }
    Ok(files)
}

/// Number of `+` lines.
fn added_lines(files: &[FilePatch]) -> usize {
    files
        .iter()
        .flat_map(|f| &f.hunks)
        .flat_map(|h| &h.body)
        .filter(|(t, _)| *t == '+')
        .count()
}

/// Reverse one file: verify every hunk line against `content` and drop the
/// added lines. `content` must be UTF-8 with `\n` terminators.
fn reverse_file(path: &str, content: &str, hunks: &[Hunk]) -> Result<String, DiffError> {
    let invalid = |m: String| DiffError::Invalid(format!("{path}: {m}"));
    let tree: Vec<&str> = content.split_inclusive('\n').collect();
    let mut keep = vec![true; tree.len()];
    let mut shift: isize = 0; // added - removed so far (removed is always 0)
    let mut last_end = 0;
    for h in hunks {
        let added = h.body.iter().filter(|(t, _)| *t == '+').count();
        // In a pure-addition diff the old position is the new one minus the
        // additions already applied above it (`-k,0` names the line BEFORE an
        // insertion, so it sits one lower).
        let new_start = isize::try_from(h.new_start).unwrap_or(isize::MAX);
        let expected_old = if h.old_len == 0 {
            new_start - 1 - shift
        } else {
            new_start - shift
        };
        if isize::try_from(h.old_start).ok() != Some(expected_old) {
            return Err(invalid(format!(
                "hunk @@ -{},{} +{},{} @@: its old position disagrees with the additions above it",
                h.old_start, h.old_len, h.new_start, h.new_len
            )));
        }
        let start = h.new_start.saturating_sub(1);
        if start < last_end {
            return Err(invalid("overlapping or out-of-order hunks".into()));
        }
        for (i, (tag, text)) in h.body.iter().enumerate() {
            let idx = start + i;
            let actual = tree.get(idx).ok_or_else(|| {
                invalid(format!(
                    "hunk runs past the end of the file (line {})",
                    idx + 1
                ))
            })?;
            let actual = actual.strip_suffix('\n').unwrap_or(actual);
            let actual = actual.strip_suffix('\r').unwrap_or(actual);
            if actual != text {
                return Err(invalid(format!(
                    "line {} is `{actual}`, the diff expects `{text}` ({})",
                    idx + 1,
                    if *tag == '+' { "added" } else { "context" }
                )));
            }
            if *tag == '+' {
                if let Some(slot) = keep.get_mut(idx) {
                    *slot = false;
                }
            }
        }
        last_end = start + h.body.len();
        shift += isize::try_from(added).unwrap_or(isize::MAX);
    }
    Ok(tree
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(l, _)| *l)
        .collect())
}

// ---------------------------------------------------------------------------
// Tree digest — the ONE implementation (Layer 3 and the Layer-6 anchor)
// ---------------------------------------------------------------------------

/// One file of a tree: repo-relative path ('/'), executable bit, content.
#[derive(Debug, Clone)]
pub struct TreeFile {
    pub path: String,
    pub executable: bool,
    pub content: Vec<u8>,
}

/// SHA-256 over the sorted `(path, mode, sha256(content))` listing.
///
/// Byte-ordered (`Vec<u8>` comparison), so locale never affects it. Mode is
/// the executable bit only — the one bit git and a `.crate` both preserve.
pub fn tree_digest(files: &[TreeFile]) -> String {
    let mut rows: Vec<Vec<u8>> = files
        .iter()
        .map(|f| {
            let content_hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, &f.content));
            let mut row = f.path.as_bytes().to_vec();
            row.push(0);
            row.push(if f.executable { b'x' } else { b'-' });
            row.push(0);
            row.extend_from_slice(content_hash.as_bytes());
            row.push(b'\n');
            row
        })
        .collect();
    rows.sort();
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    for row in rows {
        ctx.update(&row);
    }
    hex::encode(ctx.finish())
}

/// Read every file under `dir`. A symlink is an error, never followed.
pub fn read_tree(dir: &Path) -> Result<Vec<TreeFile>, String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| format!("walk error: {e}"))?;
        let rel = entry
            .path()
            .strip_prefix(dir)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        if entry.path_is_symlink() {
            return Err(format!("symlink: {rel}"));
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let content = std::fs::read(entry.path()).map_err(|e| format!("unreadable {rel}: {e}"))?;
        out.push(TreeFile {
            path: rel,
            executable: is_executable(entry.path()),
            content,
        });
    }
    Ok(out)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

/// Reconstruct the pristine tree from the vendored tree and its diff.
fn pristine_from(tree: &[TreeFile], diff: &[FilePatch]) -> Result<Vec<TreeFile>, DiffError> {
    let by_path: BTreeMap<&str, &FilePatch> = diff.iter().map(|f| (f.path.as_str(), f)).collect();
    for f in diff {
        if !tree.iter().any(|t| t.path == f.path) {
            return Err(DiffError::Invalid(format!(
                "{}: the diff patches a file absent from the tree",
                f.path
            )));
        }
    }
    let mut out = Vec::new();
    for t in tree {
        match by_path.get(t.path.as_str()) {
            None => out.push(t.clone()),
            Some(fp) => {
                let text = std::str::from_utf8(&t.content).map_err(|_| {
                    DiffError::Invalid(format!("{}: patched file is not UTF-8", t.path))
                })?;
                let pristine = reverse_file(&t.path, text, &fp.hunks)?;
                if fp.new_file {
                    if !pristine.is_empty() {
                        return Err(DiffError::Invalid(format!(
                            "{}: a `/dev/null` hunk must add the whole file",
                            t.path
                        )));
                    }
                    continue;
                }
                out.push(TreeFile {
                    path: t.path.clone(),
                    executable: t.executable,
                    content: pristine.into_bytes(),
                });
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The guard
// ---------------------------------------------------------------------------

/// What the guard examined (anti-vacuity counts on the PASS line).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scope {
    pub vendored: usize,
    pub added_lines: usize,
}

fn str_field<'a>(t: &'a toml::Table, k: &str) -> Option<&'a str> {
    t.get(k).and_then(toml::Value::as_str)
}

fn targets(repo_root: &Path) -> Result<(Vec<PatchPath>, Vec<String>), String> {
    let manifest = read_root_manifest::<toml::Value>(repo_root).map_err(|e| e.to_string())?;
    let targets = patch_path_targets(&manifest)?;
    let exclude = manifest.workspace.map(|w| w.exclude).unwrap_or_default();
    Ok((targets, exclude))
}

/// Evaluate every check. Pure apart from reads.
pub fn evaluate(repo_root: &Path) -> (Vec<Violation>, Scope) {
    let mut out = Vec::new();
    let mut scope = Scope::default();
    let (targets, exclude) = match targets(repo_root) {
        Ok(t) => t,
        Err(e) => {
            out.push(violation(
                "vendor-patches-manifest-unreadable",
                "Cargo.toml",
                e,
            ));
            return (out, scope);
        }
    };
    let lock = match read_lock(repo_root) {
        Ok(l) => l,
        Err(e) => {
            out.push(violation(
                "vendor-patches-manifest-unreadable",
                "Cargo.lock",
                format!("Cargo.lock {e}"),
            ));
            return (out, scope);
        }
    };

    // Nothing lives under vendor/ unless it is a patch target (or the metadata).
    let target_dirs: BTreeSet<String> = targets.iter().map(|t| t.path.clone()).collect();
    if let Ok(entries) = std::fs::read_dir(repo_root.join(VENDOR_DIR)) {
        for e in entries.flatten() {
            let rel = format!("{VENDOR_DIR}/{}", e.file_name().to_string_lossy());
            if rel != PATCHES_DIR && !target_dirs.contains(&rel) {
                out.push(violation(
                    "vendor-dir-not-patch-target",
                    &rel,
                    "under vendor/ but not a root `[patch.*]` path target — vendored code must be \
                     wired, recorded and guarded, or removed",
                ));
            }
        }
    }

    for t in &targets {
        scope.vendored += 1;
        if !is_vendor_target_shape(&t.path) {
            out.push(violation(
                "vendor-patch-target-outside-vendor",
                &t.path,
                "a `[patch.*]` path target must be exactly `vendor/<name>`: the vendored set and \
                 the content scans' `vendor/` prefix are one definition, and its record lives at \
                 `vendor/patches/<name>`",
            ));
            continue;
        }
        check_one(repo_root, t, &exclude, &lock, &mut out, &mut scope);
    }
    (out, scope)
}

/// `vendor/<name>`: one level under `vendor/`, and not the metadata dir.
fn is_vendor_target_shape(path: &str) -> bool {
    path.strip_prefix("vendor/")
        .is_some_and(|name| !name.is_empty() && !name.contains('/') && name != "patches")
}

fn check_one(
    repo_root: &Path,
    t: &PatchPath,
    exclude: &[String],
    lock: &[LockPackage],
    out: &mut Vec<Violation>,
    scope: &mut Scope,
) {
    let crate_dir = repo_root.join(&t.path);
    if !crate_dir.join("Cargo.toml").is_file() {
        out.push(violation(
            "vendor-crate-missing",
            &t.path,
            "patch target has no Cargo.toml",
        ));
        return;
    }
    if !exclude.iter().any(|e| e.trim_end_matches('/') == t.path) {
        out.push(violation(
            "vendor-not-excluded",
            &t.path,
            "not in root `[workspace] exclude`: a path dependency under the root would become a \
             member and its upstream code would be swept by workspace fmt/clippy/lints",
        ));
    }

    let meta_dir = metadata_dir(&t.path);
    let md = match std::fs::read_to_string(repo_root.join(&meta_dir).join("PATCH.md")) {
        Ok(s) => s,
        Err(e) => {
            out.push(violation(
                "vendor-patch-metadata-missing",
                format!("{meta_dir}/PATCH.md"),
                e.to_string(),
            ));
            return;
        }
    };
    let meta = match parse_patch_md(&md) {
        Ok(m) => m,
        Err(e) => {
            out.push(violation(
                "vendor-patch-metadata-missing",
                format!("{meta_dir}/PATCH.md"),
                e,
            ));
            return;
        }
    };
    if meta.upstream_crate != t.package {
        out.push(violation(
            "vendor-patch-metadata-missing",
            format!("{meta_dir}/PATCH.md"),
            format!(
                "upstream-crate `{}` is not the patched package `{}`",
                meta.upstream_crate, t.package
            ),
        ));
    }

    // Cargo.lock resolves the upstream name+version to the PATH copy.
    let named: Vec<&LockPackage> = lock
        .iter()
        .filter(|p| p.name == meta.upstream_crate)
        .collect();
    let patched = named
        .iter()
        .any(|p| p.version == meta.upstream_version && p.source.is_none());
    let registry_copy = named.iter().any(|p| p.source.is_some());
    if !patched || registry_copy {
        out.push(violation(
            "vendor-patch-not-in-lock",
            "Cargo.lock",
            format!(
                "`{} {}` must resolve to the path copy (no `source`) and no registry copy may remain \
                 — a requirement bumped past the vendored version silently UNPATCHES it (cargo only \
                 warns); follow the drop procedure in {meta_dir}/PATCH.md",
                meta.upstream_crate, meta.upstream_version
            ),
        ));
    }

    let diff_text = match std::fs::read_to_string(repo_root.join(&meta_dir).join("PATCH.diff")) {
        Ok(s) => s,
        Err(e) => {
            out.push(violation(
                "vendor-patch-metadata-missing",
                format!("{meta_dir}/PATCH.diff"),
                e.to_string(),
            ));
            return;
        }
    };
    let diff = match parse_diff(&diff_text) {
        Ok(d) => d,
        Err(DiffError::NotAdditive(m)) => {
            out.push(violation(
                "vendor-patch-not-additive",
                format!("{meta_dir}/PATCH.diff"),
                m,
            ));
            return;
        }
        Err(DiffError::Invalid(m)) => {
            out.push(violation(
                "vendor-patch-diff-invalid",
                format!("{meta_dir}/PATCH.diff"),
                m,
            ));
            return;
        }
    };
    let added = added_lines(&diff);
    scope.added_lines += added;
    if added == 0 {
        out.push(violation(
            "vendor-patch-diff-empty",
            format!("{meta_dir}/PATCH.diff"),
            "the recorded patch adds nothing — an unpatched crate should not be vendored",
        ));
        return;
    }

    let tree = match read_tree(&crate_dir) {
        Ok(t) => t,
        Err(e) => {
            out.push(violation(
                if e.starts_with("symlink") {
                    "vendor-tree-symlink"
                } else {
                    "vendor-tree-drift"
                },
                &t.path,
                e,
            ));
            return;
        }
    };
    let pristine = match pristine_from(&tree, &diff) {
        Ok(p) => p,
        Err(DiffError::NotAdditive(m) | DiffError::Invalid(m)) => {
            out.push(violation(
                "vendor-patch-diff-invalid",
                format!("{meta_dir}/PATCH.diff"),
                m,
            ));
            return;
        }
    };
    let digest = tree_digest(&pristine);
    if digest != meta.pristine_tree_digest {
        out.push(violation(
            "vendor-tree-drift",
            &t.path,
            format!(
                "the tree minus PATCH.diff digests to {digest}, PATCH.md records {} — an edit \
                 outside the recorded patch, or an extra/missing file",
                meta.pristine_tree_digest
            ),
        ));
    }
}

/// `--list`.
pub fn run_list(repo_root: &Path) -> Result<()> {
    let (targets, _) = targets(repo_root)
        .map_err(|e| anyhow::anyhow!("vendor-patches-manifest-unreadable: {e}"))?;
    for t in targets {
        println!("{}", t.path);
    }
    Ok(())
}

/// `--pristine-digest <dir> --vendored <path>`: the Layer-6 anchor.
pub fn run_pristine_digest(repo_root: &Path, dir: &Path, vendored: &str) -> Result<()> {
    let meta_dir = metadata_dir(vendored.trim_end_matches('/'));
    let md = std::fs::read_to_string(repo_root.join(&meta_dir).join("PATCH.md"))
        .with_context(|| format!("vendor-patch-metadata-missing: {meta_dir}/PATCH.md"))?;
    let meta = parse_patch_md(&md)
        .map_err(|e| anyhow::anyhow!("vendor-patch-metadata-missing: {meta_dir}/PATCH.md {e}"))?;
    let files = read_tree(dir).map_err(|e| anyhow::anyhow!("vendor-pristine-unreadable: {e}"))?;
    if files.is_empty() {
        bail!(
            "vendor-pristine-unreadable: {} holds no files",
            dir.display()
        );
    }
    let digest = tree_digest(&files);
    println!("PRISTINE_DIGEST={digest}");
    if digest != meta.pristine_tree_digest {
        bail!(
            "vendor-pristine-digest-mismatch: upstream {} {} digests to {digest}, PATCH.md records {}",
            meta.upstream_crate,
            meta.upstream_version,
            meta.pristine_tree_digest
        );
    }
    emit_ok(format!(
        "vendor-pristine-anchored-{}-{}",
        meta.upstream_crate, meta.upstream_version
    ));
    Ok(())
}

/// `--upstream-meta --vendored <path>`: print `name version sha256` for the
/// Layer-6 fetch (one home: PATCH.md).
pub fn run_upstream_meta(repo_root: &Path, vendored: &str) -> Result<()> {
    let meta_dir = metadata_dir(vendored.trim_end_matches('/'));
    let md = std::fs::read_to_string(repo_root.join(&meta_dir).join("PATCH.md"))
        .with_context(|| format!("vendor-patch-metadata-missing: {meta_dir}/PATCH.md"))?;
    let meta = parse_patch_md(&md)
        .map_err(|e| anyhow::anyhow!("vendor-patch-metadata-missing: {meta_dir}/PATCH.md {e}"))?;
    println!(
        "{} {} {}",
        meta.upstream_crate, meta.upstream_version, meta.upstream_crate_sha256
    );
    Ok(())
}

/// `--packages`: `<name> <version>` per vendored crate, for the audit's
/// always-NEW classification of advisories against vendored code.
pub fn run_packages(repo_root: &Path) -> Result<()> {
    for (name, version) in vendored_packages(repo_root)? {
        println!("{name} {version}");
    }
    Ok(())
}

/// `--attributed-lock <out>`. Rewrites the document (so it keeps its own
/// `toml::Value` round trip); WHICH packages comes from [`vendored_packages`].
pub fn run_attributed_lock(repo_root: &Path, out: &Path) -> Result<()> {
    let src = std::fs::read_to_string(repo_root.join("Cargo.lock"))
        .context("vendor-patches-manifest-unreadable: Cargo.lock")?;
    let mut doc: toml::Table =
        toml::from_str(&src).context("vendor-patches-manifest-unreadable: Cargo.lock")?;
    let wanted = vendored_packages(repo_root)?;
    let mut attributed = 0;
    if let Some(pkgs) = doc.get_mut("package").and_then(toml::Value::as_array_mut) {
        for p in pkgs.iter_mut().filter_map(toml::Value::as_table_mut) {
            let key = (
                str_field(p, "name").unwrap_or_default().to_string(),
                str_field(p, "version").unwrap_or_default().to_string(),
            );
            if p.get("source").is_none() && wanted.contains(&key) {
                p.insert(
                    "source".into(),
                    toml::Value::String(CRATES_IO_SOURCE.into()),
                );
                attributed += 1;
            }
        }
    }
    if attributed != wanted.len() {
        bail!(
            "vendor-patch-not-in-lock: attributed {attributed} of {} vendored crates — the lock \
             does not resolve every patch target to its path copy",
            wanted.len()
        );
    }
    std::fs::write(
        out,
        toml::to_string(&doc).context("serialise attributed lock")?,
    )
    .with_context(|| format!("write {}", out.display()))?;
    eprintln!("VENDOR_ATTRIBUTED={attributed}");
    Ok(())
}

/// `(name, version)` of every vendored crate, from its OWN manifest (never from
/// bash, never from the lock).
fn vendored_packages(repo_root: &Path) -> Result<BTreeSet<(String, String)>> {
    let (targets, _) = targets(repo_root)
        .map_err(|e| anyhow::anyhow!("vendor-patches-manifest-unreadable: {e}"))?;
    // name+version from each vendored crate's OWN manifest, never from bash.
    let mut wanted = BTreeSet::new();
    for t in &targets {
        let manifest: toml::Table = toml::from_str(
            &std::fs::read_to_string(repo_root.join(&t.path).join("Cargo.toml"))
                .with_context(|| format!("vendor-crate-missing: {}", t.path))?,
        )
        .with_context(|| format!("vendor-crate-missing: {} manifest unparseable", t.path))?;
        let pkg = manifest
            .get("package")
            .and_then(toml::Value::as_table)
            .with_context(|| format!("vendor-crate-missing: {} has no [package]", t.path))?;
        let name = str_field(pkg, "name").context("vendor-crate-missing: no package name")?;
        let version = str_field(pkg, "version").context("vendor-crate-missing: no version")?;
        wanted.insert((name.to_string(), version.to_string()));
    }
    Ok(wanted)
}

/// The guard.
pub fn run(repo_root: &Path, _explain: bool) -> Result<()> {
    let (violations, scope) = evaluate(repo_root);
    emit_scope(format!(
        "{} vendored crate(s), {} added patch line(s), {} violation(s)",
        scope.vendored,
        scope.added_lines,
        violations.len()
    ));
    if violations.is_empty() {
        emit_ok(format!(
            "vendor-patches-clean-vendored-{}-added-{}",
            scope.vendored, scope.added_lines
        ));
        return Ok(());
    }
    for v in &violations {
        if v.token == "vendor-patches-manifest-unreadable" {
            println!(
                "ERROR: PRECONDITION [{}] {} — {} (discovery/precondition failure, NOT a diff defect)",
                v.token, v.path, v.detail
            );
        } else {
            println!("VIOLATION: [{}] {} {}", v.token, v.path, v.detail);
        }
    }
    let first = violations
        .iter()
        .find(|v| v.token == "vendor-patches-manifest-unreadable")
        .or_else(|| violations.first())
        .map_or("vendor-patches-violation", |v| v.token);
    bail!("{first}-{}", violations.len())
}

/// Path helper for the wrapper-less modes.
pub fn resolve(repo_root: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        repo_root.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "a\nb\nc\nd\n";

    fn tree(content: &str) -> Vec<TreeFile> {
        vec![TreeFile {
            path: "src/x.rs".into(),
            executable: false,
            content: content.as_bytes().to_vec(),
        }]
    }

    #[test]
    fn reverse_apply_removes_exactly_the_added_lines() {
        let vendored = "a\nb\nNEW1\nNEW2\nc\nd\nTAIL\n";
        let diff = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,3 +1,5 @@\n a\n b\n+NEW1\n+NEW2\n c\n@@ -4 +6,2 @@\n d\n+TAIL\n";
        let parsed = parse_diff(diff).unwrap();
        assert_eq!(added_lines(&parsed), 3);
        let pristine = pristine_from(&tree(vendored), &parsed).unwrap();
        assert_eq!(pristine[0].content, FILE.as_bytes());
    }

    #[test]
    fn a_removed_line_is_not_additive() {
        let diff = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,2 +1,2 @@\n a\n-b\n+B\n";
        assert!(matches!(parse_diff(diff), Err(DiffError::NotAdditive(_))));
    }

    #[test]
    fn count_mismatch_and_context_mismatch_are_invalid() {
        // Header claims 3 new lines, body has 2.
        let short = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,1 +1,3 @@\n a\n+N\n";
        assert!(matches!(parse_diff(short), Err(DiffError::Invalid(_))));
        // Context line `z` does not match the tree.
        let ctx = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,1 +1,2 @@\n z\n+N\n";
        let parsed = parse_diff(ctx).unwrap();
        assert!(matches!(
            pristine_from(&tree("a\nN\nb\n"), &parsed),
            Err(DiffError::Invalid(_))
        ));
    }

    #[test]
    fn a_hunk_for_an_absent_file_is_invalid() {
        let diff = "--- a/src/gone.rs\n+++ b/src/gone.rs\n@@ -1,1 +1,2 @@\n a\n+N\n";
        let parsed = parse_diff(diff).unwrap();
        assert!(matches!(
            pristine_from(&tree(FILE), &parsed),
            Err(DiffError::Invalid(_))
        ));
    }

    #[test]
    fn digest_is_order_independent_and_sensitive_to_content_path_and_mode() {
        let a = TreeFile {
            path: "a".into(),
            executable: false,
            content: b"1".to_vec(),
        };
        let b = TreeFile {
            path: "b".into(),
            executable: false,
            content: b"2".to_vec(),
        };
        let base = tree_digest(&[a.clone(), b.clone()]);
        assert_eq!(base, tree_digest(&[b.clone(), a.clone()]));
        let mut changed = a.clone();
        changed.content = b"1x".to_vec();
        assert_ne!(base, tree_digest(&[changed, b.clone()]));
        let mut renamed = a.clone();
        renamed.path = "c".into();
        assert_ne!(base, tree_digest(&[renamed, b.clone()]));
        let mut exec = a.clone();
        exec.executable = true;
        assert_ne!(base, tree_digest(&[exec, b.clone()]));
        assert_ne!(base, tree_digest(&[a]));
    }

    #[test]
    fn patch_md_block_requires_every_field() {
        let ok = "x\n<!-- vendor-patch:begin -->\n```\nupstream-crate: w\nupstream-version: 1\nupstream-crate-sha256: s\npristine-tree-digest: d\n```\n<!-- vendor-patch:end -->\n";
        assert_eq!(parse_patch_md(ok).unwrap().upstream_version, "1");
        let missing = ok.replace("pristine-tree-digest: d\n", "");
        assert!(parse_patch_md(&missing)
            .unwrap_err()
            .contains("pristine-tree-digest"));
        assert!(parse_patch_md("no block").is_err());
    }
}
