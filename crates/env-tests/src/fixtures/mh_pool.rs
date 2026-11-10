//! The MH pool as `infra/kind/scripts/mh.sh list --json` reports it (ADR-0039
//! §7; machine contract
//! `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt`
//! §4). Story 4 I-2 landed the READ half ([`list`]). GC-2 adds the minimal
//! mutation surface ([`RunOwner::up`], owner-scoped [`RunOwner::teardown`]).
//! T-1 extends this module with `MhSlot::drain()` / `assert_baseline()`
//! (contract §8).
//!
//! # The mh.sh path has ONE home
//!
//! `scripts/layer7.sh` exports [`DT_MH_SH_ENV`] (an absolute path). This
//! fixture never guesses a fallback path: unset, relative or missing is a loud
//! PRECONDITION panic naming layer7.sh (contract §8, "fails loudly when it is
//! unset"). mh.sh selects the cluster itself (`kind-<DT_CLUSTER_NAME>`, never
//! the current context), so a caller comparing it with `kubectl` reads (which
//! use the current context) must cross-check the two pod sets — see
//! `tests/42_mh_address_invariant.rs`.
//!
//! # What `list` is evidence OF
//!
//! `address` and `host_port` are copied by mh.sh from the per-slot ConfigMaps
//! (mh.sh never computes an address). Comparing `address` against the slot
//! ConfigMap is therefore the same value read twice — a tautology. Use `list`
//! to ENUMERATE the pool and name pods; take evidence about what a pod actually
//! runs from the pod (`kubectl`) or the process (its logs, its metrics).

use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

/// The env var `scripts/layer7.sh` exports with mh.sh's absolute path.
pub const DT_MH_SH_ENV: &str = "DT_MH_SH";

/// The `list --json` schema version this reader understands (contract §4;
/// additive-only, so a higher version is refused rather than half-read).
pub const SCHEMA_VERSION: u64 = 1;

/// The per-slot key set of `list --json` v1, in mh.sh's order. Pinned against
/// mh.sh's `MH_JQ_PUBLIC` by a hermetic unit test below (one home: mh.sh).
/// Every key must be PRESENT (nullable keys as `null`): serde would otherwise
/// read a dropped key as `None`, i.e. "no pod", silently.
pub const SLOT_KEYS: &[&str] = &[
    "slot",
    "host_port",
    "address",
    "pod_ip",
    "pod_uid",
    "state",
    "terminating_past_grace",
    "deployment",
    "pod",
    "revision",
    "stale",
    "owner",
    "restart_count",
];

/// A slot's state (contract §4).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SlotState {
    Free,
    Starting,
    Serving,
    Draining,
    Terminating,
}

/// One `slots[]` entry of `list --json` v1.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SlotEntry {
    pub slot: u32,
    pub host_port: u16,
    pub address: String,
    pub pod_ip: Option<String>,
    pub pod_uid: Option<String>,
    pub state: SlotState,
    pub terminating_past_grace: bool,
    pub deployment: Option<String>,
    pub pod: Option<String>,
    pub revision: Option<String>,
    pub stale: bool,
    pub owner: Option<String>,
    pub restart_count: Option<u32>,
}

/// The whole `list --json` document (v1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolSnapshot {
    pub template_revision: String,
    pub baseline: u32,
    pub slots: Vec<SlotEntry>,
}

impl PoolSnapshot {
    /// Entries that have a pod (any state but `free`).
    #[must_use]
    pub fn occupied(&self) -> Vec<&SlotEntry> {
        self.slots.iter().filter(|s| s.pod.is_some()).collect()
    }

    /// Entries in state `serving`.
    #[must_use]
    pub fn serving(&self) -> Vec<&SlotEntry> {
        self.slots
            .iter()
            .filter(|s| s.state == SlotState::Serving)
            .collect()
    }
}

