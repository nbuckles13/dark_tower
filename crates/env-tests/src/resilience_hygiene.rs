//! Resilience-leg hygiene, checked at Layer 4 (this module is `#[cfg(test)]`
//! in the lib target, so it runs without any env-test feature; the binaries it
//! inspects are feature-gated and never compiled by the scan).
//!
//! # 1. Pool-mutating test binaries serialize EVERY test
//!
//! Story 4 has no per-test isolation of the MH pool: it relies on
//! serialization. `cargo test` runs test binaries one at a time, and
//! `serial_test` serializes only MARKED tests within a binary. So an unmarked
//! test in a binary that kills, freezes or drains an MH could run concurrently
//! with the mutation and observe it. The rule: in `tests/40_resilience.rs` (the
//! home of every destructive leg) and in every other `tests/*.rs` that calls a
//! pool-mutating fixture API, every `#[test]` / `#[tokio::test]` also carries
//! a BARE `#[serial]` (one group: a keyed `#[serial(k)]` is a separate group
//! in serial_test and runs concurrently with the others, so it is refused).
//!
//! The pool-mutating API set is DERIVED, not listed: every pub fn in
//! `src/fixtures/mh_pool.rs` whose doc starts "Pool-mutating." So a new
//! mutating verb cannot escape. The scan is token-level (`syn`): attribute
//! order, doc comments between attributes and `#[serial_test::serial]` are
//! handled; a keyed `#[serial(group)]` is flagged.
//!
//! # 2. No forbidden MH kill path in test code
//!
//! ADR-0039 §9: never `kubectl delete pod --force --grace-period=0` (it defeats
//! the hostPort lock), and never delete an MH pod at all: a crash is
//! `mh.sh signal KILL`, a planned removal `mh.sh drain`. This scans the STRING
//! LITERALS (argv is built from literals) of env-tests and the browser
//! harness. Comments may name the forbidden command to forbid it. mh.sh itself
//! is covered behaviourally by its argv-recording stub kubectl in
//! `infra/kind/scripts/mh.test.sh` (its comments contain the needles, so a text
//! scan cannot cover it). The needles are built with `concat!` so this file
//! cannot match itself, and this file is excluded from the scan by path.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// The doc marker that puts a pub fn in the pool-mutating API set.
const MUTATING_MARKER: &str = "Pool-mutating.";

/// The home of every destructive leg; always scanned.
const RESILIENCE_BINARY: &str = "40_resilience.rs";

/// Files allowed to delete a pod: canary.rs deletes its OWN canary pods
/// (never an MH), gracefully, and asserts them gone.
const POD_DELETE_ALLOWED: &[&str] = &["src/canary.rs"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn parse(path: &Path) -> syn::File {
    let src =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    syn::parse_file(&src).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn doc_text(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn last_segment_is(attr: &syn::Attribute, name: &str) -> bool {
    attr.path().segments.last().is_some_and(|s| s.ident == name)
}

/// PURE: the pool-mutating API of a fixture source (pub fns, free or in an
/// impl, whose doc starts with [`MUTATING_MARKER`]).
fn mutating_api(file: &syn::File) -> BTreeSet<String> {
    struct V(BTreeSet<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
            if matches!(f.vis, syn::Visibility::Public(_))
                && doc_text(&f.attrs).trim_start().starts_with(MUTATING_MARKER)
            {
                self.0.insert(f.sig.ident.to_string());
            }
        }
        fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
            if matches!(f.vis, syn::Visibility::Public(_))
                && doc_text(&f.attrs).trim_start().starts_with(MUTATING_MARKER)
            {
                self.0.insert(f.sig.ident.to_string());
            }
        }
    }
    let mut v = V(BTreeSet::new());
    v.visit_file(file);
    v.0
}

/// PURE: every test fn (any attribute whose last path segment is `test`) and
/// whether it is serialized (an attribute whose last segment is `serial`).
fn test_fns(file: &syn::File) -> Vec<(String, bool)> {
    struct V(Vec<(String, bool)>);
    impl<'ast> Visit<'ast> for V {
        fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
            if f.attrs.iter().any(|a| last_segment_is(a, "test")) {
                // BARE `#[serial]` only: in serial_test, a key defines a
                // SEPARATE group (`#[serial(k)]` runs concurrently with a bare
                // `#[serial]`), so a keyed attribute does not serialize
                // against the rest of the binary.
                let serial = f
                    .attrs
                    .iter()
                    .any(|a| last_segment_is(a, "serial") && matches!(a.meta, syn::Meta::Path(_)));
                self.0.push((f.sig.ident.to_string(), serial));
            }
            syn::visit::visit_item_fn(self, f);
        }
    }
    let mut v = V(Vec::new());
    v.visit_file(file);
    v.0
}

