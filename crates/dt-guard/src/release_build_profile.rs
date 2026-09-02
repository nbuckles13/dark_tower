//! `release-build-profile` subcommand — asserts the deployed-artifact
//! premises that ADR-0036 §11's compile-time controls rest on.
//!
//! # What premise, and why it needs asserting
//!
//! ADR-0036 §11 specifies: *"A development-only per-frame tracing feature is
//! guarded by a compile error in release builds, **not by a CI check**. A
//! control that has to notice fails silently for anyone building outside the
//! pipeline; a compile error has nothing to notice."*
//!
//! That control is `#[cfg(debug_assertions)] compile_error!(...)`. It is
//! **inert** the moment `debug_assertions` is off in a shipped artifact —
//! and nothing in this repository asserted that condition. This subcommand
//! asserts it.
//!
//! # What this guard does NOT claim (read before citing a green)
//!
//! * **This guard IS a CI check** — precisely the category §11 declined. That
//!   is not a contradiction: a compile error cannot assert its own premise, so
//!   something must, and only a check can. But it inherits the weakness §11
//!   named. **This guard does nothing for anyone building outside the
//!   pipeline** — a local `cargo build`, a dev container, a fork's CI.
//! * **The control being protected does not exist in tree yet.** As of
//!   2026-09-01 there is no `compile_error!` and no `debug_assertions` usage
//!   anywhere under `crates/`. §11 specifies the control; story task 23 lands
//!   the assertion that it fires. This guard is premise-only and is
//!   deliberately **not** a forcing function on that control existing —
//!   premise first, control second.
//! * **The channel list below is a vocabulary, and vocabularies are
//!   incomplete by construction.** §11's own closing argument applies:
//!   *"Vocabulary additions cannot be cited as the protection."* So the OK
//!   token and every message here say *"these enumerated inputs were checked
//!   and were clean"* — never *"the release premise holds"*. Those differ
//!   exactly at the N+1th channel, which is where it matters.
//!
//! A shape-based complement that would be total by construction —
//! `#[cfg(all(feature = "release-artifact", debug_assertions))] compile_error!`
//! with service Dockerfiles passing `--features release-artifact` — is a named
//! follow-up (touches four service crates' feature tables). It observes the
//! effective cfg rather than its causes. The two are complements: this guard
//! catches the source change early and legibly across all services at review
//! time; the cfg assertion catches the artifact totally.
//!
//! # Boundary against story task 23 (ANCHOR: do not delete this guard)
//!
//! Story task 23 lands "assert it is a compile error in a release build by
//! building with the release profile and the tracing feature enabled" — *a
//! real build, not a CI string check*. That phrasing is deliberately
//! contrastive with a string-scanning guard, and this **is** a string-scanning
//! guard. **They are complements, not duplicates**: this asserts the *premise*
//! (shipped artifacts are built release, with debug-assertions off); task 23
//! asserts the *control fires* given the premise. Neither subsumes the other,
//! and task 23 landing is not a reason to delete this.
//!
//! # Scope boundary — what is deliberately NOT scanned
//!
//! Stated once, at module level, so a future contributor does not "complete"
//! the guard by adding these:
//!
//! * `.github/workflows/ci.yml` and `ci-client.yml` `cargo build --release`
//!   lines build the **host** `dt-guard`/`dt-story` binaries. Different
//!   artifact class; not an encoding of this premise.
//! * `docs/BUILD_REQUIREMENTS.md` carries an illustrative Dockerfile.
//!   Documentation, not a build path.
//! * `RUSTFLAGS` appears legitimately in prose in six-plus tracked docs (mold
//!   linker, `-Zsanitizer`) and benignly at `ci.yml`'s `RUSTFLAGS: --cfg
//!   coverage`. None sets `debug-assertions`.
//!
//! **The RUSTFLAGS rule is value-keyed, not presence-keyed** — it fires on
//! `debug-assertions` being enabled, not on `RUSTFLAGS` appearing. That is the
//! primary false-positive defence; the execution-path scope restriction is
//! secondary belt-and-suspenders. This matters because an exclusion list rots
//! and a value key does not, and a guard that false-fails gets bypassed.
//!
//! **Note the deliberate asymmetry with `no_insecure_browser_flags`**, and do
//! not "harmonize" them: that rule *includes* docs and runbooks in scope,
//! because a runbook telling a developer to paste a cert-bypass flag **is**
//! the leak being guarded. Here, a doc showing a mold-linker `RUSTFLAGS`
//! recipe is not a leak. Two rules, opposite file scoping, for reasons that
//! are specific to each. Recording the reason and not merely the rule,
//! because a comment stating only the rule gets deleted by the same person
//! who would tidy the asymmetry away.
//!
//! # Enumerated channels
//!
//! The count is **derived** from `RULE_ORDER`, not stated here — see
//! `enumerated_channel_count`. A number in this comment would be a second
//! encoding of a fact the table already carries.
//!
//! | rule_id | Channel |
//! |---|---|
//! | `dockerfile_build_not_release` | A service Dockerfile's `cargo build` does not select the release profile |
//! | `dockerfile_cook_profile_mismatch` | `cargo chef cook` disagrees with `cargo build` on profile |
//! | `release_profile_debug_assertions` | `[profile.release]` / `[profile.release.package.*]` enables debug-assertions |
//! | `release_inheriting_profile` | An `inherits = "release"` profile enables it AND a Dockerfile selects it |
//! | `rustflags_debug_assertions` | `-C debug-assertions` (any spelling) on a build/deploy execution path |
//! | `cargo_profile_env_debug_assertions` | `CARGO_PROFILE_<PROFILE>_DEBUG_ASSERTIONS` via Dockerfile `ENV`/`ARG` or CI `env:` |
//! | `config_toml_release_profile` | A `[profile.release]` table directly inside `.cargo/config.toml` |
//!
//! ## A missing `[profile.release]` table is NOT a finding
//!
//! Deliberate, and measured rather than reasoned. Cargo's built-in release
//! profile already sets `debug-assertions = false`, so a manifest with no
//! `[profile.release]` table builds `--release` with the premise **holding**.
//! Confirmed twice independently: a throwaway crate carrying §11's exact
//! control (`#[cfg(debug_assertions)] compile_error!`) compiles clean under
//! `--release` with no profile table, and trips under the dev profile.
//! An earlier version failed closed there; that was a false positive, and a
//! guard that false-fails gets bypassed.
//!
//! The same experiment confirmed the `CARGO_PROFILE_<P>_DEBUG_ASSERTIONS`
//! channel empirically: with **no `[profile.release]` table at all**, setting
//! it flips `debug_assertions` ON. That channel had been reasoned from cargo's
//! documented semantics; it is now demonstrated, so a future reader tempted to
//! drop it as theoretical has a result to weigh rather than an argument.
//!
//! ## Why this module deserializes rather than reusing `cite_extract`'s resolvers
//!
//! `cite_extract.rs`'s `TOML_SECTION_RESOLVER` / `TOML_KEY_RESOLVER` answer a
//! different question — *does symbol X appear as a section-or-key*, for cite
//! resolution — and answer it by pattern-matching. This module needs typed,
//! section-scoped **value** reads of a Cargo manifest (`debug-assertions` as a
//! bool, nested under an arbitrary `[profile.<p>.package.<c>]` path), which a
//! resolver of that shape cannot express. That is why the crate now has two
//! TOML readers; they are not a duplication to collapse.
//!
//! Plus the **vacuity** classes and the **roster** classes — see
//! [`run`]. Vacuity failures are the point, not an edge case: a guard that
//! globs, matches zero files, compares nothing and reports OK asserts nothing
//! while looking like it does — the same inertness this guard exists to
//! prevent, turned on itself.

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::services::CANONICAL_SERVICES;
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const DOCKERFILE_BUILD_NOT_RELEASE: &str = "dockerfile_build_not_release";
pub const DOCKERFILE_COOK_PROFILE_MISMATCH: &str = "dockerfile_cook_profile_mismatch";
pub const RELEASE_PROFILE_DEBUG_ASSERTIONS: &str = "release_profile_debug_assertions";
pub const RELEASE_INHERITING_PROFILE: &str = "release_inheriting_profile";
pub const RUSTFLAGS_DEBUG_ASSERTIONS: &str = "rustflags_debug_assertions";
pub const CARGO_PROFILE_ENV_DEBUG_ASSERTIONS: &str = "cargo_profile_env_debug_assertions";
pub const CONFIG_TOML_RELEASE_PROFILE: &str = "config_toml_release_profile";

// Vacuity + drift classes. These FAIL; they never emit OK.
pub const NO_DOCKERFILES_DISCOVERED: &str = "no_dockerfiles_discovered";
pub const DOCKERFILE_NO_CARGO_BUILD_LINE: &str = "dockerfile_no_cargo_build_line";
pub const WORKSPACE_MANIFEST_UNREADABLE: &str = "workspace_manifest_unreadable";
/// A cargo CONFIG file failed to parse. Distinct from the workspace-manifest
/// class on purpose (@observability): a token saying `workspace-manifest`
/// sends the triager to `Cargo.toml`, which is fine — the broken file is
/// `.cargo/config.toml`. It is also *causable by an ordinary edit*, unlike the
/// manifest classes, so it must not inherit their "you cannot cause these by
/// editing a file" triage clause.
pub const CARGO_CONFIG_UNPARSEABLE: &str = "cargo_config_unparseable";
/// `[workspace] members` exists but no service roster could be derived from it
/// — e.g. glob members (`crates/*`), which cargo fully supports. Its own class
/// because the alternative is silence: with an empty derived roster BOTH halves
/// of the floor (per-service Dockerfile coverage and CANONICAL_SERVICES drift)
/// check nothing and the guard reports OK.
pub const SERVICE_ROSTER_UNDERIVABLE: &str = "service_roster_underivable";
pub const SERVICE_CRATE_WITHOUT_DOCKERFILE: &str = "service_crate_without_dockerfile";
pub const CANONICAL_SERVICES_ROSTER_DRIFT: &str = "canonical_services_roster_drift";