/// mh.sh's absolute path, from [`DT_MH_SH_ENV`].
///
/// # Panics
/// PRECONDITION: unset, empty, relative, or not a file.
#[must_use]
pub fn mh_sh_path() -> PathBuf {
    mh_sh_path_from(std::env::var_os(DT_MH_SH_ENV).map(PathBuf::from))
        .unwrap_or_else(|e| panic!("PRECONDITION (mh-sh-unset): {e}"))
}

/// PURE core of [`mh_sh_path`].
///
/// # Errors
/// Unset/empty, relative, or not an existing file — each named.
pub fn mh_sh_path_from(value: Option<PathBuf>) -> Result<PathBuf, String> {
    let hint = "scripts/layer7.sh exports it (an absolute path to infra/kind/scripts/mh.sh); \
                outside Layer 7 set it yourself, e.g. DT_MH_SH=\"$PWD/infra/kind/scripts/mh.sh\"";
    let path = value
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| format!("{DT_MH_SH_ENV} is unset or empty; {hint}"))?;
    if !path.is_absolute() {
        return Err(format!(
            "{DT_MH_SH_ENV}={} is not absolute; {hint}",
            path.display()
        ));
    }
    if !path.is_file() {
        return Err(format!(
            "{DT_MH_SH_ENV}={} is not an existing file; {hint}",
            path.display()
        ));
    }
    Ok(path)
}

/// `mh.sh list --json`, parsed and validated ([`parse_list`]).
///
/// # Panics
/// PRECONDITION: mh.sh cannot run, exits non-zero (the final `MH_RESULT` line
/// is quoted), or prints a document [`parse_list`] refuses.
#[must_use]
pub fn list() -> PoolSnapshot {
    // The ONE mh.sh reader: the strict MH_RESULT + stdout-`ok` agreement of
    // [`parse_mh_sh_result`] applies to `list` exactly as to `up`/`drain`.
    let result = run_mh_sh(&["list"]);
    assert!(
        result.is_ok(),
        "PRECONDITION (mh-sh-list): `mh.sh list --json` failed: {result}"
    );
    parse_list_value(&result.json)
        .unwrap_or_else(|e| panic!("PRECONDITION (mh-sh-list): {e} ({result})"))
}

/// PURE: parse a `list --json` stdout document.
///
/// # Errors
/// Not one JSON object; `ok` not `true`; `schema_version` not
/// [`SCHEMA_VERSION`]; a slot entry missing a [`SLOT_KEYS`] key; zero slots
/// (mh.sh's own contract is that every slot always appears, so zero means the
/// read observed nothing); duplicate slot numbers; or a typed field of the
/// wrong shape.
pub fn parse_list(stdout: &str) -> Result<PoolSnapshot, String> {
    let doc: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("mh.sh list --json stdout is not one JSON object: {e}"))?;
    parse_list_value(&doc)
}

/// PURE: validate an already-parsed `list --json` document (the
/// [`MhShResult::json`] of a `list` run). The checks are as for [`parse_list`],
/// minus the JSON parse.
///
/// # Errors
/// As [`parse_list`].
pub fn parse_list_value(doc: &Value) -> Result<PoolSnapshot, String> {
    if doc.get("ok") != Some(&Value::Bool(true)) {
        return Err("mh.sh list --json: `ok` is not true".to_string());
    }
    match doc.get("schema_version").and_then(Value::as_u64) {
        Some(SCHEMA_VERSION) => {}
        other => {
            return Err(format!(
                "mh.sh list --json: schema_version {other:?}, this reader understands \
                 {SCHEMA_VERSION}"
            ))
        }
    }
    let template_revision = doc
        .get("template_revision")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .ok_or("mh.sh list --json: no template_revision")?
        .to_string();
    let baseline = doc
        .get("baseline")
        .and_then(Value::as_u64)
        .and_then(|b| u32::try_from(b).ok())
        .ok_or("mh.sh list --json: baseline is not a non-negative integer")?;
    let raw = doc
        .get("slots")
        .and_then(Value::as_array)
        .ok_or("mh.sh list --json: no slots array")?;
    if raw.is_empty() {
        return Err("mh.sh list --json: zero slots (every slot always appears)".to_string());
    }
    let mut slots = Vec::with_capacity(raw.len());
    for (i, entry) in raw.iter().enumerate() {
        let obj = entry
            .as_object()
            .ok_or_else(|| format!("mh.sh list --json: slots[{i}] is not an object"))?;
        if let Some(missing) = SLOT_KEYS.iter().find(|k| !obj.contains_key(**k)) {
            return Err(format!(
                "mh.sh list --json: slots[{i}] has no `{missing}` key (schema v1 pins the key set)"
            ));
        }
        let parsed: SlotEntry = serde_json::from_value(entry.clone())
            .map_err(|e| format!("mh.sh list --json: slots[{i}] has a malformed field: {e}"))?;
        slots.push(parsed);
    }
    let mut numbers: Vec<u32> = slots.iter().map(|s| s.slot).collect();
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.len() != slots.len() {
        return Err("mh.sh list --json: duplicate slot numbers".to_string());
    }
    Ok(PoolSnapshot {
        template_revision,
        baseline,
        slots,
    })
}