/// PURE: does `file` name the pool fixture's handle types AND call one of
/// `api` (as a method or a path)?
fn calls_mutating(file: &syn::File, api: &BTreeSet<String>) -> bool {
    struct V<'a> {
        api: &'a BTreeSet<String>,
        names_pool: bool,
        calls: bool,
    }
    impl<'ast> Visit<'ast> for V<'_> {
        fn visit_ident(&mut self, i: &'ast syn::Ident) {
            let s = i.to_string();
            if s == "MhPool" || s == "MhSlot" || s == "FreezeGuard" {
                self.names_pool = true;
            }
        }
        fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
            if self.api.contains(&m.method.to_string()) {
                self.calls = true;
            }
            syn::visit::visit_expr_method_call(self, m);
        }
        fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
            if p.path
                .segments
                .last()
                .is_some_and(|s| self.api.contains(&s.ident.to_string()))
            {
                self.calls = true;
            }
            syn::visit::visit_expr_path(self, p);
        }
    }
    let mut v = V {
        api,
        names_pool: false,
        calls: false,
    };
    v.visit_file(file);
    v.names_pool && v.calls
}

/// PURE: the unserialized tests of a pool-mutating binary.
fn unserialized(file: &syn::File) -> Vec<String> {
    test_fns(file)
        .into_iter()
        .filter(|(_, serial)| !serial)
        .map(|(name, _)| name)
        .collect()
}

fn api_on_tree() -> BTreeSet<String> {
    mutating_api(&parse(&crate_dir().join("src/fixtures/mh_pool.rs")))
}

/// Every pool-mutating binary in `tests/` (always including
/// [`RESILIENCE_BINARY`]), by file name.
fn pool_mutating_binaries(api: &BTreeSet<String>) -> Vec<(String, syn::File)> {
    let mut out = Vec::new();
    let dir = crate_dir().join("tests");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let file = parse(&path);
        if name == RESILIENCE_BINARY || calls_mutating(&file, api) {
            out.push((name, file));
        }
    }
    out
}

// --- forbidden kill paths -------------------------------------------------

const NEEDLE_GRACE: &str = concat!("grace", "-period");
const NEEDLE_FORCE: &str = concat!("--", "force");
const POD_WORDS: &[&str] = &["po", "pod", "pods"];

fn is_pod_word(s: &str) -> bool {
    POD_WORDS.contains(&s) || POD_WORDS.iter().any(|w| s.starts_with(&format!("{w}/")))
}

/// PURE: violations in an ordered list of string literals (argv is built from
/// literals, so a `["delete", "pod", ..]` sequence or a `"delete pod .."`
/// literal is how a pod delete would appear). `allow_pod_delete` exempts the
/// graceful-pod-delete rule only; force/grace flags are never allowed.
fn literal_violations(literals: &[String], allow_pod_delete: bool) -> Vec<String> {
    let mut out = Vec::new();
    for (i, lit) in literals.iter().enumerate() {
        if lit.contains(NEEDLE_GRACE) || lit.split_whitespace().any(|w| w == NEEDLE_FORCE) {
            out.push(format!("forbidden kubectl flag in literal {lit:?}"));
        }
        if allow_pod_delete {
            continue;
        }
        let words: Vec<&str> = lit.split_whitespace().collect();
        let inline = words
            .windows(2)
            .any(|w| w[0] == "delete" && is_pod_word(w[1]));
        let sequence = lit == "delete" && literals.get(i + 1).is_some_and(|n| is_pod_word(n));
        if inline || sequence {
            out.push(format!("pod delete in literal {lit:?}"));
        }
    }
    out
}

