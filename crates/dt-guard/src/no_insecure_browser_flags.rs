//! `no-insecure-browser-flags` subcommand — mechanises the standing
//! prohibition on browser/test-harness settings that disable certificate
//! validation, disable web security, or force an insecure origin to be
//! treated as trustworthy.
//!
//! # Why this is a mechanism and not prose
//!
//! The prohibition is stated as a hard requirement in this story
//! (R-33) and, more widely, in story task 20: *"No Chrome flag that disables
//! certificate validation, disables web security, or forces an insecure origin
//! to be treated as trustworthy may appear in any script, config, or test
//! invocation."* Three clauses, and this guard covers all three — deliberately
//! not narrowing to the two-clause phrasing used elsewhere, because a gap
//! between two tasks in the same story is exactly the drift this work is about.
//!
//! Prose-only enforcement of a standing prohibition is not enforcement: it is
//! a control that requires someone to notice, in the same change that removes
//! that control class for the build profile (see `release_build_profile`).
//!
//! **The stake is concrete.** MC/MH certificate trust flows *exclusively*
//! through `serverCertificateHashes` pinning. Any one of the settings below
//! turns that pinning assertion into decoration while every test stays green
//! — a passing suite that proves nothing.
//!
//! # What this guard does NOT claim
//!
//! The vocabulary below is an **enumeration**, and enumerations are incomplete
//! by construction. The OK token therefore names the enumeration, never the
//! conclusion: a green means *none of the N enumerated spellings appear in the
//! scanned set*, **not** *there are no insecure browser settings*. Those differ
//! exactly at the N+1th spelling, which is where it matters. A reader citing a
//! green as evidence that the prohibition holds would be overclaiming.
//!
//! # Exact literals, never shapes — and why that is load-bearing
//!
//! Matching is on **exact literals** per prohibited setting. A shape-based
//! vocabulary (`--use-fake-*`, a broad `--disable-*` sweep, "a Chrome flag in
//! a runbook") would fire on:
//!
//! * `packages/web-app/playwright.config.ts`'s **sanctioned** fake-media args,
//!   which inject synthesized capture and auto-grant the microphone
//!   permission — they disable nothing; and
//! * story task 20's runbook text, which is **required** to name those same
//!   two flags verbatim for developers with no microphone.
//!
//! The fix under time pressure would then be a broad allowlist entry for those
//! files — which would go on to mask a real cert-bypass setting added to the
//! same file later, with every test still green. See
//! [`tests::sanctioned_fake_media_flags_are_not_in_the_vocabulary`]: the
//! sanctioned flags pass on **vocabulary non-membership**, never via an
//! allowlist entry. That is the distinction the test exists to pin.
//!
//! # Why this vocabulary lives here and not in `secret_patterns.rs`
//!
//! The crate has two existing forbidden-literal homes, and a reader finding a
//! third will reasonably try to merge them. Do not:
//!
//! * `secret_patterns.rs::HYGIENE_PATTERNS` is a table of **credential-shaped**
//!   literals with two live consumers (`alert-rules-policy`, `secret-scan`). A
//!   browser trust-bypass setting is a CLI *capability*, not a secret. Folding
//!   it in would silently widen both consumers' scan surface —
//!   `alert-rules-policy` would start reporting browser flags in alert YAML,
//!   which is meaningless there — and every future false positive in this
//!   vocabulary would become a false positive in two unrelated guards. That is
//!   concept-collapse wearing DRY's clothes.
//! * `ts_dev_trust.rs::FORBIDDEN_LITERAL` is the shape being followed here: a
//!   policy-scoped const beside its single consumer. Extract to a shared table
//!   only when a second consumer actually appears.
//!
//! # Scope INCLUDES docs and runbooks — the opposite of `release_build_profile`
//!
//! Do not "harmonize" the two rules' file scoping. `release_build_profile`'s
//! RUSTFLAGS rule *excludes* docs, because a doc showing a mold-linker recipe
//! is not a leak. This rule *includes* them, because **a runbook telling a
//! developer to paste a cert-bypass flag IS the leak being guarded** — it is
//! the highest-value coverage, not an edge case. Two rules, opposite scoping,
//! for reasons specific to each. The reason is recorded and not just the rule,
//! because a comment stating only the rule gets deleted by the same person who
//! would tidy the asymmetry away.