/// The ConfigMap deploy.sh stores the rendered MH template in (contract §4,
/// "CLUSTER-SIDE INTERFACE"): data keys `deployment.json`, `revision`,
/// `baseline`. Data, never pod-consumed.
pub const TEMPLATE_CONFIGMAP: &str = "mh-template";

/// The rendered MH Deployment template, read from the LIVE
/// [`TEMPLATE_CONFIGMAP`] (what mh.sh instantiates every MH from).
///
/// # Panics
/// PRECONDITION: the ConfigMap is unreadable, or `deployment.json` is absent
/// or not a JSON object.
#[must_use]
pub fn template_deployment() -> Value {
    let cm = crate::fixtures::kube::kubectl_get_json(&[
        "configmap",
        TEMPLATE_CONFIGMAP,
        "-n",
        crate::NAMESPACE,
    ])
    .unwrap_or_else(|e| panic!("PRECONDITION (mh-template): {e}"));
    template_from_configmap(&cm).unwrap_or_else(|e| panic!("PRECONDITION (mh-template): {e}"))
}

/// PURE core of [`template_deployment`].
///
/// # Errors
/// No `data.deployment.json`, unparseable, or not a Deployment with a pod
/// template spec.
pub fn template_from_configmap(cm: &Value) -> Result<Value, String> {
    let raw = cm
        .pointer("/data/deployment.json")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("ConfigMap {TEMPLATE_CONFIGMAP} has no data key deployment.json"))?;
    let dep: Value = serde_json::from_str(raw)
        .map_err(|e| format!("{TEMPLATE_CONFIGMAP} deployment.json is not JSON: {e}"))?;
    if dep.get("kind").and_then(Value::as_str) != Some("Deployment")
        || dep.pointer("/spec/template/spec").is_none()
    {
        return Err(format!(
            "{TEMPLATE_CONFIGMAP} deployment.json is not a Deployment with spec.template.spec"
        ));
    }
    Ok(dep)
}

/// PURE: the template's pod template as a POD-SHAPED object
/// (`{metadata: {name, labels}, spec}`), so every per-pod check in
/// `fixtures::deployment_config` applies to the template unchanged.
///
/// # Panics
/// `dep` was not validated by [`template_from_configmap`].
#[must_use]
pub fn template_as_pod(dep: &Value) -> Value {
    serde_json::json!({
        "metadata": {
            "name": format!("configmap/{TEMPLATE_CONFIGMAP}:deployment.json"),
            "labels": dep.pointer("/spec/template/metadata/labels").cloned().unwrap_or(Value::Null),
        },
        "spec": dep.pointer("/spec/template/spec").cloned().expect("validated template"),
    })
}

// ---------------------------------------------------------------------------
// Mutations: `up` and owner-scoped teardown (contract §5, §6, §8(b)).
//
// Story 4 GC-2 lands the minimal surface its env-test needs. T-1 extends it
// here (`MhSlot::drain()`, `assert_baseline()`); it is not copied elsewhere.
// ---------------------------------------------------------------------------