fn rust_literals(file: &syn::File) -> Vec<String> {
    struct V(Vec<String>);
    impl<'ast> Visit<'ast> for V {
        fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
            self.0.push(s.value());
        }
        // Doc comments are attributes, not code: skip them.
        fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}
    }
    let mut v = V(Vec::new());
    v.visit_file(file);
    v.0
}

/// PURE: the string literals of a TS source (single, double and template
/// quotes; `//` and `/* */` comments skipped).
fn ts_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            q @ (b'\'' | b'"' | b'`') => {
                let start = i + 1;
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                out.push(String::from_utf8_lossy(&b[start..i.min(b.len())]).into_owned());
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let p = e.expect("dir entry").path();
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(src: &str) -> syn::File {
        syn::parse_file(src).expect("fixture parses")
    }

    #[test]
    fn serial_scan_positive_and_negative_controls() {
        let f = file(
            r#"
            #[tokio::test]
            async fn unmarked() {}

            /// doc between
            #[serial]
            /// more doc
            #[tokio::test(flavor = "multi_thread")]
            async fn reordered() {}

            #[test]
            #[serial_test::serial]
            fn pathed() {}

            #[tokio::test]
            #[serial(mh_pool)]
            async fn grouped() {}

            mod nested {
                #[test]
                fn nested_unmarked() {}
            }

            fn helper() {}
            "#,
        );
        assert_eq!(test_fns(&f).len(), 5, "helper is not a test");
        // `grouped` is a separate serial_test group: concurrent with the rest.
        assert_eq!(
            unserialized(&f),
            vec!["unmarked", "grouped", "nested_unmarked"]
        );
        // Mixed keys in one binary: the keyed one is flagged, the bare passes.
        let mixed = file(
            "#[test] #[serial] fn a() {} #[test] #[serial(mh)] fn b() {} \
             #[test] #[serial_test::serial(other)] fn c() {}",
        );
        assert_eq!(unserialized(&mixed), vec!["b", "c"]);
    }

    #[test]
    fn mutating_api_is_derived_from_the_doc_marker() {
        let f = file(
            r#"
            impl P {
                /// Pool-mutating. Brings one up.
                pub fn up(&self) {}
                /// Reads only.
                pub fn snapshot(&self) {}
                /// Pool-mutating. But private.
                fn hidden(&self) {}
            }
            /// Pool-mutating. Free fn.
            pub fn free() {}
            "#,
        );
        assert_eq!(
            mutating_api(&f),
            ["free", "up"].iter().map(|s| (*s).to_string()).collect()
        );
        // The real fixture: vacuity control, the verbs the legs use are in it.
        let api = api_on_tree();
        for verb in [
            "up",
            "up_scraped",
            "drain",
            "drain_no_wait",
            "kill",
            "freeze",
            "resume",
            "teardown",
        ] {
            assert!(
                api.contains(verb),
                "mh_pool.rs lost the Pool-mutating marker on {verb}: {api:?}"
            );
        }
        assert!(!api.contains("snapshot"), "a read is not a mutation");
    }

    #[test]
    fn calls_mutating_needs_the_pool_types_and_a_mutating_call() {
        let api: BTreeSet<String> = ["up", "drain"].iter().map(|s| (*s).to_string()).collect();
        assert!(calls_mutating(
            &file("fn f() { let p = MhPool::for_run(); p.up(); }"),
            &api
        ));
        assert!(!calls_mutating(
            &file("fn f() { let p = MhPool::for_run(); p.snapshot(); }"),
            &api
        ));
        assert!(
            !calls_mutating(&file("fn f() { thing.up(); }"), &api),
            "no pool type named"
        );
    }

    /// THE rule on the real tree.
    #[test]
    fn every_test_in_a_pool_mutating_binary_is_serial() {
        let api = api_on_tree();
        let bins = pool_mutating_binaries(&api);
        let names: Vec<&str> = bins.iter().map(|(n, _)| n.as_str()).collect();
        // Vacuity: discovery finds the resilience binary AND 44 (which `up`s),
        // so a renamed API cannot silently empty the set.
        for must in [RESILIENCE_BINARY, "44_gc_mh_registration.rs"] {
            assert!(
                names.contains(&must),
                "pool-mutating discovery missed {must}: {names:?}"
            );
        }
        let resilience = &bins.iter().find(|(n, _)| n == RESILIENCE_BINARY).unwrap().1;
        assert!(
            test_fns(resilience).len() >= 3,
            "vacuity: {RESILIENCE_BINARY} should hold its legs"
        );
        let offenders: Vec<String> = bins
            .iter()
            .flat_map(|(n, f)| {
                unserialized(f)
                    .into_iter()
                    .map(move |t| format!("{n}::{t}"))
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "tests in pool-mutating binaries without #[serial] (serial_test serializes only \
             marked tests, so these could run concurrently with a killed/frozen/drained MH): \
             {offenders:?}"
        );
    }

    #[test]
    fn literal_scan_positive_controls() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        let force_grace = format!("{}=0", NEEDLE_GRACE);
        assert!(literal_violations(&s(&["delete", "deployment", "mh-s1"]), false).is_empty());
        assert_eq!(
            literal_violations(&s(&["delete", "pod", "x"]), false).len(),
            1
        );
        assert_eq!(literal_violations(&s(&["delete", "po/x"]), false).len(), 1);
        assert_eq!(
            literal_violations(&s(&["kubectl delete pods -l a=b"]), false).len(),
            1
        );
        assert_eq!(
            literal_violations(&s(&[&format!("--{force_grace}")]), true).len(),
            1
        );
        assert_eq!(literal_violations(&s(&[NEEDLE_FORCE]), true).len(), 1);
        assert!(
            literal_violations(&s(&["delete", "pod", "x"]), true).is_empty(),
            "allowlisted"
        );
        assert!(literal_violations(&s(&["drain", "--all", "--owner", "o"]), false).is_empty());
        assert!(literal_violations(&s(&["--force-conflicts-not-a-word"]), false).is_empty());
        // TS literal extraction skips comments and reads every quote style.
        let ts = "// kubectl delete pod x\n/* delete pod */ run('delete', \"pod\", `y`);";
        assert_eq!(ts_literals(ts), vec!["delete", "pod", "y"]);
        assert_eq!(literal_violations(&ts_literals(ts), false).len(), 1);
    }

    #[test]
    fn no_forbidden_mh_kill_path_in_test_code() {
        let root = crate_dir();
        let me = root.join("src/resilience_hygiene.rs");
        let mut rust = Vec::new();
        walk(&root.join("src"), "rs", &mut rust);
        walk(&root.join("tests"), "rs", &mut rust);
        let mut violations = Vec::new();
        let mut scanned = 0;
        for p in rust.into_iter().filter(|p| *p != me) {
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let allow = POD_DELETE_ALLOWED.contains(&rel.as_str());
            for v in literal_violations(&rust_literals(&parse(&p)), allow) {
                violations.push(format!("crates/env-tests/{rel}: {v}"));
            }
            scanned += 1;
        }
        let e2e = crate::repo_root().join("packages/web-app/e2e");
        let mut ts = Vec::new();
        walk(&e2e, "ts", &mut ts);
        for p in &ts {
            let src = std::fs::read_to_string(p).expect("read ts");
            for v in literal_violations(&ts_literals(&src), false) {
                violations.push(format!("{}: {v}", p.display()));
            }
            scanned += 1;
        }
        assert!(scanned > 50, "vacuity: only {scanned} files scanned");
        assert!(
            violations.is_empty(),
            "forbidden MH kill path (ADR-0039 §9: use `mh.sh signal KILL` / `mh.sh drain`, \
             never a pod delete or a force/zero-grace flag): {violations:?}"
        );
    }
}
