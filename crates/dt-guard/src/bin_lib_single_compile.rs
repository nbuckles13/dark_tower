//! `bin-lib-single-compile` subcommand — a crate's binary must not re-declare
//! its library's modules.
//!
//! When a crate has both a lib and a bin target and the bin root (`main.rs`)
//! declares `mod x;` for a module the lib root also declares, cargo compiles
//! that module twice — once per target — and runs its `#[cfg(test)]` unit
//! tests twice. Nothing fails: the only symptom is a slower build and a CI test
//! suite that runs hundreds of tests a second time (ac-service and gc-service
//! did exactly this until 2026-10-05). A thin bin imports the lib instead
//! (`use ac_service::config;`).
//!
//! ## What is checked
//!
//! * **Crates**: every `[workspace] members` entry of the root `Cargo.toml`
//!   whose manifest yields a lib target AND at least one bin target. Targets
//!   come from the manifest (`[lib] path`, `[[bin]] path`) with cargo's
//!   defaults (`src/lib.rs`, `src/main.rs`, `src/bin/*.rs`) when not declared.
//!   No service list: the next crate that gains a lib is covered.
//! * **Declarations**: file-module declarations at the top level of each root
//!   file — `mod x;` with any visibility (`pub`, `pub(crate)`, …) and any
//!   attributes. `#[path = "…"] mod x;` is treated exactly like `mod x;`: both
//!   resolve to a file, and the comparison is by resolved file, not by name.
//!   `#[cfg(…)] mod x;` is NOT exempt — a cfg-gated duplicate still compiles
//!   twice under that cfg.
//! * **Not checked**: inline `mod x { … }` blocks (no second file is
//!   compiled) and modules nested inside other items. A bin-only module that
//!   the lib does not declare is fine.
//!
//! ## Vacuity
//!
//! Finding zero lib+bin crates is a PRECONDITION failure, never a pass: a
//! discovery that silently returned nothing (wrong `--root`, a renamed
//! manifest key) would otherwise read exactly like a clean tree.

use crate::common::cargo_manifest::{members_contain_glob, read_root_manifest};
use crate::common::explain::{print_finding, Finding};
use crate::common::status::{emit_ok, emit_scope};
use crate::common::test_code_filter::blank_file;
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const DUPLICATE_MODULE: &str = "bin_redeclares_lib_module";
pub const MANIFEST_UNREADABLE: &str = "bin_lib_manifest_unreadable";
pub const MEMBERS_UNUSABLE: &str = "bin_lib_members_unusable";
pub const NO_LIB_BIN_CRATES: &str = "bin_lib_no_lib_bin_crates_found";
pub const ROOT_FILE_UNREADABLE: &str = "bin_lib_root_file_unreadable";

const PRECONDITION_RULES: &[&str] = &[
    MANIFEST_UNREADABLE,
    MEMBERS_UNUSABLE,
    NO_LIB_BIN_CRATES,
    ROOT_FILE_UNREADABLE,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule_id: &'static str,
    pub file: String,
    pub detail: String,
}

/// A top-level file-module declaration in a crate root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModDecl {
    pub name: String,
    /// The file it resolves to (candidates when it does not exist on disk).
    pub resolved: PathBuf,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let (hits, crates) = evaluate(repo_root);
    emit_scope(format!("{crates} lib+bin crates, {} hits", hits.len()));

    if hits.is_empty() {
        emit_ok(format!("bin-lib-single-compile-clean-crates-{crates}"));
        return Ok(());
    }
    for hit in &hits {
        let token = hit.rule_id.replace('_', "-");
        if explain {
            let policy = format!("bin-lib-single-compile::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.file,
                row: 0,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[("reason", &token)],
                src_file: file!(),
                src_line: line!(),
            });
        } else if PRECONDITION_RULES.contains(&hit.rule_id) {
            println!(
                "ERROR: PRECONDITION [{token}] {} — {} (discovery/precondition failure, \
                 NOT a diff defect; see docs/runbooks/devloop-validation.md §6.3.1)",
                hit.file, hit.detail
            );
        } else {
            println!("VIOLATION: [{token}] {} {}", hit.file, hit.detail);
        }
    }
    let first = hits
        .iter()
        .find(|h| PRECONDITION_RULES.contains(&h.rule_id))
        .or(hits.first())
        .map_or("bin-lib-single-compile", |h| h.rule_id);
    anyhow::bail!("{}-{}", first.replace('_', "-"), hits.len())
}