/// The REASON every `STATUS=OK` carries (contract §3, I-5 amendment).
pub const REASON_OK: &str = "ok";
/// `up`'s failure when the Deployment never went Available. The pod is LEFT
/// IN PLACE (contract §3); owner teardown reclaims it.
pub const REASON_NOT_AVAILABLE: &str = "not-available-timeout";
/// Capacity: every slot is occupied (exit 3).
pub const REASON_NO_FREE_SLOT: &str = "mh-no-free-slot";
/// The `mh-pool` Lease is held (exit 5); the fixture retries within a bound
/// (contract §6).
pub const REASON_BUSY: &str = "mh-pool-busy";

/// Failure-only ceiling for retrying [`REASON_BUSY`]. The Lease covers only a
/// decision plus one create/delete call (contract §6, "seconds"), so a holder
/// beyond this is a stuck or foreign run, and the test fails loudly.
pub const BUSY_RETRY_BOUND: std::time::Duration = std::time::Duration::from_secs(60);
const BUSY_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// One mh.sh invocation's machine result: the final `MH_RESULT` line (contract
/// §2) and the `--json` stdout object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MhShResult {
    /// The `STATUS=` field: `OK` or `FAIL`.
    pub status: String,
    /// The `REASON=` token.
    pub reason: String,
    /// The `SLOT=` field, if any.
    pub slot: Option<u32>,
    /// The `DETAIL=` field, if any (always last; may contain spaces).
    pub detail: Option<String>,
    /// The stdout JSON object (`{"ok": ...}` on every path with `--json`).
    pub json: Value,
}

impl MhShResult {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.status == "OK"
    }
}

impl std::fmt::Display for MhShResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "STATUS={} REASON={}", self.status, self.reason)?;
        if let Some(s) = self.slot {
            write!(f, " SLOT={s}")?;
        }
        if let Some(d) = &self.detail {
            write!(f, " DETAIL={d}")?;
        }
        Ok(())
    }
}

/// PURE: parse mh.sh's stderr and `--json` stdout into an [`MhShResult`].
///
/// # Errors
/// No final `MH_RESULT` line; a malformed STATUS/REASON/SLOT; stdout not one
/// JSON object; or the stdout `ok` disagreeing with STATUS (two channels of
/// one verdict must agree, or neither is trusted).
pub fn parse_mh_sh_result(stderr: &str, stdout: &str) -> Result<MhShResult, String> {
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or("mh.sh printed nothing on stderr (no MH_RESULT line)")?;
    let rest = line
        .strip_prefix("MH_RESULT ")
        .ok_or_else(|| format!("mh.sh's last stderr line is not MH_RESULT: {line:?}"))?;
    // DETAIL is last and runs to end of line, so split it off first.
    let (fields, detail) = match rest.split_once(" DETAIL=") {
        Some((f, d)) => (f, Some(d.to_string())),
        None => (rest, None),
    };
    let mut status = None;
    let mut reason = None;
    let mut slot = None;
    for field in fields.split_whitespace() {
        match field.split_once('=') {
            Some(("STATUS", v)) if v == "OK" || v == "FAIL" => status = Some(v.to_string()),
            Some(("REASON", v)) if !v.is_empty() => reason = Some(v.to_string()),
            Some(("SLOT", v)) => {
                slot = Some(
                    v.parse::<u32>()
                        .map_err(|_| format!("MH_RESULT SLOT={v:?} is not a slot number"))?,
                );
            }
            _ => {
                return Err(format!(
                    "MH_RESULT has a malformed field {field:?}: {line:?}"
                ))
            }
        }
    }
    let status = status.ok_or_else(|| format!("MH_RESULT has no valid STATUS: {line:?}"))?;
    let reason = reason.ok_or_else(|| format!("MH_RESULT has no REASON: {line:?}"))?;
    let json: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("mh.sh --json stdout is not one JSON object ({e}); {line}"))?;
    let ok = json.get("ok").and_then(Value::as_bool);
    if ok != Some(status == "OK") {
        return Err(format!(
            "mh.sh's stdout ok={ok:?} disagrees with its MH_RESULT STATUS={status}"
        ));
    }
    Ok(MhShResult {
        status,
        reason,
        slot,
        detail,
        json,
    })
}

