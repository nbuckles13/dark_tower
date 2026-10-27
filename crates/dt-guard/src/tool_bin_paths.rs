//! `tool-bin-paths` subcommand — the dev-tool binaries (`dt-guard`, `dt-story`)
//! have ONE path home: `scripts/lib/tool-bins.sh`.
//!
//! The tools build with a dedicated non-LTO cargo profile (`[profile.devtools]`
//! in `Cargo.toml`), so their binaries live at `target/<profile>/<crate>`. A
//! consumer that restates that path, or a producer that builds them with another
//! profile, splits producer from consumer: the build writes one path while a
//! caller reads another — and if the other path holds an old binary, the caller
//! gets a silently stale verdict instead of an error. tool-bins.sh derives every
//! path from the profile name; this guard keeps it the only place that does.
//!
//! | Rule | REASON token | Fires when |
//! |---|---|---|
//! | R1a | `tool-bin-path-literal` | a literal `target/<segment>[/<segment>…]/<crate>` (e.g. `target/<profile>/<crate>`, `target/<triple>/<profile>/<crate>`) for a crate in tool-bins.sh's `DT_TOOLS_CRATES`, in ANY scanned file — tool-bins.sh included (its paths must derive from `$DT_TOOLS_PROFILE`) |
//! | R1b | `tool-bin-build-outside-home` | a `cargo build` command naming a tool crate (`-p X`, `-pX`, `--package X`, `--package=X`) together with a profile selection (`--release`, `-r`, `--profile X`, `--profile=X`), outside tool-bins.sh; inside tool-bins.sh, a `cargo build` whose profile selection is anything but exactly `--profile "$DT_TOOLS_PROFILE"` |
//! | R2 | `tool-profile-drift` | tool-bins.sh's `DT_TOOLS_PROFILE` names no `[profile.<name>]` in the root `Cargo.toml`, names one without `inherits` (a built-in profile — e.g. `release` would silently restore the fat-LTO build), or names a built-in (`dev`, `release`, `test`, `bench`) |
//!
//! A `cargo build -p dt-guard` with NO profile flag (the CI coverage job's
//! instrumented dev-profile build) is not a finding: it does not write the
//! path consumers read and is pointed at explicitly via `$DT_GUARD`.
//!
//! ## Scope
//!
//! Every tracked file plus every untracked-not-ignored file (`git ls-files`),
//! so a new script is checked before it is staged. Excluded: `docs/devloop-outputs/`
//! and `docs/debates/` — dated records of what was true then, never instructions.
//! Everything else (scripts, hooks, workflows, SKILL.md files, runbooks, ADRs,
//! `docs/TODO.md`, Rust sources) is an instruction someone will follow, so a
//! stale path there is a recovery-loop bug. Non-UTF-8 files are skipped (binary).
//! `cargo build` commands split by a trailing `\` are joined before matching.
//!
//! The crate set and the profile name are READ from tool-bins.sh
//! (`DT_TOOLS_CRATES`, `DT_TOOLS_PROFILE`), never restated here, so a tool added
//! there is covered with no change to this module.
//!
//! ## Vacuity
//!
//! PRECONDITION, never a pass: tool-bins.sh unreadable; `DT_TOOLS_PROFILE` or
//! `DT_TOOLS_CRATES` missing, duplicated, or malformed; `Cargo.toml`
//! unreadable/unparseable; file enumeration failed, or did not include tool-bins.sh
//! itself (an empty enumeration is a sub-case) (a wrong `--root` would otherwise read clean).
//!
//! Not checked: a path assembled at runtime from pieces (`"target/" + p + ...`)
//! — the guard sees text, not evaluation. tool-bins.sh is the only place such
//! assembly is legitimate, and its build line is pinned by R1b.

use crate::common::cargo_cli::{
    logical_lines, selected_packages, selected_profile, SelectedProfile,
};
use crate::common::cargo_manifest::read_root_manifest;
use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::{get_tracked_files, get_untracked_files};
use crate::common::ident::is_ident_byte;
use crate::common::status::{emit_ok, emit_scope};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

/// The one home, repo-relative.
pub const TOOL_BINS_REL: &str = "scripts/lib/tool-bins.sh";

/// Dated records — what was true when written, never instructions.
pub const EXCLUDED_PREFIXES: &[&str] = &["docs/devloop-outputs/", "docs/debates/"];