/// Evaluate the tree at `repo_root`. Returns the hits and the number of
/// lib+bin crates examined.
pub fn evaluate(repo_root: &Path) -> (Vec<Hit>, usize) {
    let mut hits = Vec::new();
    let manifest = match read_root_manifest::<toml::Value>(repo_root) {
        Ok(m) => m,
        Err(e) => {
            hits.push(hit(
                MANIFEST_UNREADABLE,
                "Cargo.toml",
                format!("workspace Cargo.toml {e}"),
            ));
            return (hits, 0);
        }
    };
    let members = manifest.workspace.unwrap_or_default().members;
    if members.is_empty() || members_contain_glob(&members) {
        hits.push(hit(
            MEMBERS_UNUSABLE,
            "Cargo.toml",
            "`[workspace] members` is empty or uses globs; enumerate the members so every crate \
             is provably checked"
                .to_string(),
        ));
        return (hits, 0);
    }

    let mut crates = 0;
    for member in &members {
        let crate_dir = repo_root.join(member);
        let manifest_rel = format!("{member}/Cargo.toml");
        let targets = match crate_targets(&crate_dir) {
            Ok(t) => t,
            Err(e) => {
                hits.push(hit(MANIFEST_UNREADABLE, &manifest_rel, e));
                continue;
            }
        };
        let Some(lib) = targets.lib else { continue };
        if targets.bins.is_empty() {
            continue;
        }
        crates += 1;
        let lib_mods = match read_root_mods(&crate_dir, &lib) {
            Ok(m) => m,
            Err(e) => {
                hits.push(hit(ROOT_FILE_UNREADABLE, &rel(&lib, member), e));
                continue;
            }
        };
        for bin in &targets.bins {
            let bin_rel = rel(bin, member);
            let bin_mods = match read_root_mods(&crate_dir, bin) {
                Ok(m) => m,
                Err(e) => {
                    hits.push(hit(ROOT_FILE_UNREADABLE, &bin_rel, e));
                    continue;
                }
            };
            hits.extend(duplicates(&bin_rel, &bin_mods, &lib_mods));
        }
    }
    if crates == 0 && hits.is_empty() {
        hits.push(hit(
            NO_LIB_BIN_CRATES,
            "Cargo.toml",
            format!(
                "no workspace member has both a lib and a bin target ({} members examined) — \
                 discovery found nothing to check, which is never a pass",
                members.len()
            ),
        ));
    }
    (hits, crates)
}

/// Bin declarations that resolve to a file the lib also declares.
pub fn duplicates(bin_rel: &str, bin_mods: &[ModDecl], lib_mods: &[ModDecl]) -> Vec<Hit> {
    let lib_files: BTreeMap<&Path, &str> = lib_mods
        .iter()
        .map(|m| (m.resolved.as_path(), m.name.as_str()))
        .collect();
    bin_mods
        .iter()
        .filter_map(|m| {
            lib_files.get(m.resolved.as_path()).map(|lib_name| {
                hit(
                    DUPLICATE_MODULE,
                    bin_rel,
                    format!(
                        "declares `mod {}` for {}, which the lib also compiles as `{lib_name}` — \
                         the module and its unit tests compile twice; import it from the lib \
                         instead (`use <lib_crate>::{lib_name};`)",
                        m.name,
                        m.resolved.display()
                    ),
                )
            })
        })
        .collect()
}

#[derive(Debug, Default)]
struct Targets {
    lib: Option<PathBuf>,
    bins: Vec<PathBuf>,
}

