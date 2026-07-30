//! `ts-no-retained-credentials` subcommand — client credential-lifetime guard
//! (story `2026-05-02-browser-client-join`, task #58).
//!
//! ## What it forbids
//!
//! A **retained** client type carrying a credential-shaped field. The defect class
//! is *holding a password after the exchange that made it unnecessary* — the
//! web-app kept the raw password in `AuthResult` and replayed it at join time, long
//! after it held a `userToken` that GC would have accepted on its own.
//!
//! Note what is deliberately NOT the defect (@paired-client): a reset token does not
//! make a password unnecessary — it is the capability that lets you *set* one. So
//! `{ newPassword, resetToken }` is not a weak instance of this defect, it is a
//! different shape. The retention predicate is what excludes it (see Rule 1 below).
//!
//! ## One detector, two finding IDs — a STRUCTURAL subset
//!
//! There is exactly ONE predicate: **retention ∧ credential**. When the retained
//! type ALSO carries a session token, the finding is reported under the more specific
//! [`AUTH_STATE_RULE_ID`] with a sharper message; otherwise [`RETAINED_CRED_RULE_ID`].
//!
//! This is emitted from inside the retention branch — `if retained { emit(if has_token
//! {..} else {..}) }` — so it is *impossible* to raise an auth-state finding for a
//! declaration that did not satisfy retention. Two independent predicates would
//! implement "retention" twice with nothing asserting the implementations agree, i.e.
//! derived-state duplication: the failure class this guard exists to detect,
//! reproduced inside the guard (@dry-reviewer, task #58 Gate 1). A test asserting the
//! subset could be deleted; a structure cannot drift from itself.
//!
//! ## Full-tree, not diff-scoped
//!
//! Deliberate. A diff-scoped guard goes quiet the moment the offending declaration
//! stops being touched — which is exactly how the motivating defect survived review.
//! Collection is [`get_tracked_files`] over `packages/*`, so gitignored trees
//! (`node_modules/`, `dist/`) never appear.
//!
//! **Operational consequence, stated because oncall meets it at 3am**: this guard can
//! fail your devloop for a violation your diff did not introduce. It is a standing
//! invariant, not a diff lint.
//!
//! ## No bypass marker, by design
//!
//! There is no `guard:ignore` hatch. A suppression on a credential-retention guard is
//! applied by whoever is inconvenienced, at the moment of inconvenience, with no
//! security review. The remedy is to remove the field or stop retaining the type. If
//! the *guard itself* is wrong, revert the guard commit — separable by design.
//!
//! What earns the right to have no hatch is that the false-positive surface is
//! **measured, not argued** (see `docs/devloop-outputs/2026-07-29-client-credential-lifetime-guard-task58/main.md`).
//! The claim is "zero false positives measured across the current tree", NOT "zero by
//! construction" — an overclaimed invariant is how the next person concludes a real FP
//! must be a real finding.
//!
//! **The measurement is a standing obligation, not a landing figure.** Re-measure
//! against the real tree whenever the vocabulary, the retention-idiom set, or the
//! alias closure changes: any of those moves the FP surface without this module
//! changing, and the breakage lands on whoever's devloop is next.
//!
//! **Re-measure the FALSE-NEGATIVE side too — the same honesty applies.** A clean run
//! must also be a WARN-free run: a `warn_skip` from this guard means fields beyond
//! [`MAX_DECL_BLOCK_LINES`] were never scanned, i.e. a coverage gap, NOT the IO/parse
//! skip the runbook describes for other subcommands. The re-measurement clause above
//! was written for findings and initially missed this, because the regression landed in
//! the WARN channel rather than in the finding list (@security F-SEC-7). Check both.
//!
//! ## Stated limitations (enumerated, not exhaustive — read this before extending)
//!
//! 1. **Retention idioms are enumerated.** [`RETENTION_GENERICS`] plus class
//!    properties and module-scope annotated bindings. A novel storage idiom — pushing
//!    into a `Map`, an untyped object literal, a closure capture — is a false negative
//!    *by construction*. That residue belongs to the semantic lens
//!    (`scripts/guards/semantic/checks.md` §Client Credential Lifetime), not here.
//! 2. **Bare-string credentials are invisible to this guard.** `let password =
//!    $state('')` in a login form has no type annotation to key on, so neither finding
//!    ID can fire. That is the most common client credential-retention shape and it
//!    exists in-tree twice. It is SAFE in the collecting view; it becomes a finding
//!    only when it crosses out of that view — which requires reading intent, so it is
//!    the lens's job. `neg_bare_string_form_state.svelte` pins that this guard stays
//!    silent on it, so a later author does not "fix" the gap and turn this into a
//!    false-positive generator on every login form.
//! 3. **Transmission is not covered at all.** Re-sending a credential while already
//!    holding a token is a *transmission* event with zero retention. Lens-only; this
//!    module implements the retention gate exclusively.
//! 4. **`passphrase` / `passcode` are not matched.** Neither is in CATEGORY_A, and
//!    adding `pass` as a stem would match `passRate` / `passCount` / `passedChecks`.
//!    Documented gap, deliberately preferred over a false-positive class.
//!
//! ## Amending the predicate invalidates fixtures
//!
//! If you change what `retained` or `credential` means, every fixture written against
//! the old predicate must be re-derived — they do not still bind just because they
//! still pass. This has already happened once: retention-gating Rule 1 turned the
//! item-(a) positive fixture into a negative, because a bare `interface` has no
//! retention site (@code-reviewer, task #58). The durable check: **assert the detector
//! fires on the real pre-fix artifact, not only on a fixture.**

use crate::common::explain::{print_secret_finding, SecretFinding};
use crate::common::git_changes::get_tracked_files;
use crate::common::pii_vocabulary::{
    CATEGORY_A_ALLOWLIST, CREDENTIAL_TOKENS, SESSION_TOKENS, STEM_EXPANSIONS,
};
use crate::common::scan::warn_skip;
use crate::common::status::emit_ok;
use crate::common::test_code_filter::is_scan_exempt;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

/// A retained type carrying a credential field AND a session token — the
/// `AuthResult` shape. Refinement of [`RETAINED_CRED_RULE_ID`], never independent.
pub const AUTH_STATE_RULE_ID: &str = "auth_state_password_with_token";
/// A retained type carrying a credential field.
pub const RETAINED_CRED_RULE_ID: &str = "retained_credential_binding";

/// Pathspec for the client monorepo. Config-shaped constant, not a scattered literal.
const CLIENT_PATHSPEC: &str = "packages/*";
const EXTENSIONS: &[&str] = &[".ts", ".tsx", ".svelte"];

/// TS/JS build-artifact shapes layered ON TOP of
/// [`crate::common::test_code_filter::is_scan_exempt`], which already covers
/// `/tests/`, `/fixtures/`, `/__tests__/`, `/test-utils/`, `.test.*`, `.spec.*`.
/// Only the genuinely-additive entries live here (@dry-reviewer: calling the shared
/// helper first is what keeps this a layer rather than a reimplementation).
const BUILD_ARTIFACT_SUBSTRINGS: &[&str] = &[
    "/node_modules/",
    "/dist/",
    "/build/",
    "/.svelte-kit/",
    "/coverage/",
];
const BUILD_ARTIFACT_SUFFIXES: &[&str] = &[".d.ts"];

