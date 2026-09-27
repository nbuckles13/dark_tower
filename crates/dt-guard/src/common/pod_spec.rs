//! Pod-template walking shared by the policies that read workload manifests.
//!
//! Two policies ask "which ConfigMaps does this pod template consume?":
//! `env-config` (per-key `configMapKeyRef` resolution) and `kustomize` R-21
//! (every consumed ConfigMap is content-addressed). They share the WALK here —
//! one enumeration of every reference shape — and keep their POLICY apart:
//! env-config refuses to claim coverage over `envFrom`, while for R-21 an
//! `envFrom` import is simply one more reference that must be hash-suffixed.

use serde_norway::Value;

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
}