use crate::common::explain::{print_finding, Finding};
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use anyhow::Result;
use std::path::Path;

pub const CERT_VALIDATION_FLAG: &str = "cert_validation_flag";
pub const INSECURE_ORIGIN_FLAG: &str = "insecure_origin_flag";
pub const INSECURE_CONFIG_PROPERTY: &str = "insecure_config_property";
pub const NO_CANDIDATE_FILES: &str = "no_candidate_files";
/// A candidate file could not be read, so it silently left the scanned set.
///
/// Fails closed rather than warning, and the asymmetry with
/// `release_build_profile`'s sibling site is deliberate — see the note at the
/// read site below.
pub const UNREADABLE_CANDIDATE_FILE: &str = "unreadable_candidate_file";

/// Launch flags that disable or bypass **certificate validation**.
///
/// Remedy lives in an args array (e.g. `launchOptions.args`), which is why
/// this is a distinct class from [`INSECURE_ORIGIN_FLAGS`] and from
/// [`INSECURE_PROPERTIES`]: three classes, three places to look.
pub const CERT_VALIDATION_FLAGS: &[&str] = &[
    "--ignore-certificate-errors-spki-list",
    "--ignore-certificate-errors",
    "--ignore-urlfetcher-cert-requests",
    "--allow-insecure-localhost",
];

/// Launch flags that force an insecure origin to be treated as trustworthy,
/// or disable web security / mixed-content protection.
///
/// `--test-type` is included because it suppresses the security-warning
/// gating several of the others depend on.
pub const INSECURE_ORIGIN_FLAGS: &[&str] = &[
    "--unsafely-treat-insecure-origin-as-secure",
    "--allow-running-insecure-content",
    "--reduce-security-for-testing",
    "--disable-web-security",
    "--test-type",
];

/// Config properties and environment variables with the same effect and no
/// flag involved — the *cheap* bypass, and the one a flag-only matcher misses
/// entirely.
///
/// Each entry is `(name, unsafe_value)`. **Value-keyed**: `rejectUnauthorized:
/// true` and `NODE_TLS_REJECT_UNAUTHORIZED=1` are correct code and must not
/// fire. A presence-keyed matcher here would false-fail legitimate TLS
/// configuration, and a guard that false-fails gets bypassed.
pub const INSECURE_PROPERTIES: &[(&str, &str)] = &[
    // Playwright browser-context option.
    ("ignoreHTTPSErrors", "true"),
    // WebDriver / Selenium capability.
    ("acceptInsecureCerts", "true"),
    // Node TLS opt-outs. The env-test and E2E harnesses are Node processes
    // that talk to MC/MH — same threat, different runtime.
    ("NODE_TLS_REJECT_UNAUTHORIZED", "0"),
    ("rejectUnauthorized", "false"),
];

/// Literal-path allowlist. **Two entries, both justified inline.**
///
/// Deliberately literal paths, never a prefix or glob: a prefix would silently
/// widen to cover a future real violation in a sibling file, which is the
/// failure mode this allowlist is most likely to acquire. There is no
/// `docs/**` exclusion — a runbook naming a flag is precisely the leak, so
/// excluding docs wholesale would delete the highest-value coverage.
///
/// Test fixtures live in tempdirs and the e2e suite imports the vocabulary
/// constants rather than spelling literals, so **no test file needs an
/// exemption at all.**
const ALLOWLIST: &[(&str, &str)] = &[
    (
        "crates/dt-guard/src/no_insecure_browser_flags.rs",
        "canonical home of the vocabulary itself",
    ),
    (
        "docs/devloop-outputs/2026-08-03-playwright-e2e-harness/main.md",
        "historical record asserting these settings' deliberate ABSENCE, with the \
         serverCertificateHashes rationale",
    ),
];