/// Directory holding per-service Docker build contexts.
const DOCKER_DIR: &str = "infra/docker";
/// Cargo config filenames that can carry profile tables or `rustflags`.
///
/// **Both spellings** (@security row 9): cargo still honours the legacy
/// unsuffixed `.cargo/config` for backward compatibility, so globbing only
/// `config.toml` leaves a bypass with identical effect and no warning.
///
/// Reachability is higher than it looks: `.cargo/` already exists at the repo
/// root (`.cargo/audit.toml` is tracked), so nobody has to create a
/// suspicious-looking new directory — just drop a file into an existing one.
const CARGO_CONFIG_FILENAMES: &[&str] = &["config.toml", "config"];
/// CI workflow directory — an execution path for `RUSTFLAGS` / `CARGO_PROFILE_*`.
const WORKFLOWS_DIR: &str = ".github/workflows";

/// Suffix identifying a workspace member that ships as a container image.
const SERVICE_CRATE_SUFFIX: &str = "-service";
const CRATES_PREFIX: &str = "crates/";

/// rustc's truthy set for boolean codegen options. A valueless
/// `-C debug-assertions` is ALSO enabled — see [`rustflags_enables_debug_assertions`].
const TRUTHY: &[&str] = &["y", "yes", "on", "true", "1"];

#[derive(Debug, Clone)]
struct Hit {
    rule_id: &'static str,
    detail: String,
    file: String,
    line: usize,
}

// ---------------------------------------------------------------------------
// Dockerfile scanning
// ---------------------------------------------------------------------------

/// One cargo invocation found in a Dockerfile, with its logical line number.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CargoInvocation {
    line: usize,
    /// Logical line: physical lines joined across `\` continuations.
    text: String,
}

/// Join `\`-continued physical lines into logical lines, keeping the line
/// number of the FIRST physical line.
///
/// Without this a `cargo build \` / `    --release ...` split — perfectly
/// legal Dockerfile syntax — reads as a `cargo build` with no profile flag
/// and the guard false-fails. A guard that false-fails gets bypassed, so the
/// join is not a nicety.
fn logical_lines(content: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed_end = raw.trim_end();
        let continues = trimmed_end.ends_with('\\');
        let body = trimmed_end.strip_suffix('\\').unwrap_or(trimmed_end);
        match pending.as_mut() {
            Some((_, acc)) => {
                acc.push(' ');
                acc.push_str(body.trim());
            }
            None => pending = Some((line_no, body.trim().to_string())),
        }
        if !continues {
            if let Some(entry) = pending.take() {
                out.push(entry);
            }
        }
    }
    if let Some(entry) = pending.take() {
        out.push(entry);
    }
    out
}

/// The profile a cargo invocation selects, if it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectedProfile {
    /// `--release` or `--profile release`.
    Release,
    /// `--profile <name>` for some non-release name.
    Named(String),
    /// No profile flag at all — cargo defaults to `dev`.
    None,
}

/// Extract the profile a cargo command line selects.
///
/// Accepts BOTH `--release` and `--profile release`: they are semantically
/// identical, and requiring the literal `--release` would false-fail a
/// legitimate release build expressed the long way.
fn selected_profile(cmd: &str) -> SelectedProfile {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    // `enumerate` rather than an index loop: the lookahead for
    // `--profile <name>` needs the position, but nothing here should be able
    // to index out of range. ADR-0002 no-panic — a guard that panics on
    // malformed input is worse than one that fails cleanly, because a panic
    // inside Layer 3 reads as a crashed pipeline rather than a finding.
    for (i, t) in tokens.iter().enumerate() {
        if *t == "--release" || *t == "-r" {
            return SelectedProfile::Release;
        }
        if let Some(name) = t.strip_prefix("--profile=") {
            return profile_from_name(name);
        }
        if *t == "--profile" {
            if let Some(name) = tokens.get(i + 1) {
                return profile_from_name(name);
            }
        }
    }
    SelectedProfile::None
}

fn profile_from_name(name: &str) -> SelectedProfile {
    let name = name.trim_matches(['"', '\'']);
    if name == "release" {
        SelectedProfile::Release
    } else {
        SelectedProfile::Named(name.to_string())
    }
}

/// Does this logical line invoke `cargo build` (the line producing the
/// shipped binary)?
fn is_cargo_build(cmd: &str) -> bool {
    cmd.contains("cargo build")
}

/// Does this logical line invoke `cargo chef cook` (the dependency-cache
/// pre-build)?
fn is_cargo_chef_cook(cmd: &str) -> bool {
    cmd.contains("cargo chef cook")
}

/// Per-Dockerfile scan outcome.
#[derive(Debug, Default)]
struct DockerfileScan {
    build: Vec<CargoInvocation>,
    cook: Vec<CargoInvocation>,
    /// Profiles named by `--profile <name>` anywhere in the file. Feeds the
    /// `inherits = "release"` cross-check: a custom profile only matters if
    /// something actually ships with it.
    selected_named_profiles: BTreeSet<String>,
}

fn scan_dockerfile(content: &str) -> DockerfileScan {
    let mut scan = DockerfileScan::default();
    for (line, text) in logical_lines(content) {
        if is_cargo_build(&text) {
            scan.build.push(CargoInvocation {
                line,
                text: text.clone(),
            });
        }
        if is_cargo_chef_cook(&text) {
            scan.cook.push(CargoInvocation {
                line,
                text: text.clone(),
            });
        }
        if is_cargo_build(&text) || is_cargo_chef_cook(&text) {
            if let SelectedProfile::Named(name) = selected_profile(&text) {
                scan.selected_named_profiles.insert(name);
            }
        }
    }
    scan
}

// ---------------------------------------------------------------------------
// RUSTFLAGS / CARGO_PROFILE_* scanning (value-keyed)
// ---------------------------------------------------------------------------

/// Normalise a rustflags argument sequence and report whether it enables
/// debug-assertions.
///
/// Handles every spelling rustc accepts, because one unmatched form is a
/// bypass rather than a gap:
///
/// * `-C debug-assertions=on` (space-separated, or split across two array
///   elements in `.cargo/config.toml` — the cargo book's own spelling)
/// * `-Cdebug-assertions=on` (no space)
/// * `--codegen debug-assertions=on` / `--codegen=debug-assertions=on`
/// * **valueless `-C debug-assertions`** — rustc treats a valueless boolean
///   codegen option as ENABLED. This is the shortest form and a matcher keyed
///   on `=<truthy>` misses it entirely.
///
/// Truthy set is rustc's own: `y`, `yes`, `on`, `true` (plus `1`). Falsey
/// (`n`, `no`, `off`, `false`) does not fire.
fn rustflags_enables_debug_assertions(args: &[String]) -> bool {
    // Join then re-split so `["-C", "debug-assertions=on"]` and
    // `"-C debug-assertions=on"` normalise to the same token stream.
    let joined = args.join(" ");
    let tokens: Vec<&str> = joined.split_whitespace().collect();
    // See `selected_profile` for why this is `enumerate` and not an index
    // loop: same lookahead shape, same ADR-0002 no-panic reason.
    for (i, t) in tokens.iter().enumerate() {
        // Forms carrying the option name in the SAME token.
        let inline = t
            .strip_prefix("-C")
            .or_else(|| t.strip_prefix("--codegen="))
            .or_else(|| t.strip_prefix("--codegen"));
        if let Some(rest) = inline {
            if !rest.is_empty() {
                if let Some(v) = codegen_debug_assertions_value(rest) {
                    return v;
                }
            }
        }
        // Forms where the option name is the NEXT token: `-C debug-assertions…`
        if *t == "-C" || *t == "--codegen" {
            if let Some(next) = tokens.get(i + 1) {
                if let Some(v) = codegen_debug_assertions_value(next) {
                    return v;
                }
            }
        }
    }
    false
}

/// Given a codegen-option body (`debug-assertions=on`, `debug-assertions`,
/// `opt-level=3`, …), return `Some(enabled)` if it is the debug-assertions
/// option, else `None`.
fn codegen_debug_assertions_value(body: &str) -> Option<bool> {
    let body = body.trim();
    let (name, value) = match body.split_once('=') {
        Some((n, v)) => (n.trim(), Some(v.trim())),
        // Valueless boolean codegen flag => enabled.
        None => (body, None),
    };
    if name != "debug-assertions" && name != "debug_assertions" {
        return None;
    }
    Some(match value {
        None => true,
        Some(v) => TRUTHY.contains(&v.trim_matches(['"', '\'']).to_ascii_lowercase().as_str()),
    })
}

/// Split a `CARGO_ENCODED_RUSTFLAGS` value on its `\x1f` unit separator.
///
/// Plain `RUSTFLAGS` is whitespace-separated; `CARGO_ENCODED_RUSTFLAGS` is
/// NOT, and a whitespace scan cannot see its argument boundaries. It is also
/// the variable someone reaches for precisely when scripting a build.
fn split_encoded_rustflags(value: &str) -> Vec<String> {
    value.split('\u{1f}').map(str::to_string).collect()
}

