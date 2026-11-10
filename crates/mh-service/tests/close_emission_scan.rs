//! Source-level guards for the ADR-0039 §7 close contract in mh-service.
//!
//! Behavioural tests prove the paths that exist close correctly; these prove
//! no NEW path can bypass the one emission site:
//!
//! 1. **One emitter.** Only `src/webtransport/close.rs` may close a session, or
//!    close anything with an argument (a code), and it makes exactly one
//!    session-close call.
//! 2. **Code-less ends route through `end_without_code`.** No source drops a
//!    live connection by hand (a drop can FIN the CONNECT stream, which a
//!    browser reads as a clean close with code 0).
//! 3. **The peer's account never feeds MH.** Production code never calls the
//!    vendored test accessor `session_close_received` or the capsule encoder
//!    (security P5): how the PEER says a session ended must not drive an MH
//!    decision, and the encoder is the transport's, not MH's.
//!
//! Every scan walks `src/` with `read_dir` (no hand-listed files, so a new file
//! cannot escape it), skips comment lines and ONLY the bodies of
//! `#[cfg(test)]`-gated `mod` blocks (a `#[cfg(test)]` helper fn mid-file does
//! not hide the production code after it), and carries positive controls so an
//! empty or mis-rooted scan fails with its own message rather than passing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

const EMITTER: &str = "src/webtransport/close.rs";
const CONNECTION: &str = "src/webtransport/connection.rs";

/// The session-close call into the transport.
const SESSION_CLOSE_CALL: &str = ".close_session(";
/// A close with ANY argument: `.close(` not immediately followed by `)`. A
/// no-argument `downlinks.close()` is not matched; a rustfmt-wrapped `.close(`
/// at end of line is.
const ARGUMENTED_CLOSE: &str = ".close(";
/// Never in production source.
const FORBIDDEN_IN_SRC: &[&str] = &["session_close_received", "encode_close_session_capsule"];

fn src_files() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src/ is readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let rel = path
                    .strip_prefix(root)
                    .expect("under the crate root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).expect("source is readable");
                out.push((rel, text));
            }
        }
    }
    out.sort();
    out
}

/// Production, non-comment lines: everything EXCEPT the brace-matched body of a
/// `#[cfg(test)]`-gated `mod`.
fn code_lines(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        if line.trim() == "#[cfg(test)]" {
            let mut k = i + 1;
            while lines
                .get(k)
                .is_some_and(|l| l.trim_start().starts_with("#["))
            {
                k += 1;
            }
            let item = lines.get(k).map_or("", |l| l.trim_start());
            if (item.starts_with("mod ") || item.starts_with("pub mod ")) && item.ends_with('{') {
                let mut depth = 0_usize;
                let mut m = k;
                while let Some(l) = lines.get(m) {
                    depth += l.matches('{').count();
                    depth = depth.saturating_sub(l.matches('}').count());
                    m += 1;
                    if depth == 0 {
                        break;
                    }
                }
                i = m;
                continue;
            }
        }
        if !line.trim_start().starts_with("//") {
            out.push(*line);
        }
        i += 1;
    }
    out
}

fn count(text: &str, needle: &str) -> usize {
    code_lines(text)
        .iter()
        .map(|l| l.matches(needle).count())
        .sum()
}

/// Argumented `.close(` occurrences.
fn argumented_closes(text: &str) -> usize {
    code_lines(text)
        .iter()
        .map(|l| {
            l.match_indices(ARGUMENTED_CLOSE)
                .filter(|(at, _)| {
                    !l.get(at + ARGUMENTED_CLOSE.len()..)
                        .is_some_and(|rest| rest.starts_with(')'))
                })
                .count()
        })
        .sum()
}

fn find<'a>(files: &'a [(String, String)], rel: &str) -> &'a str {
    let found = files.iter().find(|(p, _)| p == rel);
    assert!(
        found.is_some(),
        "INPUT ABSENT: {rel} not found by the scan — mis-rooted walk"
    );
    &found.unwrap().1
}

#[test]
fn the_scan_sees_the_tree() {
    let files = src_files();
    assert!(
        files.len() > 20,
        "VACUOUS SCAN: only {} .rs files under src/ — the walk is broken, not the code",
        files.len()
    );
    find(&files, CONNECTION);
    find(&files, EMITTER);
}

/// The scope rule itself: a `#[cfg(test)]` helper fn mid-file does NOT hide the
/// code after it; a `#[cfg(test)] mod` body IS skipped; argumented closes are
/// matched (including a wrapped one), no-argument closes are not.
#[test]
fn the_scope_and_needle_rules_hold_on_synthetic_files() {
    let helper_then_call =
        "fn a() {}\n#[cfg(test)]\nfn helper() {}\nfn b() { c.session_close_received(); }\n";
    assert_eq!(count(helper_then_call, FORBIDDEN_IN_SRC[0]), 1);
    let test_mod = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() { c.session_close_received(); }\n}\nfn b() { c.session_close_received(); }\n";
    assert_eq!(
        count(test_mod, FORBIDDEN_IN_SRC[0]),
        1,
        "only the call after the test mod counts"
    );
    assert_eq!(argumented_closes("x.close(code, b\"\");"), 1);
    assert_eq!(argumented_closes("x.close(\n    code,\n);"), 1);
    assert_eq!(argumented_closes("downlinks.close();"), 0);
    assert_eq!(
        count("// c.session_close_received()", FORBIDDEN_IN_SRC[0]),
        0
    );
}

#[test]
fn only_close_rs_closes_a_session_or_closes_with_an_argument() {
    let files = src_files();
    // Positive controls: the matchers hit the real emitter.
    assert_eq!(
        count(find(&files, EMITTER), SESSION_CLOSE_CALL),
        1,
        "close.rs must make exactly ONE session-close call (the emitter)"
    );
    assert!(
        argumented_closes(find(&files, EMITTER)) > 0,
        "positive control: the argumented-close matcher must hit close.rs's own QUIC closes"
    );
    for (rel, text) in &files {
        if rel == EMITTER {
            continue;
        }
        assert_eq!(
            count(text, SESSION_CLOSE_CALL),
            0,
            "{rel}: a session close outside close.rs bypasses the one mapping"
        );
        assert_eq!(
            argumented_closes(text),
            0,
            "{rel}: a close with an argument (a code) outside close.rs"
        );
    }
}

#[test]
fn every_codeless_end_routes_through_end_without_code() {
    let files = src_files();
    let connection = find(&files, CONNECTION);
    // The wrapper maps the code-less end to the one code-less closer.
    assert_eq!(
        count(
            connection,
            "SessionEnd::WithoutCode => close::end_without_code("
        ),
        1,
        "connection.rs must route SessionEnd::WithoutCode through close::end_without_code"
    );
    // Positive control for the drop needle on a synthetic line.
    assert_eq!(count("    drop(connection);", "drop(connection"), 1);
    for (rel, text) in &files {
        if !rel.starts_with("src/webtransport/") {
            continue;
        }
        assert_eq!(
            count(text, "drop(connection"),
            0,
            "{rel}: a hand-dropped connection can FIN the CONNECT stream — end it with \
             close::end_without_code"
        );
    }
}

#[test]
fn production_code_never_reads_the_peers_close_or_encodes_capsules() {
    let files = src_files();
    for (rel, text) in &files {
        for needle in FORBIDDEN_IN_SRC {
            assert_eq!(
                count(text, needle),
                0,
                "{rel}: `{needle}` is a test/transport surface; MH decisions must never read the \
                 peer's account of a close"
            );
        }
    }
}
