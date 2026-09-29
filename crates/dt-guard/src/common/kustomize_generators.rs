//! The ONE structured reader of kustomize `configMapGenerator` entries.
//!
//! ADR-0038 §2 makes every pod-consumed ConfigMap a generator, so four
//! policies need to know what a kustomization generates: `kustomize` R-16
//! (a generator source is a declared file, not an orphan), R-21 (a
//! hash-suffixed reference must name a generator that exists), `env-config`
//! (a `configMapKeyRef` resolves against the generator's keys), and the
//! `configmap_annotation_size` rule in `kustomize_configmaps` (which measures
//! the generated DATA). They share this parser, this env-file reader and this
//! source resolver ([`generator_data`], containment-gated) rather than each
//! growing its own — the size rule's line-oriented parser was folded in here
//! (ADR-0038 devloop 2).
//!
//! It is a real YAML parse, deliberately NOT the line scanner
//! `kustomize.rs::extract_declared_generator_files`: that scanner keeps only
//! `- ` bullets containing a `/`, so it cannot see `envs: [config.env]`, has no
//! notion of `literals:`, and cannot say which generator owns which file. It
//! stays for R-20 / alert-rules, whose basename-only question it answers.
//!
//! # Env-file contract (kustomize reads `envs:` files literally)
//!
//! `KEY=VALUE`, one per line; blank lines and `#` comment lines are skipped.
//! Kustomize keeps the value byte-for-byte, so the reader REJECTS the shapes
//! that silently change a value when a YAML `KEY: "v"` is transcribed:
//! a quote-wrapped value (`KEY=""` is the two-character string `""`, not empty
//! — for `CORS_ALLOWED_ORIGINS` that is a non-empty allowlist in the fail-closed
//! prod base), leading/trailing whitespace in the value, and any line that is
//! not a comment, blank, or `KEY=VALUE`. Every rejection is a hard error, never
//! a skip.

use anyhow::{bail, Context, Result};
use serde_norway::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::common::path_safety::resolve_cited_path;

/// One source file of a `files:` entry: `key=path` or bare `path` (key =
/// basename).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorFile {
    pub key: String,
    pub path: String,
}

/// One `configMapGenerator` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorEntry {
    pub name: String,
    pub namespace: Option<String>,
    /// `create` (the default), `merge` or `replace`.
    pub behavior: Option<String>,
    pub envs: Vec<String>,
    pub files: Vec<GeneratorFile>,
    pub literals: Vec<String>,
}

impl GeneratorEntry {
    /// The entry's `literals:` as `(key, value)`, through the same
    /// `KEY=VALUE` contract as env files.
    pub fn literal_pairs(&self) -> Result<Vec<(String, String)>> {
        self.literals
            .iter()
            .map(|lit| {
                let origin = || format!("configMapGenerator {:?} literal", self.name);
                checked_pair(lit, &origin).map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect()
    }

    /// Every file this entry reads, relative to its kustomization directory.
    pub fn source_paths(&self) -> impl Iterator<Item = &str> {
        self.envs
            .iter()
            .map(String::as_str)
            .chain(self.files.iter().map(|f| f.path.as_str()))
    }
}

fn string_list(entry: &Value, field: &str, name: &str) -> Result<Vec<String>> {
    match entry.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Sequence(seq)) => seq
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).with_context(|| {
                    format!("configMapGenerator {name:?}: `{field}` entry is not a string")
                })
            })
            .collect(),
        Some(_) => bail!("configMapGenerator {name:?}: `{field}` is not a list"),
    }
}

/// Parse every `configMapGenerator` entry of one kustomization file.
///
/// A malformed entry (no `name`, a non-list field, a non-string element) is an
/// error, never a skip: a generator this parser cannot read is a ConfigMap the
/// consuming policies would otherwise silently not see.
pub fn parse_config_map_generators(kustomization_yaml: &str) -> Result<Vec<GeneratorEntry>> {
    let doc: Value =
        serde_norway::from_str(kustomization_yaml).context("parsing kustomization YAML")?;
    config_map_generators(&doc)
}