/// Generic-call retention idioms: `$state<T>`, `writable<T>`, … Storage, whatever the
/// binding is named — which is why renaming a type to `JoinParams` is not a bypass.
const RETENTION_GENERICS: &[&str] = &["$state", "$derived", "writable", "readable"];

/// Runaway backstop for the declaration-block walk. **Not a coverage limiter.**
///
/// The walk normally terminates when the declaration's brace depth returns to zero;
/// this cap exists only for input where that never happens — unbalanced braces, minified
/// or generated source, a file truncated mid-declaration. So it must sit far above any
/// declaration real code produces, or it stops bounding pathology and starts silently
/// reducing coverage instead.
///
/// **Tuning rule, ENFORCED not asserted**: it must exceed the largest declaration in
/// `packages/**` with a wide margin. `cap_clears_the_largest_real_declaration_with_margin`
/// derives that figure by walking the real tree at test time and fails the build if the
/// margin is lost — naming the offending file, declaration and span. So this doc's
/// measurement (2026-07-30: `SignalingClient` **468** lines, then `MeetingSession` 302,
/// `MediaTransport` 226; 2000 is ~4x) is a convenience for the reader, NOT the thing the
/// invariant rests on.
///
/// That distinction is the finding: the first version of that test compared 2000 against
/// a hardcoded 468 in this same file, so it evaluated `2000 >= 1404` forever regardless
/// of what the tree did — a class growing past the cap would have left the test green,
/// the WARN firing, and coverage silently reduced (@paired-infrastructure F-INFRA-4).
///
/// It was 400 when `class` joined the matcher, which put `SignalingClient` over the cap
/// and emitted a truncation WARN on **every green run**. Three reviewers raised that
/// independently (@security F-SEC-7, @observability F-O4, @paired-infrastructure F-INFRA-3,
/// @code-reviewer Finding 2), and the decisive argument was not the missed fields — there
/// were none — but that a permanently-on warning has no signal value, on the one failure
/// mode with no bypass hatch and no other detector behind it.
///
/// Why the cap still counts LINES rather than fields, which @code-reviewer reasonably
/// proposed: the walk already terminates at EOF via `cursor + 1 < lines.len()` in the
/// loop condition, so **termination is not what this bound buys**. What it buys is
/// *bounded work per declaration* — without it, an unbalanced-brace file makes each
/// declaration's walk O(file length), so N declarations cost O(N x file length) and the
/// scan goes quadratic on pathological input. Counting fields collected would not bound
/// that. So the fix was to tune the line bound to the pathology it actually guards, not
/// to change what it counts.
///
/// (An earlier version of this comment claimed the cap was what made the walk
/// terminate. That was false — @code-reviewer checked it against the loop condition
/// rather than taking it. Left noted because the failure mode matters: a load-bearing
/// comment written to stop the next contributor re-deriving a rejected option is worse
/// than absent if it is wrong, since "the cap isn't needed for termination" leads
/// straight to "the cap isn't needed.")
///
/// On trip it calls [`warn_skip`] rather than silently returning a truncated field set —
/// a truncated set under-matches, which is this guard's worst failure shape.
const MAX_DECL_BLOCK_LINES: usize = 2000;

// ---------------------------------------------------------------------------
// Field-name matching — segment equality over the CATEGORY_A partition
// ---------------------------------------------------------------------------

/// Split an identifier into lowercase segments on `_`, non-word chars, and camelCase
/// boundaries. `userToken` -> `[user, token]`; `access_token` -> `[access, token]`.
///
/// Segment equality rather than `\b…\b` (the sibling-guard idiom) because word
/// boundaries cannot see inside camelCase: `\btoken\b` matches **neither**
/// `userToken` nor `user_token`, so a word-boundary matcher is silent on the exact
/// field that motivated this guard (task #58, A-P1 — verified by running it).
pub fn segments(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower_or_digit = false;
    for ch in name.chars() {
        if !ch.is_ascii_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower_or_digit && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(ch.to_ascii_lowercase());
        prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn normalized(name: &str) -> String {
    segments(name).concat()
}

/// All spellings a vocabulary term matches: itself, plus its declared
/// [`STEM_EXPANSIONS`] if it has one. Never a prefix rule — see the const's doc for
/// the `creditCard` worked example.
fn spellings(term: &str) -> Vec<&'static str> {
    for (key, expansions) in STEM_EXPANSIONS {
        if *key == term {
            return expansions.to_vec();
        }
    }
    // SAFETY of the leak-free path: `term` comes from a `&'static` catalog slice.
    for t in CREDENTIAL_TOKENS.iter().chain(SESSION_TOKENS.iter()) {
        if *t == term {
            return vec![t];
        }
    }
    Vec::new()
}

/// Does `field` match any term in `subset`? Honors [`CATEGORY_A_ALLOWLIST`] on the
/// normalized join, so the single `token_type` entry also covers `tokenType`.
fn matches_subset(field: &str, subset: &[&str]) -> Option<&'static str> {
    let norm = normalized(field);
    if CATEGORY_A_ALLOWLIST.iter().any(|a| normalized(a) == norm) {
        return None;
    }
    let fs = segments(field);
    for term in subset {
        for spelling in spellings(term) {
            let ts = segments(spelling);
            if ts.is_empty() || ts.len() > fs.len() {
                continue;
            }
            if fs.windows(ts.len()).any(|w| w == ts.as_slice()) {
                return Some(spelling);
            }
        }
    }
    None
}

pub fn is_credential_field(field: &str) -> bool {
    matches_subset(field, CREDENTIAL_TOKENS).is_some()
}

pub fn is_session_token_field(field: &str) -> bool {
    matches_subset(field, SESSION_TOKENS).is_some()
}

// ---------------------------------------------------------------------------
// Phase 1 — repo-wide declaration index
// ---------------------------------------------------------------------------

/// What one type declaration contributes to the closure.
#[derive(Debug, Default, Clone)]
struct Decl {
    /// Declares a credential-vocabulary field directly (incl. nested object literals).
    has_credential: bool,
    /// Declares a session-token-vocabulary field directly.
    has_token: bool,
    /// In-repo type names this declaration inherits credential-bearing-ness from:
    /// union/intersection members, `extends` bases, and **member-of** (field types).
    refs: BTreeSet<String>,
}

/// Repo-wide credential-bearing type index. Built by
/// [`collect_credential_types`] over the WHOLE file set — Rule 2 is inherently
/// cross-file (`AuthResult` is declared in `types.ts` and retained in `App.svelte`),
/// so a per-file kernel would hand this an empty index and every assertion would come
/// back silent, which is exactly what a negative fixture asserts (task #58 F11).
#[derive(Debug, Default)]
pub struct DeclIndex {
    /// Type name -> carries a credential (after closure).
    credential_bearing: BTreeMap<String, bool>,
    /// Type name -> also carries a session token (after closure).
    token_bearing: BTreeMap<String, bool>,
}

impl DeclIndex {
    pub fn is_credential_bearing(&self, ty: &str) -> bool {
        self.credential_bearing.get(ty).copied().unwrap_or(false)
    }
    pub fn is_token_bearing(&self, ty: &str) -> bool {
        self.token_bearing.get(ty).copied().unwrap_or(false)
    }
    pub fn len(&self) -> usize {
        self.credential_bearing.values().filter(|v| **v).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static DECL_START_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:^|\s)(?:export\s+)?(?:declare\s+)?(?:abstract\s+)?(interface|type|class)\s+([A-Za-z_$][\w$]*)")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:readonly\s+)?(?:#)?([A-Za-z_$][\w$]*)\s*\??\s*:")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static TYPE_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b([A-Z][\w$]*)\b").expect("static pattern compiles"));

