//! The ONE structured reader of kustomize `configMapGenerator` entries.
//!
//! ADR-0038 §2 makes every pod-consumed ConfigMap a generator, so three
//! policies need to know what a kustomization generates: `kustomize` R-16
//! (a generator source is a declared file, not an orphan), R-21 (a
//! hash-suffixed reference must name a generator that exists), and
//! `env-config` (a `configMapKeyRef` resolves against the generator's keys).
//! They share this parser rather than each growing its own.
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
use std::collections::BTreeSet;
use std::path::Path;

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
/// module doc; return the key.
fn checked_pair<'a>(pair: &'a str, origin: &dyn Fn() -> String) -> Result<&'a str> {
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
    Ok(key)
}

/// Keys of an `envs:` file, validated per the module-doc contract.
pub fn read_env_file_keys(content: &str, origin: &str) -> Result<Vec<String>> {
    let mut keys = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let at = || format!("{origin}:{}", i + 1);
        keys.push(checked_pair(line, &at)?.to_string());
    }
    Ok(keys)
}

/// The data keys an entry generates, reading its sources from `kust_dir`.
/// An unreadable source is an error.
pub fn generator_keys(kust_dir: &Path, entry: &GeneratorEntry) -> Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for env in &entry.envs {
        let path = kust_dir.join(env);
        let content = std::fs::read_to_string(&path).with_context(|| {
            format!(
                "configMapGenerator {:?}: reading env source {}",
                entry.name,
                path.display()
            )
        })?;
        keys.extend(read_env_file_keys(&content, &path.display().to_string())?);
    }
    for f in &entry.files {
        let path = kust_dir.join(&f.path);
        if !path.is_file() {
            bail!(
                "configMapGenerator {:?}: file source {} does not exist",
                entry.name,
                path.display()
            );
        }
        keys.insert(f.key.clone());
    }
    for lit in &entry.literals {
        let origin = || format!("configMapGenerator {:?} literal", entry.name);
        keys.insert(checked_pair(lit, &origin)?.to_string());
    }
    Ok(keys)
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
        let keys = generator_keys(td.path(), &e).expect("keys");
        assert_eq!(
            keys.into_iter().collect::<Vec<_>>(),
            vec!["A", "L", "k.yaml"]
        );
        let missing = GeneratorEntry {
            envs: vec!["nope.env".into()],
            ..e.clone()
        };
        assert!(generator_keys(td.path(), &missing).is_err());
        let missing_file = GeneratorEntry {
            envs: vec![],
            files: vec![GeneratorFile {
                key: "k".into(),
                path: "nope".into(),
            }],
            ..e
        };
        assert!(generator_keys(td.path(), &missing_file).is_err());
    }
}