/// Run mh.sh with `args` (`--json` appended), retrying [`REASON_BUSY`] within
/// [`BUSY_RETRY_BOUND`]. Blocking: call it from `spawn_blocking`.
///
/// # Panics
/// PRECONDITION: mh.sh cannot be spawned, or its output breaks the machine
/// contract ([`parse_mh_sh_result`]). A contract-conforming FAIL is returned,
/// not panicked, so the caller can name its own reason token.
#[must_use]
pub fn run_mh_sh(args: &[&str]) -> MhShResult {
    let path = mh_sh_path();
    let deadline = std::time::Instant::now() + BUSY_RETRY_BOUND;
    loop {
        let out = Command::new(&path)
            .args(args)
            .arg("--json")
            .output()
            .unwrap_or_else(|e| {
                panic!(
                    "PRECONDITION (mh-sh-run): cannot run {}: {e}",
                    path.display()
                )
            });
        let result = parse_mh_sh_result(
            &String::from_utf8_lossy(&out.stderr),
            &String::from_utf8_lossy(&out.stdout),
        )
        .unwrap_or_else(|e| {
            panic!(
                "PRECONDITION (mh-sh-contract): `{} {}` (exit {}): {e}",
                path.display(),
                args.join(" "),
                out.status
            )
        });
        if result.reason != REASON_BUSY || std::time::Instant::now() > deadline {
            return result;
        }
        std::thread::sleep(BUSY_RETRY_INTERVAL);
    }
}

/// The per-run MH owner (contract §6: the `dark-tower.io/mh-owner` label =
/// "the Layer 7 per-run token for fixtures"). Every MH this run creates carries
/// it, so teardown is owner-scoped (`drain --owner <token> --all`) and needs no
/// slot number from any output. This matters because `up`'s
/// not-available-timeout leaves its pod in place.
///
/// [`RunOwner::teardown`] is the explicit, asserted cleanup. `Drop` is only a
/// backstop that PRINTS the reclaim command (contract §8(b)); it never cleans
/// up silently.
pub struct RunOwner {
    token: String,
    torn_down: bool,
}

impl RunOwner {
    /// The run's owner: the per-run org subdomain
    /// (`auth_client::org_subdomain`, `ENV_TEST_ORG_SUBDOMAIN`, exported by
    /// `scripts/layer7.sh`). It is already validated against the subdomain rule,
    /// which `validate-subdomain-regex-sync.sh` keeps byte-identical to mh.sh's
    /// `MH_OWNER_RE`. There is no second, hand-rolled owner predicate here.
    ///
    /// # Panics
    /// PRECONDITION: the variable is unset or invalid (no default, same as AC).
    #[must_use]
    pub fn for_run() -> Self {
        let token = crate::fixtures::auth_client::org_subdomain()
            .unwrap_or_else(|e| panic!("PRECONDITION (run-owner): {e}"));
        Self {
            token,
            torn_down: false,
        }
    }

    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The manual reclaim command, printed on every teardown failure and by
    /// the Drop backstop.
    #[must_use]
    pub fn reclaim_command(&self) -> String {
        format!(
            "{} drain --all --owner {}",
            mh_sh_path().display(),
            self.token
        )
    }

    /// `mh.sh up --owner <token>`: the next-fit slot, returned once the MH is
    /// Available (= MH `/ready` = RegisterMH accepted, contract §5). Blocking.
    /// The bound is mh.sh's own derived one; no test-side timeout is added.
    ///
    /// # Errors
    /// The contract-conforming FAIL result, for the caller to classify.
    pub fn up(&self) -> Result<SlotEntry, MhShResult> {
        let result = run_mh_sh(&["up", "--owner", &self.token]);
        if !result.is_ok() {
            return Err(result);
        }
        let entry: SlotEntry = serde_json::from_value(result.json.clone()).unwrap_or_else(|e| {
            panic!("PRECONDITION (mh-sh-contract): up's OK object is not a slot entry: {e}")
        });
        assert_eq!(
            entry.owner.as_deref(),
            Some(self.token.as_str()),
            "PRECONDITION (mh-sh-contract): up returned a slot not owned by this run"
        );
        Ok(entry)
    }