/// [`parse_config_map_generators`] over an already-parsed kustomization.
pub fn config_map_generators(doc: &Value) -> Result<Vec<GeneratorEntry>> {
    let Some(gens) = doc.get("configMapGenerator") else {
        return Ok(Vec::new());
    };
    let Some(seq) = gens.as_sequence() else {
        bail!("`configMapGenerator` is not a list");
    };
    let mut out = Vec::with_capacity(seq.len());
    for entry in seq {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .context("configMapGenerator entry has no string `name`")?
            .to_string();
        let files = string_list(entry, "files", &name)?
            .into_iter()
            .map(|spec| match spec.split_once('=') {
                Some((key, path)) => GeneratorFile {
                    key: key.to_string(),
                    path: path.to_string(),
                },
                None => GeneratorFile {
                    key: spec.rsplit('/').next().unwrap_or(&spec).to_string(),
                    path: spec,
                },
            })
            .collect();
        out.push(GeneratorEntry {
            namespace: entry
                .get("namespace")
                .and_then(Value::as_str)
                .map(str::to_string),
            behavior: entry
                .get("behavior")
                .and_then(Value::as_str)
                .map(str::to_string),
            envs: string_list(entry, "envs", &name)?,
            literals: string_list(entry, "literals", &name)?,
            files,
            name,
        });
    }
    Ok(out)
}

/// Is `s` a valid env-file / literal key? (kustomize's own rule is looser; a
/// ConfigMap key here is always an env var name or a file name.)
fn is_key(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// Validate one `KEY=VALUE` pair against the literal-value contract in the
/// module doc; return the key and the value (split at the FIRST `=`, so a value
/// may itself contain `=`).
fn checked_pair<'a>(pair: &'a str, origin: &dyn Fn() -> String) -> Result<(&'a str, &'a str)> {
    let Some((key, value)) = pair.split_once('=') else {
        bail!("{}: not KEY=VALUE: {pair:?}", origin());
    };
    if !is_key(key) {
        bail!("{}: invalid key {key:?}", origin());
    }
    let quote_wrapped = ['"', '\'']
        .iter()
        .any(|&q| value.len() >= 2 && value.starts_with(q) && value.ends_with(q));
    if quote_wrapped {
        bail!(
            "{}: value of {key} is quote-wrapped ({value}); kustomize keeps quotes literally — write `{key}=` for empty, unquoted otherwise",
            origin()
        );
    }
    if value != value.trim() {
        bail!(
            "{}: value of {key} has leading/trailing whitespace, which kustomize keeps",
            origin()
        );
    }
    Ok((key, value))
}

/// `(key, value)` pairs of an `envs:` file, validated per the module-doc
/// contract. THE env-file reader: [`read_env_file_keys`] projects from it, and
/// the annotation-size rule measures its values.
pub fn read_env_file(content: &str, origin: &str) -> Result<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let at = || format!("{origin}:{}", i + 1);
        let (k, v) = checked_pair(line, &at)?;
        pairs.push((k.to_string(), v.to_string()));
    }
    Ok(pairs)
}

/// Keys of an `envs:` file, validated per the module-doc contract.
pub fn read_env_file_keys(content: &str, origin: &str) -> Result<Vec<String>> {
    Ok(read_env_file(content, origin)?
        .into_iter()
        .map(|(k, _)| k)
        .collect())
}

/// Resolve one YAML-supplied source path, relative to the kustomization's own
/// directory, through the repository's containment gate.
///
/// `files:` / `envs:` entries are attacker-shaped input: `dir.join(path)` alone
/// follows `../` out of the tree, and an ABSOLUTE entry replaces the base
/// outright. Containment is NOT re-implemented here — the joined path is handed
/// to [`resolve_cited_path`], the single SoT. Fail CLOSED: an escape is an
/// error, never a skip. The two failures are reported DIFFERENTLY because the
/// responses differ (fix a typo vs stop citing outside the repo); they are
/// told apart by whether the joined path canonicalizes at all. Messages quote
/// the path AS WRITTEN, never a canonicalized out-of-tree absolute path.
///
/// `what` qualifies the message (`""` for `files:`, `"env file "` for `envs:`).
fn resolve_source(
    repo_root: &Path,
    kust_dir: &Path,
    gen_name: &str,
    what: &str,
    path: &str,
) -> Result<PathBuf> {
    let joined = kust_dir.join(path);
    if let Some(resolved) = resolve_cited_path(repo_root, &joined.to_string_lossy()) {
        return Ok(resolved);
    }
    if std::fs::canonicalize(&joined).is_ok() {
        bail!(
            "configMapGenerator `{gen_name}` references {what}`{path}`, which resolves outside \
             the repository, so it cannot be read. Generator sources must stay inside the repo \
             (kustomize itself refuses paths outside its root)."
        );
    }
    bail!("configMapGenerator `{gen_name}` references {what}`{path}` but it cannot be read")
}