/// Cargo's built-in profiles. `DT_TOOLS_PROFILE` naming one of these is drift
/// even though `Cargo.toml` may carry a `[profile.release]` table.
const BUILTIN_PROFILES: &[&str] = &["dev", "release", "test", "bench"];

pub const PATH_LITERAL: &str = "tool_bin_path_literal";
pub const BUILD_OUTSIDE_HOME: &str = "tool_bin_build_outside_home";
pub const PROFILE_DRIFT: &str = "tool_profile_drift";

pub const TOOL_BINS_UNREADABLE: &str = "tool_bins_unreadable";
pub const TOOL_BINS_ASSIGNMENT_UNUSABLE: &str = "tool_bins_assignment_unusable";
pub const MANIFEST_UNREADABLE: &str = "tool_bins_manifest_unreadable";
pub const ENUMERATION_FAILED: &str = "tool_bins_enumeration_failed";
pub const HOME_NOT_ENUMERATED: &str = "tool_bins_home_not_enumerated";

const PRECONDITION_RULES: &[&str] = &[
    TOOL_BINS_UNREADABLE,
    TOOL_BINS_ASSIGNMENT_UNUSABLE,
    MANIFEST_UNREADABLE,
    ENUMERATION_FAILED,
    HOME_NOT_ENUMERATED,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule_id: &'static str,
    pub file: String,
    /// 1-based; 0 = file-level.
    pub line: usize,
    pub detail: String,
}

/// What tool-bins.sh declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolBins {
    pub profile: String,
    pub crates: Vec<String>,
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let (mut hits, scanned) = evaluate(repo_root);
    emit_scope(format!("{scanned} files scanned, {} hits", hits.len()));

    if hits.is_empty() {
        emit_ok(format!("tool-bin-paths-clean-files-{scanned}"));
        return Ok(());
    }
    // Preconditions first: Layer 3 re-emits only the first few lines.
    hits.sort_by_key(|h| !PRECONDITION_RULES.contains(&h.rule_id));
    for hit in &hits {
        let token = hit.rule_id.replace('_', "-");
        if explain {
            let policy = format!("tool-bin-paths::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.file,
                row: hit.line,
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
            println!(
                "VIOLATION: [{token}] {}:{} {} — derive it from {TOOL_BINS_REL} \
                 (DT_GUARD_BIN / DT_STORY_BIN, dt_build_tool, or \
                 `scripts/lib/tool-bins.sh build` / `scripts/lib/tool-bins.sh <crate> <args>`)",
                hit.file, hit.line, hit.detail
            );
        }
    }
    let first = hits.first().map_or("tool-bin-paths", |h| h.rule_id);
    anyhow::bail!("{}-{}", first.replace('_', "-"), hits.len())
}

/// Evaluate the tree at `repo_root`. Returns the hits and the number of files
/// whose text was scanned.
pub fn evaluate(repo_root: &Path) -> (Vec<Hit>, usize) {
    let mut hits = Vec::new();

    let tool_bins = match std::fs::read_to_string(repo_root.join(TOOL_BINS_REL)) {
        Ok(src) => match parse_tool_bins(&src) {
            Ok(t) => t,
            Err(detail) => {
                hits.push(hit(TOOL_BINS_ASSIGNMENT_UNUSABLE, TOOL_BINS_REL, 0, detail));
                return (hits, 0);
            }
        },
        Err(e) => {
            hits.push(hit(
                TOOL_BINS_UNREADABLE,
                TOOL_BINS_REL,
                0,
                format!("the dev-tool binary home is unreadable ({e})"),
            ));
            return (hits, 0);
        }
    };

    hits.extend(check_profile(repo_root, &tool_bins.profile));

    let files = match enumerate(repo_root) {
        Ok(f) => f,
        Err(e) => {
            hits.push(hit(
                ENUMERATION_FAILED,
                ".",
                0,
                format!("git file enumeration failed ({e:#})"),
            ));
            return (hits, 0);
        }
    };
    if !files.contains(TOOL_BINS_REL) {
        hits.push(hit(
            HOME_NOT_ENUMERATED,
            ".",
            0,
            format!(
                "{} files enumerated and {TOOL_BINS_REL} is not among them — wrong --root or \
                 not a git work tree; nothing was provably checked",
                files.len()
            ),
        ));
        return (hits, 0);
    }

    let mut scanned = 0;
    for rel in &files {
        if EXCLUDED_PREFIXES.iter().any(|p| rel.starts_with(p)) {
            continue;
        }
        // Deleted-but-tracked, directories (submodules), and binary files carry
        // no instruction text to check.
        let Ok(bytes) = std::fs::read(repo_root.join(rel)) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        scanned += 1;
        hits.extend(scan_text(rel, &text, &tool_bins));
    }
    (hits, scanned)
}