/// Lib and bin root files (relative to the crate dir), per cargo's rules.
fn crate_targets(crate_dir: &Path) -> std::result::Result<Targets, String> {
    let src = std::fs::read_to_string(crate_dir.join("Cargo.toml"))
        .map_err(|e| format!("unreadable ({e})"))?;
    let doc: toml::Value =
        toml::from_str(&src).map_err(|e| format!("failed to parse as TOML ({e})"))?;
    let mut t = Targets::default();

    let lib_path = doc
        .get("lib")
        .and_then(|l| l.get("path"))
        .and_then(|p| p.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("src/lib.rs"));
    if crate_dir.join(&lib_path).is_file() {
        t.lib = Some(lib_path);
    }

    let declared: Vec<PathBuf> = doc
        .get("bin")
        .and_then(|b| b.as_array())
        .map(|bins| {
            bins.iter()
                .filter_map(|b| b.get("path").and_then(|p| p.as_str()))
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default();
    let autobins = doc
        .get("package")
        .and_then(|p| p.get("autobins"))
        .and_then(|a| a.as_bool())
        .unwrap_or(true);
    let mut bins = declared;
    if autobins {
        let main = PathBuf::from("src/main.rs");
        if crate_dir.join(&main).is_file() && !bins.contains(&main) {
            bins.push(main);
        }
        if let Ok(rd) = std::fs::read_dir(crate_dir.join("src/bin")) {
            let mut extra: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "rs"))
                .filter_map(|p| p.strip_prefix(crate_dir).ok().map(Path::to_path_buf))
                .filter(|p| !bins.contains(p))
                .collect();
            extra.sort();
            bins.extend(extra);
        }
    }
    t.bins = bins
        .into_iter()
        .filter(|b| crate_dir.join(b).is_file())
        .collect();
    Ok(t)
}

fn read_root_mods(crate_dir: &Path, root_rel: &Path) -> std::result::Result<Vec<ModDecl>, String> {
    let root = crate_dir.join(root_rel);
    let src = std::fs::read_to_string(&root).map_err(|e| format!("unreadable ({e})"))?;
    let dir = root_rel.parent().unwrap_or(Path::new(""));
    Ok(top_level_file_mods(&src)?
        .into_iter()
        .map(|(name, path_attr)| {
            let resolved = resolve(crate_dir, dir, &name, path_attr.as_deref());
            ModDecl { name, resolved }
        })
        .collect())
}

/// The file a root-level `mod name;` loads, relative to the crate dir.
/// `#[path]` is relative to the root file's directory; otherwise `name.rs`
/// or `name/mod.rs` there (the first that exists; `name.rs` when neither does).
fn resolve(crate_dir: &Path, dir: &Path, name: &str, path_attr: Option<&str>) -> PathBuf {
    if let Some(p) = path_attr {
        return normalize(&dir.join(p));
    }
    let flat = dir.join(format!("{name}.rs"));
    let nested = dir.join(name).join("mod.rs");
    if !crate_dir.join(&flat).is_file() && crate_dir.join(&nested).is_file() {
        nested
    } else {
        flat
    }
}

/// Lexical `.`/`..` normalization (no filesystem access).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn rel(file: &Path, member: &str) -> String {
    format!("{member}/{}", file.display())
}

fn hit(rule_id: &'static str, file: &str, detail: String) -> Hit {
    Hit {
        rule_id,
        file: file.to_string(),
        detail,
    }
}

/// `mod NAME;` declarations at brace depth 0 of `src`, with their
/// `#[path = "…"]` attribute if present. Comments and string bodies are blanked
/// by the shared lexer (`blank_file`) before matching, so a commented-out or quoted
/// `mod x;` never counts.
pub fn top_level_file_mods(src: &str) -> Result<Vec<(String, Option<String>)>, String> {
    let (blanked, depth) = blank_with_depth(src)?;
    let mut out = Vec::new();
    for caps in MOD_DECL_RE.captures_iter(&blanked) {
        let (Some(whole), Some(name)) = (caps.get(0), caps.get(2)) else {
            continue;
        };
        // The `mod` keyword's own depth decides top-level-ness; attributes
        // before it share the depth (they cannot contain braces once blanked).
        if depth.get(name.start()).copied() != Some(0) {
            continue;
        }
        // A preceding identifier character means this `mod` is a suffix of a
        // longer word the regex started mid-way through (e.g. `xmod a;`).
        if let Some(prev) = whole
            .start()
            .checked_sub(1)
            .and_then(|p| blanked.as_bytes().get(p))
        {
            if prev.is_ascii_alphanumeric() || *prev == b'_' {
                continue;
            }
        }
        let attrs = caps.get(1).map_or("", |m| m.as_str());
        // `#[path]` needs the literal's text: read it from the source at the
        // same offsets (blanking preserves byte positions).
        let attr_src = caps.get(1).and_then(|m| src.get(m.range())).unwrap_or("");
        let path_attr = if attrs.contains("path") {
            PATH_ATTR_RE
                .captures(attr_src)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        } else {
            None
        };
        out.push((name.as_str().to_string(), path_attr));
    }
    Ok(out)
}

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static MOD_DECL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?s)((?:#\s*\[[^\]]*\]\s*)*)(?:pub\s*(?:\([^)]*\))?\s+)?mod\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)\s*;",
    )
    .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static PATH_ATTR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"#\s*\[\s*path\s*=\s*"([^"]*)"\s*\]"#).expect("static pattern compiles")
});

