//! Pod-template walking shared by the policies that read workload manifests.
//!
//! Two policies ask "which ConfigMaps does this pod template consume?":
//! `env-config` (per-key `configMapKeyRef` resolution) and `kustomize` R-21
//! (every consumed ConfigMap is content-addressed). They share the WALK here —
//! one enumeration of every reference shape — and keep their POLICY apart:
//! env-config refuses to claim coverage over `envFrom`, while for R-21 an
//! `envFrom` import is simply one more reference that must be hash-suffixed.
//!
//! The same module carries the downward-API `resourceFieldRef` walk
//! ([`pod_resource_field_refs`]) for `kustomize`'s resource-field rule
//! (`kustomize_resource_field_refs`): again, the walk here, the policy there.

use serde::Deserialize as _;
use serde_norway::Value;

/// Every non-null document of a multi-document YAML stream (a trailing `---`
/// yields a null document, which is not a manifest). THE one stream parser for
/// rendered output and manifest files; each caller maps the error to its own
/// rule id or context.
///
/// # Errors
/// The first YAML parse error, rendered as a string.
pub fn parse_docs(stream: &str) -> Result<Vec<Value>, String> {
    let mut docs = Vec::new();
    for de in serde_norway::Deserializer::from_str(stream) {
        match Value::deserialize(de) {
            Ok(v) if !v.is_null() => docs.push(v),
            Ok(_) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(docs)
}

/// `metadata.<field>` of a manifest document, if it is a string.
pub fn meta_str<'a>(doc: &'a Value, field: &str) -> Option<&'a str> {
    doc.get("metadata")?.get(field)?.as_str()
}

/// Locate a workload's pod spec. One path, no per-kind branching — see
/// `env_config::WORKLOAD_KINDS_WITH_POD_SPEC` on why `CronJob`'s deeper nesting
/// is out.
pub fn pod_spec(doc: &Value) -> Option<&Value> {
    doc.get("spec")?.get("template")?.get("spec")
}

/// All containers in a pod spec, regular and init.
///
/// `initContainers` is included deliberately: it is one more field on the same
/// pod spec rather than a separate code path, and omitting it is not neutral —
/// a required env var declared only on an init container would produce a false
/// `missing_in_manifest`, and its ConfigMap key a false orphan. A guard that
/// invents findings is worse than one that misses them.
pub fn containers(pod: &Value) -> Vec<&Value> {
    let mut out: Vec<&Value> = Vec::new();
    for field in ["containers", "initContainers"] {
        if let Some(seq) = pod.get(field).and_then(Value::as_sequence) {
            out.extend(seq.iter());
        }
    }
    out
}

/// How a pod template references a ConfigMap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigMapRefKind {
    /// `env[].valueFrom.configMapKeyRef`.
    KeyRef,
    /// `envFrom[].configMapRef`.
    EnvFrom,
    /// `volumes[].configMap`.
    Volume,
    /// `volumes[].projected.sources[].configMap`.
    Projected,
}

/// One ConfigMap reference. `name`/`key` are `None` when the manifest omits
/// them — the consumer decides whether that is a finding; the walker never
/// drops a reference it cannot fully read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigMapRef<'a> {
    pub kind: ConfigMapRefKind,
    pub name: Option<&'a str>,
    /// The key, for `KeyRef` only.
    pub key: Option<&'a str>,
    /// The `env[].name` the value lands in, for `KeyRef` only.
    pub env_name: Option<&'a str>,
}

/// ConfigMap references of ONE container: `envFrom` imports first, then
/// `configMapKeyRef`s in `env` order. An `envFrom` that is not a list yields
/// nothing here — [`env_from_is_list`] lets a consumer tell that apart.
pub fn container_configmap_refs(container: &Value) -> Vec<ConfigMapRef<'_>> {
    let mut out = Vec::new();
    if let Some(seq) = container.get("envFrom").and_then(Value::as_sequence) {
        for src in seq {
            if let Some(cm) = src.get("configMapRef") {
                out.push(ConfigMapRef {
                    kind: ConfigMapRefKind::EnvFrom,
                    name: cm.get("name").and_then(Value::as_str),
                    key: None,
                    env_name: None,
                });
            }
        }
    }
    if let Some(seq) = container.get("env").and_then(Value::as_sequence) {
        for entry in seq {
            if let Some(cm) = entry
                .get("valueFrom")
                .and_then(|v| v.get("configMapKeyRef"))
            {
                out.push(ConfigMapRef {
                    kind: ConfigMapRefKind::KeyRef,
                    name: cm.get("name").and_then(Value::as_str),
                    key: cm.get("key").and_then(Value::as_str),
                    env_name: entry.get("name").and_then(Value::as_str),
                });
            }
        }
    }
    out
}