fn enumerate(repo_root: &Path) -> Result<BTreeSet<String>> {
    let mut files: BTreeSet<String> = get_tracked_files(repo_root, ".", &[])?
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    files.extend(
        get_untracked_files(repo_root, Path::new("."), &[])?
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    Ok(files)
}

/// Parse tool-bins.sh's `DT_TOOLS_PROFILE=` and `DT_TOOLS_CRATES=` assignments.
/// Each must appear exactly once, at line start, unconditionally.
pub fn parse_tool_bins(src: &str) -> std::result::Result<ToolBins, String> {
    let profile = single_assignment(src, "DT_TOOLS_PROFILE")?;
    if profile.is_empty() || !profile.chars().all(is_name_char) {
        return Err(format!(
            "DT_TOOLS_PROFILE='{profile}' is not a plain profile name"
        ));
    }
    let crates_raw = single_assignment(src, "DT_TOOLS_CRATES")?;
    let crates: Vec<String> = crates_raw.split_whitespace().map(str::to_string).collect();
    if crates.is_empty() || crates.iter().any(|c| !c.chars().all(is_name_char)) {
        return Err(format!(
            "DT_TOOLS_CRATES='{crates_raw}' is not a space-separated list of crate names"
        ));
    }
    Ok(ToolBins { profile, crates })
}

fn single_assignment(src: &str, var: &str) -> std::result::Result<String, String> {
    let prefix = format!("{var}=");
    let values: Vec<&str> = src
        .lines()
        .filter_map(|l| l.strip_prefix(&prefix))
        .collect();
    match values.as_slice() {
        [v] => Ok(v.trim().trim_matches(|c| c == '"' || c == '\'').to_string()),
        [] => Err(format!(
            "no `{var}=` assignment at line start — the guard cannot learn what to police"
        )),
        _ => Err(format!(
            "{} `{var}=` assignments — exactly one is the contract",
            values.len()
        )),
    }
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// R2: the named profile exists in Cargo.toml as a custom (inheriting) profile.
fn check_profile(repo_root: &Path, profile: &str) -> Vec<Hit> {
    let manifest = match read_root_manifest::<toml::Value>(repo_root) {
        Ok(m) => m,
        Err(e) => {
            return vec![hit(
                MANIFEST_UNREADABLE,
                "Cargo.toml",
                0,
                format!("workspace Cargo.toml {e}"),
            )]
        }
    };
    if BUILTIN_PROFILES.contains(&profile) {
        return vec![hit(
            PROFILE_DRIFT,
            TOOL_BINS_REL,
            0,
            format!(
                "DT_TOOLS_PROFILE='{profile}' is a cargo built-in profile; the dev tools need \
                 their own `[profile.<name>]` (inherits = \"release\", no LTO)"
            ),
        )];
    }
    match manifest.profile.get(profile) {
        None => vec![hit(
            PROFILE_DRIFT,
            TOOL_BINS_REL,
            0,
            format!(
                "DT_TOOLS_PROFILE='{profile}' but Cargo.toml has no `[profile.{profile}]` — the \
                 build would fail and every consumer would read a path nothing writes"
            ),
        )],
        Some(table) if table.get("inherits").is_none() => vec![hit(
            PROFILE_DRIFT,
            "Cargo.toml",
            0,
            format!(
                "`[profile.{profile}]` has no `inherits` key — cargo rejects a custom profile \
                 without one"
            ),
        )],
        Some(_) => Vec::new(),
    }
}

/// R1a + R1b over one file's text.
pub fn scan_text(rel: &str, text: &str, tool_bins: &ToolBins) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        for crate_name in &tool_bins.crates {
            if let Some(m) = find_path_literal(line, crate_name) {
                hits.push(hit(PATH_LITERAL, rel, idx + 1, m));
            }
        }
    }
    let is_home = rel == TOOL_BINS_REL;
    for (line_no, logical) in logical_lines(text) {
        if let Some(detail) = check_cargo_build(&logical, tool_bins, is_home) {
            hits.push(hit(BUILD_OUTSIDE_HOME, rel, line_no, detail));
        }
    }
    hits
}