/// File suffixes that can carry a script, config, runbook or test invocation.
const CANDIDATE_SUFFIXES: &[&str] = &[
    ".sh", ".bash", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".json", ".yaml", ".yml", ".md", ".rs",
    ".py", ".toml", ".svelte",
];
/// Extensionless filenames worth scanning.
const CANDIDATE_FILENAMES: &[&str] = &["Dockerfile"];

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    token: &'static str,
    literal: String,
    file: String,
    line: usize,
}

fn is_candidate(rel: &str) -> bool {
    if CANDIDATE_SUFFIXES.iter().any(|s| rel.ends_with(s)) {
        return true;
    }
    rel.rsplit('/')
        .next()
        .map(|n| CANDIDATE_FILENAMES.contains(&n))
        .unwrap_or(false)
}

/// rule_id -> operator-facing REASON token.
///
/// One token per class, never fused: a cert-bypass launch flag, an
/// insecure-origin launch flag and a config property are three different
/// remedies in three different PLACES (an args array, a launch args array, a
/// `use:` block or an env var). A single token could not tell a reader where
/// to look.
fn token_for(rule_id: &str) -> &'static str {
    match rule_id {
        CERT_VALIDATION_FLAG => "insecure-browser-cert-validation-flag",
        INSECURE_ORIGIN_FLAG => "insecure-browser-origin-or-websecurity-flag",
        INSECURE_CONFIG_PROPERTY => "insecure-browser-config-property",
        NO_CANDIDATE_FILES => "no-insecure-browser-flags-no-candidate-files",
        UNREADABLE_CANDIDATE_FILE => "no-insecure-browser-flags-unreadable-candidate-file",
        _ => "insecure-browser-setting",
    }
}

fn allowlist_reason(rel: &str) -> Option<&'static str> {
    ALLOWLIST
        .iter()
        .find(|(p, _)| *p == rel)
        .map(|(_, why)| *why)
}

/// Does `line` set `name` to `unsafe_value`?
///
/// Accepts `name: value`, `name = value`, `name=value`, and quoted variants —
/// the spellings these settings actually take in TS/JSON/YAML/shell.
///
/// # Two real defects fixed here at Gate 2 (not merely a lint)
///
/// The first version did `line.find(name)` + `line.as_bytes()[idx - 1]`.
/// `clippy::indexing-slicing` flagged the index; looking at why surfaced two
/// behavioural bugs behind it, both of which made this guard *quietly* weaker:
///
/// 1. **First-match-only was a bypass.** `find` returns only the FIRST
///    occurrence. If a non-boundary occurrence preceded a real one on the same
///    line — `const xignoreHTTPSErrors = 1; ignoreHTTPSErrors: true` — the
///    boundary check rejected the decoy and returned `false`, **missing the
///    genuine violation**. A security guard reporting clean on a line that
///    does contain the setting is the exact "reads as coverage" failure this
///    subcommand exists to prevent. Now every match position is considered.
/// 2. **Byte-wise boundary was UTF-8-wrong.** `as_bytes()[idx - 1] as char`
///    reads a raw byte; when the preceding character is multi-byte, that byte
///    is a continuation byte (`0x80..=0xBF`), which maps to a non-alphanumeric
///    `char` — so a genuinely-adjacent word character read as a word boundary
///    and produced a false positive. Now the boundary is the preceding
///    `char`, via a total `get(..idx)`.
///
/// Every access is total (`get`, `chars`): no indexing, no slicing, no
/// arithmetic that can underflow. ADR-0002 no-panic — a guard that panics on
/// malformed input is worse than one that fails cleanly, because a panic
/// inside Layer 3 reads as a crashed pipeline rather than a finding.
fn property_set_to(line: &str, name: &str, unsafe_value: &str) -> bool {
    // ALL match positions, not just the first — see defect (1) above.
    line.match_indices(name)
        .any(|(idx, _)| property_assignment_at(line, idx, name, unsafe_value))
}