/// True when a container's `envFrom` is absent or a list.
pub fn env_from_is_list(container: &Value) -> bool {
    container
        .get("envFrom")
        .is_none_or(|v| v.as_sequence().is_some())
}

/// ConfigMap references of a pod spec's volumes (plain and projected).
pub fn volume_configmap_refs(pod: &Value) -> Vec<ConfigMapRef<'_>> {
    let mut out = Vec::new();
    let Some(vols) = pod.get("volumes").and_then(Value::as_sequence) else {
        return out;
    };
    for vol in vols {
        if let Some(cm) = vol.get("configMap") {
            out.push(ConfigMapRef {
                kind: ConfigMapRefKind::Volume,
                name: cm.get("name").and_then(Value::as_str),
                key: None,
                env_name: None,
            });
        }
        if let Some(sources) = vol
            .get("projected")
            .and_then(|p| p.get("sources"))
            .and_then(Value::as_sequence)
        {
            for src in sources {
                if let Some(cm) = src.get("configMap") {
                    out.push(ConfigMapRef {
                        kind: ConfigMapRefKind::Projected,
                        name: cm.get("name").and_then(Value::as_str),
                        key: None,
                        env_name: None,
                    });
                }
            }
        }
    }
    out
}

/// Every ConfigMap reference in a pod spec: all containers, then volumes.
pub fn pod_configmap_refs(pod: &Value) -> Vec<ConfigMapRef<'_>> {
    let mut out: Vec<ConfigMapRef<'_>> = containers(pod)
        .into_iter()
        .flat_map(container_configmap_refs)
        .collect();
    out.extend(volume_configmap_refs(pod));
    out
}

/// Where a downward-API `resourceFieldRef` sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceFieldOrigin<'a> {
    /// `containers[].env[].valueFrom.resourceFieldRef` (or `initContainers`);
    /// the payload is the env var name.
    Env(Option<&'a str>),
    /// `volumes[].downwardAPI.items[]` or
    /// `volumes[].projected.sources[].downwardAPI.items[]`; the payload is the
    /// volume name.
    Volume(Option<&'a str>),
}

/// One downward-API `resourceFieldRef`. Fields are `None` when the manifest
/// omits them — the consumer decides whether that is a finding; the walker
/// never drops a reference it cannot fully read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceFieldRef<'a> {
    pub origin: ResourceFieldOrigin<'a>,
    /// The container whose `env` carries the ref (env refs only).
    pub owner_container: Option<&'a str>,
    /// `resourceFieldRef.containerName`.
    pub container_name: Option<&'a str>,
    /// `resourceFieldRef.resource`, e.g. `limits.memory`.
    pub resource: Option<&'a str>,
}

fn downward_items<'a>(
    api: &'a Value,
    volume: Option<&'a str>,
    out: &mut Vec<ResourceFieldRef<'a>>,
) {
    let Some(items) = api.get("items").and_then(Value::as_sequence) else {
        return;
    };
    for item in items {
        if let Some(rf) = item.get("resourceFieldRef") {
            out.push(ResourceFieldRef {
                origin: ResourceFieldOrigin::Volume(volume),
                owner_container: None,
                container_name: rf.get("containerName").and_then(Value::as_str),
                resource: rf.get("resource").and_then(Value::as_str),
            });
        }
    }
}