/// `target/<segment>[/<segment>...]/<crate>`: one or more literal path
/// segments (`[A-Za-z0-9_.-]+`; a `$VAR` segment is derivation, not a literal),
/// so both `target/<profile>/<crate>` and the `--target <triple>` layout
/// `target/<triple>/<profile>/<crate>` match. `target` must not be the tail of
/// a longer word, and `<crate>` must not be the head of a longer name. Returns
/// the matched text.
fn find_path_literal(line: &str, crate_name: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(off) = line.get(from..)?.find("target/") {
        let start = from + off;
        from = start + 1;
        if start > 0 && bytes.get(start - 1).is_some_and(|b| is_ident_byte(*b)) {
            continue;
        }
        let mut pos = start + "target/".len();
        let mut segments = 0;
        while let Some(rest) = line.get(pos..) {
            if segments > 0 {
                if let Some(after_crate) = rest.strip_prefix(crate_name) {
                    let bounded = !after_crate
                        .bytes()
                        .next()
                        .is_some_and(|b| is_ident_byte(b) || b == b'-');
                    if bounded {
                        return line.get(start..pos + crate_name.len()).map(str::to_string);
                    }
                }
            }
            let seg_len = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
                .count();
            if seg_len == 0 || rest.as_bytes().get(seg_len) != Some(&b'/') {
                break;
            }
            pos += seg_len + 1;
            segments += 1;
        }
    }
    None
}

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CARGO_BUILD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:^|[^A-Za-z0-9_-])cargo\s+(?:\+\S+\s+)?build(?:\s|$)")
        .expect("static pattern compiles")
});

/// R1b over one logical line. Returns a detail string on a finding. Profile
/// and package extraction is the shared `common::cargo_cli` tokenizer (the one
/// `release_build_profile` uses); this function owns only the policy.
/// Returns the FIRST offending command on the line.
fn check_cargo_build(logical: &str, tool_bins: &ToolBins, is_home: bool) -> Option<String> {
    for m in CARGO_BUILD_RE.find_iter(logical) {
        let args = logical.get(m.end()..).unwrap_or("");
        // One command: stop at a shell separator, so a profile flag on a
        // following `&& cargo build --release -p svc` is not attributed here.
        let command = args
            .split_whitespace()
            .take_while(|t| !matches!(*t, "&&" | "||" | "|" | ";") && !t.ends_with(';'))
            .collect::<Vec<_>>()
            .join(" ");
        let profile = selected_profile(&command);
        if is_home {
            // `--profile "$DT_TOOLS_PROFILE"` (quotes trimmed by the tokenizer).
            let derived = "$DT_TOOLS_PROFILE";
            if profile != SelectedProfile::Named(derived.to_string()) {
                return Some(format!(
                    "cargo build in the tool home selects {} — only `--profile \"{derived}\"` \
                     keeps the build on the profile every path is derived from",
                    describe(&profile)
                ));
            }
        } else if profile != SelectedProfile::None {
            if let Some(pkg) = selected_packages(&command)
                .into_iter()
                .find(|p| tool_bins.crates.contains(p))
            {
                return Some(format!(
                    "`cargo build` selecting {} for `{pkg}` outside the tool home builds a tool \
                     binary to a path consumers may not read",
                    describe(&profile)
                ));
            }
        }
    }
    None
}

fn describe(profile: &SelectedProfile) -> String {
    match profile {
        SelectedProfile::Release => "the release profile".to_string(),
        SelectedProfile::Named(n) => format!("profile `{n}`"),
        SelectedProfile::None => "no profile (dev)".to_string(),
    }
}

