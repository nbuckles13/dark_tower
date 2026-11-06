//! Shared Cargo manifest reading — the ONE read+parse of the workspace root
//! `Cargo.toml`, plus a typed walk of a crate manifest's dependency tables.
//!
//! Consumers: `release_build_profile` (profiles + the `*-service` roster
//! floor) and `workspace_deps` (members / exclude / `[workspace.dependencies]`).
//! Each consumer maps [`ManifestError`] onto its OWN precondition token; this
//! module owns parsing, never policy.
//!
//! Deliberately the real `toml` parser rather than a line scanner (@security
//! ruling 2026-09-01, recorded on the `toml` dependency in `Cargo.toml`): the
//! artifact is decided by how *cargo* reads the manifest, and every divergence
//! between a hand-rolled scanner and cargo's grammar (dotted keys, inline
//! tables, quoted keys, `[target.'cfg(..)'.dependencies]`) is a false-negative
//! window.

use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

/// The workspace root manifest.
///
/// Generic over the profile-table type so `release_build_profile` keeps its
/// own typed `ProfileTable` while still going through the single parse here:
/// one parse, one failure path (a profile of the wrong shape fails the WHOLE
/// parse, exactly as it did before the extraction).
#[derive(Debug, serde::Deserialize)]
#[serde(bound(deserialize = "P: DeserializeOwned"))]
pub struct RootManifest<P = toml::Value> {
    #[serde(default = "BTreeMap::new")]
    pub profile: BTreeMap<String, P>,
    /// `Option`, not `#[serde(default)]`: "there is no `[workspace]` table"
    /// and "there is an empty one" are different facts.
    #[serde(default)]
    pub workspace: Option<WorkspaceTable>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct WorkspaceTable {
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    /// `None` = no `[workspace.dependencies]` table at all.
    #[serde(default)]
    pub dependencies: Option<BTreeMap<String, toml::Value>>,
}

/// Why a manifest could not be evaluated. Callers map each variant to their
/// own PRECONDITION token; the two are kept distinct so "missing" and
/// "malformed" never collapse into one triage message.
#[derive(Debug)]
pub enum ManifestError {
    Unreadable(std::io::Error),
    Unparseable(toml::de::Error),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Unreadable(e) => write!(f, "unreadable ({e})"),
            ManifestError::Unparseable(e) => write!(f, "failed to parse as TOML ({e})"),
        }
    }
}

/// Read and parse `<repo_root>/Cargo.toml`. Parse is TOTAL: unparseable is an
/// error, never a skip.
pub fn read_root_manifest<P: DeserializeOwned>(
    repo_root: &Path,
) -> Result<RootManifest<P>, ManifestError> {
    let src =
        std::fs::read_to_string(repo_root.join("Cargo.toml")).map_err(ManifestError::Unreadable)?;
    toml::from_str::<RootManifest<P>>(&src).map_err(ManifestError::Unparseable)
}

/// Does any `[workspace] members` entry use glob syntax?
///
/// Cargo supports glob members (`crates/*`). Any consumer that derives
/// something from the enumerated list must refuse rather than derive from an
/// unexpanded pattern — fire on the PRESENCE of a glob, not on an empty
/// derivation (see `release_build_profile::check_service_roster` for the
/// incident that established this).
pub fn members_contain_glob(members: &[String]) -> bool {
    members.iter().any(|m| m.contains('*'))
}

/// How one dependency entry is specified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepSpec {
    /// `{ workspace = true, ... }` — inherits from `[workspace.dependencies]`.
    /// `features` are the member's ADDITIONS (cargo unions them with the
    /// workspace entry's).
    WorkspaceInherit {
        default_features: Option<bool>,
        features: BTreeSet<String>,
    },
    /// A registry dependency carrying its own version requirement.
    Version {
        req: String,
        default_features: Option<bool>,
        features: BTreeSet<String>,
    },
    /// `{ path = ... }` (with or without a version).
    Path,
    /// `{ git = ... }`.
    Git,
    /// A table with none of the above (e.g. only `optional`/`features`) —
    /// not a version pin, not inheritance.
    Other,
    /// A `features` that is not an array of strings, or a `default-features`
    /// that is not a bool. Cargo rejects both, but the excluded fuzz manifests
    /// are never compiled by the pipeline, so on that side this reader is the
    /// only one: reading the value as "no features" / "unset" would make a
    /// features-superset rule FAIL-OPEN. Consumers MUST treat this as a
    /// precondition, never as a pass and never as [`DepSpec::Other`].
    Malformed { detail: String },
}