/// `CARGO_PROFILE_<PROFILE>_DEBUG_ASSERTIONS=<truthy>`: flips the profile
/// setting with `Cargo.toml` untouched and every Dockerfile still `--release`.
/// The subtlest channel — it leaves every file the task names byte-identical.
fn cargo_profile_env_enables(key: &str, value: &str) -> bool {
    let upper = key.trim().to_ascii_uppercase();
    if !upper.starts_with("CARGO_PROFILE_") || !upper.ends_with("_DEBUG_ASSERTIONS") {
        return false;
    }
    TRUTHY.contains(
        &value
            .trim()
            .trim_matches(['"', '\''])
            .to_ascii_lowercase()
            .as_str(),
    )
}

/// Scan a shell/Dockerfile/YAML line for an env assignment of interest.
/// Returns `(key, value)` for `KEY=value`, `KEY: value`, `ENV KEY value`,
/// `ENV KEY=value`, `ARG KEY=value`.
fn env_assignment(line: &str) -> Option<(String, String)> {
    let l = line.trim();
    let l = l
        .strip_prefix("ENV ")
        .or_else(|| l.strip_prefix("ARG "))
        .or_else(|| l.strip_prefix("export "))
        .unwrap_or(l);
    let l = l.trim();
    if let Some((k, v)) = l.split_once('=') {
        return Some((k.trim().to_string(), v.trim().to_string()));
    }
    if let Some((k, v)) = l.split_once(':') {
        return Some((k.trim().to_string(), v.trim().to_string()));
    }
    // `ENV KEY value` (space form)
    let mut parts = l.split_whitespace();
    let k = parts.next()?;
    let v = parts.next()?;
    Some((k.to_string(), v.to_string()))
}