fn hit(rule_id: &'static str, file: &str, line: usize, detail: String) -> Hit {
    Hit {
        rule_id,
        file: file.to_string(),
        line,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    // Fixture literals are assembled with concat!/format! so this file carries no
    // literal tool-binary path or profile-selecting tool build of its own.
    const CARGO: &str = concat!("car", "go");
    const CB: &str = concat!("car", "go build");

    /// Expand the `{CB}` / `{CARGO}` placeholders.
    fn cb(s: &str) -> String {
        s.replace("{CB}", CB).replace("{CARGO}", CARGO)
    }
    const REL_GUARD: &str = concat!("tar", "get/release/dt-guard");
    const DEV_STORY: &str = concat!("tar", "get/devtools/dt-story");
    const DBG_GUARD: &str = concat!("tar", "get/debug/dt-guard");
    // `cargo build --target <triple>` layout.
    const TRIPLE_GUARD: &str = concat!("tar", "get/x86_64-unknown-linux-gnu/release/dt-guard");

    fn tb() -> ToolBins {
        ToolBins {
            profile: "devtools".into(),
            crates: vec!["dt-guard".into(), "dt-story".into()],
        }
    }

    fn rules(hits: &[Hit]) -> Vec<&'static str> {
        hits.iter().map(|h| h.rule_id).collect()
    }

    fn scan(rel: &str, text: &str) -> Vec<Hit> {
        scan_text(rel, text, &tb())
    }

    // ---- R1a: path literals ----------------------------------------------------

    #[test]
    fn path_literal_each_profile_segment_hits_with_line() {
        for lit in [REL_GUARD, DEV_STORY, DBG_GUARD, TRIPLE_GUARD] {
            let text = format!("ok\nBIN=\"$ROOT/{lit}\"\n");
            let hits = scan("scripts/x.sh", &text);
            assert_eq!(rules(&hits), vec![PATH_LITERAL], "{lit}");
            assert_eq!(hits[0].line, 2, "{lit}");
            assert_eq!(hits[0].detail, lit);
        }
    }

    #[test]
    fn path_literal_applies_inside_the_tool_home_too() {
        let hits = scan(TOOL_BINS_REL, &format!("X=\"$R/{DEV_STORY}\"\n"));
        assert_eq!(rules(&hits), vec![PATH_LITERAL]);
    }

    #[test]
    fn path_literal_non_matches() {
        for text in [
            // Derived segment — the tool home's own form.
            "DT_GUARD_BIN=\"$DT_TOOLS_ROOT/target/$DT_TOOLS_PROFILE/dt-guard\"\n",
            // Uppercase env var tail, not a `target/` path.
            "DT=\"${CARGO_LLVM_COV_TARGET_DIR:?}/debug/dt-guard\"\n",
            // Longer word ending in `target`.
            "x=mytarget/release/dt-guard\n",
            // A different binary sharing a prefix.
            "x=target/release/dt-guard-core\n",
            // Derived multi-segment path.
            "x=\"$R/target/$TRIPLE/$PROFILE/dt-guard\"\n",
            // Literal first segment, derived second: still derivation.
            "x=\"$R/target/x86_64-unknown-linux-gnu/$PROFILE/dt-guard\"\n",
            // A service binary.
            "COPY target/release/gc-service /bin/\n",
        ] {
            assert!(
                scan("scripts/x.sh", text).is_empty(),
                "false positive: {text}"
            );
        }
    }

    // ---- R1b: profile-selecting tool builds --------------------------------------

    #[test]
    fn cargo_build_spellings_hit() {
        for cmd in [
            "{CB} --release -p dt-story",
            "{CB} -r -p dt-guard",
            "{CB} --package dt-guard --release",
            "{CB} --package=dt-story --profile devtools",
            "{CB} -pdt-guard --profile=devtools",
            "{CARGO} +nightly build --release -p dt-guard",
            "run: `{CB} --release -p dt-guard -p dt-story`.",
        ] {
            let hits = scan("docs/runbooks/x.md", &cb(&format!("{cmd}\n")));
            assert_eq!(rules(&hits), vec![BUILD_OUTSIDE_HOME], "{cmd}");
        }
    }

    #[test]
    fn cargo_build_continuation_lines_are_joined() {
        let text = &cb("x\n{CB} \\\n  --locked \\\n  -p dt-guard --release\n");
        let hits = scan("scripts/x.sh", text);
        assert_eq!(rules(&hits), vec![BUILD_OUTSIDE_HOME]);
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn cargo_build_non_matches() {
        for cmd in [
            // CI coverage: instrumented dev-profile build, pointed at via $DT_GUARD.
            "{CB} -p dt-guard --locked",
            // Services keep the release profile.
            "{CB} --release -p gc-service",
            // Profile flag belongs to the NEXT command.
            "{CB} -p dt-guard && {CB} --release -p gc-service",
            // A crate whose name merely starts with a tool name.
            "{CB} --release -p dt-guard-core",
            // Not a build.
            "cargo run -q -p dt-guard -- env-config --root .",
        ] {
            assert!(
                scan("scripts/x.sh", &cb(&format!("{cmd}\n"))).is_empty(),
                "false positive: {cmd}"
            );
        }
    }

    #[test]
    fn first_offending_build_on_a_line_is_reported() {
        let text = cb("{CB} --release -p dt-story && {CB} -r -p dt-guard\n");
        let hits = scan("scripts/x.sh", &text);
        assert_eq!(rules(&hits), vec![BUILD_OUTSIDE_HOME]);
        assert!(hits[0].detail.contains("`dt-story`"), "{hits:?}");
    }

    #[test]
    fn tool_home_accepts_only_the_derived_profile() {
        let ok = &cb("( cd \"$R\" && {CB} --profile \"$DT_TOOLS_PROFILE\" --target-dir \"$R/target\" -p \"$crate\" \"$@\" )\n");
        assert!(scan(TOOL_BINS_REL, ok).is_empty());
        for bad in [
            "{CB} --release -p \"$crate\"\n",
            "{CB} --profile devtools -p \"$crate\"\n",
            "{CB} -p \"$crate\"\n",
        ] {
            assert_eq!(
                rules(&scan(TOOL_BINS_REL, &cb(bad))),
                vec![BUILD_OUTSIDE_HOME],
                "{bad}"
            );
        }
    }

    // The crate set comes from tool-bins.sh: dropping a crate silences both
    // rules for it, and adding one covers it with no code change. (The
    // matcher-mutation proof is recorded in the devloop's main.md.)
    #[test]
    fn crate_set_is_read_not_hardcoded() {
        let crates_only_story = ToolBins {
            profile: "devtools".into(),
            crates: vec!["dt-story".into()],
        };
        // The crate set is READ, not hard-coded: dropping dt-guard from it
        // silences both rules for dt-guard.
        assert!(scan_text(
            "s.sh",
            &cb("{CB} -r --package dt-guard\n"),
            &crates_only_story
        )
        .is_empty());
        assert!(scan_text("s.sh", &format!("{REL_GUARD}\n"), &crates_only_story).is_empty());
        // ...and a third crate added there is covered with no code change.
        let with_third = ToolBins {
            profile: "devtools".into(),
            crates: vec!["dt-guard".into(), "dt-story".into(), "dt-new".into()],
        };
        assert_eq!(
            rules(&scan_text(
                "s.sh",
                &cb("{CB} -r --package dt-new\n"),
                &with_third
            )),
            vec![BUILD_OUTSIDE_HOME]
        );
    }

    // ---- tool-bins.sh parsing -----------------------------------------------------

    #[test]
    fn parse_reads_profile_and_crates() {
        let t = parse_tool_bins(
            "# c\nDT_TOOLS_PROFILE=devtools\nDT_TOOLS_CRATES=\"dt-guard dt-story\"\n",
        )
        .expect("parses");
        assert_eq!(t, tb());
    }

    #[test]
    fn parse_refuses_missing_duplicate_and_malformed() {
        for src in [
            "DT_TOOLS_CRATES=\"dt-guard\"\n",
            "DT_TOOLS_PROFILE=devtools\nDT_TOOLS_PROFILE=other\nDT_TOOLS_CRATES=\"dt-guard\"\n",
            "DT_TOOLS_PROFILE=\"${DT_TOOLS_PROFILE:-devtools}\"\nDT_TOOLS_CRATES=\"dt-guard\"\n",
            "DT_TOOLS_PROFILE=devtools\n",
            "DT_TOOLS_PROFILE=devtools\nDT_TOOLS_CRATES=\"\"\n",
        ] {
            assert!(parse_tool_bins(src).is_err(), "should refuse: {src}");
        }
    }

    // ---- whole-tree evaluate() over a real git repo -------------------------------

    fn git(dir: &Path, args: &[&str]) {
        let st = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .expect("git runs");
        assert!(st.success(), "git {args:?}");
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().expect("has parent")).expect("mkdir");
        std::fs::write(p, content).expect("write");
    }

    const HOME_SRC: &str = "DT_TOOLS_PROFILE=devtools\nDT_TOOLS_CRATES=\"dt-guard dt-story\"\n";
    const MANIFEST: &str =
        "[workspace]\nmembers = []\n\n[profile.devtools]\ninherits = \"release\"\nlto = false\n";

    fn repo(home: &str, manifest: &str) -> TempDir {
        let d = TempDir::new().expect("tempdir");
        git(d.path(), &["init", "-q"]);
        write(d.path(), TOOL_BINS_REL, home);
        write(d.path(), "Cargo.toml", manifest);
        git(d.path(), &["add", "-A"]);
        d
    }

    #[test]
    fn clean_repo_passes_and_counts_files() {
        let d = repo(HOME_SRC, MANIFEST);
        let (hits, scanned) = evaluate(d.path());
        assert!(hits.is_empty(), "{hits:?}");
        assert_eq!(scanned, 2);
    }

    #[test]
    fn untracked_file_is_scanned() {
        let d = repo(HOME_SRC, MANIFEST);
        write(d.path(), "scripts/new.sh", &format!("x={REL_GUARD}\n"));
        let (hits, _) = evaluate(d.path());
        assert_eq!(rules(&hits), vec![PATH_LITERAL]);
        assert_eq!(hits[0].file, "scripts/new.sh");
    }

    #[test]
    fn exclusion_is_only_the_dated_record_dirs() {
        let d = repo(HOME_SRC, MANIFEST);
        let lit = format!("was {REL_GUARD}\n");
        write(d.path(), "docs/debates/2026-01-01-x.md", &lit);
        write(d.path(), "docs/devloop-outputs/2026-01-01-x/main.md", &lit);
        assert!(evaluate(d.path()).0.is_empty());
        write(d.path(), "docs/runbooks/x.md", &lit);
        let (hits, _) = evaluate(d.path());
        assert_eq!(rules(&hits), vec![PATH_LITERAL]);
        assert_eq!(hits[0].file, "docs/runbooks/x.md");
    }

    #[test]
    fn profile_drift_cases() {
        let no_profile = "[workspace]\nmembers = []\n";
        let no_inherits = "[workspace]\nmembers = []\n[profile.devtools]\nlto = false\n";
        let hits = evaluate(repo(HOME_SRC, no_profile).path()).0;
        assert_eq!(rules(&hits), vec![PROFILE_DRIFT]);
        let hits = evaluate(repo(HOME_SRC, no_inherits).path()).0;
        assert_eq!(rules(&hits), vec![PROFILE_DRIFT]);
        let release_home = "DT_TOOLS_PROFILE=release\nDT_TOOLS_CRATES=\"dt-guard dt-story\"\n";
        let with_release = format!("{MANIFEST}\n[profile.release]\nlto = true\n");
        let hits = evaluate(repo(release_home, &with_release).path()).0;
        assert_eq!(rules(&hits), vec![PROFILE_DRIFT]);
    }

    #[test]
    fn vacuity_is_a_precondition_never_clean() {
        // Not a git repo: enumeration fails.
        let d = TempDir::new().expect("tempdir");
        write(d.path(), TOOL_BINS_REL, HOME_SRC);
        write(d.path(), "Cargo.toml", MANIFEST);
        let (hits, scanned) = evaluate(d.path());
        assert_eq!(rules(&hits), vec![ENUMERATION_FAILED]);
        assert_eq!(scanned, 0);

        // A git repo where the home is ignored, so it is not enumerated.
        let d = repo(HOME_SRC, MANIFEST);
        git(d.path(), &["rm", "-q", "--cached", TOOL_BINS_REL]);
        write(d.path(), ".gitignore", "scripts/\n");
        let (hits, _) = evaluate(d.path());
        assert_eq!(rules(&hits), vec![HOME_NOT_ENUMERATED]);

        // Home missing / assignments unusable / manifest missing.
        let d = TempDir::new().expect("tempdir");
        assert_eq!(rules(&evaluate(d.path()).0), vec![TOOL_BINS_UNREADABLE]);
        let d = repo("DT_TOOLS_PROFILE=devtools\n", MANIFEST);
        assert_eq!(
            rules(&evaluate(d.path()).0),
            vec![TOOL_BINS_ASSIGNMENT_UNUSABLE]
        );
        let d = repo(HOME_SRC, MANIFEST);
        std::fs::remove_file(d.path().join("Cargo.toml")).expect("rm");
        assert!(rules(&evaluate(d.path()).0).contains(&MANIFEST_UNREADABLE));
    }
}