/// The `default-features` (either spelling) and `features` keys of a dependency
/// table, read FAIL-CLOSED: a present-but-wrong-shaped value is an error, never
/// a silent `None` / empty set.
fn feature_keys(t: &toml::Table) -> Result<(Option<bool>, BTreeSet<String>), String> {
    let default_features = match t
        .get("default-features")
        .or_else(|| t.get("default_features"))
    {
        None => None,
        Some(v) => Some(
            v.as_bool()
                .ok_or_else(|| format!("`default-features` is not a bool ({v})"))?,
        ),
    };
    let features = match t.get("features") {
        None => BTreeSet::new(),
        Some(toml::Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("`features` holds a non-string element ({item})"))
            })
            .collect::<Result<_, _>>()?,
        Some(v) => return Err(format!("`features` is not an array ({v})")),
    };
    Ok((default_features, features))
}

/// One dependency declaration in a crate manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepDecl {
    /// Table it was found in: `dependencies`, `dev-dependencies`,
    /// `build-dependencies`, or `target.<cfg>.<one of those>`.
    pub table: String,
    /// The dependency key as written.
    pub key: String,
    /// The registry package name (`package = "..."` rename honoured).
    pub package: String,
    pub spec: DepSpec,
}

const DEP_TABLES: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

/// Classify one dependency value (string or table), as cargo reads it.
pub fn classify_dep(key: &str, value: &toml::Value) -> (String, DepSpec) {
    match value {
        toml::Value::String(req) => (
            key.to_string(),
            DepSpec::Version {
                req: req.clone(),
                default_features: None,
                features: BTreeSet::new(),
            },
        ),
        toml::Value::Table(t) => {
            let package = t
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key)
                .to_string();
            // Cargo accepts both `default-features` spellings.
            let (default_features, features) = match feature_keys(t) {
                Ok(keys) => keys,
                Err(detail) => return (package, DepSpec::Malformed { detail }),
            };
            let spec = if t.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                DepSpec::WorkspaceInherit {
                    default_features,
                    features,
                }
            } else if t.contains_key("path") {
                DepSpec::Path
            } else if t.contains_key("git") {
                DepSpec::Git
            } else if let Some(req) = t.get("version").and_then(toml::Value::as_str) {
                DepSpec::Version {
                    req: req.to_string(),
                    default_features,
                    features,
                }
            } else {
                DepSpec::Other
            };
            (package, spec)
        }
        _ => (key.to_string(), DepSpec::Other),
    }
}

/// Every dependency declaration in a crate manifest: the three dependency
/// tables plus their `[target.<cfg>.*]` variants.
pub fn member_dependencies(src: &str) -> Result<Vec<DepDecl>, toml::de::Error> {
    let doc: toml::Table = toml::from_str(src)?;
    let mut out = Vec::new();
    collect_dep_tables(&doc, "", &mut out);
    if let Some(targets) = doc.get("target").and_then(toml::Value::as_table) {
        for (cfg, body) in targets {
            if let Some(body) = body.as_table() {
                collect_dep_tables(body, &format!("target.{cfg}."), &mut out);
            }
        }
    }
    Ok(out)
}