    /// `mh.sh drain --all --owner <token>`: the explicit run teardown. Blocking.
    ///
    /// # Errors
    /// A non-OK result. The message names the REASON and the reclaim command.
    pub fn teardown(mut self) -> Result<(), String> {
        self.torn_down = true;
        let result = run_mh_sh(&["drain", "--all", "--owner", &self.token]);
        if result.is_ok() {
            Ok(())
        } else {
            Err(format!(
                "TEARDOWN (mh-teardown-failed): owner {} not drained: {result}. Reclaim manually: \
                 {}",
                self.token,
                self.reclaim_command()
            ))
        }
    }
}

impl RunOwner {
    /// [`RunOwner::teardown`], then, only if it drained,
    /// [`gc_fleet::wait_removed_mh_unselectable`](crate::fixtures::gc_fleet::wait_removed_mh_unselectable):
    /// the run's removed MHs are no longer selectable by any GC replica before
    /// this returns. Every leg that removes an MH ends with this (44 now,
    /// T-1's legs next), so a later suite is never handed a dead handler.
    ///
    /// # Errors
    /// The teardown's error, or the settle wait's (`removed-mh-still-selectable`).
    pub async fn teardown_and_settle(
        self,
        prom: &crate::fixtures::metrics::PrometheusClient,
    ) -> Result<(), String> {
        tokio::task::spawn_blocking(move || self.teardown())
            .await
            .map_err(|e| format!("TEARDOWN (mh-teardown-failed): teardown task: {e}"))??;
        crate::fixtures::gc_fleet::wait_removed_mh_unselectable(prom).await
    }
}