/// Is the occurrence of `name` at byte offset `idx` a real assignment of
/// `unsafe_value`? Split out so the multi-occurrence walk above stays one
/// readable line.
fn property_assignment_at(line: &str, idx: usize, name: &str, unsafe_value: &str) -> bool {
    // Word boundary before the name, so `myIgnoreHTTPSErrors` does not match.
    // Character-wise, not byte-wise — see defect (2) above. `idx == 0` yields
    // `None` naturally, so there is no `idx - 1` to underflow.
    if let Some(prev) = line
        .get(..idx)
        .and_then(|before| before.chars().next_back())
    {
        if prev.is_alphanumeric() || prev == '_' {
            return false;
        }
    }
    let Some(rest) = line.get(idx.saturating_add(name.len())..) else {
        return false;
    };
    let rest = rest
        .trim_start()
        .trim_start_matches(['"', '\''])
        .trim_start();
    let Some(rest) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) else {
        return false;
    };
    let value = rest
        .trim_start()
        .trim_start_matches(['"', '\''])
        .trim_start();
    value
        .to_ascii_lowercase()
        .starts_with(&unsafe_value.to_ascii_lowercase())
}

/// Scan one file's content for every vocabulary entry.
fn scan_content(rel: &str, content: &str, hits: &mut Vec<Hit>) {
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        for flag in CERT_VALIDATION_FLAGS {
            if line.contains(flag) {
                hits.push(Hit {
                    rule_id: CERT_VALIDATION_FLAG,
                    token: token_for(CERT_VALIDATION_FLAG),
                    literal: (*flag).to_string(),
                    file: rel.to_string(),
                    line: line_no,
                });
                break;
            }
        }
        for flag in INSECURE_ORIGIN_FLAGS {
            if line.contains(flag) {
                hits.push(Hit {
                    rule_id: INSECURE_ORIGIN_FLAG,
                    token: token_for(INSECURE_ORIGIN_FLAG),
                    literal: (*flag).to_string(),
                    file: rel.to_string(),
                    line: line_no,
                });
                break;
            }
        }
        for (name, unsafe_value) in INSECURE_PROPERTIES {
            if property_set_to(line, name, unsafe_value) {
                hits.push(Hit {
                    rule_id: INSECURE_CONFIG_PROPERTY,
                    token: token_for(INSECURE_CONFIG_PROPERTY),
                    literal: format!("{name}={unsafe_value}"),
                    file: rel.to_string(),
                    line: line_no,
                });
                break;
            }
        }
    }
}