/// Check one file's lines for the RUSTFLAGS and CARGO_PROFILE_* channels.
fn scan_execution_path_file(rel: &str, content: &str, hits: &mut Vec<Hit>) {
    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let Some((key, value)) = env_assignment(raw) else {
            continue;
        };
        let upper = key.to_ascii_uppercase();
        if upper == "RUSTFLAGS"
            || upper.ends_with("_RUSTFLAGS") && upper != "CARGO_ENCODED_RUSTFLAGS"
        {
            let cleaned = value.trim_matches(['"', '\'']).to_string();
            if rustflags_enables_debug_assertions(&[cleaned]) {
                hits.push(Hit {
                    rule_id: RUSTFLAGS_DEBUG_ASSERTIONS,
                    detail: format!("{key} enables debug-assertions"),
                    file: rel.to_string(),
                    line: line_no,
                });
            }
        } else if upper == "CARGO_ENCODED_RUSTFLAGS" {
            let cleaned = value.trim_matches(['"', '\'']).to_string();
            if rustflags_enables_debug_assertions(&split_encoded_rustflags(&cleaned)) {
                hits.push(Hit {
                    rule_id: RUSTFLAGS_DEBUG_ASSERTIONS,
                    detail: "CARGO_ENCODED_RUSTFLAGS enables debug-assertions".to_string(),
                    file: rel.to_string(),
                    line: line_no,
                });
            }
        } else if cargo_profile_env_enables(&key, &value) {
            hits.push(Hit {
                rule_id: CARGO_PROFILE_ENV_DEBUG_ASSERTIONS,
                detail: format!("{key} enables debug-assertions with Cargo.toml untouched"),
                file: rel.to_string(),
                line: line_no,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// TOML profile parsing — typed, total, one parser for both paths
// ---------------------------------------------------------------------------

/// A `[profile.<name>]` table's fields relevant to this premise.
///
/// Typed deser rather than a line scan, deliberately: TOML permits
/// `[profile] release.debug-assertions = true` as a dotted key and inline
/// tables, so a line-oriented scanner has a real miss window — and a premise
/// guard with a miss window "reads as coverage". `toml`/`winnow` is the
/// family cargo itself parses with, so the guard models the same semantics
/// as the consumer whose behaviour it asserts about. Per-package overrides
/// ARE part of cargo's profile semantics; a fixed-table-path check models a
/// simplified cargo, the parser lets us model the real one.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProfileTable {
    #[serde(default, alias = "debug_assertions")]
    debug_assertions: Option<bool>,
    #[serde(default)]
    inherits: Option<String>,
    /// `[profile.<p>.package.<name>]` — per-package overrides. Cargo permits
    /// `debug-assertions` here (unlike `panic`/`lto`/`codegen-units`), so
    /// this reaches the shipped binary. `"*"` is a valid package key meaning
    /// "all dependencies".
    #[serde(default)]
    package: Option<std::collections::BTreeMap<String, ProfileTable>>,
    /// `[profile.<p>.build-override]` — affects build scripts and proc
    /// macros ONLY, never the shipped binary. Parsed so the walk is total,
    /// but deliberately NOT a violation. Recorded rather than silently
    /// ignored so a future reader knows the omission is a decision.
    #[serde(default)]
    build_override: Option<Box<ProfileTable>>,
}

/// The workspace root manifest, parsed **once** into everything this guard
/// needs from it.
///
/// Previously this was two structs deserialized from the same string at two
/// call sites, with the second failure swallowed on the grounds that "the
/// caller's total parse already recorded it". That was false in one narrow
/// case (@code-reviewer): a `[workspace] members` of the wrong *type* leaves a
/// profile-only parse succeeding while the workspace parse fails — and the
/// roster floor then returned silently, green. One parse, one failure path.
#[derive(Debug, Default, serde::Deserialize)]
struct WorkspaceManifest {
    #[serde(default)]
    profile: std::collections::BTreeMap<String, ProfileTable>,
    /// `Option`, not `#[serde(default)]`: "there is no `[workspace]` table"
    /// and "there is an empty one" are different facts, and the roster floor
    /// needs to tell them apart rather than treat both as an empty roster.
    #[serde(default)]
    workspace: Option<WorkspaceTable>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct WorkspaceTable {
    #[serde(default)]
    members: Vec<String>,
}

/// Does this profile reach the shipped artifact?
///
/// `release` itself, plus any profile that `inherits = "release"` **and** is
/// actually selected by a service Dockerfile's `--profile <name>`. Coupling
/// the inheritance to actual selection keeps a legitimate local-only profile
/// from firing.
fn profile_reaches_release(
    name: &str,
    table: &ProfileTable,
    selected_named_profiles: &BTreeSet<String>,
) -> bool {
    if name == "release" {
        return true;
    }
    table.inherits.as_deref() == Some("release") && selected_named_profiles.contains(name)
}

/// Walk a release-reaching profile subtree and record every place
/// `debug-assertions = true` appears.
///
/// **Recursive by design (per @security row 8).** A fixed table path
/// (`[profile.release]` direct keys only) passes clean on
/// `[profile.release.package.mh-service] debug-assertions = true` — which is
/// the most precisely-aimed version of this attack, since it re-arms the
/// compile-time control in exactly the crate §11's control protects, while
/// `[profile.release]` itself stays byte-identical. One recursive check
/// closes that row and any sibling nobody has enumerated.
fn walk_profile_subtree(
    rel: &str,
    path_label: &str,
    table: &ProfileTable,
    rule_id: &'static str,
    hits: &mut Vec<Hit>,
) {
    if table.debug_assertions == Some(true) {
        hits.push(Hit {
            rule_id,
            detail: format!(
                "[{path_label}] sets debug-assertions = true — ADR-0036 §11's compile-time \
                 control is inert in artifacts built with this profile"
            ),
            file: rel.to_string(),
            line: 0,
        });
    }
    if let Some(pkgs) = &table.package {
        for (pkg, pkg_table) in pkgs {
            walk_profile_subtree(
                rel,
                &format!("{path_label}.package.{pkg}"),
                pkg_table,
                rule_id,
                hits,
            );
        }
    }
    // build-override is deliberately NOT walked for violations: it governs
    // build scripts and proc macros, which do not reach the shipped binary.
    // Touched here only to make the decision visible at the code site.
    let _ = &table.build_override;
}

/// Check every profile table in one TOML file.
fn check_profiles(
    rel: &str,
    profiles: &std::collections::BTreeMap<String, ProfileTable>,
    selected_named_profiles: &BTreeSet<String>,
    release_rule: &'static str,
    hits: &mut Vec<Hit>,
) {
    for (name, table) in profiles {
        if !profile_reaches_release(name, table, selected_named_profiles) {
            continue;
        }
        let rule = if name == "release" {
            release_rule
        } else {
            RELEASE_INHERITING_PROFILE
        };
        walk_profile_subtree(rel, &format!("profile.{name}"), table, rule, hits);
    }
}

/// `.cargo/config.toml`'s `[build]` / `[target.*]` rustflags arrays.
#[derive(Debug, Default, serde::Deserialize)]
struct CargoConfig {
    #[serde(default)]
    build: Option<BuildSection>,
    #[serde(default)]
    target: Option<std::collections::BTreeMap<String, BuildSection>>,
    #[serde(default)]
    profile: std::collections::BTreeMap<String, ProfileTable>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct BuildSection {
    #[serde(default)]
    rustflags: Option<RustflagsValue>,
}

/// `rustflags` accepts a string OR an array of strings. The array form splits
/// `-C` from its value across two elements — the cargo book's own spelling and
/// invisible to an adjacency matcher.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum RustflagsValue {
    List(Vec<String>),
    Str(String),
}

impl RustflagsValue {
    fn as_args(&self) -> Vec<String> {
        match self {
            RustflagsValue::List(v) => v.clone(),
            RustflagsValue::Str(s) => vec![s.clone()],
        }
    }
}

/// Discover every cargo config file in the repo.
///
/// Prefers `git ls-files` (tracked-only), mirroring the lesson recorded in
/// commit `28defd8` (validate-subdomain-regex-sync): an untracked scratch file
/// must not fail the build, and an unbounded walk over `target/` or
/// `node_modules/` produces noise or a bail that reads as a pass.
///
/// **Falls back to a bounded walk when git is unavailable** (tempdir fixtures,
/// a source export, a detached build context). The fallback must exist: if
/// this returned `Err` outside a git worktree the whole guard would abort, and
/// an aborting guard is indistinguishable from a broken one at the point where
/// it matters. The root-level path is always checked directly, because
/// `git ls-files` leading-glob pathspec behaviour for a root-level dotted
/// directory is not uniform across git versions.
fn discover_cargo_configs(repo_root: &Path) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();

    for name in CARGO_CONFIG_FILENAMES {
        let root_rel = format!(".cargo/{name}");
        if repo_root.join(&root_rel).is_file() {
            out.push(root_rel);
        }
    }

    let tracked_ok =
        CARGO_CONFIG_FILENAMES.iter().all(
            |name| match crate::common::git_changes::get_tracked_files(
                repo_root,
                &format!("*.cargo/{name}"),
                &[],
            ) {
                Ok(paths) => {
                    for p in paths {
                        out.push(p.display().to_string());
                    }
                    true
                }
                Err(_) => false,
            },
        );

    if !tracked_ok {
        // Bounded fallback walk. Depth cap plus the standard build/vendor
        // exclusions keep this from becoming the unbounded scan the
        // tracked-only rule exists to avoid.
        for entry in walkdir::WalkDir::new(repo_root)
            .max_depth(6)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !matches!(name.as_ref(), "target" | "node_modules" | ".git")
            })
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let is_cargo_config = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| CARGO_CONFIG_FILENAMES.contains(&n))
                .unwrap_or(false)
                && path.parent().and_then(|p| p.file_name())
                    == Some(std::ffi::OsStr::new(".cargo"));
            if is_cargo_config {
                if let Ok(rel) = path.strip_prefix(repo_root) {
                    out.push(rel.display().to_string());
                }
            }
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the release-build-profile policy.
///
/// Emits `STATUS=OK REASON=release-build-profile-inputs-checked-clean` on a
/// clean tree — a token naming the **enumeration**, never the conclusion, so a
/// reader cannot derive "the release premise holds" from a green. A counts
/// line precedes it making the scanned scope visible without reading source.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut hits: Vec<Hit> = Vec::new();

    // --- Discover Dockerfiles (scan set = the walk) -------------------------
    let docker_dir = repo_root.join(DOCKER_DIR);
    let mut dockerfiles: Vec<(String, PathBuf)> = Vec::new();
    if docker_dir.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&docker_dir)
            .with_context(|| format!("reading {}", docker_dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();
        for dir in entries {
            let dockerfile = dir.join("Dockerfile");
            if dockerfile.is_file() {
                let rel = format!(
                    "{DOCKER_DIR}/{}/Dockerfile",
                    dir.file_name().unwrap_or_default().to_string_lossy()
                );
                dockerfiles.push((rel, dockerfile));
            }
        }
    }

    // --- Scan each Dockerfile ---------------------------------------------
    let mut cargo_dockerfiles = 0usize;
    let mut all_named_profiles: BTreeSet<String> = BTreeSet::new();
    let mut scanned_service_dirs: BTreeSet<String> = BTreeSet::new();
    for (rel, path) in &dockerfiles {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

        // Execution-path channels (RUSTFLAGS / CARGO_ENCODED_RUSTFLAGS /
        // CARGO_PROFILE_*) are scanned from the SAME read as the
        // cargo-invocation channels. These used to be a second, tolerant
        // (`if let Ok`) pass over the identical paths — a redundant read plus a
        // strict-vs-tolerant inconsistency on the same inputs
        // (@code-reviewer MINOR 2). One read, one error policy: strict.
        //
        // Runs BEFORE the no-cargo-line `continue` below, deliberately: an
        // `ENV CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true` in a build context
        // is worth flagging regardless of which stage runs cargo.
        scan_execution_path_file(rel, &content, &mut hits);

        let scan = scan_dockerfile(&content);
        all_named_profiles.extend(scan.selected_named_profiles.iter().cloned());

        // A Dockerfile with no cargo at all is a non-Rust build context
        // (postgres/, grafana/, certs/, prometheus/). Those self-exclude by
        // having no cargo line — no hardcoded exclusion list to rot.
        if scan.build.is_empty() && scan.cook.is_empty() {
            continue;
        }
        cargo_dockerfiles += 1;
        if let Some(dir) = rel
            .strip_prefix(&format!("{DOCKER_DIR}/"))
            .and_then(|r| r.strip_suffix("/Dockerfile"))
        {
            scanned_service_dirs.insert(dir.to_string());
        }

        // VACUITY: a Rust build context with `cargo chef cook` but no
        // `cargo build` produces no binary from a checked line.
        if scan.build.is_empty() {
            hits.push(Hit {
                rule_id: DOCKERFILE_NO_CARGO_BUILD_LINE,
                detail: "Dockerfile has cargo activity but no `cargo build` line — \
                         nothing to check; this is a DISCOVERY failure, not a diff defect"
                    .to_string(),
                file: rel.clone(),
                line: 0,
            });
            continue;
        }

        let mut build_profiles: BTreeSet<String> = BTreeSet::new();
        for inv in &scan.build {
            match selected_profile(&inv.text) {
                SelectedProfile::Release => {
                    build_profiles.insert("release".to_string());
                }
                SelectedProfile::Named(name) => {
                    build_profiles.insert(name.clone());
                    hits.push(Hit {
                        rule_id: DOCKERFILE_BUILD_NOT_RELEASE,
                        detail: format!(
                            "`cargo build --profile {name}` — the shipped binary is not a \
                             release build, so ADR-0036 §11's compile-time control is inert"
                        ),
                        file: rel.clone(),
                        line: inv.line,
                    });
                }
                SelectedProfile::None => {
                    build_profiles.insert("dev".to_string());
                    hits.push(Hit {
                        rule_id: DOCKERFILE_BUILD_NOT_RELEASE,
                        detail: "`cargo build` selects no profile (cargo defaults to dev) — \
                                 the shipped binary is not a release build, so ADR-0036 §11's \
                                 compile-time control is inert"
                            .to_string(),
                        file: rel.clone(),
                        line: inv.line,
                    });
                }
            }
        }

        // cook-vs-build CONSISTENCY (not "cook must be release"): the
        // invariant is that the two agree. The shipped binary comes from
        // `cargo build`, so a mismatch is build-CACHE layer drift, NOT a
        // premise break — hence its own rule_id and a message that says so.
        for inv in &scan.cook {
            let cook_profile = match selected_profile(&inv.text) {
                SelectedProfile::Release => "release".to_string(),
                SelectedProfile::Named(n) => n,
                SelectedProfile::None => "dev".to_string(),
            };
            if !build_profiles.contains(&cook_profile) {
                hits.push(Hit {
                    rule_id: DOCKERFILE_COOK_PROFILE_MISMATCH,
                    detail: format!(
                        "`cargo chef cook` selects profile `{cook_profile}` but `cargo build` \
                         selects {:?}. THE SHIPPED BINARY IS UNAFFECTED — this is build-cache \
                         layer drift, not a premise break: chef cook only pre-builds \
                         dependencies, and cargo rebuilds them under the build profile anyway.",
                        build_profiles
                    ),
                    file: rel.clone(),
                    line: inv.line,
                });
            }
        }
    }

    // --- VACUITY: zero Dockerfiles discovered ------------------------------
    if cargo_dockerfiles == 0 {
        hits.push(Hit {
            rule_id: NO_DOCKERFILES_DISCOVERED,
            detail: format!(
                "no Dockerfile under {DOCKER_DIR}/*/ contains a cargo build — the walk matched \
                 nothing, so this guard checked nothing. This is a DISCOVERY/PRECONDITION \
                 failure (layout moved? wrong repo root?), NOT a diff defect; see \
                 docs/runbooks/devloop-validation.md §6.3.1 for lane routing."
            ),
            file: DOCKER_DIR.to_string(),
            line: 0,
        });
    }

    // --- Workspace manifest: profiles + member-derived floor ---------------
    let manifest_path = repo_root.join("Cargo.toml");
    let manifest_src = std::fs::read_to_string(&manifest_path).ok();
    match manifest_src.as_deref() {
        None => hits.push(Hit {
            rule_id: WORKSPACE_MANIFEST_UNREADABLE,
            detail: "workspace Cargo.toml is absent or unreadable — the release-profile premise \
                     could not be evaluated. DISCOVERY/PRECONDITION failure, not a diff defect."
                .to_string(),
            file: "Cargo.toml".to_string(),
            line: 0,
        }),
        Some(src) => {
            // Parse must be TOTAL: unparseable FAILs, never skips. That
            // totality is the entire justification for taking a real parser.
            // ONE parse serves both the profile rules and the roster floor.
            match toml::from_str::<WorkspaceManifest>(src) {
                Err(e) => hits.push(Hit {
                    rule_id: WORKSPACE_MANIFEST_UNREADABLE,
                    detail: format!(
                        "workspace Cargo.toml failed to parse as TOML ({e}) — the \
                         release-profile premise could not be evaluated. \
                         DISCOVERY/PRECONDITION failure, not a diff defect."
                    ),
                    file: "Cargo.toml".to_string(),
                    line: 0,
                }),
                Ok(parsed) => {
                    // NOTE: a MISSING `[profile.release]` table is deliberately
                    // NOT a finding — see the module doc. Cargo's built-in
                    // release defaults already set `debug-assertions = false`,
                    // so the premise holds; failing there would be a false
                    // positive, and a guard that false-fails gets bypassed.
                    check_profiles(
                        "Cargo.toml",
                        &parsed.profile,
                        &all_named_profiles,
                        RELEASE_PROFILE_DEBUG_ASSERTIONS,
                        &mut hits,
                    );
                    check_service_roster(
                        parsed.workspace.as_ref(),
                        &scanned_service_dirs,
                        &mut hits,
                    );
                }
            }
        }
    }

    // --- Cargo config files: rustflags AND profile tables ------------------
    // One parser, two paths (@security J5.3) — structural, not disciplinary.
    //
    // Discovery is a repo-wide walk for `**/.cargo/config{,.toml}`, not a
    // root-only lookup (@security row 9). Cargo merges config from every
    // ancestor of the invocation CWD, so `crates/mh-service/.cargo/config.toml`
    // applies to anyone running `cargo build` from inside that crate. The
    // Dockerfiles build from the workspace root, so root is the load-bearing
    // one for the shipped artifact — but globbing costs nothing over the
    // root-only form and removes the "which CWD was this built from?"
    // reasoning step entirely.
    for cfg_rel in discover_cargo_configs(repo_root)? {
        let cfg_path = repo_root.join(&cfg_rel);
        let src = std::fs::read_to_string(&cfg_path)
            .with_context(|| format!("reading {}", cfg_path.display()))?;
        match toml::from_str::<CargoConfig>(&src) {
            Err(e) => hits.push(Hit {
                rule_id: CARGO_CONFIG_UNPARSEABLE,
                detail: format!(
                    "{cfg_rel} failed to parse as TOML ({e}) — its profile table and rustflags \
                     could not be evaluated. Unlike the workspace-manifest classes this IS \
                     causable by an ordinary edit: fix {cfg_rel}."
                ),
                file: cfg_rel.clone(),
                line: 0,
            }),
            Ok(cfg) => {
                let mut flag_sets: Vec<Vec<String>> = Vec::new();
                if let Some(b) = &cfg.build {
                    if let Some(rf) = &b.rustflags {
                        flag_sets.push(rf.as_args());
                    }
                }
                if let Some(targets) = &cfg.target {
                    for section in targets.values() {
                        if let Some(rf) = &section.rustflags {
                            flag_sets.push(rf.as_args());
                        }
                    }
                }
                for args in &flag_sets {
                    if rustflags_enables_debug_assertions(args) {
                        hits.push(Hit {
                            rule_id: RUSTFLAGS_DEBUG_ASSERTIONS,
                            detail: format!("{cfg_rel} rustflags enable debug-assertions"),
                            file: cfg_rel.clone(),
                            line: 0,
                        });
                    }
                }
                // Row 7: a [profile.release] table DIRECTLY in a cargo config.
                // Cargo honours config profiles; this spelling defeats the
                // premise with Cargo.toml and every Dockerfile untouched.
                check_profiles(
                    &cfg_rel,
                    &cfg.profile,
                    &all_named_profiles,
                    CONFIG_TOML_RELEASE_PROFILE,
                    &mut hits,
                );
            }
        }
    }

    // --- Execution paths: CI workflows -------------------------------------
    // (Dockerfiles are covered in the discovery loop above, from the same read.)
    let workflows = repo_root.join(WORKFLOWS_DIR);
    if workflows.is_dir() {
        let mut wf: Vec<PathBuf> = std::fs::read_dir(&workflows)
            .with_context(|| format!("reading {}", workflows.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|e| e == "yml" || e == "yaml")
                    .unwrap_or(false)
            })
            .collect();
        wf.sort();
        for path in wf {
            let rel = path
                .strip_prefix(repo_root)
                .unwrap_or(&path)
                .display()
                .to_string();
            // Silent-skip fix (@dry-reviewer F-DRY-F): a bare `if let Ok`
            // dropped unreadable workflow files with no signal at all.
            // `warn_skip` is the shared home for this pattern
            // (`common/scan.rs`, 8 consumer modules).
            //
            // WARNS rather than failing, unlike the sibling site in
            // `no_insecure_browser_flags` — and the asymmetry is principled,
            // not arbitrary. This module's PRIMARY premise surface (service
            // Dockerfiles, the workspace manifest, cargo configs) is already
            // strict: those reads propagate with `?` or record an explicit
            // precondition hit, so an unreadable file there aborts or fails.
            // The CI-workflow sweep is the secondary belt-and-suspenders half
            // of the value-keyed `RUSTFLAGS` rule (see the scope note in the
            // module doc), so losing one workflow file degrades a backstop
            // rather than the premise itself. `no_insecure_browser_flags` has
            // no such primary/secondary split — every file in its set carries
            // equal weight in a coverage claim — which is why it fails closed.
            match std::fs::read_to_string(&path) {
                Ok(content) => scan_execution_path_file(&rel, &content, &mut hits),
                Err(e) => warn_skip("release-build-profile workflow read", &path, &e),
            }
        }
    }

    // --- Report -------------------------------------------------------------
    //
    // OUTPUT-CHANNEL NOTE (verified 2026-09-01 against run-guards.sh, and the
    // reason these lines are shaped the way they are):
    //
    // Layer 3 runs `run-guards.sh` NON-VERBOSE (`scripts/layer3.sh` passes no
    // flag; `run-guards.sh:45` defaults VERBOSE=false). The non-verbose branch
    // captures this binary's stdout AND stderr — `OUTPUT=$(… 2>&1)` at
    // `run-guards.sh:217` — and `classify_guard_exit`'s failure arm re-emits
    // ONLY lines matching `VIOLATION|violation|ERROR|error|WARN`, capped at
    // `head -5`.
    //
    // Consequences this code is built around:
    //   * The `STATUS=` line this binary prints is CONSUMED by exit-code
    //     classification and never reaches an operator's log. So every REASON
    //     token is ALSO carried verbatim inside its `VIOLATION:`/`ERROR:` line.
    //     Without that the vocabulary is decorative.
    //   * Emitting `STATUS=PRECONDITION_FAILURE` from here does NOT reach the
    //     operator lane — it is captured like everything else and matches none
    //     of the grep tokens. The lane rides in the reason token instead.
    //   * The `head -5` cap means ordering is load-bearing: DISCOVERY /
    //     PRECONDITION lines are printed FIRST, because a coverage refusal
    //     buried at line six is exactly the failure this guard exists to
    //     prevent.
    //   * The SCOPE counts line below sits on both paths but is only visible
    //     standalone or under `--verbose` — the OK path never echoes captured
    //     output at all. It is for the standalone triage reader, not the
    //     devloop log.
    // DERIVED, never hand-counted. "How many inputs does this guard check" was
    // previously a literal `9` while the module-doc table listed 7 and
    // `RULE_ORDER` implied 7 — three encodings, two of them wrong, in the
    // operator-facing line whose entire purpose is making the scanned scope
    // legible. CLAUDE.md's drift rule landing inside a guard about premises
    // drifting silently. There is now one encoding: `RULE_ORDER` minus the
    // precondition classes. The module doc deliberately states NO number.
    println!(
        "SCOPE: {cargo_dockerfiles} cargo Dockerfiles, {} enumerated premise channels, {} hits",
        enumerated_channel_count(),
        hits.len()
    );

    if hits.is_empty() {
        // Token names the ENUMERATION, not the conclusion: a reader must not
        // be able to derive "the release premise holds" from this green.
        emit_ok("release-build-profile-enumerated-inputs-clean");
        return Ok(());
    }

    // Sort by the same precedence the REASON token uses, so the discovery /
    // precondition classes survive `head -5`.
    let mut ordered: Vec<&Hit> = hits.iter().collect();
    ordered.sort_by_key(|h| rule_precedence(h.rule_id));

    for hit in ordered {
        let token = token_for_rule(hit.rule_id);
        if explain {
            let policy = format!("release-build-profile::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.file,
                row: hit.line,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[("reason", token)],
                src_file: file!(),
                src_line: line!(),
            });
        } else if is_precondition_rule(hit.rule_id) {
            // `ERROR:` prefix so the line survives run-guards.sh's grep, and
            // says plainly which lane it belongs to.
            println!(
                "ERROR: PRECONDITION [{token}] {} — {} (discovery/precondition failure, \
                 NOT a diff defect; see docs/runbooks/devloop-validation.md §6.3.1)",
                hit.file, hit.detail
            );
        } else {
            println!(
                "VIOLATION: [{token}] {}:{} {}",
                hit.file, hit.line, hit.detail
            );
        }
    }

    anyhow::bail!("{}", reason_for(&hits))
}