impl Drop for RunOwner {
    fn drop(&mut self) {
        if !self.torn_down {
            eprintln!(
                "RunOwner {} dropped without teardown; reclaim its MHs with: {}",
                self.token,
                self.reclaim_command()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mh_result_parses_ok_and_fail_with_detail_to_end_of_line() {
        let ok = parse_mh_sh_result(
            "human text\nMH_RESULT STATUS=OK REASON=ok SLOT=3 DETAIL=created timeout=90s\n",
            r#"{"ok":true,"slot":3}"#,
        )
        .unwrap();
        assert!(ok.is_ok());
        assert_eq!(ok.reason, "ok");
        assert_eq!(ok.slot, Some(3));
        assert_eq!(ok.detail.as_deref(), Some("created timeout=90s"));

        let fail = parse_mh_sh_result(
            "MH_RESULT STATUS=FAIL REASON=not-available-timeout SLOT=2 DETAIL=timeout=90s; mh-s2 left in place\n",
            r#"{"ok":false,"reason":"not-available-timeout","slot":2,"detail":"x"}"#,
        )
        .unwrap();
        assert!(!fail.is_ok());
        assert_eq!(fail.reason, REASON_NOT_AVAILABLE);
        assert_eq!(fail.slot, Some(2));
        assert_eq!(
            fail.to_string(),
            "STATUS=FAIL REASON=not-available-timeout SLOT=2 DETAIL=timeout=90s; mh-s2 left in place"
        );

        let no_slot = parse_mh_sh_result(
            "MH_RESULT STATUS=FAIL REASON=mh-no-free-slot\n",
            r#"{"ok":false,"reason":"mh-no-free-slot","slot":null,"detail":null}"#,
        )
        .unwrap();
        assert_eq!(no_slot.slot, None);
        assert_eq!(no_slot.detail, None);
    }

    #[test]
    fn mh_result_refusals_are_each_named() {
        assert!(parse_mh_sh_result("", "{}")
            .unwrap_err()
            .contains("nothing on stderr"));
        assert!(parse_mh_sh_result("oops\n", "{}")
            .unwrap_err()
            .contains("not MH_RESULT"));
        assert!(
            parse_mh_sh_result("MH_RESULT STATUS=MAYBE REASON=ok\n", r#"{"ok":true}"#)
                .unwrap_err()
                .contains("malformed field")
        );
        assert!(
            parse_mh_sh_result("MH_RESULT STATUS=OK\n", r#"{"ok":true}"#)
                .unwrap_err()
                .contains("no REASON")
        );
        assert!(
            parse_mh_sh_result("MH_RESULT STATUS=OK REASON=ok SLOT=x\n", r#"{"ok":true}"#)
                .unwrap_err()
                .contains("not a slot number")
        );
        assert!(
            parse_mh_sh_result("MH_RESULT STATUS=OK REASON=ok\n", "not json")
                .unwrap_err()
                .contains("not one JSON")
        );
        assert!(
            parse_mh_sh_result("MH_RESULT STATUS=OK REASON=ok\n", r#"{"ok":false}"#)
                .unwrap_err()
                .contains("disagrees")
        );
    }

    /// The REASON tokens this fixture branches on exist in mh.sh's token
    /// array (its ONE home), so a renamed token fails here, not as a silent
    /// misclassification at run time.
    #[test]
    fn branched_reasons_are_in_mh_sh_tokens() {
        let text = std::fs::read_to_string(crate::repo_root().join("infra/kind/scripts/mh.sh"))
            .expect("read mh.sh");
        let tokens: Vec<&str> = text
            .split_once("readonly -a MH_TOKENS=(")
            .and_then(|(_, rest)| rest.split_once("\n)"))
            .map(|(body, _)| body)
            .expect("mh.sh defines `readonly -a MH_TOKENS=( ... )`")
            .lines()
            .filter_map(|l| l.trim().strip_prefix('"')?.split_once(':'))
            .map(|(token, _)| token)
            .collect();
        // Vacuity control: the contract lists 17 tokens; a broken extraction
        // yields few or none.
        assert!(
            tokens.len() > 10,
            "extraction produced too few tokens: {tokens:?}"
        );
        for token in [
            REASON_OK,
            REASON_NOT_AVAILABLE,
            REASON_NO_FREE_SLOT,
            REASON_BUSY,
        ] {
            assert!(
                tokens.contains(&token),
                "mh.sh's MH_TOKENS has no entry for {token:?}: {tokens:?}"
            );
        }
    }

    fn entry(slot: u32, state: &str, pod: Option<&str>) -> Value {
        json!({"slot": slot, "host_port": 4440 + slot, "address": format!("https://127.0.0.1:{}", 4440 + slot),
               "pod_ip": pod.map(|_| "10.244.0.9"), "pod_uid": pod.map(|_| "u"), "state": state,
               "terminating_past_grace": false, "deployment": pod.map(|_| format!("mh-s{slot}")),
               "pod": pod, "revision": pod.map(|_| "r1"), "stale": false,
               "owner": pod.map(|_| "baseline"), "restart_count": pod.map(|_| 0)})
    }

    fn doc(slots: Vec<Value>) -> Value {
        json!({"ok": true, "schema_version": 1, "template_revision": "r1", "baseline": 2, "slots": slots})
    }

    #[test]
    fn a_v1_document_parses_with_occupied_and_serving_views() {
        let d = doc(vec![
            entry(0, "serving", Some("mh-s0-a")),
            entry(1, "free", None),
            entry(2, "starting", Some("mh-s2-b")),
        ]);
        let p = parse_list(&d.to_string()).expect("parses");
        assert_eq!(p.baseline, 2);
        assert_eq!(p.template_revision, "r1");
        assert_eq!(p.slots.len(), 3);
        assert_eq!(p.occupied().len(), 2);
        assert_eq!(p.serving().len(), 1);
        assert_eq!(p.serving()[0].slot, 0);
    }

    #[test]
    fn refusals_are_each_named() {
        let mut not_ok = doc(vec![entry(0, "free", None)]);
        not_ok["ok"] = json!(false);
        assert!(parse_list(&not_ok.to_string())
            .unwrap_err()
            .contains("`ok`"));

        let mut v2 = doc(vec![entry(0, "free", None)]);
        v2["schema_version"] = json!(2);
        assert!(parse_list(&v2.to_string())
            .unwrap_err()
            .contains("schema_version"));

        assert!(parse_list(&doc(vec![]).to_string())
            .unwrap_err()
            .contains("zero slots"));

        let mut dropped = entry(0, "free", None);
        dropped.as_object_mut().unwrap().remove("pod");
        assert!(parse_list(&doc(vec![dropped]).to_string())
            .unwrap_err()
            .contains("no `pod` key"));

        let dup = doc(vec![entry(1, "free", None), entry(1, "free", None)]);
        assert!(parse_list(&dup.to_string())
            .unwrap_err()
            .contains("duplicate"));

        let bad_state = doc(vec![entry(0, "zombie", None)]);
        assert!(parse_list(&bad_state.to_string())
            .unwrap_err()
            .contains("malformed"));

        assert!(parse_list("not json").unwrap_err().contains("not one JSON"));
    }

    #[test]
    fn template_is_read_from_deployment_json_and_projected_as_a_pod() {
        let dep = json!({"kind": "Deployment", "spec": {"template": {
            "metadata": {"labels": {"app": "mh-service"}},
            "spec": {"terminationGracePeriodSeconds": 35}}}});
        let cm =
            json!({"data": {"deployment.json": dep.to_string(), "revision": "r", "baseline": "2"}});
        let got = template_from_configmap(&cm).expect("valid");
        let pod = template_as_pod(&got);
        assert_eq!(
            pod.pointer("/spec/terminationGracePeriodSeconds"),
            Some(&json!(35))
        );
        assert_eq!(
            pod.pointer("/metadata/labels/app"),
            Some(&json!("mh-service"))
        );
        assert!(template_from_configmap(&json!({"data": {}}))
            .unwrap_err()
            .contains("no data key"));
        let svc = json!({"data": {"deployment.json": "{\"kind\":\"Service\"}"}});
        assert!(template_from_configmap(&svc)
            .unwrap_err()
            .contains("not a Deployment"));
    }

    #[test]
    fn mh_sh_path_refuses_unset_relative_and_missing() {
        assert!(mh_sh_path_from(None).unwrap_err().contains("unset"));
        assert!(mh_sh_path_from(Some(PathBuf::new()))
            .unwrap_err()
            .contains("unset"));
        assert!(mh_sh_path_from(Some("infra/kind/scripts/mh.sh".into()))
            .unwrap_err()
            .contains("not absolute"));
        assert!(mh_sh_path_from(Some("/nonexistent/mh.sh".into()))
            .unwrap_err()
            .contains("not an existing file"));
        let real = crate::repo_root().join("infra/kind/scripts/mh.sh");
        assert_eq!(mh_sh_path_from(Some(real.clone())).unwrap(), real);
    }

    /// [`SLOT_KEYS`] == the key list of mh.sh's `MH_JQ_PUBLIC` (the ONE home
    /// of the v1 slot schema), read from the committed script.
    #[test]
    fn slot_keys_match_mh_sh_public_schema() {
        let text = std::fs::read_to_string(crate::repo_root().join("infra/kind/scripts/mh.sh"))
            .expect("read mh.sh");
        let line = text
            .lines()
            .find(|l| l.starts_with("readonly MH_JQ_PUBLIC="))
            .expect("mh.sh defines MH_JQ_PUBLIC");
        let inner = line
            .split_once("def public: {")
            .and_then(|(_, rest)| rest.split_once('}'))
            .map(|(keys, _)| keys)
            .expect("MH_JQ_PUBLIC has the `def public: {...}` shape");
        let keys: Vec<&str> = inner.split(',').map(str::trim).collect();
        assert!(keys.len() > 5, "extraction produced too few keys: {keys:?}");
        assert_eq!(
            keys, SLOT_KEYS,
            "mh.sh's list --json slot key set drifted from SLOT_KEYS"
        );
    }
}