/// Candidate files, tracked-only.
///
/// Tracked-only mirrors commit `28defd8`'s lesson: untracked Playwright traces
/// and `node_modules` otherwise produce noise, or a bail that reads as a pass.
fn candidate_files(repo_root: &Path) -> Result<Vec<String>> {
    let tracked = crate::common::git_changes::get_tracked_files(repo_root, ".", &[])?;
    let mut out: Vec<String> = tracked
        .into_iter()
        .map(|p| p.display().to_string())
        .filter(|rel| is_candidate(rel))
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// Run the no-insecure-browser-flags policy.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let files = candidate_files(repo_root)?;
    let mut hits: Vec<Hit> = Vec::new();

    // VACUITY — and this is the easiest vacuity miss in the whole change,
    // because this guard's green TODAY literally is "found nothing": the
    // invariant holds by convention, with the only in-tree mention being a
    // record explaining the deliberate absence. A walk that matches zero
    // candidate files and reports clean is indistinguishable from a working
    // one, which is the pathology this guard exists to prevent.
    if files.is_empty() {
        let token = token_for(NO_CANDIDATE_FILES);
        println!(
            "ERROR: PRECONDITION [{token}] {} — the walk matched zero scannable files, so this \
             guard checked nothing (discovery/precondition failure, NOT a diff defect; see \
             docs/runbooks/devloop-validation.md §6.3.1)",
            repo_root.display()
        );
        anyhow::bail!("{token}-1");
    }

    let mut allowlisted_hits = 0usize;
    for rel in &files {
        let abs = repo_root.join(rel);
        // A file that cannot be read leaves the scanned set. Previously that
        // was a bare `continue` — so the file dropped out silently while the
        // `SCOPE:` line below still counted it, making the count and the
        // coverage claim disagree with nothing saying so. Same
        // "could-not-evaluate reads as a pass" shape as the vacuity bail
        // twenty lines up, one branch lower (@dry-reviewer F-DRY-F).
        //
        // `warn_skip` is the shared home for this exact pattern
        // (`common/scan.rs`, 8 consumer modules); it puts the underlying IO
        // error on stderr for an operator.
        //
        // STRICTER THAN THE SIBLING SITE, ON PURPOSE. `release_build_profile`
        // also warns here, but this guard additionally FAILS, because its
        // green is a coverage claim over an enumerated file set — "none of the
        // N enumerated spellings appear in the scanned set". A file missing
        // from that set weakens the claim itself, and this guard enforces a
        // security prohibition where the whole value is coverage. Same rule
        // already applied to `dev-web.sh`'s missing `ss`: a check that CANNOT
        // RUN is not a check that passed. Two guards, opposite treatment of
        // the same IO error, for reasons specific to each — as with the
        // docs-scoping asymmetry documented above.
        let content = match std::fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(e) => {
                warn_skip("no-insecure-browser-flags candidate read", &abs, &e);
                hits.push(Hit {
                    rule_id: UNREADABLE_CANDIDATE_FILE,
                    token: token_for(UNREADABLE_CANDIDATE_FILE),
                    literal: e.to_string(),
                    file: rel.clone(),
                    line: 0,
                });
                continue;
            }
        };
        let mut file_hits: Vec<Hit> = Vec::new();
        scan_content(rel, &content, &mut file_hits);
        if file_hits.is_empty() {
            continue;
        }
        if let Some(why) = allowlist_reason(rel) {
            allowlisted_hits += file_hits.len();
            eprintln!("WARN dt-guard allowlisted mention in {rel} ({why})");
            continue;
        }
        hits.extend(file_hits);
    }

    // Counts line: visible standalone or under `--verbose` only. Layer 3 runs
    // the guard runner non-verbose and never echoes captured output on the OK
    // path, so this is for the standalone triage reader, not the devloop log.
    println!(
        "SCOPE: {} candidate files, {} enumerated settings, {allowlisted_hits} allowlisted \
         mentions, {} hits",
        files.len(),
        CERT_VALIDATION_FLAGS.len() + INSECURE_ORIGIN_FLAGS.len() + INSECURE_PROPERTIES.len(),
        hits.len()
    );

    if hits.is_empty() {
        // Token names the ENUMERATION, not the conclusion. "no-insecure-
        // browser-flags" alone would read as a universal claim.
        emit_ok("no-insecure-browser-flags-enumerated-settings-clean");
        return Ok(());
    }

    // Precondition lines first: `run-guards.sh` caps re-emitted output at
    // `head -5`, so a coverage refusal buried below violations can be
    // truncated away.
    let mut ordered: Vec<&Hit> = hits.iter().collect();
    ordered.sort_by_key(|h| usize::from(h.rule_id != UNREADABLE_CANDIDATE_FILE));

    for hit in ordered {
        if hit.rule_id == UNREADABLE_CANDIDATE_FILE && !explain {
            println!(
                "ERROR: PRECONDITION [{}] {} — candidate file could not be read ({}), so it left \
                 the scanned set and this guard's coverage claim no longer covers it \
                 (discovery/precondition failure, NOT a diff defect; see \
                 docs/runbooks/devloop-validation.md §6.3.1)",
                hit.token, hit.file, hit.literal
            );
            continue;
        }
        if explain {
            let policy = format!("no-insecure-browser-flags::{}", hit.rule_id);
            print_finding(&Finding {
                file: &hit.file,
                row: hit.line,
                col: 0,
                policy: &policy,
                matched: &hit.literal,
                extras: &[("reason", hit.token)],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            // Token rides INSIDE the VIOLATION line: `run-guards.sh` captures
            // guard stdout non-verbose and re-emits only lines matching
            // VIOLATION|ERROR|WARN, so a token living solely in the STATUS
            // line never reaches an operator.
            println!(
                "VIOLATION: [{}] {}:{} `{}` disables a security control that MC/MH trust \
                 depends on (certificate pinning via serverCertificateHashes). Remove it; \
                 an allowlist entry is not the remedy.",
                hit.token, hit.file, hit.line, hit.literal
            );
        }
    }

    // Precondition class outranks every violation class, mirroring
    // `release_build_profile::reason_for`: `classify_guard_exit` routes only
    // 124/137 to the operator lane, so the token is the only channel that can
    // carry it.
    let unreadable = hits
        .iter()
        .filter(|h| h.rule_id == UNREADABLE_CANDIDATE_FILE)
        .count();
    if unreadable > 0 {
        anyhow::bail!("{}-{unreadable}", token_for(UNREADABLE_CANDIDATE_FILE));
    }

    let cert = hits
        .iter()
        .filter(|h| h.rule_id == CERT_VALIDATION_FLAG)
        .count();
    let origin = hits
        .iter()
        .filter(|h| h.rule_id == INSECURE_ORIGIN_FLAG)
        .count();
    let prop = hits
        .iter()
        .filter(|h| h.rule_id == INSECURE_CONFIG_PROPERTY)
        .count();
    // Distinct tokens per class: three classes are three remedies in three
    // different PLACES (an args array vs a config property vs an env var), so
    // a fused token could not tell a reader where to look.
    let (token, n) = [
        (token_for(CERT_VALIDATION_FLAG), cert),
        (token_for(INSECURE_ORIGIN_FLAG), origin),
        (token_for(INSECURE_CONFIG_PROPERTY), prop),
    ]
    .into_iter()
    .max_by_key(|(_, n)| *n)
    .unwrap_or(("insecure-browser-setting", hits.len()));
    anyhow::bail!("{token}-{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two sanctioned fake-media flags. They inject synthesized capture
    /// and auto-grant the microphone permission; they disable nothing.
    const SANCTIONED_FAKE_MEDIA: &[&str] = &[
        "--use-fake-device-for-media-stream",
        "--use-fake-ui-for-media-stream",
    ];

    #[test]
    fn sanctioned_fake_media_flags_are_not_in_the_vocabulary() {
        // THE boundary test. These must pass on **vocabulary non-membership**,
        // never via an allowlist entry — that is what proves the vocabulary is
        // property-scoped rather than `--use-fake-*`-shaped.
        //
        // If someone later "harmonizes" the vocabulary to a shape, this fires
        // BEFORE the guard starts firing on packages/web-app/playwright.config.ts
        // and on story task 20's required runbook text — which is the point at
        // which the tempting fix becomes a broad allowlist entry that would go
        // on to mask a real cert-bypass setting in the same file.
        for flag in SANCTIONED_FAKE_MEDIA {
            let mut hits = Vec::new();
            scan_content(
                "packages/web-app/playwright.config.ts",
                &format!("      args: ['{flag}'],"),
                &mut hits,
            );
            assert!(
                hits.is_empty(),
                "{flag} is sanctioned and must not match the vocabulary"
            );
            assert!(
                allowlist_reason("packages/web-app/playwright.config.ts").is_none(),
                "it must pass on NON-MEMBERSHIP, not via an allowlist entry"
            );
        }
    }

    #[test]
    fn cert_validation_flags_are_caught() {
        for flag in CERT_VALIDATION_FLAGS {
            let mut hits = Vec::new();
            scan_content("scripts/example.sh", &format!("chrome {flag} &"), &mut hits);
            assert_eq!(hits.len(), 1, "{flag} must be caught");
            assert_eq!(hits[0].rule_id, CERT_VALIDATION_FLAG);
        }
    }

    #[test]
    fn insecure_origin_flags_are_caught() {
        for flag in INSECURE_ORIGIN_FLAGS {
            let mut hits = Vec::new();
            scan_content("scripts/example.sh", &format!("chrome {flag} &"), &mut hits);
            assert_eq!(hits.len(), 1, "{flag} must be caught");
            assert_eq!(hits[0].rule_id, INSECURE_ORIGIN_FLAG);
        }
    }

    #[test]
    fn config_properties_are_value_keyed() {
        // The cheap bypass: identical effect, zero flags present.
        let mut hits = Vec::new();
        scan_content(
            "packages/web-app/playwright.config.ts",
            "    ignoreHTTPSErrors: true,",
            &mut hits,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, INSECURE_CONFIG_PROPERTY);

        // ...and the SAFE settings must not fire, or the guard false-fails on
        // correct TLS configuration and gets bypassed.
        for safe in [
            "    ignoreHTTPSErrors: false,",
            "    rejectUnauthorized: true,",
            "    acceptInsecureCerts: false,",
            "NODE_TLS_REJECT_UNAUTHORIZED=1",
        ] {
            let mut hits = Vec::new();
            scan_content("x.ts", safe, &mut hits);
            assert!(hits.is_empty(), "{safe} is correct code and must not fire");
        }
    }

    #[test]
    fn node_tls_opt_outs_are_caught() {
        for unsafe_line in [
            "NODE_TLS_REJECT_UNAUTHORIZED=0",
            "  rejectUnauthorized: false,",
        ] {
            let mut hits = Vec::new();
            scan_content("scripts/x.sh", unsafe_line, &mut hits);
            assert_eq!(hits.len(), 1, "{unsafe_line} must be caught");
        }
    }

    #[test]
    fn word_boundary_prevents_substring_false_positives() {
        let mut hits = Vec::new();
        scan_content("x.ts", "  myIgnoreHTTPSErrors: true,", &mut hits);
        assert!(hits.is_empty());
    }

    #[test]
    fn decoy_occurrence_does_not_hide_a_real_one_on_the_same_line() {
        // GATE-2 REGRESSION (real bypass, not a lint). The first version used
        // `line.find(name)` — first occurrence only — so a non-boundary decoy
        // earlier on the line made the boundary check reject it and return
        // false, MISSING the genuine setting that followed. A security guard
        // reporting clean on a line that does contain the setting is the
        // "reads as coverage" failure this subcommand exists to prevent.
        let mut hits = Vec::new();
        scan_content(
            "packages/web-app/playwright.config.ts",
            "const xignoreHTTPSErrors = 1; ignoreHTTPSErrors: true,",
            &mut hits,
        );
        assert_eq!(
            hits.len(),
            1,
            "a decoy occurrence must not mask the real assignment behind it"
        );
        assert_eq!(hits[0].rule_id, INSECURE_CONFIG_PROPERTY);

        // ...and the decoy alone still must NOT fire, or the fix would have
        // traded a false negative for a false positive.
        let mut hits = Vec::new();
        scan_content("x.ts", "const xignoreHTTPSErrors = 1;", &mut hits);
        assert!(hits.is_empty(), "the decoy alone is not a violation");
    }

    #[test]
    fn word_boundary_is_character_wise_not_byte_wise() {
        // GATE-2 REGRESSION. `as_bytes()[idx - 1] as char` read a raw byte;
        // when the preceding character is multi-byte, that byte is a
        // continuation byte (0x80..=0xBF) which maps to a non-alphanumeric
        // char — so an adjacent word character read as a word BOUNDARY and
        // produced a false positive. The `у` here is Cyrillic U+0443.
        //
        // NOTE the lowercase `i`: an earlier draft of this test wrote
        // `mуIgnoreHTTPSErrors` with a capital I, so `find` never matched the
        // vocabulary entry at all and the test passed WITHOUT EVER REACHING
        // the boundary check — vacuous, in the regression test for a guard
        // about vacuous passes. Verified against the pre-fix implementation:
        // this input returned `true` (false positive) before, `false` now.
        let mut hits = Vec::new();
        scan_content("x.ts", "  mуignoreHTTPSErrors: true,", &mut hits);
        assert!(
            hits.is_empty(),
            "a multi-byte alphanumeric before the name is still a word \
             character, so this is not a boundary and must not match"
        );
    }

    #[test]
    fn property_matching_is_total_on_hostile_input() {
        // ADR-0002 no-panic: a guard that panics reads as a crashed pipeline
        // rather than a finding. Exercises the byte-offset arithmetic against
        // inputs designed to break naive indexing — the name at offset 0
        // (no preceding char), multi-byte neighbours on both sides, and a
        // truncated tail.
        for line in [
            "ignoreHTTPSErrors: true",
            "ignoreHTTPSErrors",
            "…ignoreHTTPSErrors…: true",
            "ignoreHTTPSErrors:",
            "",
            "🔒ignoreHTTPSErrors: true",
        ] {
            let mut hits = Vec::new();
            scan_content("x.ts", line, &mut hits); // must not panic
            let _ = hits;
        }
    }

    #[test]
    fn allowlist_is_two_literal_paths_with_justifications() {
        assert_eq!(
            ALLOWLIST.len(),
            2,
            "the allowlist must stay reviewable at a glance"
        );
        for (path, why) in ALLOWLIST {
            assert!(!why.is_empty(), "{path} needs a justification");
            assert!(
                !path.contains('*') && !path.ends_with('/'),
                "{path} must be a literal path — a prefix or glob silently widens \
                 to cover future real violations in sibling files"
            );
        }
    }

    #[test]
    fn allowlist_exempts_only_the_exact_path() {
        // Proves the exemption WORKS...
        assert!(allowlist_reason(ALLOWLIST[1].0).is_some());
        // ...and that it is NARROW: the same literal in a sibling path under
        // the same directory still fails. This is the widening vector.
        assert!(
            allowlist_reason("docs/devloop-outputs/2026-08-03-playwright-e2e-harness/other.md")
                .is_none(),
            "a sibling path must NOT inherit the exemption"
        );
        assert!(allowlist_reason("docs/devloop-outputs/some-other-loop/main.md").is_none());
    }

    #[test]
    fn candidate_filter_covers_scripts_configs_runbooks_and_tests() {
        for rel in [
            "scripts/dev-web.sh",
            "packages/web-app/playwright.config.ts",
            "docs/runbooks/client-dev-local.md",
            "infra/docker/mh-service/Dockerfile",
            "crates/env-tests/tests/26_mh_quic.rs",
            "package.json",
            ".github/workflows/ci.yml",
        ] {
            assert!(is_candidate(rel), "{rel} must be scanned");
        }
        assert!(!is_candidate("infra/docker/certs/ca.pem"));
    }

    #[test]
    fn vocabulary_covers_all_three_clauses_of_the_prohibition() {
        // Task 20 states the prohibition WIDER than this task's text: disable
        // certificate validation, disable web security, OR force an insecure
        // origin. Do not narrow below it — a gap between two tasks in one
        // story is the drift this whole change is about.
        assert!(!CERT_VALIDATION_FLAGS.is_empty(), "cert-validation clause");
        assert!(
            INSECURE_ORIGIN_FLAGS.contains(&"--disable-web-security"),
            "web-security clause"
        );
        assert!(
            INSECURE_ORIGIN_FLAGS.contains(&"--unsafely-treat-insecure-origin-as-secure"),
            "insecure-origin clause"
        );
    }
}
