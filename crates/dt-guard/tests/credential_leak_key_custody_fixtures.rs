// Crate-level test override, mirroring `ts_retained_credentials_fixtures.rs`:
// `allow-expect-in-tests` / `allow-unwrap-in-tests` only apply inside
// `#[cfg(test)]` modules and `#[test]` fns, so an integration test's non-`#[test]`
// helpers still trip `expect_used` / `panic`. Failing loudly on a missing or
// unreadable fixture is the correct behaviour: a harness that degraded to "no
// fixtures" would pass while proving nothing, which is the exact failure class
// this file exists to lock.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration-test helpers outside #[test] fns; a missing fixture or an unreadable checks.md MUST abort loudly rather than yield an empty set (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

//! Machine-checkable half of the **credential-leak key-custody** demonstration
//! (`scripts/guards/semantic/checks.md` items 11-13; ADR-0036 §11, story task 23).
//!
//! # Read this before reading a green from this file
//!
//! **This suite cannot assert that the semantic check fires.** Items 11-13 are
//! judged by the `semantic-guard` *agent*, by value rather than by name, so no
//! Rust test can execute them. The FIRE half is a Lead-directed
//! fixture-verification run recorded in the devloop output.
//!
//! A green here means: *the fixtures are intact, the check's scope is non-empty
//! against the real tree, the poles and the fixtures agree, and the mechanical
//! floor behaves as documented.* It does **not** mean items 11-13 were
//! demonstrated. Conflating those two is precisely the "reads as coverage"
//! failure ADR-0036 §11 names, relocated into the test for it.
//!
//! # The four things it does assert
//!
//! 1. [`catalog_and_directory_agree`] — bidirectional. A fixture cannot be added
//!    or dropped silently, and each carries its banner and `// Invariant:` block.
//! 2. [`poles_must_fire_spellings_each_have_a_fixture`] — the drift tripwire,
//!    parsing `checks.md` §Fixture poles as the single home for must-fire
//!    spellings.
//! 3. [`mechanical_floor_extent_is_pinned_per_spelling`] — runs the **shipped
//!    guard** over throwaway roots and pins which spellings it catches.
//! 4. [`check_scope_is_a_real_non_empty_tree`] — the *applies* half.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Fixture directory, relative to the dt-guard crate root.
const LENS_DIR: &str = "tests/fixtures/credential_leak_key_custody/lens";

/// The section the poles live under. Condition A2: if this heading is gone, the
/// parse must FAIL, not silently search the whole document.
const CHECKS_HEADING: &str = "## Check: Credential Leak";

/// Fence delimiting the *existing* "Must fire:" sentence. Applied by the check's
/// owners (`security` + `semantic-guard`) around prose that was already there —
/// deliberately NOT a parallel machine-readable list, because two enumerations
/// (one the agent reads, one the parser reads) is the drift this parse prevents.
const POLES_BEGIN: &str = "<!-- fixture-poles:must-fire:begin -->";
const POLES_END: &str = "<!-- fixture-poles:must-fire:end -->";

/// Floor for classified must-fire spellings.
///
/// The classifier has its own vacuity mode: a structural filter can silently
/// yield a thinner set — a future non-snake_case must-fire spelling would vanish
/// from the drift set with no signal, and a prose edit could drop the count to
/// zero while the fence still parses. Asserting a floor turns "could not
/// evaluate" into a loud failure instead of a pass.
const MIN_MUST_FIRE_SPELLINGS: usize = 2;

