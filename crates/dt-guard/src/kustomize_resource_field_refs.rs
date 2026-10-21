//! `kustomize` downward-API resource rule — every `resourceFieldRef` names a
//! resource its target container DECLARES.
//!
//! **Policy content: operations. Machinery: infrastructure** (story 3,
//! operations' catch). The pod-spec walk is shared
//! ([`crate::common::pod_spec::pod_resource_field_refs`]); the rule lives here,
//! in the kustomize policy family, because it reads RENDERED output.
//!
//! # The rule
//!
//! Any downward-API `resourceFieldRef` with `resource: limits.<r>` (or
//! `requests.<r>`) — an env var's `valueFrom`, or a `downwardAPI` volume item
//! (plain or projected) — requires its TARGET container to declare
//! `resources.limits.<r>` (respectively `resources.requests.<r>`). The target
//! is `containerName` when set, else the container whose env carries the ref.
//! Containers and initContainers are both searched and both checked. An absent
//! or empty `resources` block counts as missing. A request never satisfies a
//! limit reference (or vice versa).
//!
//! **A `requests.<r>` ref with only a limit is NOT actually fail-open** —
//! Kubernetes defaults an unset request to the declared limit. The rule still
//! requires the explicit `requests.<r>` declaration, DELIBERATELY: one explicit
//! home for the value the consumer reads, and the same shape as limits. Do not
//! "correct" the rule into accepting a limit for a requests ref.
//!
//! # Why — the failure is silent and fail-OPEN
//!
//! If the referenced limit is absent, Kubernetes does not reject the pod: it
//! silently resolves the value to the NODE's allocatable capacity. For
//! `MH_POD_MEMORY_LIMIT_BYTES` that means MH's boot-time memory check (from
//! MH-A2: backlog x egress stream ceiling plus the receive-side bound x
//! `MH_MAX_CONNECTIONS`, within a fixed fraction of the limit) passes
//! vacuously against gigabytes of node memory — the pod boots "validated" and
//! is OOM-killed under load, with no signal before. A wrong `divisor` is NOT
//! policed: it shrinks the value, so the consumer's check fails CLOSED and
//! loudly. Only a missing resource is fail-open.
//!
//! # Findings
//!
//! * [`RESOURCE_FIELD_REF_UNDECLARED_RULE_ID`] — the target container does not
//!   declare the referenced resource. Names workload, container, resource.
//! * [`RESOURCE_FIELD_REF_UNKNOWN_CONTAINER_RULE_ID`] — `containerName` names
//!   no container in the pod spec.
//! * [`RESOURCE_FIELD_REF_MALFORMED_RULE_ID`] — no `resource`, a resource that
//!   is not `limits.<r>`/`requests.<r>`, or a volume item without the
//!   `containerName` Kubernetes requires there.
//!
//! # Where it runs, and why it cannot pass silently
//!
//! Over the rendered BASE targets (attributes a defect to the base that owns
//! it) and the rendered ENVIRONMENT ROOT (catches an overlay patch that strips
//! a limit — the root is what deploy.sh applies). A base defect therefore
//! appears twice (base and root); that is the price of catching overlay
//! regressions, and both lines name the same workload.
//!
//! SOUNDNESS: the rule is build-dependent, and the other build-dependent
//! checks degrade to WARN without a kustomize tool. This one does not need its
//! own fail-closed branch ONLY because R-21 (`kustomize_content_addressing`)
//! FAILS the whole run when no tool exists (`content_addressing_unverifiable`),
//! so a run in which this rule could not render anything is always red. Anyone
//! making R-21's unverifiable branch non-fatal MUST give this rule its own
//! fail-closed branch first. Pinned by `kustomize::tests::
//! no_tool_run_is_red_so_build_dependent_rules_cannot_pass_silently`.

use crate::common::pod_spec::{
    containers, meta_str, parse_docs, pod_resource_field_refs, pod_spec, ResourceFieldOrigin,
};
use serde_norway::Value;