/// Strip `//` and `/* */` comments and string-literal *contents* from a line, so
/// braces inside them never move the depth counter. Returns `(stripped, still_in_block_comment)`.
fn strip_noise(line: &str, mut in_block_comment: bool) -> (String, bool) {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string: Option<char> = None;
    while let Some(c) = chars.next() {
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if let Some(q) = in_string {
            if c == '\\' {
                chars.next();
                continue;
            }
            if c == q {
                in_string = None;
            }
            continue; // string CONTENTS are dropped, quotes elided
        }
        match c {
            '/' => match chars.peek() {
                Some('/') => return (out, false),
                Some('*') => {
                    chars.next();
                    in_block_comment = true;
                }
                _ => out.push(c),
            },
            '"' | '\'' | '`' => in_string = Some(c),
            _ => out.push(c),
        }
    }
    (out, in_block_comment)
}

/// For `.svelte`, restrict scanning to `<script>` blocks: markup carries `{}`
/// template expressions that would otherwise corrupt every depth counter in the file.
/// Non-script lines are blanked (not removed) so reported line numbers stay true.
fn script_scoped_lines(path: &Path, content: &str) -> Vec<String> {
    let is_svelte = path.extension().is_some_and(|e| e == "svelte");
    if !is_svelte {
        return content.lines().map(str::to_string).collect();
    }
    let mut out = Vec::new();
    let mut in_script = false;
    for line in content.lines() {
        let t = line.trim_start();
        if t.starts_with("<script") {
            in_script = true;
            out.push(String::new());
            continue;
        }
        if t.starts_with("</script") {
            in_script = false;
            out.push(String::new());
            continue;
        }
        out.push(if in_script {
            line.to_string()
        } else {
            String::new()
        });
    }
    out
}

/// Phase 1 over the WHOLE file set. Takes `(path, content)` pairs so callers read
/// each file exactly once and phase 2 evaluates against this index rather than
/// re-reading — two read paths could otherwise disagree about content.
pub fn collect_credential_types(files: &[(PathBuf, String)]) -> DeclIndex {
    let mut decls: BTreeMap<String, Decl> = BTreeMap::new();
    for (path, content) in files {
        collect_from_file(path, content, &mut decls);
    }
    close_over(decls)
}

/// Harvest credential/token flags + referenced type names from one body fragment.
///
/// Shared by the single-line and multi-line paths so the two cannot disagree about what
/// counts as a field — two read paths that can diverge is the defect this guard exists
/// to detect, and it would be embarrassing to build one in.
fn collect_fields_from(fragment: &str, decl: &mut Decl) {
    for part in fragment.split(';') {
        let Some(caps) = FIELD_RE.captures(part) else {
            continue;
        };
        let Some(f) = caps.get(1) else { continue };
        if is_credential_field(f.as_str()) {
            decl.has_credential = true;
        }
        if is_session_token_field(f.as_str()) {
            decl.has_token = true;
        }
        // member-of: the field's declared type name propagates through the closure.
        if let Some(colon) = part.find(':') {
            for m in TYPE_NAME_RE.captures_iter(&part[colon..]) {
                if let Some(g) = m.get(1) {
                    decl.refs.insert(g.as_str().to_string());
                }
            }
        }
    }
}

fn collect_from_file(path: &Path, content: &str, decls: &mut BTreeMap<String, Decl>) {
    let lines = script_scoped_lines(path, content);
    let mut in_block_comment = false;
    let mut idx = 0usize;
    while idx < lines.len() {
        let Some(raw) = lines.get(idx) else { break };
        let (stripped, ends_comment) = strip_noise(raw, in_block_comment);
        in_block_comment = ends_comment;

        let Some(caps) = DECL_START_RE.captures(&stripped) else {
            idx += 1;
            continue;
        };
        let (Some(kind), Some(name)) = (caps.get(1), caps.get(2)) else {
            idx += 1;
            continue;
        };
        let name = name.as_str().to_string();
        let mut decl = decls.remove(&name).unwrap_or_default();

        // `type X = A | B;` — an alias with no body of its own. Record the referenced
        // names so the closure can propagate credential-bearing-ness through it.
        // Without this, `$state<JoinCredentials>` retains a password-carrying object
        // and produces NO finding (task #58 F-SEC-1).
        let after = &stripped[caps.get(0).map_or(0, |m| m.end())..];
        if kind.as_str() == "type" && !after.contains('{') && after.contains('=') {
            for m in TYPE_NAME_RE.captures_iter(after) {
                if let Some(g) = m.get(1) {
                    decl.refs.insert(g.as_str().to_string());
                }
            }
            decls.insert(name, decl);
            idx += 1;
            continue;
        }
        // `interface X extends Y {` — inherits from Y.
        if let Some(pos) = after.find("extends") {
            for m in TYPE_NAME_RE.captures_iter(&after[pos..]) {
                if let Some(g) = m.get(1) {
                    decl.refs.insert(g.as_str().to_string());
                }
            }
        }

        // Block-bodied declaration. Two shape rules, BOTH learned by running this against
        // the real tree rather than by reasoning about it:
        //
        // 1. **Depth policy depends on the declaration KIND.** For `interface` / `type`
        //    the whole body is type syntax, so fields are collected at ANY nesting depth
        //    — that is what makes `readonly creds: { password: string }` attribute to the
        //    enclosing declaration. A `class` body is NOT type syntax: it contains
        //    executable code, and an object literal inside a method (`password:
        //    input.password` in `AuthApiClient.register`) is field-SHAPED without being a
        //    field. Collecting at any depth there produced two false positives on the real
        //    tree the instant `class` joined the matcher. Classes therefore collect at
        //    relative depth 1 only — direct members.
        // 2. **A declaration can open AND close on one line.**
        //    `interface A { password: string }` was never indexed at all, because the
        //    walker only ever inspected SUBSEQUENT lines for the opener.
        let is_class = kind.as_str() == "class";
        let field_depth_limit: i64 = if is_class { 1 } else { i64::MAX };

        let mut depth = stripped.matches('{').count() as i64 - stripped.matches('}').count() as i64;
        let opens_here = stripped.contains('{');
        if opens_here {
            if let Some(brace) = stripped.find('{') {
                collect_fields_from(&stripped[brace + 1..], &mut decl);
            }
        }
        let mut consumed = 0usize;
        let mut cursor = idx;
        let mut opened = depth > 0 || opens_here;
        // Paren depth INSIDE the declaration body. A class's methods carry parameters,
        // and a multi-line signature puts `credentials: UserTokenCredentials,` on its own
        // line at brace-depth 1 — lexically identical to a field declaration. Without
        // this, `MeetingApiClient.createMeeting`'s parameter list made the CLASS
        // credential-bearing and the guard red-lined `MeetingSession` holding it.
        //
        // Note this is the SAME trap phase 2 already handles for retention sites
        // (`MeetingSession.ts:335 options: JoinOptions,`). It reappeared here the moment
        // classes entered the index, because only classes have method bodies. Same shape,
        // second location — worth stating so the next person adding a declaration kind
        // checks for it rather than rediscovering it.
        let mut paren_depth: i64 = 0;
        while (!opened || depth > 0) && cursor + 1 < lines.len() && consumed < MAX_DECL_BLOCK_LINES
        {
            cursor += 1;
            consumed += 1;
            let Some(raw_body) = lines.get(cursor) else {
                break;
            };
            let (body, ends) = strip_noise(raw_body, in_block_comment);
            in_block_comment = ends;
            // `depth` is nesting INSIDE the declaration body: 1 == a direct member.
            if depth <= field_depth_limit && paren_depth == 0 {
                collect_fields_from(&body, &mut decl);
            }
            paren_depth += body.matches('(').count() as i64;
            paren_depth -= body.matches(')').count() as i64;
            paren_depth = paren_depth.max(0);
            depth += body.matches('{').count() as i64;
            depth -= body.matches('}').count() as i64;
            if depth > 0 {
                opened = true;
            }
        }
        if consumed >= MAX_DECL_BLOCK_LINES {
            warn_skip(
                "ts-no-retained-credentials declaration block exceeded line cap; field set may be truncated",
                path,
                &format!("declaration `{name}` at line {}", idx + 1),
            );
        }
        decls.insert(name, decl);
        idx = cursor.max(idx + 1);
    }
}