fn crate_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR`, never `current_dir()`: the latter depends on how the
    // test binary was invoked.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(Path::parent)
        .expect("dt-guard crate sits two levels under the repo root")
        .to_path_buf()
}

fn lens_dir() -> PathBuf {
    crate_root().join(LENS_DIR)
}

/// Every fixture in `lens/`, sorted, as `(file_name, contents)`.
fn lens_fixtures() -> Vec<(String, String)> {
    let dir = lens_dir();
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("lens fixture dir {} unreadable: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let body = std::fs::read_to_string(e.path())
                .unwrap_or_else(|err| panic!("fixture {name} unreadable: {err}"));
            (name, body)
        })
        .collect();
    out.sort();
    assert!(
        !out.is_empty(),
        "lens fixture set is EMPTY — every assertion below would pass vacuously"
    );
    out
}

// ---------------------------------------------------------------------------
// 1. Catalog integrity
// ---------------------------------------------------------------------------

/// The catalog. Transcribed from each fixture's `// Invariant:` block, which is
/// the specification — if this disagrees with a fixture, **the fixture wins and
/// the disagreement is a question for `security` + `semantic-guard`**. Editing a
/// row to match a fixture that was quietly relaxed would be the failure this
/// suite exists to detect, relocated into the suite.
const CATALOG: &[&str] = &[
    "neg_derive_debug_secretbox_wrapper.rs",
    "neg_join_response_kek_delivery.rs",
    "neg_kek_generation_metadata.rs",
    "neg_redacted_len_hand_rolled_debug.rs",
    "pos_contract_crossing_meeting_kek.rs",
    "pos_contract_crossing_renamed_material.rs",
    "pos_contract_crossing_transmit_key_bytes.rs",
    "pos_derive_debug_raw_kek_bytes.rs",
    "pos_mc_log_meeting_kek.rs",
    "pos_mc_log_transmit_key_bytes.rs",
    "pos_skip_debug_entry_removed.rs",
];