/// Every downward-API `resourceFieldRef` in a pod spec: env entries of every
/// container and initContainer (in order), then downwardAPI volume items
/// (plain and projected).
pub fn pod_resource_field_refs(pod: &Value) -> Vec<ResourceFieldRef<'_>> {
    let mut out = Vec::new();
    for container in containers(pod) {
        let owner = container.get("name").and_then(Value::as_str);
        let Some(env) = container.get("env").and_then(Value::as_sequence) else {
            continue;
        };
        for entry in env {
            if let Some(rf) = entry
                .get("valueFrom")
                .and_then(|v| v.get("resourceFieldRef"))
            {
                out.push(ResourceFieldRef {
                    origin: ResourceFieldOrigin::Env(entry.get("name").and_then(Value::as_str)),
                    owner_container: owner,
                    container_name: rf.get("containerName").and_then(Value::as_str),
                    resource: rf.get("resource").and_then(Value::as_str),
                });
            }
        }
    }
    if let Some(vols) = pod.get("volumes").and_then(Value::as_sequence) {
        for vol in vols {
            let vname = vol.get("name").and_then(Value::as_str);
            if let Some(api) = vol.get("downwardAPI") {
                downward_items(api, vname, &mut out);
            }
            if let Some(sources) = vol
                .get("projected")
                .and_then(|p| p.get("sources"))
                .and_then(Value::as_sequence)
            {
                for src in sources {
                    if let Some(api) = src.get("downwardAPI") {
                        downward_items(api, vname, &mut out);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(y: &str) -> Value {
        serde_norway::from_str(y).expect("yaml")
    }

    #[test]
    fn enumerates_every_reference_shape() {
        let doc = parse(
            r"
spec:
  template:
    spec:
      initContainers:
      - name: init
        envFrom:
        - configMapRef: {name: bulk}
        - secretRef: {name: s}
      containers:
      - name: app
        env:
        - name: A
          valueFrom: {configMapKeyRef: {name: cm, key: K}}
        - name: B
          value: plain
        - name: C
          valueFrom: {secretKeyRef: {name: s, key: k}}
      volumes:
      - name: v
        configMap: {name: vol}
      - name: p
        projected:
          sources:
          - configMap: {name: proj}
          - secret: {name: s}
      - name: e
        emptyDir: {}
",
        );
        let pod = pod_spec(&doc).expect("pod");
        let refs: Vec<(ConfigMapRefKind, Option<&str>, Option<&str>)> = pod_configmap_refs(pod)
            .into_iter()
            .map(|r| (r.kind, r.name, r.key))
            .collect();
        assert_eq!(
            refs,
            vec![
                (ConfigMapRefKind::KeyRef, Some("cm"), Some("K")),
                (ConfigMapRefKind::EnvFrom, Some("bulk"), None),
                (ConfigMapRefKind::Volume, Some("vol"), None),
                (ConfigMapRefKind::Projected, Some("proj"), None),
            ]
        );
    }

    #[test]
    fn unreadable_names_are_yielded_not_dropped() {
        let c = parse("env:\n- name: A\n  valueFrom: {configMapKeyRef: {key: K}}\n");
        let refs = container_configmap_refs(&c);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, None);
        assert_eq!(refs[0].env_name, Some("A"));
    }

    #[test]
    fn env_from_shape_is_reported() {
        assert!(env_from_is_list(&parse("name: a\n")));
        assert!(env_from_is_list(&parse("envFrom: []\n")));
        assert!(!env_from_is_list(&parse(
            "envFrom: {configMapRef: {name: x}}\n"
        )));
    }

    #[test]
    fn resource_field_refs_cover_env_init_and_both_volume_shapes() {
        let doc = parse(
            r"
spec:
  template:
    spec:
      initContainers:
      - name: init
        env:
        - name: I
          valueFrom: {resourceFieldRef: {resource: requests.cpu}}
      containers:
      - name: app
        env:
        - name: M
          valueFrom: {resourceFieldRef: {containerName: app, resource: limits.memory, divisor: '1'}}
        - name: P
          valueFrom: {fieldRef: {fieldPath: status.podIP}}
      volumes:
      - name: dv
        downwardAPI:
          items:
          - path: mem
            resourceFieldRef: {containerName: app, resource: limits.memory}
          - path: labels
            fieldRef: {fieldPath: metadata.labels}
      - name: pv
        projected:
          sources:
          - downwardAPI:
              items:
              - path: cpu
                resourceFieldRef: {resource: limits.cpu}
",
        );
        let pod = pod_spec(&doc).expect("pod");
        let refs = pod_resource_field_refs(pod);
        assert_eq!(
            refs,
            vec![
                ResourceFieldRef {
                    origin: ResourceFieldOrigin::Env(Some("M")),
                    owner_container: Some("app"),
                    container_name: Some("app"),
                    resource: Some("limits.memory"),
                },
                ResourceFieldRef {
                    origin: ResourceFieldOrigin::Env(Some("I")),
                    owner_container: Some("init"),
                    container_name: None,
                    resource: Some("requests.cpu"),
                },
                ResourceFieldRef {
                    origin: ResourceFieldOrigin::Volume(Some("dv")),
                    owner_container: None,
                    container_name: Some("app"),
                    resource: Some("limits.memory"),
                },
                ResourceFieldRef {
                    origin: ResourceFieldOrigin::Volume(Some("pv")),
                    owner_container: None,
                    container_name: None,
                    resource: Some("limits.cpu"),
                },
            ]
        );
    }
}