/// Transitive closure with a **visited set**. TypeScript permits mutually recursive
/// types (`interface A { next: B }` / `interface B { next: A }`), which are idiomatic
/// for tree/list shapes; without cycle detection this does not terminate, and a hang
/// presents as `STATUS=FAIL REASON=guard-timeout-…` on every devloop — a *performance*
/// symptom that sends triage to file counts rather than to a recursive type
/// (@operations, task #58).
fn close_over(decls: BTreeMap<String, Decl>) -> DeclIndex {
    let mut index = DeclIndex::default();
    for name in decls.keys() {
        let (cred, tok) = reachable_flags(name, &decls);
        index.credential_bearing.insert(name.clone(), cred);
        index.token_bearing.insert(name.clone(), tok);
    }
    index
}

fn reachable_flags(start: &str, decls: &BTreeMap<String, Decl>) -> (bool, bool) {
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    let (mut cred, mut tok) = (false, false);
    queue.push_back(start);
    while let Some(name) = queue.pop_front() {
        if !visited.insert(name) {
            continue; // cycle guard
        }
        let Some(decl) = decls.get(name) else {
            continue;
        };
        cred |= decl.has_credential;
        tok |= decl.has_token;
        for r in &decl.refs {
            if !visited.contains(r.as_str()) {
                queue.push_back(r.as_str());
            }
        }
    }
    (cred, tok)
}

// ---------------------------------------------------------------------------
// Phase 2 — retention sites
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub rule_id: &'static str,
    pub file: PathBuf,
    pub line: usize,
    pub type_name: String,
}

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static GENERIC_RETENTION_RE: Lazy<Regex> = Lazy::new(|| {
    // Derived from RETENTION_GENERICS so the enumerated idiom set has ONE home —
    // adding an idiom there changes detection without a second edit here.
    //
    // Group 2 captures the whole angle-bracket SPAN, not the first identifier
    // (@security F-SEC-5). Reading a single identifier made `$state<undefined |
    // AuthResult>` silent while `$state<AuthResult | undefined>` fired — and since
    // reordering a union is semantically a no-op, that was an INVISIBLE BYPASS: with no
    // `guard:ignore` hatch, the natural response to a finding is to edit the line until
    // it goes green, and a reorder does exactly that while looking cosmetic and leaving
    // no greppable marker. `readonly T[]` and `Array<T>` were silent for the same reason.
    let alternation = RETENTION_GENERICS
        .iter()
        .map(|g| regex::escape(g))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(r"({alternation})\s*<([^;=]*?)>")).expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static BINDING_RE: Lazy<Regex> = Lazy::new(|| {
    // Group 2 is the whole annotation SPAN up to `=` / `;` (F-SEC-5). A `{`-literal
    // annotation (`let { auth }: { auth: AuthSession } = $props()`) is excluded by the
    // negated class, which is what keeps `$props()` destructures out of scope.
    Regex::new(r"^\s*(?:export\s+)?(?:let|const|var)\s+[A-Za-z_$][\w$]*\s*:\s*([^={;]+)")
        .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CLASS_FIELD_RE: Lazy<Regex> = Lazy::new(|| {
    // Group 2 is the whole annotation SPAN (F-SEC-5), so `readonly #f: Map<string, Creds>`
    // is seen rather than stopping at `Map`.
    Regex::new(
        r"^\s*(?:(?:public|private|protected|static|readonly|override)\s+)*(?:#)?[A-Za-z_$][\w$]*\s*\??\s*:\s*([^={;]+)",
    )
    .expect("static pattern compiles")
});

#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    reason = "module-local canonical-home static-regex initializer; pattern compiles at load-time or binary fails — ADR-0034 §6 + ADR-0002 §expect-over-allow"
)]
static CLASS_START_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:^|\s)(?:export\s+)?(?:abstract\s+)?class\s+[A-Za-z_$][\w$]*")
        .expect("static pattern compiles")
});

/// Phase 2: flag retention sites in ONE file, evaluated against the repo-wide index.
///
/// ## What counts as a retention site
/// * a [`RETENTION_GENERICS`] call with an explicit type argument;
/// * a class **property** declaration;
/// * a module-scope `let`/`const`/`var` whose annotation is a bare type NAME.
///
/// ## What must NOT (all three measured against real call sites, task #58 A-P19/A-P28)
/// * **interface / type-literal members** — `readonly credentials: JoinCredentials`
///   inside `JoinOptions` is a parameter type, not storage;
/// * **`$props()` annotations** — `let { auth }: { auth: AuthSession } = $props()` is
///   an inline type literal, and props are transient. This is why the binding rule
///   requires a bare type *name*: a `{`-literal annotation is excluded by shape;
/// * **function parameters in multi-line signatures** — `options: JoinOptions,` on its
///   own line is lexically indistinguishable from a field declaration, so paren depth
///   is tracked. Without this the guard red-lines `MeetingSession.join`'s own
///   signature: the function this task exists to fix.
pub fn scan_retention(path: &Path, content: &str, index: &DeclIndex) -> Vec<Hit> {
    let lines = script_scoped_lines(path, content);
    let mut hits: Vec<Hit> = Vec::new();
    let mut in_block_comment = false;
    let mut brace_depth: i64 = 0;
    let mut paren_depth: i64 = 0;
    // Stack of enclosing block kinds; `true` == class body (properties are storage).
    let mut class_stack: Vec<bool> = Vec::new();

    for (i, raw) in lines.iter().enumerate() {
        let (line, ends) = strip_noise(raw, in_block_comment);
        in_block_comment = ends;
        let line_no = i + 1;

        // Generic-call retention: name-agnostic, fires at any depth.
        for caps in GENERIC_RETENTION_RE.captures_iter(&line) {
            if let Some(span) = caps.get(2) {
                push_span_hits(&mut hits, path, line_no, span.as_str(), index);
            }
        }

        let in_class_body = class_stack.last().copied().unwrap_or(false);
        if paren_depth == 0 {
            if brace_depth == 0 {
                if let Some(caps) = BINDING_RE.captures(&line) {
                    if let Some(span) = caps.get(1) {
                        push_span_hits(&mut hits, path, line_no, span.as_str(), index);
                    }
                }
            } else if in_class_body {
                if let Some(caps) = CLASS_FIELD_RE.captures(&line) {
                    if let Some(span) = caps.get(1) {
                        push_span_hits(&mut hits, path, line_no, span.as_str(), index);
                    }
                }
            }
        }

        // Depth bookkeeping AFTER evaluation, so a line opening a block is judged in
        // its enclosing context.
        let opens_class = CLASS_START_RE.is_match(&line);
        for ch in line.chars() {
            match ch {
                '{' => {
                    class_stack.push(opens_class && brace_depth >= 0);
                    brace_depth += 1;
                }
                '}' => {
                    class_stack.pop();
                    brace_depth -= 1;
                }
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                _ => {}
            }
        }
        brace_depth = brace_depth.max(0);
        paren_depth = paren_depth.max(0);
    }
    hits.sort_by(|a, b| (a.line, a.rule_id).cmp(&(b.line, b.rule_id)));
    hits.dedup();
    hits
}