#[test]
fn catalog_and_directory_agree() {
    let on_disk: BTreeSet<String> = lens_fixtures().into_iter().map(|(n, _)| n).collect();
    let catalogued: BTreeSet<String> = CATALOG.iter().map(|s| (*s).to_string()).collect();

    // BOTH directions. A dropped `pos_` fixture is a silent coverage loss; an
    // uncatalogued fixture is one the directed run will not know to verify.
    let missing: Vec<_> = catalogued.difference(&on_disk).collect();
    let uncatalogued: Vec<_> = on_disk.difference(&catalogued).collect();
    assert!(
        missing.is_empty(),
        "catalogued fixtures absent from {LENS_DIR}: {missing:?}"
    );
    assert!(
        uncatalogued.is_empty(),
        "fixtures present but not catalogued (the directed run would skip them): {uncatalogued:?}"
    );

    // Every fixture must carry both halves of its contract.
    for (name, body) in lens_fixtures() {
        assert!(
            body.contains("// Invariant:"),
            "{name} has no `// Invariant:` block — that block IS the expected \
             verdict and the only home for it"
        );
        assert!(
            body.contains("Expected verdict:"),
            "{name}'s Invariant block must state `Expected verdict: FIRE` or \
             `CLEAR` so a future directed run can be checked against it"
        );
        assert!(
            body.contains("NOT PRODUCTION CODE, NOT COMPILED"),
            "{name} is missing the deliberate-leak banner"
        );
        let expects_fire = name.starts_with("pos_");
        let says_fire = body.contains("Expected verdict: FIRE");
        assert_eq!(
            expects_fire, says_fire,
            "{name}: filename prefix and Invariant block disagree about the \
             expected verdict"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Poles → fixtures drift tripwire
// ---------------------------------------------------------------------------

/// Distinct failure reasons. **Deliberately distinct from a content failure**:
/// if "could not evaluate" and "poles and fixtures disagree" look alike, the
/// vacuity case gets triaged as a drift bug and then "fixed" by relaxing the
/// parse, which restores a vacuous green with a passing test on top of it.
const REASON_PARSE: &str = "POLES-PARSE-COULD-NOT-EVALUATE";
const REASON_DRIFT: &str = "POLES-FIXTURE-DRIFT";

/// Extract must-fire *spellings* from the fenced sentence.
///
/// **Structural classifier, not literal-span matching.** The fenced sentence
/// legitimately contains spans that are not spellings at all — a dotted proto
/// package, a `#[derive(Debug)]` attribute, a file path, a `skip_debug(...)`
/// call site. A "every inline-code span must appear in some `pos_` fixture"
/// contract reds on at least two of those for reasons unrelated to drift (the
/// Rust fixtures use the `::` colon form, not the dotted one; the `build.rs`
/// path is only ever a scratch copy). So: keep only snake_case identifier
/// tokens, excluding any span containing `.`, `/`, `#` or `(`. That yields
/// exactly the must-fire spellings the task names and drops the four problem
/// spans **by construction**, with no hand-maintained exclusion list.
///
/// **Exact-token, never substring.** Substring containment would false-match
/// `MeetingKek` inside `MeetingKekUpdate` and `bytes` inside
/// `transmit_key_bytes` — both legitimate fenced content — i.e. it would flag
/// the sentence against its own contents.
fn must_fire_spellings(checks_md: &str) -> Vec<String> {
    let section_start = checks_md.find(CHECKS_HEADING).unwrap_or_else(|| {
        panic!(
            "{REASON_PARSE}: heading `{CHECKS_HEADING}` not found in \
             scripts/guards/semantic/checks.md. The check was renamed or removed. \
             This is COULD-NOT-EVALUATE, not drift — do not 'fix' it by widening \
             the search to the whole document."
        )
    });
    let section = &checks_md[section_start..];

    let begin = section.find(POLES_BEGIN).unwrap_or_else(|| {
        panic!(
            "{REASON_PARSE}: fence `{POLES_BEGIN}` not found inside \
             `{CHECKS_HEADING}`. COULD-NOT-EVALUATE — the marker was removed or \
             moved out of the section. Restore the fence; do not delete this test."
        )
    });
    let after = &section[begin + POLES_BEGIN.len()..];
    let end = after.find(POLES_END).unwrap_or_else(|| {
        panic!("{REASON_PARSE}: fence `{POLES_END}` not found after `{POLES_BEGIN}`.")
    });
    let fenced = &after[..end];

    // Inline-code spans, then the structural classifier.
    let mut spellings: Vec<String> = Vec::new();
    for span in fenced.split('`').skip(1).step_by(2) {
        let s = span.trim();
        if s.is_empty() {
            continue;
        }
        // Non-spelling shapes: paths, dotted packages, attributes, call sites.
        if s.contains('.') || s.contains('/') || s.contains('#') || s.contains('(') {
            continue;
        }
        // A spelling is a snake_case identifier: lowercase alphanumerics and
        // underscores, containing at least one underscore (a bare single word is
        // not one of the compound spellings this check is about).
        let ident: &str = s.split(':').next().unwrap_or(s).trim();
        if !ident.is_empty()
            && ident.contains('_')
            && ident
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !spellings.contains(&ident.to_string())
        {
            spellings.push(ident.to_string());
        }
    }
    spellings
}

#[test]
fn poles_must_fire_spellings_each_have_a_fixture() {
    let checks_path = repo_root().join("scripts/guards/semantic/checks.md");
    let checks_md = std::fs::read_to_string(&checks_path).unwrap_or_else(|e| {
        panic!(
            "{REASON_PARSE}: cannot read {} : {e}",
            checks_path.display()
        )
    });

    let spellings = must_fire_spellings(&checks_md);

    // The classifier's own vacuity guard.
    assert!(
        spellings.len() >= MIN_MUST_FIRE_SPELLINGS,
        "{REASON_PARSE}: classified only {} must-fire spelling(s) ({spellings:?}) \
         from §Fixture poles, below the floor of {MIN_MUST_FIRE_SPELLINGS}. \
         An assertion over a thin or empty set passes vacuously. NOTE the \
         classifier selects snake_case spellings ONLY — if a non-snake_case \
         must-fire spelling was added to the poles, this test must be revisited \
         rather than the floor lowered.",
        spellings.len()
    );

    let positives: Vec<(String, String)> = lens_fixtures()
        .into_iter()
        .filter(|(n, _)| n.starts_with("pos_"))
        .collect();

    for spelling in &spellings {
        let covered = positives.iter().any(|(_, body)| {
            // Exact-token: the spelling must appear as a whole identifier, not as
            // a substring of a longer one.
            body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .any(|tok| tok == spelling)
        });
        assert!(
            covered,
            "{REASON_DRIFT}: must-fire spelling `{spelling}` from checks.md \
             §Fixture poles appears in no `pos_` fixture under {LENS_DIR}. \
             Either add a fixture planting it, or (if the poles changed \
             deliberately) raise it with `security` + `semantic-guard` — the \
             poles are the single home and this test derives from them."
        );
    }
}

// ---------------------------------------------------------------------------
// 3. The mechanical floor's extent, pinned per spelling
// ---------------------------------------------------------------------------

/// A spelling and whether the **word-boundary** log-site matcher catches it.
///
/// **This table's spelling set is deliberately DISJOINT from the parsed poles
/// set, and that is not an oversight.** `meetingKek` is here precisely *because*
/// it is neither a pole nor planted — recording that nothing catches it is the
/// entire content of its row. Unifying the two lists would delete the only row
/// documenting an uncovered spelling. These are N sites each making their own
/// decision under one rule, so an assertion is the right instrument; a shared
/// list would be a false single-source-of-truth hiding the fork.
///
/// **Why this table exists at all.** ADR-0036 §11: *"Vocabulary additions cannot
/// be cited as the protection."* The rows below make that a machine-checked fact
/// rather than a claim — and they are what reds if anyone takes the tempting
/// shortcut of adding a vocabulary entry to green a fixture. Do not do that:
/// `crates/dt-guard/src/common/pii_vocabulary.rs` records why bare `kek` is
/// deliberately absent, and greening a fixture by widening the floor leaves
/// every unenumerated spelling uncovered while reporting success.
const FLOOR_EXTENT: &[(&str, bool)] = &[
    // CATEGORY_A entry; `\bmeeting_kek\b` matches.
    ("meeting_kek", true),
    // CATEGORY_A holds `transmit_key`, but `_` is a word character so
    // `\btransmit_key\b` has no boundary before `_bytes`. The KNOWN LIMIT
    // comment in `pii_vocabulary.rs` records this for the sibling spelling.
    ("transmit_key_bytes", false),
    // In no vocabulary in any form, and no backstop at a log site.
    ("meetingKek", false),
];

/// Locate the shipped guard binary. Layer 1 (`scripts/lang/rust/compile.sh`)
/// builds it before Layer 4 runs, and `DT_GUARD` is the documented override.
fn dt_guard_bin() -> PathBuf {
    std::env::var_os("DT_GUARD").map_or_else(
        || repo_root().join("target/release/dt-guard"),
        PathBuf::from,
    )
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn mechanical_floor_extent_is_pinned_per_spelling() {
    let bin = dt_guard_bin();
    assert!(
        bin.is_file(),
        "dt-guard binary not found at {}. Layer 1 builds it \
         (scripts/lang/rust/compile.sh); set DT_GUARD to override. This is a \
         LOUD failure on purpose — skipping here would silently drop the only \
         assertion proving the vocabulary floor is not the protection.",
        bin.display()
    );

    for (spelling, expect_hit) in FLOOR_EXTENT {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        // Scaffolding the guard's changed-file collection needs. Discovered by
        // running it, not assumed: it shells out to the diff-base resolver.
        std::fs::create_dir_all(root.join("scripts/lang")).expect("mkdir scripts/lang");
        for f in ["_get_base_ref.sh", "_common.sh"] {
            std::fs::copy(
                repo_root().join("scripts/lang").join(f),
                root.join("scripts/lang").join(f),
            )
            .unwrap_or_else(|e| panic!("copy {f}: {e}"));
        }
        std::fs::create_dir_all(root.join("crates/mc-service/src")).expect("mkdir mc-service");

        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "scaffold"]);

        // The probe stays UNTRACKED so the guard's changed-file union sees it.
        let probe = root.join("crates/mc-service/src/probe.rs");
        std::fs::write(
            &probe,
            format!("pub fn p({spelling}: &[u8]) {{\n    tracing::info!(\"v={{:x?}}\", {spelling});\n}}\n"),
        )
        .expect("write probe");

        // DEVLOOP_TMP must be per-root. `_get_base_ref.sh` caches the resolved
        // base ref and changed-file list under `$DEVLOOP_TMP` (default
        // `/tmp/devloop`), which is SHARED with the real repository's pipeline
        // runs — so without this the throwaway root inherits a base-ref SHA that
        // does not exist in it and the guard dies on `fatal: bad object`.
        // Found by the scaffolding assertion below rather than by reasoning: the
        // first version of this test hit exactly that and said so loudly instead
        // of reporting a no-hit result for every row.
        // `current_dir(root)` is LOAD-BEARING, not tidiness. The guard shells out
        // to `scripts/lang/_get_base_ref.sh`, which resolves the diff base with
        // `git merge-base origin/main HEAD` **in the process's working
        // directory** — not in `--root`. Without this the resolver runs against
        // the real repository, returns a SHA that does not exist in the throwaway
        // root, and the guard dies on `fatal: bad object`.
        //
        // This is worth a comment because it is exactly the trap this suite is
        // about: an earlier manual verification of these same rows "passed" only
        // because it happened to be run from inside the throwaway root. The
        // scaffolding assertions below are what turned that latent difference
        // into a loud failure instead of a silent no-hit for every row.
        let devloop_tmp = root.join(".devloop-tmp");
        let out = Command::new(&bin)
            .current_dir(root)
            .env("DEVLOOP_TMP", &devloop_tmp)
            .args(["rust-no-secrets-in-logs", "--root"])
            .arg(root)
            .output()
            .expect("run dt-guard");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        // Proof the run was meaningful at all: the guard must have reached a
        // verdict, not died on scaffolding. Without this, a broken invocation
        // reads identically to "clean" for every `expect_hit == false` row.
        assert!(
            combined.contains("STATUS="),
            "guard emitted no STATUS line for `{spelling}` — the invocation \
             broke and every no-hit expectation below would be vacuous.\n{combined}"
        );
        assert!(
            !combined.contains("STATUS=FAIL REASON=collecting-changed-rust-files"),
            "guard could not collect changed files for `{spelling}`; scaffolding \
             is wrong and a no-hit result would be meaningless.\n{combined}"
        );

        let hit = combined.contains("VIOLATION:");
        assert_eq!(
            hit, *expect_hit,
            "mechanical floor extent changed for `{spelling}`: expected \
             hit={expect_hit}, got hit={hit}.\n\n\
             If this reds because someone ADDED a vocabulary entry to make a \
             fixture green: that is foreclosed by ADR-0036 §11 (\"Vocabulary \
             additions cannot be cited as the protection\") and by \
             pii_vocabulary.rs's own comment on this cohort. Revert it and \
             record the gap instead.\n\n\
             If it reds because the MATCHER changed (e.g. `segments()` was \
             promoted and `rust_log_secrets` repointed onto it), that is the \
             tracked fix for the trailing-compound gap: update this row, and \
             tell `security` + `observability` the floor moved.\n\n{combined}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. The *applies* half — the check's scope is real and non-empty
// ---------------------------------------------------------------------------

#[test]
fn check_scope_is_a_real_non_empty_tree() {
    // checks.md scopes items 11-13 to `crates/mc-service/**` production code plus
    // any `dark_tower.internal.v1` message construction anywhere in the tree.
    // If either goes to zero, the check is alive-but-never-applied — the first
    // row of §11's own failure table — and this test is what notices.
    let mc_src = repo_root().join("crates/mc-service/src");
    assert!(
        mc_src.is_dir(),
        "check scope `crates/mc-service/**` does not resolve: {} — items 11-13 \
         are scoped to a tree that no longer exists",
        mc_src.display()
    );

    let mut rs_files = 0_usize;
    let mut internal_v1_sites = 0_usize;
    let mut tracing_sites = 0_usize;
    let mut stack = vec![mc_src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .expect("readable mc-service dir")
            .flatten()
        {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                rs_files += 1;
                let body = std::fs::read_to_string(&p).unwrap_or_default();
                if body.contains("internal::v1") {
                    internal_v1_sites += 1;
                }
                if body.contains("info!") || body.contains("debug!") || body.contains("error!") {
                    tracing_sites += 1;
                }
            }
        }
    }

    assert!(
        rs_files > 0,
        "no Rust production files under crates/mc-service/src — the check's \
         scope is empty and every verdict from it would be vacuous"
    );
    assert!(
        internal_v1_sites > 0,
        "no `internal::v1` references under crates/mc-service/src — item 11 \
         keys on constructions of that contract, so it currently applies to \
         nothing. Either the contract moved or MC stopped speaking it; both \
         mean item 11's scope needs revisiting."
    );
    assert!(
        tracing_sites > 0,
        "no tracing macros under crates/mc-service/src — item 12 keys on key \
         material reaching an MC sink, so it currently applies to nothing"
    );
}