/// Rule-id → REASON token, and the precedence that orders both the token
/// choice and the printed lines.
///
/// DISCOVERY/PRECONDITION classes come first deliberately.
/// `run-guards.sh:classify_guard_exit` routes only exit 124/137 to the
/// operator lane, so a discovery failure exiting 1 would otherwise be triaged
/// as a diff defect and land on an implementer with nothing to fix. The token
/// is the only channel that can carry the lane — and the `head -5` cap on
/// re-emitted lines means it must also be printed first.
const RULE_ORDER: &[(&str, &str)] = &[
    (
        NO_DOCKERFILES_DISCOVERED,
        "release-build-profile-no-dockerfiles-discovered",
    ),
    (
        WORKSPACE_MANIFEST_UNREADABLE,
        "release-build-profile-workspace-manifest-unreadable",
    ),
    (
        CARGO_CONFIG_UNPARSEABLE,
        "release-build-profile-cargo-config-unparseable",
    ),
    (
        DOCKERFILE_NO_CARGO_BUILD_LINE,
        "release-build-profile-dockerfile-no-cargo-build-line",
    ),
    (
        SERVICE_ROSTER_UNDERIVABLE,
        "release-build-profile-service-roster-underivable",
    ),
    (
        SERVICE_CRATE_WITHOUT_DOCKERFILE,
        "release-build-profile-service-crate-without-dockerfile",
    ),
    (
        CANONICAL_SERVICES_ROSTER_DRIFT,
        "release-build-profile-canonical-services-roster-drift",
    ),
    (DOCKERFILE_BUILD_NOT_RELEASE, "dockerfile-build-not-release"),
    (
        RELEASE_PROFILE_DEBUG_ASSERTIONS,
        "release-profile-debug-assertions-enabled",
    ),
    (
        CONFIG_TOML_RELEASE_PROFILE,
        "cargo-config-release-profile-debug-assertions",
    ),
    (
        RELEASE_INHERITING_PROFILE,
        "release-inheriting-profile-debug-assertions",
    ),
    (RUSTFLAGS_DEBUG_ASSERTIONS, "rustflags-debug-assertions"),
    (
        CARGO_PROFILE_ENV_DEBUG_ASSERTIONS,
        "cargo-profile-env-debug-assertions",
    ),
    (
        DOCKERFILE_COOK_PROFILE_MISMATCH,
        "dockerfile-cook-profile-mismatch",
    ),
];