/// Test EVERY type identifier in an annotation span against the index.
///
/// A retention site's annotation is set-valued, not scalar: `undefined | AuthResult`,
/// `readonly AuthResult[]`, `Array<AuthResult>`, `Map<string, Creds>` all RETAIN a
/// credential-bearing type. Reading only the first identifier made a union reorder an
/// invisible bypass (@security F-SEC-5). This mirrors phase 1's member-of extraction,
/// which already ran `TYPE_NAME_RE` over a span for exactly this reason — the scalar
/// read was the same bug one layer down.
fn push_span_hits(hits: &mut Vec<Hit>, path: &Path, line: usize, span: &str, index: &DeclIndex) {
    for caps in TYPE_NAME_RE.captures_iter(span) {
        if let Some(ty) = caps.get(1) {
            push_hit(hits, path, line, ty.as_str(), index);
        }
    }
}

/// THE structural subset. One predicate (`is_credential_bearing`); the rule ID is
/// chosen *inside* the branch, so an auth-state finding cannot exist without
/// retention. See the module doc.
fn push_hit(hits: &mut Vec<Hit>, path: &Path, line: usize, ty: &str, index: &DeclIndex) {
    if !index.is_credential_bearing(ty) {
        return;
    }
    let rule_id = if index.is_token_bearing(ty) {
        AUTH_STATE_RULE_ID
    } else {
        RETAINED_CRED_RULE_ID
    };
    hits.push(Hit {
        rule_id,
        file: path.to_path_buf(),
        line,
        type_name: ty.to_string(),
    });
}

// ---------------------------------------------------------------------------
// Subcommand entry point
// ---------------------------------------------------------------------------

fn is_excluded_path(path: &Path) -> bool {
    if is_scan_exempt(path) {
        return true;
    }
    let Some(s) = path.to_str() else { return true };
    BUILD_ARTIFACT_SUBSTRINGS.iter().any(|p| s.contains(p))
        || BUILD_ARTIFACT_SUFFIXES.iter().any(|p| s.ends_with(p))
}

/// Collect + read the in-scope file set exactly once.
///
/// The exclusion filter runs on **phase 1's** input too: otherwise Rule 2 would index
/// fixtures and test files that the retention scan correctly skips, and the two phases
/// would disagree about scope — the same derived-state divergence the structural
/// subset exists to prevent (@dry-reviewer).
pub fn load_in_scope_files(repo_root: &Path) -> Result<Vec<(PathBuf, String)>> {
    let tracked = get_tracked_files(repo_root, CLIENT_PATHSPEC, EXTENSIONS)
        .context("collecting tracked client sources")?;
    let mut out = Vec::new();
    for rel in tracked {
        if is_excluded_path(&rel) {
            continue;
        }
        let abs = repo_root.join(&rel);
        match std::fs::read_to_string(&abs) {
            Ok(content) => out.push((rel, content)),
            Err(e) => warn_skip("ts-no-retained-credentials source read", &abs, &e),
        }
    }
    Ok(out)
}