/// The data an entry generates — `key -> value`, exactly as kustomize would
/// build it — reading its sources from `kust_dir` through the containment gate.
/// An unreadable, escaping or contract-violating source is an error.
pub fn generator_data(
    repo_root: &Path,
    kust_dir: &Path,
    entry: &GeneratorEntry,
) -> Result<BTreeMap<String, String>> {
    let mut data = BTreeMap::new();
    for env in &entry.envs {
        let path = resolve_source(repo_root, kust_dir, &entry.name, "env file ", env)?;
        let content = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "configMapGenerator `{}` references env file `{env}` but it cannot be read",
                entry.name
            )
        })?;
        data.extend(read_env_file(&content, env)?);
    }
    for f in &entry.files {
        let path = resolve_source(repo_root, kust_dir, &entry.name, "", &f.path)?;
        let content = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "configMapGenerator `{}` references `{}` but it cannot be read",
                entry.name, f.path
            )
        })?;
        data.insert(f.key.clone(), content);
    }
    for (k, v) in entry.literal_pairs()? {
        data.insert(k, v);
    }
    Ok(data)
}

/// The data keys an entry generates — a projection of [`generator_data`], so
/// every consumer resolves sources through the same containment gate.
pub fn generator_keys(
    repo_root: &Path,
    kust_dir: &Path,
    entry: &GeneratorEntry,
) -> Result<BTreeSet<String>> {
    Ok(generator_data(repo_root, kust_dir, entry)?
        .into_keys()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_source_shape() {
        let y = r"
configMapGenerator:
  - name: a
    namespace: ns
    envs: [config.env]
    options:
      labels: {x: y}
  - name: b
    files:
      - config.yaml=collector.yaml
      - sub/redis.conf
    literals:
      - K=v
    behavior: merge
";
        let g = parse_config_map_generators(y).expect("parse");
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].name, "a");
        assert_eq!(g[0].namespace.as_deref(), Some("ns"));
        assert_eq!(g[0].envs, vec!["config.env"]);
        assert_eq!(
            g[1].files,
            vec![
                GeneratorFile {
                    key: "config.yaml".into(),
                    path: "collector.yaml".into()
                },
                GeneratorFile {
                    key: "redis.conf".into(),
                    path: "sub/redis.conf".into()
                },
            ]
        );
        assert_eq!(g[1].literals, vec!["K=v"]);
        assert_eq!(g[1].behavior.as_deref(), Some("merge"));
    }

    #[test]
    fn no_generator_is_empty_not_error() {
        assert!(parse_config_map_generators("resources: [a.yaml]\n")
            .expect("parse")
            .is_empty());
    }

    #[test]
    fn malformed_entries_are_errors() {
        assert!(parse_config_map_generators("configMapGenerator: {}\n").is_err());
        assert!(parse_config_map_generators("configMapGenerator:\n  - envs: [a]\n").is_err());
        assert!(
            parse_config_map_generators("configMapGenerator:\n  - name: a\n    envs: x\n").is_err()
        );
    }

    #[test]
    fn env_reader_keys_and_skips() {
        let k =
            read_env_file_keys("# c\n\nA=1\nEMPTY=\nURL=https://x:1/a#frag\n", "t").expect("read");
        assert_eq!(k, vec!["A", "EMPTY", "URL"]);
    }

    #[test]
    fn env_reader_rejects_quote_wrapped_values() {
        // Trap: `CORS_ALLOWED_ORIGINS=""` is a 2-char origin, not empty.
        for bad in ["CORS_ALLOWED_ORIGINS=\"\"", "A=\"x\"", "A='x'"] {
            let err = read_env_file_keys(bad, "t").expect_err(bad);
            assert!(err.to_string().contains("quote-wrapped"), "{err}");
        }
        // A quote that does not wrap the whole value is data, not wrapping.
        assert!(read_env_file_keys("A=say \"hi\" now\n", "t").is_ok());
    }

    #[test]
    fn env_reader_rejects_malformed_and_whitespace() {
        for bad in ["NOEQUALS", "  A=1", "A=1 ", "A B=1", "=1"] {
            assert!(read_env_file_keys(bad, "t").is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn read_env_file_value_with_equals() {
        let p = read_env_file("URL=https://x/?a=b&c=d\n", "t").expect("read");
        assert_eq!(
            p,
            vec![("URL".to_string(), "https://x/?a=b&c=d".to_string())],
            "split at the FIRST `=`"
        );
    }

    #[test]
    fn read_env_file_empty_value() {
        let p = read_env_file("EMPTY=\n", "t").expect("read");
        assert_eq!(p, vec![("EMPTY".to_string(), String::new())]);
    }

    #[test]
    fn read_env_file_rejects_every_contract_violation() {
        for bad in [
            "NOEQUALS", "  A=1", "A=1 ", "A B=1", "=1", "A=\"x\"", "A='x'",
        ] {
            assert!(read_env_file(bad, "t").is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn literals_split_through_checked_pair() {
        let e = GeneratorEntry {
            name: "n".into(),
            namespace: None,
            behavior: None,
            envs: vec![],
            files: vec![],
            literals: vec!["A=x=y".into(), "B=".into()],
        };
        assert_eq!(
            e.literal_pairs().expect("pairs"),
            vec![
                ("A".to_string(), "x=y".to_string()),
                ("B".to_string(), String::new())
            ]
        );
        let bad = GeneratorEntry {
            literals: vec!["A=\"q\"".into()],
            ..e
        };
        assert!(bad.literal_pairs().is_err());
    }

    /// Containment is shared: an escaping source is an error naming the
    /// escape, and a missing one a DIFFERENT error, for every consumer.
    #[test]
    fn generator_data_escaping_source_is_err() {
        let outer = tempfile::tempdir().expect("tempdir");
        std::fs::write(outer.path().join("outside.env"), "A=1\n").expect("write");
        let root = outer.path().join("repo");
        let kdir = root.join("infra/x");
        std::fs::create_dir_all(&kdir).expect("mkdir");
        let e = GeneratorEntry {
            name: "esc".into(),
            namespace: None,
            behavior: None,
            envs: vec!["../../../outside.env".into()],
            files: vec![],
            literals: vec![],
        };
        let err = generator_data(&root, &kdir, &e).expect_err("escape");
        assert!(
            err.to_string().contains("resolves outside the repository"),
            "{err:#}"
        );
        let missing = GeneratorEntry {
            envs: vec!["nope.env".into()],
            ..e
        };
        let err = generator_data(&root, &kdir, &missing).expect_err("missing");
        assert!(err.to_string().contains("cannot be read"), "{err:#}");
    }

    #[test]
    fn generator_data_reads_values() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::write(td.path().join("c.env"), "A=1\n").expect("write");
        std::fs::write(td.path().join("f.yaml"), "x: 1\n").expect("write");
        let e = GeneratorEntry {
            name: "n".into(),
            namespace: None,
            behavior: None,
            envs: vec!["c.env".into()],
            files: vec![GeneratorFile {
                key: "k.yaml".into(),
                path: "f.yaml".into(),
            }],
            literals: vec!["L=2".into()],
        };
        let d = generator_data(td.path(), td.path(), &e).expect("data");
        assert_eq!(d.get("A").map(String::as_str), Some("1"));
        assert_eq!(d.get("k.yaml").map(String::as_str), Some("x: 1\n"));
        assert_eq!(d.get("L").map(String::as_str), Some("2"));
    }

    #[test]
    fn generator_keys_resolves_all_sources_and_fails_on_missing() {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::write(td.path().join("c.env"), "A=1\n").expect("write");
        std::fs::write(td.path().join("f.yaml"), "x: 1\n").expect("write");
        let e = GeneratorEntry {
            name: "n".into(),
            namespace: None,
            behavior: None,
            envs: vec!["c.env".into()],
            files: vec![GeneratorFile {
                key: "k.yaml".into(),
                path: "f.yaml".into(),
            }],
            literals: vec!["L=2".into()],
        };
        let keys = generator_keys(td.path(), td.path(), &e).expect("keys");
        assert_eq!(
            keys.into_iter().collect::<Vec<_>>(),
            vec!["A", "L", "k.yaml"]
        );
        let missing = GeneratorEntry {
            envs: vec!["nope.env".into()],
            ..e.clone()
        };
        assert!(generator_keys(td.path(), td.path(), &missing).is_err());
        let missing_file = GeneratorEntry {
            envs: vec![],
            files: vec![GeneratorFile {
                key: "k".into(),
                path: "nope".into(),
            }],
            ..e
        };
        assert!(generator_keys(td.path(), td.path(), &missing_file).is_err());
    }
}