/// The DISCOVERY/PRECONDITION rule ids — "the guard could not check what
/// it is responsible for", which is an operator problem, not an implementer's.
const PRECONDITION_RULES: &[&str] = &[
    NO_DOCKERFILES_DISCOVERED,
    WORKSPACE_MANIFEST_UNREADABLE,
    CARGO_CONFIG_UNPARSEABLE,
    DOCKERFILE_NO_CARGO_BUILD_LINE,
    SERVICE_ROSTER_UNDERIVABLE,
    SERVICE_CRATE_WITHOUT_DOCKERFILE,
    CANONICAL_SERVICES_ROSTER_DRIFT,
];

fn is_precondition_rule(rule_id: &str) -> bool {
    PRECONDITION_RULES.contains(&rule_id)
}

/// Number of premise channels this guard enumerates: every `RULE_ORDER` entry
/// that is not a discovery/precondition class. Derived so that adding a channel
/// cannot leave a count behind.
fn enumerated_channel_count() -> usize {
    RULE_ORDER
        .iter()
        .filter(|(id, _)| !is_precondition_rule(id))
        .count()
}

fn rule_precedence(rule_id: &str) -> usize {
    RULE_ORDER
        .iter()
        .position(|(id, _)| *id == rule_id)
        .unwrap_or(usize::MAX)
}

fn token_for_rule(rule_id: &str) -> &'static str {
    RULE_ORDER
        .iter()
        .find(|(id, _)| *id == rule_id)
        .map(|(_, token)| *token)
        .unwrap_or("release-build-profile-violation")
}

/// Map the hit set to a single REASON token, highest-precedence class first.
fn reason_for(hits: &[Hit]) -> String {
    for (rule_id, token) in RULE_ORDER {
        let n = hits.iter().filter(|h| h.rule_id == *rule_id).count();
        if n > 0 {
            return format!("{token}-{n}");
        }
    }
    format!("release-build-profile-violations-{}", hits.len())
}