fn collect_dep_tables(doc: &toml::Table, prefix: &str, out: &mut Vec<DepDecl>) {
    for table in DEP_TABLES {
        let Some(deps) = doc.get(*table).and_then(toml::Value::as_table) else {
            continue;
        };
        for (key, value) in deps {
            let (package, spec) = classify_dep(key, value);
            out.push(DepDecl {
                table: format!("{prefix}{table}"),
                key: key.clone(),
                package,
                spec,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_spec_shape() {
        let src = r#"
[dependencies]
a = "1.0"
b = { workspace = true, features = ["x"] }
c = { path = "../c", version = "0.1" }
d = { git = "https://example.org/d" }
e = { version = "2", default-features = false }
f = { package = "real-f", version = "3" }
g = { workspace = true, default_features = false }
h = { optional = true }
"#;
        let deps = member_dependencies(src).unwrap();
        let by_key = |k: &str| deps.iter().find(|d| d.key == k).unwrap().clone();
        let set = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(
            by_key("a").spec,
            DepSpec::Version {
                req: "1.0".into(),
                default_features: None,
                features: set(&[]),
            }
        );
        assert_eq!(
            by_key("b").spec,
            DepSpec::WorkspaceInherit {
                default_features: None,
                features: set(&["x"]),
            }
        );
        assert_eq!(by_key("c").spec, DepSpec::Path);
        assert_eq!(by_key("d").spec, DepSpec::Git);
        assert_eq!(
            by_key("e").spec,
            DepSpec::Version {
                req: "2".into(),
                default_features: Some(false),
                features: set(&[]),
            }
        );
        assert_eq!(by_key("f").package, "real-f");
        assert_eq!(
            by_key("g").spec,
            DepSpec::WorkspaceInherit {
                default_features: Some(false),
                features: set(&[]),
            }
        );
        assert_eq!(by_key("h").spec, DepSpec::Other);
    }

    #[test]
    fn features_are_read_as_a_sorted_set() {
        let deps = member_dependencies(
            "[dependencies]\na = { version = \"1\", features = [\"z\", \"a\", \"z\"] }\n",
        )
        .unwrap();
        assert!(
            matches!(
                &deps[0].spec,
                DepSpec::Version { features, .. }
                    if features.iter().map(String::as_str).eq(["a", "z"])
            ),
            "{:?}",
            deps[0].spec
        );
    }

    #[test]
    fn malformed_feature_keys_fail_closed() {
        // Every wrong shape is Malformed, never an empty set / unset flag
        // (which a superset rule would read as "requires nothing" = pass).
        for (body, needle) in [
            (
                "a = { version = \"1\", features = \"std\" }",
                "not an array",
            ),
            (
                "a = { version = \"1\", features = [\"std\", 1] }",
                "non-string element",
            ),
            (
                "a = { workspace = true, features = { x = 1 } }",
                "not an array",
            ),
            (
                "a = { version = \"1\", default-features = \"no\" }",
                "not a bool",
            ),
            (
                "a = { version = \"1\", default_features = 0 }",
                "not a bool",
            ),
        ] {
            let deps = member_dependencies(&format!("[dependencies]\n{body}\n")).unwrap();
            assert!(
                matches!(&deps[0].spec, DepSpec::Malformed { detail } if detail.contains(needle)),
                "{body}: expected Malformed naming `{needle}`, got {:?}",
                deps[0].spec
            );
        }
    }

    #[test]
    fn walks_dev_build_and_target_tables() {
        let src = r#"
[dev-dependencies]
a = "1"
[build-dependencies]
b = "1"
[target.'cfg(unix)'.dependencies]
c = "1"
[target.x86_64-unknown-linux-gnu.dev-dependencies]
d = "1"
"#;
        let deps = member_dependencies(src).unwrap();
        let tables: Vec<&str> = deps.iter().map(|d| d.table.as_str()).collect();
        assert!(tables.contains(&"dev-dependencies"));
        assert!(tables.contains(&"build-dependencies"));
        assert!(tables.contains(&"target.cfg(unix).dependencies"));
        assert!(tables.contains(&"target.x86_64-unknown-linux-gnu.dev-dependencies"));
        assert_eq!(deps.len(), 4);
    }

    #[test]
    fn root_manifest_distinguishes_missing_from_malformed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            read_root_manifest::<toml::Value>(dir.path()),
            Err(ManifestError::Unreadable(_))
        ));
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace\n").unwrap();
        assert!(matches!(
            read_root_manifest::<toml::Value>(dir.path()),
            Err(ManifestError::Unparseable(_))
        ));
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\nexclude = [\"b\"]\n[workspace.dependencies]\nx = \"1\"\n",
        )
        .unwrap();
        let m = read_root_manifest::<toml::Value>(dir.path()).unwrap();
        let ws = m.workspace.unwrap();
        assert_eq!(ws.members, vec!["a".to_string()]);
        assert_eq!(ws.exclude, vec!["b".to_string()]);
        assert!(ws.dependencies.unwrap().contains_key("x"));
    }

    #[test]
    fn glob_detection() {
        assert!(members_contain_glob(&["crates/*".to_string()]));
        assert!(!members_contain_glob(&["crates/a".to_string()]));
    }
}