/// Longest declaration block in `files`, as `(file, declaration name, line span)`.
///
/// Exists so the cap's headroom can be **derived from the tree** instead of compared
/// against a number someone typed once (@paired-infrastructure F-INFRA-4). It reuses the
/// same `strip_noise` + brace-walk primitives the scanner does, deliberately: a second
/// measuring implementation could disagree with the one being measured, which is the
/// divergence this guard exists to detect.
pub fn largest_declaration_span(files: &[(PathBuf, String)]) -> Option<(PathBuf, String, usize)> {
    let mut largest: Option<(PathBuf, String, usize)> = None;
    for (path, content) in files {
        let lines = script_scoped_lines(path, content);
        let mut in_block_comment = false;
        let mut idx = 0usize;
        while idx < lines.len() {
            let Some(raw) = lines.get(idx) else { break };
            let (stripped, ends) = strip_noise(raw, in_block_comment);
            in_block_comment = ends;
            let Some(caps) = DECL_START_RE.captures(&stripped) else {
                idx += 1;
                continue;
            };
            let name = caps
                .get(2)
                .map_or_else(String::new, |m| m.as_str().to_string());
            let mut depth =
                stripped.matches('{').count() as i64 - stripped.matches('}').count() as i64;
            let mut opened = stripped.contains('{');
            let mut cursor = idx;
            while (!opened || depth > 0) && cursor + 1 < lines.len() {
                cursor += 1;
                let Some(body_raw) = lines.get(cursor) else {
                    break;
                };
                let (body, ends2) = strip_noise(body_raw, in_block_comment);
                in_block_comment = ends2;
                depth += body.matches('{').count() as i64;
                depth -= body.matches('}').count() as i64;
                if depth > 0 {
                    opened = true;
                }
            }
            let span = cursor.saturating_sub(idx);
            if largest.as_ref().is_none_or(|(_, _, best)| span > *best) {
                largest = Some((path.clone(), name, span));
            }
            idx = cursor.max(idx + 1);
        }
    }
    largest
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let files = load_in_scope_files(repo_root)?;
    if files.is_empty() {
        emit_ok("ts-no-retained-credentials-no-files");
        return Ok(());
    }

    let index = collect_credential_types(&files);
    let mut all_hits: Vec<Hit> = Vec::new();
    for (path, content) in &files {
        all_hits.extend(scan_retention(path, content, &index));
    }

    if all_hits.is_empty() {
        emit_ok(format!(
            "ts-no-retained-credentials-clean-{}-files",
            files.len()
        ));
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("ts-no-retained-credentials::{}", hit.rule_id);
            print_secret_finding(&SecretFinding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: &policy,
                pattern_name: hit.rule_id,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            // Type NAME only — never the matched field, and never a value. This guard
            // matches by construction on lines adjacent to credentials.
            println!(
                "VIOLATION: {}:{} [{}] retained type `{}` carries a credential field{}. \
                 Fix: remove the field, or stop retaining the type. There is no bypass \
                 marker; if the guard itself is wrong, revert the guard commit.",
                file_disp,
                hit.line,
                hit.rule_id,
                hit.type_name,
                if hit.rule_id == AUTH_STATE_RULE_ID {
                    " alongside a session token (the credential is provably unnecessary — you already hold a token)"
                } else {
                    ""
                }
            );
        }
    }

    anyhow::bail!(
        "ts-retained-credentials-violation-found-{}-of-{}-files",
        all_hits.len(),
        files.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(files: &[(&str, &str)]) -> DeclIndex {
        let owned: Vec<(PathBuf, String)> = files
            .iter()
            .map(|(p, c)| (PathBuf::from(p), (*c).to_string()))
            .collect();
        collect_credential_types(&owned)
    }

    fn scan(path: &str, content: &str, index: &DeclIndex) -> Vec<Hit> {
        scan_retention(Path::new(path), content, index)
    }

    // --- segment matcher: the A-P1 regression lock -------------------------

    #[test]
    fn segments_splits_camel_and_snake() {
        assert_eq!(segments("userToken"), vec!["user", "token"]);
        assert_eq!(segments("user_token"), vec!["user", "token"]);
        assert_eq!(segments("accessToken"), vec!["access", "token"]);
        assert_eq!(segments("password"), vec!["password"]);
    }

    /// The bug that nearly shipped: `\btoken\b` matches NEITHER `userToken` (no
    /// boundary between `r` and `T`) nor `user_token` (`_` is a word char). Rule 1
    /// would have been silent on the exact type that motivated this guard.
    #[test]
    fn token_limb_matches_camel_and_snake_spellings() {
        for f in [
            "userToken",
            "user_token",
            "meetingToken",
            "joinToken",
            "bindingToken",
            "accessToken",
            "refreshToken",
            "access_token",
            "jwt",
        ] {
            assert!(is_session_token_field(f), "{f} should match the token limb");
        }
    }

    #[test]
    fn credential_limb_matches_stem_expansions() {
        for f in [
            "password",
            "passwd",
            "pwd",
            "clientSecret",
            "cred",
            "creds",
            "credential",
            "credentials",
        ] {
            assert!(
                is_credential_field(f),
                "{f} should match the credential limb"
            );
        }
    }

    /// The `creditCard` regression: an open `cred` prefix would fire on ordinary
    /// English. `credit_card` is already CATEGORY_B vocabulary.
    #[test]
    fn credential_limb_does_not_fire_on_credit_words() {
        for f in [
            "creditCard",
            "credit_card",
            "creditLimit",
            "credited",
            "credits",
        ] {
            assert!(
                !is_credential_field(f),
                "{f} must NOT match the credential limb"
            );
        }
    }

    #[test]
    fn allowlist_covers_both_spellings_of_token_type() {
        assert!(!is_session_token_field("token_type"));
        assert!(!is_session_token_field("tokenType"));
        // and a word merely starting with `token` is not a token field
        assert!(!is_session_token_field("tokenizer"));
    }

    #[test]
    fn non_credential_names_stay_silent() {
        for f in [
            "displayName",
            "subdomain",
            "meetingCode",
            "name",
            "email",
            "expiresIn",
        ] {
            assert!(
                !is_credential_field(f),
                "{f} credential-limb false positive"
            );
            assert!(!is_session_token_field(f), "{f} token-limb false positive");
        }
    }

    // --- phase 1 / closure -------------------------------------------------

    #[test]
    fn collects_direct_credential_and_token_fields() {
        let i = idx(&[(
            "packages/web-app/src/lib/types.ts",
            "export interface AuthResult {\n  readonly subdomain: string;\n  readonly password: string;\n  readonly userToken: string;\n}\n",
        )]);
        assert!(i.is_credential_bearing("AuthResult"));
        assert!(i.is_token_bearing("AuthResult"));
    }

    #[test]
    fn nested_object_literal_attributes_to_enclosing_decl() {
        let i = idx(&[(
            "packages/x/src/a.ts",
            "export interface StoredAuth {\n  readonly creds: { password: string };\n}\n",
        )]);
        assert!(i.is_credential_bearing("StoredAuth"));
    }

    /// F-SEC-1: a union alias has no fields of its own, so without closure
    /// `$state<JoinCredentials>` retains a password-carrying object silently.
    #[test]
    fn union_alias_inherits_from_members() {
        let i = idx(&[(
            "packages/sdk-core/src/session/events.ts",
            "export interface LoginCredentials {\n  readonly password: string;\n}\n\
             export interface TokenCredentials {\n  readonly userToken: string;\n}\n\
             export type JoinCredentials = TokenCredentials | LoginCredentials;\n",
        )]);
        assert!(i.is_credential_bearing("JoinCredentials"));
    }

    #[test]
    fn extends_and_member_of_propagate() {
        let i = idx(&[(
            "packages/x/src/a.ts",
            "export interface Base {\n  readonly password: string;\n}\n\
             export interface Derived extends Base {\n  readonly other: string;\n}\n\
             export interface Holder {\n  readonly inner: Base;\n}\n",
        )]);
        assert!(i.is_credential_bearing("Derived"), "extends must propagate");
        assert!(
            i.is_credential_bearing("Holder"),
            "member-of must propagate"
        );
    }

    /// Mutually recursive types are legal TS. Without a visited set this hangs, and a
    /// hang presents as a 30s guard timeout on every devloop repo-wide.
    #[test]
    fn mutually_recursive_types_terminate() {
        let i = idx(&[(
            "packages/x/src/cycle.ts",
            "export interface A {\n  readonly next: B;\n}\n\
             export interface B {\n  readonly prev: A;\n  readonly password: string;\n}\n",
        )]);
        assert!(i.is_credential_bearing("A"));
        assert!(i.is_credential_bearing("B"));
    }

    #[test]
    fn clean_types_are_not_credential_bearing() {
        let i = idx(&[(
            "packages/web-app/src/lib/types.ts",
            "export interface AuthSession {\n  readonly subdomain: string;\n  readonly displayName?: string;\n  readonly userToken: string;\n}\n",
        )]);
        assert!(!i.is_credential_bearing("AuthSession"));
    }

    // --- phase 2 / retention sites -----------------------------------------

    #[test]
    fn state_rune_is_a_retention_site_and_reports_the_refined_rule() {
        let i = idx(&[(
            "packages/web-app/src/lib/types.ts",
            "export interface AuthResult {\n  readonly password: string;\n  readonly userToken: string;\n}\n",
        )]);
        let hits = scan(
            "packages/web-app/src/App.svelte",
            "<script lang=\"ts\">\n  let auth = $state<AuthResult | undefined>(undefined);\n</script>\n",
            &i,
        );
        assert_eq!(hits.len(), 1, "got {hits:?}");
        assert_eq!(hits[0].rule_id, AUTH_STATE_RULE_ID);
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn credential_without_token_reports_the_base_rule() {
        let i = idx(&[(
            "packages/x/src/a.ts",
            "export interface ResetState {\n  readonly password: string;\n}\n",
        )]);
        let hits = scan(
            "packages/x/src/b.svelte",
            "<script lang=\"ts\">\n  let s = $state<ResetState | undefined>(undefined);\n</script>\n",
            &i,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, RETAINED_CRED_RULE_ID);
    }

    #[test]
    fn class_property_is_a_retention_site() {
        let i = idx(&[(
            "packages/x/src/a.ts",
            "export interface Creds {\n  readonly password: string;\n}\n",
        )]);
        let hits = scan(
            "packages/x/src/holder.ts",
            "export class Holder {\n  readonly #creds: Creds;\n}\n",
            &i,
        );
        assert_eq!(hits.len(), 1, "got {hits:?}");
        assert_eq!(hits[0].line, 2);
    }

    /// Real site `events.ts:59` — a member of the `JoinOptions` INTERFACE. A
    /// parameter type, not storage.
    #[test]
    fn interface_member_is_not_a_retention_site() {
        let i = idx(&[(
            "packages/sdk-core/src/session/events.ts",
            "export interface LoginCredentials {\n  readonly password: string;\n}\n\
             export type JoinCredentials = LoginCredentials;\n",
        )]);
        let hits = scan(
            "packages/sdk-core/src/session/events.ts",
            "export interface JoinOptions {\n  readonly credentials: JoinCredentials;\n}\n",
            &i,
        );
        assert!(hits.is_empty(), "interface member must not fire: {hits:?}");
    }

    /// Real site `CreateMeeting.svelte:16` — a `$props()` type literal. Props are
    /// transient; the annotation is a `{`-literal, not a bare type name.
    #[test]
    fn props_type_literal_is_not_a_retention_site() {
        let i = idx(&[(
            "packages/web-app/src/lib/types.ts",
            "export interface AuthResult {\n  readonly password: string;\n  readonly userToken: string;\n}\n",
        )]);
        let hits = scan(
            "packages/web-app/src/views/CreateMeeting.svelte",
            "<script lang=\"ts\">\n  let {\n    config,\n    auth,\n  }: {\n    config: DemoConfig;\n    auth: AuthResult;\n  } = $props();\n</script>\n",
            &i,
        );
        assert!(
            hits.is_empty(),
            "$props() annotation must not fire: {hits:?}"
        );
    }

    /// Real site `MeetingSession.ts:335` — a parameter in a multi-line signature,
    /// lexically indistinguishable from a field declaration. Without paren-depth
    /// tracking the guard red-lines `MeetingSession.join`'s own signature.
    #[test]
    fn multiline_function_parameter_is_not_a_retention_site() {
        let i = idx(&[(
            "packages/sdk-core/src/session/events.ts",
            "export interface LoginCredentials {\n  readonly password: string;\n}\n\
             export interface JoinOptions {\n  readonly credentials: LoginCredentials;\n}\n",
        )]);
        let hits = scan(
            "packages/sdk-core/src/session/MeetingSession.ts",
            "  async #joinSignaling(\n    signaling: SignalingClient,\n    options: JoinOptions,\n  ): Promise<JoinedEvent> {\n    return null;\n  }\n",
            &i,
        );
        assert!(
            hits.is_empty(),
            "multi-line parameter must not fire: {hits:?}"
        );
    }

    /// Limitation 2, pinned so a later author does not "fix" it into an FP generator
    /// on every login form.
    #[test]
    fn bare_string_form_state_is_not_detected() {
        let i = idx(&[("packages/x/src/a.ts", "export interface Unused {}\n")]);
        let hits = scan(
            "packages/web-app/src/views/SignIn.svelte",
            "<script lang=\"ts\">\n  let password = $state('');\n</script>\n",
            &i,
        );
        assert!(
            hits.is_empty(),
            "bare-string form state is out of scope: {hits:?}"
        );
    }

    /// Rule 2 is inherently cross-file. Scanned alone, the retention file must be
    /// silent — the declaration lives elsewhere. This is the F11 lock: a per-file
    /// kernel would make EVERY rule-2 assertion silent, i.e. a green matrix with the
    /// rule structurally disabled.
    #[test]
    fn cross_file_retention_requires_the_declaration_in_the_index() {
        let decl = (
            "packages/web-app/src/lib/types.ts",
            "export interface AuthResult {\n  readonly password: string;\n  readonly userToken: string;\n}\n",
        );
        let retention = (
            "packages/web-app/src/App.svelte",
            "<script lang=\"ts\">\n  let auth = $state<AuthResult | undefined>(undefined);\n</script>\n",
        );
        // Index built from the retention file ALONE: no declaration, so no finding.
        let lonely = idx(&[retention]);
        assert!(
            scan(retention.0, retention.1, &lonely).is_empty(),
            "retention file alone must be silent — the declaration is in another file"
        );
        // Index built from the SET: fires.
        let together = idx(&[decl, retention]);
        assert_eq!(scan(retention.0, retention.1, &together).len(), 1);
        // And the declaration file alone is silent (no retention site in it).
        assert!(scan(decl.0, decl.1, &together).is_empty());
    }

    #[test]
    fn svelte_markup_braces_do_not_corrupt_depth() {
        let i = idx(&[(
            "packages/x/src/a.ts",
            "export interface Creds {\n  readonly password: string;\n}\n",
        )]);
        let hits = scan(
            "packages/x/src/v.svelte",
            "<script lang=\"ts\">\n  let c = $state<Creds | undefined>(undefined);\n</script>\n\
             <ul>\n  {#each items as it}\n    <li>{it.name}</li>\n  {/each}\n</ul>\n",
            &i,
        );
        assert_eq!(
            hits.len(),
            1,
            "markup braces must not shift depth: {hits:?}"
        );
    }

    #[test]
    fn exclusion_filter_layers_on_shared_helper() {
        assert!(is_excluded_path(Path::new("packages/x/src/__tests__/a.ts")));
        assert!(is_excluded_path(Path::new("packages/x/node_modules/a.ts")));
        assert!(is_excluded_path(Path::new("packages/x/dist/a.js")));
        assert!(is_excluded_path(Path::new("packages/x/src/types.d.ts")));
        assert!(!is_excluded_path(Path::new("packages/x/src/index.ts")));
    }

    // --- F-SEC-5 / F-SEC-6 regression locks (@security, Gate 3) ----------------
    //
    // Every shape below was verified SILENT against the built binary before the fix.
    // The union-reorder case is the one that mattered most: reordering a union is
    // semantically a no-op, so it was an INVISIBLE BYPASS — with no `guard:ignore`
    // hatch, editing the line until it goes green is the natural response to a
    // finding, and a reorder does that while looking cosmetic and leaving no marker.

    fn creds_index() -> DeclIndex {
        idx(&[(
            "packages/x/src/types.ts",
            "export interface AuthResult {\n  readonly password: string;\n  readonly userToken: string;\n}\n",
        )])
    }

    #[test]
    fn union_order_does_not_change_detection() {
        let i = creds_index();
        for (label, src) in [
            (
                "credential first",
                "  let a = $state<AuthResult | undefined>(undefined);\n",
            ),
            (
                "credential second",
                "  let a = $state<undefined | AuthResult>(undefined);\n",
            ),
            (
                "three-arm",
                "  let a = $state<null | undefined | AuthResult>(undefined);\n",
            ),
        ] {
            assert_eq!(
                scan("packages/x/src/a.ts", src, &i).len(),
                1,
                "{label}: reordering a union must not change detection"
            );
        }
    }

    #[test]
    fn generic_wrappers_are_still_retention() {
        let i = creds_index();
        for (label, src) in [
            (
                "array shorthand",
                "  let a = $state<readonly AuthResult[]>([]);\n",
            ),
            ("Array<T>", "  let a = $state<Array<AuthResult>>([]);\n"),
            (
                "nested generic",
                "  let a = $state<Map<string, AuthResult>>(new Map());\n",
            ),
        ] {
            assert_eq!(scan("packages/x/src/a.ts", src, &i).len(), 1, "{label}");
        }
    }

    #[test]
    fn annotated_binding_and_class_field_see_the_whole_span() {
        let i = creds_index();
        assert_eq!(
            scan(
                "packages/x/src/a.ts",
                "let b: undefined | AuthResult = undefined;\n",
                &i
            )
            .len(),
            1,
            "module-scope binding must read the whole annotation"
        );
        assert_eq!(
            scan(
                "packages/x/src/a.ts",
                "export class H {\n  readonly #f: Map<string, AuthResult> = new Map();\n}\n",
                &i,
            )
            .len(),
            1,
            "class field must read the whole annotation"
        );
    }

    /// F-SEC-6: `class` was absent from the declaration matcher, so a class that IS a
    /// credential-bearing type was invisible — while phase 2 already treated class
    /// PROPERTIES as retention sites. Classes were storage but never storable.
    #[test]
    fn class_declarations_are_indexed() {
        let i = idx(&[(
            "packages/x/src/c.ts",
            "export class ClassCreds {\n  password: string = '';\n  userToken: string = '';\n}\n",
        )]);
        assert!(i.is_credential_bearing("ClassCreds"));
        assert_eq!(
            scan(
                "packages/x/src/a.ts",
                "  let a = $state<ClassCreds | undefined>(undefined);\n",
                &i
            )
            .len(),
            1,
            "a retained class instance must fire"
        );
    }

    /// The span widening must not swallow the exclusions it was measured against.
    #[test]
    fn span_widening_preserves_the_three_exclusions() {
        let i = idx(&[(
            "packages/x/src/types.ts",
            "export interface AuthResult {\n  readonly password: string;\n  readonly userToken: string;\n}\n",
        )]);
        assert!(
            scan(
                "packages/x/src/i.ts",
                "export interface Opts {\n  readonly a: AuthResult;\n}\n",
                &i
            )
            .is_empty(),
            "interface member"
        );
        assert!(
            scan(
                "packages/x/src/p.svelte",
                "<script lang=\"ts\">\n  let { a }: { a: AuthResult } = $props();\n</script>\n",
                &i,
            )
            .is_empty(),
            "$props() type literal"
        );
        assert!(
            scan(
                "packages/x/src/f.ts",
                "async function j(\n  s: string,\n  o: AuthResult,\n): Promise<void> {}\n",
                &i,
            )
            .is_empty(),
            "multi-line function parameter"
        );
    }

    /// Both FPs the F-SEC-6 fix introduced, caught by running the guard on the real tree
    /// rather than by reasoning about it. Locked so the next declaration kind added to
    /// the matcher has to satisfy them.
    #[test]
    fn class_bodies_are_code_not_type_syntax() {
        // An object literal inside a method is field-SHAPED without being a field.
        // Collecting at any depth (correct for interfaces) red-lined `AuthApiClient`.
        let i = idx(&[(
            "packages/x/src/client.ts",
            "export class ApiClient {\n  readonly #base: string;\n  async register(input: RegisterInput) {\n    return fetch('/x', {\n      body: JSON.stringify({\n        password: input.password,\n      }),\n    });\n  }\n}\n",
        )]);
        assert!(
            !i.is_credential_bearing("ApiClient"),
            "a class is not credential-bearing because a method body builds a request \
             containing a password"
        );
    }

    #[test]
    fn class_method_parameters_are_not_class_fields() {
        // A multi-line method signature puts `credentials: UserTokenCredentials,` on its
        // own line at brace-depth 1 — lexically identical to a field. This is the SAME
        // trap phase 2 handles for retention sites, reappearing in phase 1 the moment
        // classes entered the index.
        let i = idx(&[(
            "packages/x/src/client.ts",
            "export interface UserTokenCredentials {\n  readonly userToken: string;\n}\n\
             export interface Creds {\n  readonly password: string;\n}\n\
             export class ApiClient {\n  async createMeeting(\n    input: string,\n    credentials: Creds,\n  ): Promise<void> {}\n}\n",
        )]);
        assert!(
            !i.is_credential_bearing("ApiClient"),
            "method parameters in a multi-line signature must not become class fields"
        );
    }

    /// A declaration can open AND close on one line; the walker only inspected
    /// SUBSEQUENT lines, so these were never indexed at all.
    #[test]
    fn single_line_declarations_are_indexed() {
        let i = idx(&[(
            "packages/x/src/t.ts",
            "export interface OneLine { readonly password: string; readonly userToken: string; }\n",
        )]);
        assert!(i.is_credential_bearing("OneLine"));
        assert!(i.is_token_bearing("OneLine"));
    }

    /// The runaway backstop must still fire on genuinely degenerate input — retuning it
    /// for ordinary code must not disable it. Unbalanced braces would otherwise walk to
    /// EOF (or, in a real file, consume the rest of the source silently).
    #[test]
    fn unbalanced_braces_trip_the_runaway_backstop_rather_than_running_away() {
        let mut src = String::from("export interface Degenerate {\n  readonly password: string;\n");
        for i in 0..(MAX_DECL_BLOCK_LINES + 50) {
            src.push_str(&format!("  nested{i}: {{\n"));
        }
        // Never closed. The walk must terminate via the cap, not consume unbounded.
        let i = idx(&[("packages/x/src/degenerate.ts", &src)]);
        assert!(
            i.is_credential_bearing("Degenerate"),
            "fields seen before the cap must still be collected"
        );
    }

    /// The tuning rule as an executable check, MEASURED FROM THE TREE.
    ///
    /// The first version of this test compared two literals in this file
    /// (`MAX_DECL_BLOCK_LINES >= 468 * 3`) and would have evaluated `2000 >= 1404`
    /// forever, whatever `packages/**` did — so a class growing past the cap would
    /// leave the test green, the WARN firing, and coverage silently reduced: exactly
    /// the failure it was written to prevent (@paired-infrastructure F-INFRA-4).
    ///
    /// It was the third instance in this loop of *a value that must correspond to
    /// something else, with nothing asserting the correspondence* — and the one where
    /// staleness was guaranteed rather than merely possible, because the tree changes
    /// without anyone touching this file.
    ///
    /// The cross-tree coupling is the MECHANISM, not a side effect: writing a
    /// 700-line class in `packages/` should fail here, at build time, with a message
    /// naming the cause — before any WARN can reach a green run.
    #[test]
    fn cap_clears_the_largest_real_declaration_with_margin() {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repo root is two levels above the crate manifest")
            .to_path_buf();
        let files = load_in_scope_files(&repo_root).expect("collect in-scope client sources");
        assert!(
            !files.is_empty(),
            "no in-scope files found under {} — the measurement would be vacuous",
            repo_root.display()
        );

        let (path, name, span) =
            largest_declaration_span(&files).expect("at least one declaration in packages/**");
        assert!(
            MAX_DECL_BLOCK_LINES >= span * 3,
            "MAX_DECL_BLOCK_LINES ({MAX_DECL_BLOCK_LINES}) no longer clears the largest \
             real declaration with margin: `{name}` in {} spans {span} lines. At parity \
             the cap stops bounding pathology and starts silently reducing coverage, and \
             the truncation WARN decays into noise on every green run. Raise the cap (and \
             the measured figure in its doc comment), or split the declaration.",
            path.display()
        );
    }
}