/// Anti-vacuity FLOOR (derived) + roster drift check.
///
/// The floor is derived from `[workspace] members` matching `crates/*-service`
/// — a true SSoT, because cargo itself fails to build if that list is wrong,
/// so it cannot rot unnoticed. A hand-maintained roster would go vacuous for
/// exactly the newly-added service: the one case the floor exists to catch.
///
/// The second half guards `common::services::CANONICAL_SERVICES` against that
/// derivation. That array is documented as `(metric_prefix, directory_name)`
/// for services that EMIT METRICS, and this gate reads it as services that
/// SHIP A CONTAINER. Those sets coincide today, and that coincidence is now
/// load-bearing — so it is machine-checked here rather than left to a doc
/// comment a future editor must notice.
fn check_service_roster(
    workspace: Option<&WorkspaceTable>,
    scanned_service_dirs: &BTreeSet<String>,
    hits: &mut Vec<Hit>,
) {
    // No `[workspace]` table at all: nothing to derive from, and nothing to
    // assert. Not a finding — a single-crate manifest is a legitimate shape.
    let Some(workspace) = workspace else {
        return;
    };
    if workspace.members.is_empty() {
        return;
    }

    let derived: BTreeSet<String> = workspace
        .members
        .iter()
        .filter_map(|m| m.strip_prefix(CRATES_PREFIX))
        .filter(|m| m.ends_with(SERVICE_CRATE_SUFFIX) && !m.contains('/'))
        .map(str::to_string)
        .collect();

    // ANTI-VACUITY, and the one that nearly got away (@security F1,
    // @dry-reviewer F-DRY-B, @code-reviewer F1 — three reviewers, same line).
    //
    // Cargo fully supports GLOB members: `members = ["crates/*"]` is legal and
    // is exactly the tidy-up someone does when the list gets long. `"crates/*"`
    // strips to `"*"`, which does not end in `-service`, so the derived roster
    // comes back EMPTY — and an empty roster makes BOTH halves of this floor
    // check nothing. Reproduced on identical trees differing only in how
    // members are spelled: enumerated members FAILed with two precondition
    // hits, glob members reported STATUS=OK.
    //
    // Fire on the PRESENCE of a glob, not on an empty derivation. A glob makes
    // the roster untrustworthy even when it is non-empty —
    // `["crates/mh-service", "crates/*"]` derives one service while the glob
    // could be hiding another — so emptiness is the wrong discriminator.
    if workspace.members.iter().any(|m| m.contains('*')) {
        hits.push(Hit {
            rule_id: SERVICE_ROSTER_UNDERIVABLE,
            detail: format!(
                "[workspace] members contains glob entries, which cannot be resolved to a \
                 `{CRATES_PREFIX}*{SERVICE_CRATE_SUFFIX}` roster without expanding them — so the \
                 service-coverage floor checked NOTHING and a service crate could sit outside \
                 the premise gate unnoticed. Enumerate the members, or teach this derivation to \
                 expand globs."
            ),
            file: "Cargo.toml".to_string(),
            line: 0,
        });
        return;
    }

    // Enumerated members with no `*-service` entry is NOT a derivation failure:
    // the roster is accurately empty, so there is genuinely nothing that could
    // sit outside the gate. Distinct from the glob case above, where the answer
    // is unknown rather than zero — and the reason emptiness alone must not be
    // the trigger.
    if derived.is_empty() {
        return;
    }

    for svc in &derived {
        if !scanned_service_dirs.contains(svc) {
            hits.push(Hit {
                rule_id: SERVICE_CRATE_WITHOUT_DOCKERFILE,
                detail: format!(
                    "workspace member crates/{svc} ships as a service but no \
                     {DOCKER_DIR}/{svc}/Dockerfile with a cargo build was scanned — \
                     it is outside the release-profile premise gate"
                ),
                file: format!("{DOCKER_DIR}/{svc}/Dockerfile"),
                line: 0,
            });
        }
    }

    let canonical: BTreeSet<String> = CANONICAL_SERVICES
        .iter()
        .map(|(_, dir)| (*dir).to_string())
        .collect();
    if canonical != derived {
        hits.push(Hit {
            rule_id: CANONICAL_SERVICES_ROSTER_DRIFT,
            detail: format!(
                "common::services::CANONICAL_SERVICES {canonical:?} disagrees with the \
                 workspace-derived service roster {derived:?}. CANONICAL_SERVICES is \
                 documented as the metrics-emitting service list and is ALSO read here as \
                 the container-shipping list; this rule keeps that coincidence true."
            ),
            file: "crates/dt-guard/src/common/services.rs".to_string(),
            line: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- logical line joining ---------------------------------------------

    #[test]
    fn logical_lines_joins_backslash_continuations() {
        let src = "RUN cargo build \\\n    --release \\\n    --package mh-service\nFROM x\n";
        let lines = logical_lines(src);
        assert_eq!(lines[0].0, 1, "line number is the FIRST physical line");
        assert!(lines[0].1.contains("--release"));
        assert_eq!(lines[1].1, "FROM x");
    }

    // --- profile selection -------------------------------------------------

    #[test]
    fn selected_profile_accepts_release_flag() {
        assert_eq!(
            selected_profile("cargo build --release --package mh-service"),
            SelectedProfile::Release
        );
    }

    #[test]
    fn selected_profile_accepts_long_form_release() {
        assert_eq!(
            selected_profile("cargo build --profile release -p mh-service"),
            SelectedProfile::Release
        );
        assert_eq!(
            selected_profile("cargo build --profile=release"),
            SelectedProfile::Release
        );
    }

    #[test]
    fn selected_profile_flags_other_profiles() {
        assert_eq!(
            selected_profile("cargo build --profile quick"),
            SelectedProfile::Named("quick".to_string())
        );
    }

    #[test]
    fn selected_profile_none_when_absent() {
        assert_eq!(
            selected_profile("cargo build --package mh-service"),
            SelectedProfile::None
        );
    }

    // --- rustflags spellings ----------------------------------------------

    #[test]
    fn rustflags_split_array_form_is_caught() {
        // The cargo book's own spelling; invisible to an adjacency matcher.
        let args = vec!["-C".to_string(), "debug-assertions=on".to_string()];
        assert!(rustflags_enables_debug_assertions(&args));
    }

    #[test]
    fn rustflags_no_space_form_is_caught() {
        assert!(rustflags_enables_debug_assertions(&[
            "-Cdebug-assertions=yes".to_string()
        ]));
    }

    #[test]
    fn rustflags_long_codegen_forms_are_caught() {
        assert!(rustflags_enables_debug_assertions(&[
            "--codegen debug-assertions=true".to_string()
        ]));
        assert!(rustflags_enables_debug_assertions(&[
            "--codegen=debug-assertions=on".to_string()
        ]));
    }

    #[test]
    fn rustflags_valueless_form_is_enabled() {
        // rustc treats a valueless boolean codegen option as ENABLED — the
        // shortest bypass, and a `=<truthy>` matcher misses it entirely.
        assert!(rustflags_enables_debug_assertions(&[
            "-C debug-assertions".to_string()
        ]));
        assert!(rustflags_enables_debug_assertions(&[
            "-C".to_string(),
            "debug-assertions".to_string()
        ]));
    }

    #[test]
    fn rustflags_full_truthy_set() {
        for v in ["y", "yes", "on", "true", "1"] {
            assert!(
                rustflags_enables_debug_assertions(&[format!("-C debug-assertions={v}")]),
                "{v} should be truthy"
            );
        }
    }

    #[test]
    fn rustflags_falsey_set_does_not_fire() {
        for v in ["n", "no", "off", "false"] {
            assert!(
                !rustflags_enables_debug_assertions(&[format!("-C debug-assertions={v}")]),
                "{v} should be falsey"
            );
        }
    }

    #[test]
    fn rustflags_benign_values_do_not_fire() {
        // Value-keyed, not presence-keyed: ci.yml's real `--cfg coverage`
        // and the mold-linker recipe must not fire.
        assert!(!rustflags_enables_debug_assertions(&[
            "--cfg coverage".to_string()
        ]));
        assert!(!rustflags_enables_debug_assertions(&[
            "-C link-arg=-fuse-ld=mold".to_string()
        ]));
        assert!(!rustflags_enables_debug_assertions(&[
            "-Zsanitizer=address".to_string()
        ]));
        assert!(!rustflags_enables_debug_assertions(&[
            "-C opt-level=3".to_string()
        ]));
    }

    #[test]
    fn encoded_rustflags_splits_on_unit_separator() {
        // CARGO_ENCODED_RUSTFLAGS is \x1f-delimited, not whitespace.
        let raw = "-C\u{1f}debug-assertions=on";
        assert!(rustflags_enables_debug_assertions(
            &split_encoded_rustflags(raw)
        ));
    }

    // --- CARGO_PROFILE_* env ----------------------------------------------

    #[test]
    fn cargo_profile_env_detected() {
        assert!(cargo_profile_env_enables(
            "CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS",
            "true"
        ));
        assert!(cargo_profile_env_enables(
            "CARGO_PROFILE_DIST_DEBUG_ASSERTIONS",
            "yes"
        ));
        assert!(!cargo_profile_env_enables(
            "CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS",
            "false"
        ));
        assert!(!cargo_profile_env_enables(
            "CARGO_PROFILE_RELEASE_LTO",
            "true"
        ));
    }

    // --- Dockerfile scanning ----------------------------------------------

    #[test]
    fn dockerfile_scan_finds_build_and_cook() {
        let src = "RUN cargo chef cook --release --recipe-path recipe.json\n\
                   RUN cargo build --release --package mh-service\n";
        let scan = scan_dockerfile(src);
        assert_eq!(scan.build.len(), 1);
        assert_eq!(scan.cook.len(), 1);
    }

    // --- TOML profile parsing ---------------------------------------------

    fn profiles_of(src: &str) -> std::collections::BTreeMap<String, ProfileTable> {
        toml::from_str::<WorkspaceManifest>(src).unwrap().profile
    }

    #[test]
    fn profile_release_debug_assertions_detected() {
        let mut hits = Vec::new();
        let p = profiles_of("[profile.release]\ndebug-assertions = true\n");
        check_profiles(
            "Cargo.toml",
            &p,
            &BTreeSet::new(),
            RELEASE_PROFILE_DEBUG_ASSERTIONS,
            &mut hits,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, RELEASE_PROFILE_DEBUG_ASSERTIONS);
    }

    #[test]
    fn profile_release_package_override_detected() {
        // The row the task's own framing misses: re-arms the compile-time
        // control in one crate while every named file stays byte-identical.
        let mut hits = Vec::new();
        let p = profiles_of(
            "[profile.release]\nopt-level = 3\n\
             [profile.release.package.mh-service]\ndebug-assertions = true\n",
        );
        check_profiles(
            "Cargo.toml",
            &p,
            &BTreeSet::new(),
            RELEASE_PROFILE_DEBUG_ASSERTIONS,
            &mut hits,
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].detail.contains("mh-service"));
    }

    #[test]
    fn profile_dotted_key_form_detected() {
        // Legal TOML that a line-oriented scanner misses — the miss window
        // that justifies taking the real parser.
        let mut hits = Vec::new();
        let p = profiles_of("[profile]\nrelease.debug-assertions = true\n");
        check_profiles(
            "Cargo.toml",
            &p,
            &BTreeSet::new(),
            RELEASE_PROFILE_DEBUG_ASSERTIONS,
            &mut hits,
        );
        assert_eq!(hits.len(), 1, "dotted-key spelling must be caught");
    }

    #[test]
    fn profile_release_clean_passes() {
        // Today's actual [profile.release] shape.
        let mut hits = Vec::new();
        let p = profiles_of(
            "[profile.release]\nopt-level = 3\nlto = true\ncodegen-units = 1\nstrip = true\n",
        );
        check_profiles(
            "Cargo.toml",
            &p,
            &BTreeSet::new(),
            RELEASE_PROFILE_DEBUG_ASSERTIONS,
            &mut hits,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn inheriting_profile_fires_only_when_a_dockerfile_selects_it() {
        let src = "[profile.dist]\ninherits = \"release\"\ndebug-assertions = true\n";
        let p = profiles_of(src);

        let mut unselected = Vec::new();
        check_profiles(
            "Cargo.toml",
            &p,
            &BTreeSet::new(),
            RELEASE_PROFILE_DEBUG_ASSERTIONS,
            &mut unselected,
        );
        assert!(
            unselected.is_empty(),
            "a local-only profile nobody ships must not fire"
        );

        let mut selected = Vec::new();
        let mut names = BTreeSet::new();
        names.insert("dist".to_string());
        check_profiles(
            "Cargo.toml",
            &p,
            &names,
            RELEASE_PROFILE_DEBUG_ASSERTIONS,
            &mut selected,
        );
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].rule_id, RELEASE_INHERITING_PROFILE);
    }

    #[test]
    fn config_toml_rustflags_array_and_profile_table_both_parse() {
        let cfg: CargoConfig = toml::from_str(
            "[build]\nrustflags = [\"-C\", \"debug-assertions=on\"]\n\
             [profile.release]\ndebug-assertions = true\n",
        )
        .unwrap();
        let args = cfg
            .build
            .as_ref()
            .unwrap()
            .rustflags
            .as_ref()
            .unwrap()
            .as_args();
        assert!(rustflags_enables_debug_assertions(&args));
        assert_eq!(cfg.profile["release"].debug_assertions, Some(true));
    }

    // --- env assignment parsing -------------------------------------------

    #[test]
    fn env_assignment_handles_dockerfile_and_yaml_shapes() {
        assert_eq!(
            env_assignment("ENV RUSTFLAGS=\"-C debug-assertions=on\""),
            Some(("RUSTFLAGS".into(), "\"-C debug-assertions=on\"".into()))
        );
        assert_eq!(
            env_assignment("          RUSTFLAGS: --cfg coverage"),
            Some(("RUSTFLAGS".into(), "--cfg coverage".into()))
        );
        assert_eq!(
            env_assignment("ARG CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true"),
            Some((
                "CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS".into(),
                "true".into()
            ))
        );
    }

    #[test]
    fn scan_execution_path_flags_env_channels() {
        let mut hits = Vec::new();
        scan_execution_path_file(
            "infra/docker/mh-service/Dockerfile",
            "FROM rust\nENV RUSTFLAGS=\"-C debug-assertions=on\"\n\
             ARG CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true\n",
            &mut hits,
        );
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.rule_id == RUSTFLAGS_DEBUG_ASSERTIONS));
        assert!(hits
            .iter()
            .any(|h| h.rule_id == CARGO_PROFILE_ENV_DEBUG_ASSERTIONS));
    }

    #[test]
    fn scan_execution_path_ignores_benign_ci_rustflags() {
        // ci.yml:170 sets `RUSTFLAGS: --cfg coverage` legitimately.
        let mut hits = Vec::new();
        scan_execution_path_file(
            ".github/workflows/ci.yml",
            "        env:\n          RUSTFLAGS: --cfg coverage\n",
            &mut hits,
        );
        assert!(
            hits.is_empty(),
            "value-keyed matcher must not fire on --cfg coverage"
        );
    }

    // --- REASON token vocabulary ------------------------------------------

    #[test]
    fn reason_tokens_discriminate_failure_classes() {
        let mk = |rule_id: &'static str| Hit {
            rule_id,
            detail: String::new(),
            file: String::new(),
            line: 0,
        };
        // Each class produces a token a reader can triage from alone.
        assert!(reason_for(&[mk(DOCKERFILE_BUILD_NOT_RELEASE)])
            .starts_with("dockerfile-build-not-release"));
        assert!(reason_for(&[mk(RELEASE_PROFILE_DEBUG_ASSERTIONS)])
            .starts_with("release-profile-debug-assertions-enabled"));
        assert!(reason_for(&[mk(DOCKERFILE_COOK_PROFILE_MISMATCH)])
            .starts_with("dockerfile-cook-profile-mismatch"));
        assert!(reason_for(&[mk(CONFIG_TOML_RELEASE_PROFILE)])
            .starts_with("cargo-config-release-profile-debug-assertions"));
        assert!(reason_for(&[mk(CARGO_PROFILE_ENV_DEBUG_ASSERTIONS)])
            .starts_with("cargo-profile-env-debug-assertions"));
        assert!(
            reason_for(&[mk(RUSTFLAGS_DEBUG_ASSERTIONS)]).starts_with("rustflags-debug-assertions")
        );
    }

    #[test]
    fn vacuity_tokens_are_distinct_and_precedence_first() {
        let mk = |rule_id: &'static str| Hit {
            rule_id,
            detail: String::new(),
            file: String::new(),
            line: 0,
        };
        assert_eq!(
            reason_for(&[mk(NO_DOCKERFILES_DISCOVERED)]),
            "release-build-profile-no-dockerfiles-discovered-1"
        );
        assert_eq!(
            reason_for(&[mk(WORKSPACE_MANIFEST_UNREADABLE)]),
            "release-build-profile-workspace-manifest-unreadable-1"
        );
        assert_eq!(
            reason_for(&[mk(DOCKERFILE_NO_CARGO_BUILD_LINE)]),
            "release-build-profile-dockerfile-no-cargo-build-line-1"
        );
        // Discovery/precondition classes take precedence over violations so
        // the token carries the lane (classify_guard_exit cannot).
        assert!(reason_for(&[
            mk(DOCKERFILE_BUILD_NOT_RELEASE),
            mk(NO_DOCKERFILES_DISCOVERED)
        ])
        .starts_with("release-build-profile-no-dockerfiles-discovered"));
    }

    // --- roster floor ------------------------------------------------------

    fn workspace_of(src: &str) -> WorkspaceManifest {
        toml::from_str::<WorkspaceManifest>(src).unwrap()
    }

    #[test]
    fn service_crate_without_dockerfile_fails() {
        let manifest = "[workspace]\nmembers = [\"crates/ac-service\", \"crates/gc-service\", \
                        \"crates/mc-service\", \"crates/mh-service\", \"crates/common\"]\n";
        let mut scanned = BTreeSet::new();
        scanned.insert("ac-service".to_string());
        scanned.insert("gc-service".to_string());
        scanned.insert("mc-service".to_string());
        // mh-service deliberately missing.
        let mut hits = Vec::new();
        check_service_roster(
            workspace_of(manifest).workspace.as_ref(),
            &scanned,
            &mut hits,
        );
        assert!(hits
            .iter()
            .any(|h| h.rule_id == SERVICE_CRATE_WITHOUT_DOCKERFILE
                && h.detail.contains("mh-service")));
    }

    #[test]
    fn canonical_services_roster_drift_fires_on_disagreement() {
        // A workspace with a 5th service that CANONICAL_SERVICES lacks: the
        // floor would silently miss it, so the drift rule fires instead.
        let manifest = "[workspace]\nmembers = [\"crates/ac-service\", \"crates/gc-service\", \
                        \"crates/mc-service\", \"crates/mh-service\", \"crates/xx-service\"]\n";
        let scanned: BTreeSet<String> = [
            "ac-service",
            "gc-service",
            "mc-service",
            "mh-service",
            "xx-service",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut hits = Vec::new();
        check_service_roster(
            workspace_of(manifest).workspace.as_ref(),
            &scanned,
            &mut hits,
        );
        assert!(hits
            .iter()
            .any(|h| h.rule_id == CANONICAL_SERVICES_ROSTER_DRIFT));
    }

    #[test]
    fn glob_members_fail_rather_than_silently_voiding_the_floor() {
        // @security F1 / @dry-reviewer F-DRY-B / @code-reviewer F1 — three
        // reviewers, one line. `members = ["crates/*"]` is legal cargo and is
        // the tidy-up someone does when the list gets long; it strips to "*",
        // which does not end in `-service`, so the derived roster is EMPTY and
        // both halves of the floor check nothing. It used to report OK.
        for manifest in [
            // Pure glob — derives nothing.
            "[workspace]\nmembers = [\"crates/*\"]\n",
            // Mixed: derives ONE service while the glob could hide another, so
            // a non-empty roster is still untrustworthy. This is why glob
            // PRESENCE is the trigger and emptiness is not.
            "[workspace]\nmembers = [\"crates/mh-service\", \"crates/*\"]\n",
        ] {
            let mut hits = Vec::new();
            check_service_roster(
                workspace_of(manifest).workspace.as_ref(),
                &BTreeSet::new(),
                &mut hits,
            );
            assert_eq!(
                hits.len(),
                1,
                "an untrustworthy roster must FAIL, not go silent"
            );
            assert_eq!(hits[0].rule_id, SERVICE_ROSTER_UNDERIVABLE);
            assert!(hits[0].detail.contains("glob"), "got: {}", hits[0].detail);
        }
    }

    #[test]
    fn enumerated_members_with_no_service_crate_is_not_a_finding() {
        // Accurately empty, not underivable — there is genuinely nothing that
        // could sit outside the gate. The distinction the glob rule turns on.
        let mut hits = Vec::new();
        check_service_roster(
            workspace_of("[workspace]\nmembers = [\"crates/common\"]\n")
                .workspace
                .as_ref(),
            &BTreeSet::new(),
            &mut hits,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn absent_workspace_table_is_not_a_finding() {
        // A single-crate manifest is a legitimate shape — there is no roster to
        // derive and nothing to assert. Distinct from a members list that
        // exists but yields nothing, which is the case above.
        let mut hits = Vec::new();
        check_service_roster(
            workspace_of("[profile.release]\nopt-level = 3\n")
                .workspace
                .as_ref(),
            &BTreeSet::new(),
            &mut hits,
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn enumerated_channel_count_is_derived_from_rule_order() {
        // Pins the count against its single encoding, so adding a channel
        // cannot leave the operator-facing SCOPE line behind.
        let expected = RULE_ORDER.len() - PRECONDITION_RULES.len();
        assert_eq!(enumerated_channel_count(), expected);
        assert!(
            enumerated_channel_count() >= 7,
            "the seven documented premise channels must all be registered"
        );
    }

    #[test]
    fn canonical_services_roster_matches_workspace_today() {
        // The real tree: CANONICAL_SERVICES and the workspace-derived roster
        // must agree, or the floor is reading a stale list.
        let repo_root = repo_root_for_tests();
        let manifest = std::fs::read_to_string(repo_root.join("Cargo.toml")).unwrap();
        let scanned: BTreeSet<String> = CANONICAL_SERVICES
            .iter()
            .map(|(_, d)| (*d).to_string())
            .collect();
        let mut hits = Vec::new();
        check_service_roster(
            workspace_of(&manifest).workspace.as_ref(),
            &scanned,
            &mut hits,
        );
        assert!(
            !hits
                .iter()
                .any(|h| h.rule_id == CANONICAL_SERVICES_ROSTER_DRIFT),
            "CANONICAL_SERVICES has drifted from [workspace] members"
        );
    }

    // --- the premise assertion itself --------------------------------------

    /// Resolve the repository root for real-tree tests.
    ///
    /// FAILS LOUDLY if it cannot find one. A test that walks for `.git`,
    /// finds nothing, and silently passes is the vacuous pass one level up —
    /// the same defect the guard exists to prevent, in the test that guards
    /// the guard.
    fn repo_root_for_tests() -> PathBuf {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            if dir.join(".git").exists() {
                return dir;
            }
            assert!(
                dir.pop(),
                "could not locate repo root (no .git ancestor) — this test would \
                 otherwise pass vacuously by checking nothing"
            );
        }
    }

    #[test]
    fn current_tree_premise_holds() {
        // The assertion the task states, and the test that fires the day
        // someone drops --release. Asserts BOTH that discovery worked and
        // that the enumerated inputs are clean.
        let repo_root = repo_root_for_tests();

        let docker_dir = repo_root.join(DOCKER_DIR);
        assert!(
            docker_dir.is_dir(),
            "{DOCKER_DIR} must exist — otherwise this test checks nothing"
        );

        let mut cargo_dockerfiles = 0usize;
        for entry in std::fs::read_dir(&docker_dir).unwrap().flatten() {
            let dockerfile = entry.path().join("Dockerfile");
            if !dockerfile.is_file() {
                continue;
            }
            let content = std::fs::read_to_string(&dockerfile).unwrap();
            let scan = scan_dockerfile(&content);
            if scan.build.is_empty() {
                continue;
            }
            cargo_dockerfiles += 1;
            for inv in &scan.build {
                assert_eq!(
                    selected_profile(&inv.text),
                    SelectedProfile::Release,
                    "{} line {} does not select the release profile",
                    dockerfile.display(),
                    inv.line
                );
            }
        }
        assert!(
            cargo_dockerfiles >= CANONICAL_SERVICES.len(),
            "expected at least {} cargo-building Dockerfiles, scanned {cargo_dockerfiles} \
             — discovery is under-matching and the premise assertion is vacuous",
            CANONICAL_SERVICES.len()
        );

        let manifest = std::fs::read_to_string(repo_root.join("Cargo.toml")).unwrap();
        let parsed: WorkspaceManifest = toml::from_str(&manifest).unwrap();
        let release = parsed
            .profile
            .get("release")
            .expect("[profile.release] must exist");
        assert_ne!(
            release.debug_assertions,
            Some(true),
            "[profile.release] enables debug-assertions — ADR-0036 §11's compile-time \
             control is inert in shipped artifacts"
        );
    }
}