pub const RESOURCE_FIELD_REF_UNDECLARED_RULE_ID: &str = "resource_field_ref_undeclared";
pub const RESOURCE_FIELD_REF_UNKNOWN_CONTAINER_RULE_ID: &str =
    "resource_field_ref_unknown_container";
pub const RESOURCE_FIELD_REF_MALFORMED_RULE_ID: &str = "resource_field_ref_malformed";

/// Every rule id this module emits — the kustomize fail-token groups them.
pub const RULE_IDS: &[&str] = &[
    RESOURCE_FIELD_REF_UNDECLARED_RULE_ID,
    RESOURCE_FIELD_REF_UNKNOWN_CONTAINER_RULE_ID,
    RESOURCE_FIELD_REF_MALFORMED_RULE_ID,
];

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceFieldFinding {
    pub rule_id: &'static str,
    pub detail: String,
}

/// Outcome over one rendered target.
#[derive(Debug, Default)]
pub struct ResourceFieldReport {
    pub findings: Vec<ResourceFieldFinding>,
    /// `resourceFieldRef`s examined — the positive control: the kustomize OK
    /// line carries the environment root's count, so a walk that went blind
    /// shows as 0 in every Layer-3 log.
    pub checked: usize,
}

/// `limits.memory` -> `("limits", "memory")`; anything else is `None`.
fn split_resource(resource: &str) -> Option<(&str, &str)> {
    let (section, name) = resource.split_once('.')?;
    (matches!(section, "limits" | "requests") && !name.is_empty()).then_some((section, name))
}

fn origin_label(origin: ResourceFieldOrigin<'_>) -> String {
    match origin {
        ResourceFieldOrigin::Env(n) => format!("env {}", n.unwrap_or("<unnamed>")),
        ResourceFieldOrigin::Volume(n) => {
            format!("downwardAPI volume {}", n.unwrap_or("<unnamed>"))
        }
    }
}