/// Brace depth at every byte of `blanked` (comments and literals already
/// blanked by [`blank_file`], so every remaining brace is code), plus the
/// blanked text itself. Fails if blanking changed the length, which would make
/// offsets invalid in the source.
fn blank_with_depth(src: &str) -> Result<(String, Vec<u32>), String> {
    let blanked = blank_file(src);
    if blanked.len() != src.len() {
        return Err(format!(
            "blanking changed the file length ({} -> {} bytes); cannot map offsets",
            src.len(),
            blanked.len()
        ));
    }
    let mut depth = Vec::with_capacity(blanked.len() + 1);
    let mut d: u32 = 0;
    for b in blanked.bytes() {
        if b == b'}' {
            d = d.saturating_sub(1);
        }
        depth.push(d);
        if b == b'{' {
            d += 1;
        }
    }
    depth.push(d);
    Ok((blanked, depth))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn names(src: &str) -> Vec<String> {
        top_level_file_mods(src)
            .unwrap()
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    }

    #[test]
    fn plain_and_visibility_forms_are_file_modules() {
        let src = "mod a;\npub mod b;\npub(crate) mod c;\npub(in crate) mod d;\n#[cfg(unix)]\nmod e;\nmod r#f;\n";
        assert_eq!(names(src), ["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn inline_modules_and_nested_declarations_are_ignored() {
        let src = "mod tests { mod inner; }\nfn f() { mod local; }\nmod real;\n";
        assert_eq!(names(src), ["real"]);
    }

    #[test]
    fn comments_and_literals_never_count() {
        let src = "// mod a;\n/* mod b; /* nested mod c; */ */\nconst S: &str = \"mod d;\";\n\
                   const R: &str = r#\"mod e; { \"#;\nconst C: char = '{';\nfn g<'a>(x: &'a str) {}\nmod f;\n";
        assert_eq!(names(src), ["f"]);
    }

    #[test]
    fn path_attribute_is_captured() {
        let got =
            top_level_file_mods("#[path = \"shared/x.rs\"]\nmod x;\n#[allow(unused)]\nmod y;\n")
                .unwrap();
        assert_eq!(
            got,
            [
                ("x".to_string(), Some("shared/x.rs".to_string())),
                ("y".to_string(), None)
            ]
        );
    }

    #[test]
    fn identifier_suffix_is_not_a_mod_keyword() {
        assert_eq!(names("const xmod: u8 = 0; struct amod;\nmod z;\n"), ["z"]);
    }

    // --- whole-tree fixtures ---------------------------------------------

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn workspace(members: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        let list = members
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ");
        write(
            dir.path(),
            "Cargo.toml",
            &format!("[workspace]\nmembers = [{list}]\n"),
        );
        dir
    }

    fn svc(dir: &Path, name: &str, lib: &str, main: &str) {
        write(
            dir,
            &format!("{name}/Cargo.toml"),
            &format!("[package]\nname = \"{name}\"\n"),
        );
        write(dir, &format!("{name}/src/lib.rs"), lib);
        write(dir, &format!("{name}/src/main.rs"), main);
        write(dir, &format!("{name}/src/config.rs"), "");
        write(dir, &format!("{name}/src/errors/mod.rs"), "");
    }

    fn rules(hits: &[Hit]) -> Vec<&'static str> {
        hits.iter().map(|h| h.rule_id).collect()
    }

    #[test]
    fn redeclared_module_must_hit() {
        // Positive control: the shape ac-service had at 3651ebf7.
        let ws = workspace(&["svc"]);
        svc(
            ws.path(),
            "svc",
            "pub mod config;\npub mod errors;\n",
            "mod config;\nmod errors;\nuse config::X;\nfn main() {}\n",
        );
        let (hits, crates) = evaluate(ws.path());
        assert_eq!(crates, 1);
        assert_eq!(rules(&hits), [DUPLICATE_MODULE, DUPLICATE_MODULE]);
        assert!(hits.iter().all(|h| h.file == "svc/src/main.rs"));
    }

    #[test]
    fn thin_main_is_clean() {
        let ws = workspace(&["svc"]);
        svc(
            ws.path(),
            "svc",
            "pub mod config;\npub mod errors;\n",
            "use svc::config;\nfn main() {}\n",
        );
        let (hits, crates) = evaluate(ws.path());
        assert_eq!((hits, crates), (vec![], 1));
    }

    #[test]
    fn bin_only_module_is_clean() {
        let ws = workspace(&["svc"]);
        svc(
            ws.path(),
            "svc",
            "pub mod config;\n",
            "mod errors;\nfn main() {}\n",
        );
        assert!(evaluate(ws.path()).0.is_empty());
    }

    #[test]
    fn path_attribute_resolving_to_a_lib_module_hits() {
        let ws = workspace(&["svc"]);
        svc(
            ws.path(),
            "svc",
            "pub mod config;\n",
            "#[path = \"./config.rs\"]\nmod settings;\nfn main() {}\n",
        );
        assert_eq!(rules(&evaluate(ws.path()).0), [DUPLICATE_MODULE]);
    }

    #[test]
    fn declared_bin_paths_and_src_bin_are_checked() {
        let ws = workspace(&["svc"]);
        write(
            ws.path(),
            "svc/Cargo.toml",
            "[package]\nname = \"svc\"\n[lib]\npath = \"src/core.rs\"\n[[bin]]\nname = \"x\"\npath = \"src/x.rs\"\n",
        );
        write(ws.path(), "svc/src/core.rs", "pub mod config;\n");
        write(ws.path(), "svc/src/config.rs", "");
        write(ws.path(), "svc/src/x.rs", "mod config;\nfn main() {}\n");
        write(ws.path(), "svc/src/bin/tool.rs", "fn main() {}\n");
        let (hits, crates) = evaluate(ws.path());
        assert_eq!(crates, 1);
        assert_eq!(rules(&hits), [DUPLICATE_MODULE]);
        assert_eq!(hits[0].file, "svc/src/x.rs");
    }

    #[test]
    fn no_lib_bin_crate_is_a_precondition_not_a_pass() {
        let ws = workspace(&["lib_only", "bin_only"]);
        write(
            ws.path(),
            "lib_only/Cargo.toml",
            "[package]\nname = \"lib_only\"\n",
        );
        write(ws.path(), "lib_only/src/lib.rs", "");
        write(
            ws.path(),
            "bin_only/Cargo.toml",
            "[package]\nname = \"bin_only\"\n",
        );
        write(ws.path(), "bin_only/src/main.rs", "mod a;\nfn main() {}\n");
        let (hits, crates) = evaluate(ws.path());
        assert_eq!(crates, 0);
        assert_eq!(rules(&hits), [NO_LIB_BIN_CRATES]);
    }

    #[test]
    fn glob_members_and_missing_manifest_are_preconditions() {
        let ws = workspace(&["crates/*"]);
        assert_eq!(rules(&evaluate(ws.path()).0), [MEMBERS_UNUSABLE]);
        let ws = workspace(&["gone"]);
        assert_eq!(rules(&evaluate(ws.path()).0), [MANIFEST_UNREADABLE]);
    }

    #[test]
    fn real_tree_has_lib_bin_crates_and_is_clean() {
        // The repo itself: discovery must find the service crates (vacuity
        // floor), and after the 2026-10-05 thin-main change none re-declares.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (hits, crates) = evaluate(&root);
        assert!(crates >= 4, "expected >= 4 lib+bin crates, found {crates}");
        assert!(hits.is_empty(), "{hits:?}");
    }
}