/// Check one workload document's pod spec; returns the refs examined.
fn check_workload(doc: &Value, push: &mut dyn FnMut(&'static str, String)) -> usize {
    let Some(pod) = pod_spec(doc) else {
        return 0;
    };
    let workload = format!(
        "{}/{}",
        doc.get("kind").and_then(Value::as_str).unwrap_or("?"),
        meta_str(doc, "name").unwrap_or("<unnamed>")
    );
    let all = containers(pod);
    let refs = pod_resource_field_refs(pod);
    let checked = refs.len();
    for r in refs {
        let at = origin_label(r.origin);
        let target_name = match (r.container_name, r.origin) {
            (Some(n), _) => n,
            (None, ResourceFieldOrigin::Env(_)) => r.owner_container.unwrap_or("<unnamed>"),
            (None, ResourceFieldOrigin::Volume(_)) => {
                push(
                    RESOURCE_FIELD_REF_MALFORMED_RULE_ID,
                    format!("{workload}: {at}: a downwardAPI volume resourceFieldRef has no containerName (Kubernetes requires it there)"),
                );
                continue;
            }
        };
        let Some(resource) = r.resource else {
            push(
                RESOURCE_FIELD_REF_MALFORMED_RULE_ID,
                format!("{workload}: {at}: resourceFieldRef has no `resource`"),
            );
            continue;
        };
        let Some((section, res)) = split_resource(resource) else {
            push(
                RESOURCE_FIELD_REF_MALFORMED_RULE_ID,
                format!("{workload}: {at}: resourceFieldRef resource {resource:?} is not limits.<r> or requests.<r>"),
            );
            continue;
        };
        let Some(target) = all
            .iter()
            .find(|c| c.get("name").and_then(Value::as_str) == Some(target_name))
        else {
            push(
                RESOURCE_FIELD_REF_UNKNOWN_CONTAINER_RULE_ID,
                format!("{workload}: {at}: resourceFieldRef containerName {target_name:?} names no container or initContainer in this pod spec"),
            );
            continue;
        };
        let declared = target
            .get("resources")
            .and_then(|rs| rs.get(section))
            .and_then(|sec| sec.get(res))
            .is_some_and(|v| !v.is_null());
        if !declared {
            push(
                RESOURCE_FIELD_REF_UNDECLARED_RULE_ID,
                format!(
                    "{workload}: container {target_name:?} does not declare resources.{section}.{res}, but {at} reads {resource} from it — Kubernetes would silently substitute the NODE's allocatable value (fail-open)"
                ),
            );
        }
    }
    checked
}

/// Check one rendered multi-document YAML stream. An unparseable render is
/// one malformed finding (nothing could be checked), never a silent pass.
pub fn check_rendered(rendered: &str, label: &str) -> ResourceFieldReport {
    let mut report = ResourceFieldReport::default();
    let mut findings = Vec::new();
    let mut push = |rule_id: &'static str, detail: String| {
        findings.push(ResourceFieldFinding {
            rule_id,
            detail: format!("{label} — {detail}"),
        });
    };
    match parse_docs(rendered) {
        Ok(docs) => {
            for doc in &docs {
                report.checked += check_workload(doc, &mut push);
            }
        }
        Err(e) => push(
            RESOURCE_FIELD_REF_MALFORMED_RULE_ID,
            format!("rendered output is not parseable YAML ({e}); nothing can be checked"),
        ),
    }
    report.findings = findings;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Findings only (shadows the glob import for brevity in assertions).
    fn check_rendered(rendered: &str, label: &str) -> Vec<ResourceFieldFinding> {
        super::check_rendered(rendered, label).findings
    }

    #[test]
    fn checked_count_is_the_positive_control() {
        let doc = format!(
            "{}---\n{}",
            mh_shaped(LIMITS, NAMED),
            mh_shaped(LIMITS, SELF)
        );
        let r = super::check_rendered(&doc, "t");
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        assert_eq!(r.checked, 2);
        assert_eq!(super::check_rendered("kind: Service\n", "t").checked, 0);
    }

    /// An MH-shaped Deployment: one `mh-service` container whose env reads
    /// `limits.memory` via the downward API, with `resources` as given.
    fn mh_shaped(resources: &str, field_ref: &str) -> String {
        format!(
            "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: mh-0\nspec:\n  template:\n    spec:\n      containers:\n      - name: mh-service\n        env:\n        - name: MH_POD_MEMORY_LIMIT_BYTES\n          valueFrom:\n            resourceFieldRef: {field_ref}\n{resources}"
        )
    }

    const LIMITS: &str = "        resources:\n          requests:\n            memory: 512Mi\n          limits:\n            memory: 1Gi\n";
    const REQUESTS_ONLY: &str =
        "        resources:\n          requests:\n            memory: 512Mi\n";
    const NAMED: &str = "{containerName: mh-service, resource: limits.memory, divisor: '1'}";
    const SELF: &str = "{resource: limits.memory}";

    fn rules(f: &[ResourceFieldFinding]) -> Vec<&'static str> {
        f.iter().map(|x| x.rule_id).collect()
    }

    #[test]
    fn declared_limit_passes_named_and_own_container() {
        assert!(check_rendered(&mh_shaped(LIMITS, NAMED), "t").is_empty());
        assert!(check_rendered(&mh_shaped(LIMITS, SELF), "t").is_empty());
    }

    #[test]
    fn mh_shaped_workload_without_limits_memory_fails_naming_everything() {
        let f = check_rendered(&mh_shaped("", NAMED), "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]);
        let d = &f[0].detail;
        assert!(d.contains("Deployment/mh-0"), "{d}");
        assert!(d.contains("\"mh-service\""), "{d}");
        assert!(d.contains("resources.limits.memory"), "{d}");
        assert!(d.contains("MH_POD_MEMORY_LIMIT_BYTES"), "{d}");
    }

    #[test]
    fn empty_resources_block_counts_as_missing() {
        let f = check_rendered(&mh_shaped("        resources: {}\n", SELF), "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]);
    }

    #[test]
    fn a_request_does_not_satisfy_a_limit_reference() {
        let f = check_rendered(&mh_shaped(REQUESTS_ONLY, SELF), "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]);
    }

    #[test]
    fn requests_variant_is_checked_against_requests() {
        let req = "{resource: requests.memory}";
        assert!(check_rendered(&mh_shaped(REQUESTS_ONLY, req), "t").is_empty());
        let lim_only = "        resources:\n          limits:\n            memory: 1Gi\n";
        let f = check_rendered(&mh_shaped(lim_only, req), "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]);
        assert!(f[0].detail.contains("resources.requests.memory"));
    }

    #[test]
    fn named_sibling_without_the_limit_fails_even_if_the_owner_has_it() {
        let doc = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: w\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        env:\n        - name: X\n          valueFrom:\n            resourceFieldRef: {containerName: sidecar, resource: limits.memory}\n        resources:\n          limits: {memory: 1Gi}\n      - name: sidecar\n";
        let f = check_rendered(doc, "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]);
        assert!(f[0].detail.contains("\"sidecar\""), "{}", f[0].detail);
    }

    #[test]
    fn unknown_container_name_is_its_own_failure() {
        let f = check_rendered(
            &mh_shaped(
                LIMITS,
                "{containerName: mh-servce, resource: limits.memory}",
            ),
            "t",
        );
        assert_eq!(
            rules(&f),
            vec![RESOURCE_FIELD_REF_UNKNOWN_CONTAINER_RULE_ID]
        );
        assert!(f[0].detail.contains("mh-servce"));
    }

    #[test]
    fn missing_or_unparseable_resource_is_malformed() {
        let f = check_rendered(&mh_shaped(LIMITS, "{containerName: mh-service}"), "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_MALFORMED_RULE_ID]);
        let f = check_rendered(&mh_shaped(LIMITS, "{resource: memory}"), "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_MALFORMED_RULE_ID]);
    }

    #[test]
    fn init_container_refs_are_checked() {
        let doc = "apiVersion: batch/v1\nkind: Job\nmetadata:\n  name: j\nspec:\n  template:\n    spec:\n      initContainers:\n      - name: init\n        env:\n        - name: X\n          valueFrom:\n            resourceFieldRef: {resource: limits.cpu}\n      containers:\n      - name: app\n";
        let f = check_rendered(doc, "t");
        assert_eq!(rules(&f), vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]);
        assert!(f[0].detail.contains("\"init\""));
    }

    #[test]
    fn downward_api_volume_items_are_checked() {
        let doc = |name: &str| {
            format!("apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: w\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        resources:\n          limits: {{memory: 1Gi}}\n      volumes:\n      - name: dv\n        downwardAPI:\n          items:\n          - path: x\n            resourceFieldRef: {{containerName: {name}, resource: limits.memory}}\n")
        };
        assert!(check_rendered(&doc("app"), "t").is_empty());
        let cpu = doc("app").replace("limits.memory", "limits.cpu");
        assert_eq!(
            rules(&check_rendered(&cpu, "t")),
            vec![RESOURCE_FIELD_REF_UNDECLARED_RULE_ID]
        );
        let no_name = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: w\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n      volumes:\n      - name: pv\n        projected:\n          sources:\n          - downwardAPI:\n              items:\n              - path: x\n                resourceFieldRef: {resource: limits.memory}\n";
        assert_eq!(
            rules(&check_rendered(no_name, "t")),
            vec![RESOURCE_FIELD_REF_MALFORMED_RULE_ID]
        );
    }

    #[test]
    fn unrelated_env_sources_are_untouched() {
        let doc = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: w\nspec:\n  template:\n    spec:\n      containers:\n      - name: app\n        env:\n        - name: POD_IP\n          valueFrom: {fieldRef: {fieldPath: status.podIP}}\n        - name: L\n          value: x\n";
        assert!(check_rendered(doc, "t").is_empty());
    }
}
