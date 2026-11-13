//! The MH pool as `infra/kind/scripts/mh.sh list --json` reports it (ADR-0039
//! §7; machine contract
//! `docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt`
//! §4). Story 4 I-2 landed the READ half ([`list`]); GC-2 the first mutations;
//! T-1 the full fixture of contract §8: [`MhPool`] (`snapshot`, `up`,
//! `assert_baseline`, owner teardown), [`MhSlot`] (`drain`, `drain_no_wait`,
//! `kill`, `freeze`) and [`FreezeGuard`], over ONE mh.sh runner with the BUSY
//! retry and a strict `MH_RESULT` parse.
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
    snapshot_at(&mh_sh_path())
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
// Mutations (contract §3, §5, §6, §8): the ONE Rust home for driving the pool.
//
// `MhPool` is the run-scoped handle (owner = the Layer 7 per-run token),
// `MhSlot` one run-owned MH, `FreezeGuard` a SIGSTOPped one. Every pub fn that
// changes the pool or an MH process carries the doc marker "Pool-mutating."; the
// `#[serial]` hygiene scan (`crate::resilience_hygiene`) derives its API set from
// that marker, so a new mutating verb cannot escape serialization.
//
// The kill / freeze / resume RECIPE (debug container, uid, pid lookup, never
// PID 1, shareProcessNamespace preflight, liveness budget) lives in ONE place,
// `mh.sh signal` (contract §5 amendment). This module keeps only the outcome
// assertions. Its TypeScript twin is `packages/web-app/e2e/mhPool.ts`.
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

/// mh.sh's token table, `token -> exit code` (contract §3). Pinned in BOTH
/// directions, codes included, against mh.sh's `MH_TOKENS` array (its one
/// home) by `mh_tokens_match_mh_sh_both_directions`. A REASON outside this
/// table, or an exit code outside its token's group, is a contract break.
pub const MH_TOKENS: &[(&str, i32)] = &[
    ("ok", 0),
    ("mh-rollout-failed", 1),
    ("drain-timeout", 1),
    ("not-available-timeout", 1),
    ("kubectl-failed", 1),
    ("internal-error", 1),
    ("signal-failed", 1),
    ("debug-container-timeout", 1),
    ("usage", 2),
    ("no-kubeconfig", 2),
    ("no-template", 2),
    ("prerequisite-missing", 2),
    ("mh-no-share-process-namespace", 2),
    ("mh-no-free-slot", 3),
    ("last-serving-mh", 4),
    ("stuck-terminating", 4),
    ("foreign-owner", 4),
    ("mh-not-serving", 4),
    ("mh-pool-busy", 5),
];

/// The exit code mh.sh pairs with `token`, if it is a known token.
#[must_use]
pub fn token_exit_code(token: &str) -> Option<i32> {
    MH_TOKENS
        .iter()
        .find(|(t, _)| *t == token)
        .map(|(_, code)| *code)
}

// Fixture-side failure tokens (NOT mh.sh's; each has this one home, and
// `packages/web-app/tests/mhPool.test.ts` pins its TS copies to these lines).
/// A foreign owner or a non-baseline pool (contract §7).
pub const FOREIGN_MH_ACTIVITY: &str = "foreign-mh-activity";
/// Something is still owned by this run after teardown.
pub const MH_RUN_LEAK: &str = "mh-run-leak";
/// A freeze outlived the liveness-probe budget: kubelet may have restarted
/// the MH, so the leg would observe a restart, not a zombie.
pub const FREEZE_BUDGET_EXCEEDED: &str = "freeze-budget-exceeded";
/// A freeze/resume cycle changed restart_count or pod_uid (a kubelet restart
/// or a different incarnation passed off as a zombie).
pub const RESTART_COUNT_CHANGED: &str = "restart-count-changed";
/// A KILL never produced an in-place restart (restart_count + 1, same pod_uid).
pub const RESTART_NOT_OBSERVED: &str = "restart-not-observed";
/// Prometheus never scraped the MH (`up == 1` by pod IP).
pub const MH_SCRAPE_UP_TIMEOUT: &str = "mh-scrape-up-timeout";
/// Every fixture-side token, for the Rust disjointness check against
/// [`MH_TOKENS`] (the TS drift pin reads the individual consts above).
pub const FIXTURE_TOKENS: &[&str] = &[
    FOREIGN_MH_ACTIVITY,
    MH_RUN_LEAK,
    FREEZE_BUDGET_EXCEEDED,
    RESTART_COUNT_CHANGED,
    RESTART_NOT_OBSERVED,
    MH_SCRAPE_UP_TIMEOUT,
];

/// Failure-only ceiling for retrying [`REASON_BUSY`]. The Lease covers only a
/// decision plus one create/delete call (contract §6, "seconds"); a crashed
/// holder's Lease is reclaimable after `MH_LEASE_S` + `MH_LEASE_FUTURE_SKEW_S`.
/// This bound must outlast that plus one interval (pinned against mh.sh by
/// `busy_bound_outlasts_a_crashed_holders_lease`), so exhaustion means a LIVE
/// concurrent mutator, never a stale Lease.
pub const BUSY_RETRY_BOUND: std::time::Duration = std::time::Duration::from_secs(60);
/// Sleep between BUSY attempts.
pub const BUSY_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// One mh.sh invocation's machine result: the `MH_RESULT` line (contract §2),
/// the exit code, and the `--json` stdout object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MhShResult {
    /// The `STATUS=` field: `OK` or `FAIL`.
    pub status: String,
    /// The `REASON=` token (a [`MH_TOKENS`] member).
    pub reason: String,
    /// The `SLOT=` field, if any.
    pub slot: Option<u32>,
    /// The `DETAIL=` field, if any (always last; may contain spaces). mh.sh
    /// builds it from validated fields only (contract §2 amendment).
    pub detail: Option<String>,
    /// The process exit code.
    pub exit_code: i32,
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
        write!(
            f,
            "STATUS={} REASON={} EXIT={}",
            self.status, self.reason, self.exit_code
        )?;
        if let Some(s) = self.slot {
            write!(f, " SLOT={s}")?;
        }
        if let Some(d) = &self.detail {
            write!(f, " DETAIL={d}")?;
        }
        Ok(())
    }
}

/// PURE: parse one mh.sh run (stderr, `--json` stdout, exit code) into an
/// [`MhShResult`]. The rejection cases, by name (the TS twin in
/// `packages/web-app/e2e/mhPool.ts` carries the SAME names):
///
/// - `one-line`: exactly ONE `MH_RESULT` line, and it is the last non-empty
///   stderr line (contract §2);
/// - `fields`: STATUS is OK|FAIL, REASON non-empty, SLOT a number;
/// - `known-token`: REASON is in [`MH_TOKENS`];
/// - `exit-group`: the exit code is the token's group (contract §3);
/// - `ok-reason`: STATUS=OK iff REASON=ok;
/// - `json-ok-agrees`: stdout is one JSON object whose `ok` equals STATUS=OK.
///
/// # Errors
/// The failed case's name and the reason. Never raw stdout/stderr beyond the
/// `MH_RESULT` line itself.
pub fn parse_mh_sh_result(
    stderr: &str,
    stdout: &str,
    exit_code: Option<i32>,
) -> Result<MhShResult, String> {
    let result_lines = stderr
        .lines()
        .filter(|l| l.starts_with("MH_RESULT "))
        .count();
    if result_lines > 1 {
        return Err(format!(
            "one-line: mh.sh printed {result_lines} MH_RESULT lines (contract §2: exactly one)"
        ));
    }
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or("one-line: mh.sh printed nothing on stderr (no MH_RESULT line)")?;
    let rest = line
        .strip_prefix("MH_RESULT ")
        .ok_or("one-line: mh.sh's last stderr line is not its MH_RESULT line")?;
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
                slot =
                    Some(v.parse::<u32>().map_err(|_| {
                        format!("fields: MH_RESULT SLOT={v:?} is not a slot number")
                    })?);
            }
            _ => return Err(format!("fields: MH_RESULT has a malformed field {field:?}")),
        }
    }
    let status = status.ok_or("fields: MH_RESULT has no valid STATUS")?;
    let reason = reason.ok_or("fields: MH_RESULT has no REASON")?;
    let group = token_exit_code(&reason).ok_or_else(|| {
        format!("known-token: REASON={reason} is not in mh.sh's token table (MH_TOKENS)")
    })?;
    let exit_code = exit_code.ok_or("exit-group: mh.sh was killed by a signal (no exit code)")?;
    if exit_code != group {
        return Err(format!(
            "exit-group: REASON={reason} belongs to exit {group}, mh.sh exited {exit_code}"
        ));
    }
    if (status == "OK") != (reason == REASON_OK) {
        return Err(format!(
            "ok-reason: STATUS={status} with REASON={reason} (OK iff REASON=ok)"
        ));
    }
    let json: Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        format!("json-ok-agrees: mh.sh --json stdout is not one JSON object ({e}); {line}")
    })?;
    let ok = json.get("ok").and_then(Value::as_bool);
    if ok != Some(status == "OK") {
        return Err(format!(
            "json-ok-agrees: mh.sh's stdout ok={ok:?} disagrees with its MH_RESULT STATUS={status}"
        ));
    }
    Ok(MhShResult {
        status,
        reason,
        slot,
        detail,
        exit_code,
        json,
    })
}

/// One raw mh.sh run, before parsing (the runner seam).
#[derive(Clone, Debug)]
pub struct RawRun {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// PURE (given its seams): run `runner` until it is not [`REASON_BUSY`],
/// sleeping `interval` between attempts, for at most `bound` of sleeping
/// (attempts = bound / interval + 1). Count-based, so the sleeping total is
/// never less than `bound` whatever each run costs.
///
/// Only BUSY is retried; every other result (OK or a contract-conforming
/// FAIL) is returned on the attempt that produced it.
///
/// # Errors
/// A contract break ([`parse_mh_sh_result`]), or BUSY exhaustion naming the
/// attempts, the holder (`DETAIL=holder=<owner>`) and the Lease TTL relation.
pub fn run_with_busy_retry(
    what: &str,
    mut runner: impl FnMut() -> RawRun,
    bound: std::time::Duration,
    interval: std::time::Duration,
    mut sleep: impl FnMut(std::time::Duration),
) -> Result<MhShResult, String> {
    let attempts = bound.as_millis() / interval.as_millis().max(1) + 1;
    for attempt in 1..=attempts {
        let raw = runner();
        let result = parse_mh_sh_result(&raw.stderr, &raw.stdout, raw.exit_code)
            .map_err(|e| format!("PRECONDITION (mh-sh-contract): `mh.sh {what}`: {e}"))?;
        if result.reason != REASON_BUSY {
            return Ok(result);
        }
        if attempt == attempts {
            return Err(format!(
                "BUSY ({REASON_BUSY}): `mh.sh {what}` still busy after {attempts} attempts over \
                 {bound:?} (> a crashed holder's lease MH_LEASE_S + MH_LEASE_FUTURE_SKEW_S): \
                 {} is renewing, so this is a LIVE concurrent mutator, not a stale Lease. \
                 Layer 7 assumes one suite per cluster; inspect `$DT_MH_SH list`",
                result.detail.as_deref().unwrap_or("holder=<unknown>")
            ));
        }
        sleep(interval);
    }
    unreachable!("the loop returns on its last attempt")
}

/// Run `path args --json` once.
fn raw_run(path: &std::path::Path, cluster: Option<&str>, args: &[&str]) -> RawRun {
    let mut cmd = Command::new(path);
    // The validated cluster goes INTO the child, so the cluster mh.sh mutates
    // is by construction the one every printed command names.
    if let Some(c) = cluster {
        cmd.env(DT_CLUSTER_NAME_ENV, c);
    }
    let out = cmd.args(args).arg("--json").output().unwrap_or_else(|e| {
        panic!(
            "PRECONDITION (mh-sh-run): cannot run {}: {e}",
            path.display()
        )
    });
    RawRun {
        exit_code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Run mh.sh at `path` with `args` (`--json` appended), retrying BUSY within
/// [`BUSY_RETRY_BOUND`]. Blocking: call it from `spawn_blocking`.
///
/// # Panics
/// PRECONDITION: mh.sh cannot be spawned, or breaks the machine contract; or
/// BUSY exhausted. A contract-conforming FAIL is RETURNED, so the caller can
/// name its own reason.
#[must_use]
pub fn run_mh_sh_at(path: &std::path::Path, args: &[&str]) -> MhShResult {
    run_mh_sh_in(path, None, args)
}

/// [`run_mh_sh_at`] with `DT_CLUSTER_NAME` set to `cluster` in the child (every
/// pool-handle call: the cluster mutated is the cluster printed).
///
/// # Panics
/// As [`run_mh_sh_at`].
#[must_use]
pub fn run_mh_sh_in(path: &std::path::Path, cluster: Option<&str>, args: &[&str]) -> MhShResult {
    run_with_busy_retry(
        &args.join(" "),
        || raw_run(path, cluster, args),
        BUSY_RETRY_BOUND,
        BUSY_RETRY_INTERVAL,
        std::thread::sleep,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// [`run_mh_sh_at`] with the path from [`DT_MH_SH_ENV`].
///
/// # Panics
/// As [`run_mh_sh_at`], or PRECONDITION when the variable is unset.
#[must_use]
pub fn run_mh_sh(args: &[&str]) -> MhShResult {
    run_mh_sh_at(&mh_sh_path(), args)
}

/// The committed mh.sh, repo-relative: the ONE home of the contract
/// constants the hermetic pins read (`MH_TOKENS`, `MH_JQ_PUBLIC`,
/// `MH_SCHEMA_VERSION`, the Lease constants, `MH_DEBUG_IMAGE`). Distinct from
/// [`DT_MH_SH_ENV`], the RUNNABLE seam Layer 7 exports.
pub const MH_SH_REPO_PATH: &str = "infra/kind/scripts/mh.sh";

/// The committed mh.sh source ([`MH_SH_REPO_PATH`]).
///
/// # Panics
/// The file is unreadable.
#[must_use]
pub fn committed_mh_sh_source() -> String {
    let path = crate::repo_root().join(MH_SH_REPO_PATH);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("PRECONDITION: read {}: {e}", path.display()))
}

/// The argv of owner teardown: the ONE spelling, both executed
/// ([`MhPool::teardown`]) and printed ([`reclaim_command`]).
#[must_use]
pub fn teardown_args(owner: &str) -> Vec<String> {
    ["drain", "--all", "--owner", owner]
        .map(str::to_string)
        .to_vec()
}

/// The argv of `mh.sh signal`: the ONE spelling, executed by
/// [`MhSlot::freeze`] / [`MhSlot::kill`] / [`FreezeGuard::resume`] and the
/// Drop backstop, and printed by [`signal_command`].
#[must_use]
pub fn signal_args(slot: u32, signal: Signal, owner: &str) -> Vec<String> {
    vec![
        "signal".to_string(),
        slot.to_string(),
        signal.as_str().to_string(),
        "--owner".to_string(),
        owner.to_string(),
    ]
}

fn as_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// PURE: the paste-ready command line for `args` (mh.sh selects its cluster
/// from `DT_CLUSTER_NAME`, so it is part of the line).
fn command_line(path: &std::path::Path, cluster: &str, args: &[String]) -> String {
    format!(
        "DT_CLUSTER_NAME={cluster} {} {}",
        path.display(),
        args.join(" ")
    )
}

/// PURE: the ONE manual reclaim command (teardown errors, the Drop backstop,
/// the foreign-activity hint): byte-for-byte the teardown the fixture runs.
#[must_use]
pub fn reclaim_command(path: &std::path::Path, cluster: &str, owner: &str) -> String {
    command_line(path, cluster, &teardown_args(owner))
}

/// The argv of the leg-B baseline drain (story 4 MC-F; contract §8 MC-F
/// amendment): `drain <k> --owner baseline --no-wait`. NEVER `--force`: the
/// matching owner clears foreign-owner, and `last-serving-mh` stays live, so
/// the leg can never remove the last serving MH. The ONE spelling.
#[must_use]
pub fn baseline_drain_args(slot: u32) -> Vec<String> {
    vec![
        "drain".to_string(),
        slot.to_string(),
        "--owner".to_string(),
        BASELINE_OWNER.to_string(),
        "--no-wait".to_string(),
    ]
}

/// The argv of one baseline top-up, the same verb deploy.sh's
/// `converge_mh_pool` runs: `up --owner baseline`. The ONE spelling, both
/// executed ([`restore_baseline_at`]) and printed
/// ([`baseline_restore_command`]).
#[must_use]
pub fn baseline_restore_args() -> Vec<String> {
    ["up", "--owner", BASELINE_OWNER]
        .map(str::to_string)
        .to_vec()
}

/// PURE: the paste-ready baseline restore command (run it once per missing
/// baseline MH, or re-run deploy.sh, which tops the pool back up to B).
#[must_use]
pub fn baseline_restore_command(path: &std::path::Path, cluster: &str) -> String {
    command_line(path, cluster, &baseline_restore_args())
}

/// PURE: how many baseline MHs the pool is SHORT of B. Counted from the
/// pool's state, never from a "did I drain" flag: a leg that failed before
/// its drain, or whose drain was refused, has deficit 0, so a restore can
/// never add a B+1-th baseline MH. `serving` and `starting` count (a starting
/// one is already on its way); `draining`/`terminating` do not (they are
/// leaving).
#[must_use]
pub fn baseline_deficit(snapshot: &PoolSnapshot) -> u32 {
    let present = snapshot
        .slots
        .iter()
        .filter(|s| {
            s.owner.as_deref() == Some(BASELINE_OWNER)
                && matches!(s.state, SlotState::Serving | SlotState::Starting)
        })
        .count();
    let present = u32::try_from(present).unwrap_or(u32::MAX);
    snapshot.baseline.saturating_sub(present)
}

/// PURE: the manual `mh.sh signal` command for `slot`.
#[must_use]
pub fn signal_command(
    path: &std::path::Path,
    cluster: &str,
    slot: u32,
    signal: Signal,
    owner: &str,
) -> String {
    command_line(path, cluster, &signal_args(slot, signal, owner))
}

/// PURE core of the `DT_CLUSTER_NAME` read: mh.sh would otherwise fall back to
/// its default cluster and mutate a pool the fixture's kubectl/Prometheus
/// reads never see.
///
/// # Errors
/// Unset or empty.
pub fn dt_cluster_name_from(value: Option<String>) -> Result<String, String> {
    value.filter(|c| !c.trim().is_empty()).ok_or_else(|| {
        format!(
            "{DT_CLUSTER_NAME_ENV} is unset or empty; scripts/layer7.sh exports it (the devloop \
             cluster mh.sh targets as kind-<name>). Outside Layer 7 set it yourself; there is no \
             default, because mh.sh's own default could be another cluster"
        )
    })
}

/// The env var mh.sh selects its cluster from (`kind-<name>`).
pub const DT_CLUSTER_NAME_ENV: &str = "DT_CLUSTER_NAME";

/// The signals `mh.sh signal` accepts (a closed set; mh.sh validates it again).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Kill,
    Stop,
    Cont,
}

impl Signal {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Signal::Kill => "KILL",
            Signal::Stop => "STOP",
            Signal::Cont => "CONT",
        }
    }
}

/// `process_state` after KILL: the process is gone (restart evidence comes
/// from restart_count and pod_uid, never from a state letter).
pub const PROCESS_GONE: &str = "gone";
/// `process_state` of a SIGSTOPped process (`/proc/<pid>/stat`).
pub const PROCESS_STOPPED: &str = "T";

/// `mh.sh signal --json`'s OK object (contract §5 amendment).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalOutcome {
    pub entry: SlotEntry,
    pub process_state: String,
    /// STOP only: `(failureThreshold - 1) x periodSeconds + timeoutSeconds` of
    /// the LIVE pod's mh-service livenessProbe, derived by mh.sh.
    pub liveness_budget: Option<std::time::Duration>,
    /// KILL only: mh.sh's own derived `up` bound (probe window + margin), the
    /// failure-only ceiling for the in-place restart to serve again.
    pub restart_budget: Option<std::time::Duration>,
}

/// PURE: validate `mh.sh signal`'s OK object for `signal`.
///
/// # Errors
/// Not a slot entry; no `process_state`; a state that contradicts the signal
/// (KILL -> `gone`; STOP -> `T`; CONT -> neither); STOP without a positive
/// `liveness_budget_seconds`; KILL without a positive `restart_budget_seconds`.
pub fn parse_signal(json: &Value, signal: Signal) -> Result<SignalOutcome, String> {
    let entry: SlotEntry = serde_json::from_value(json.clone())
        .map_err(|e| format!("mh.sh signal's OK object is not a slot entry: {e}"))?;
    let process_state = json
        .get("process_state")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("mh.sh signal's OK object has no process_state")?
        .to_string();
    let consistent = match signal {
        Signal::Kill => process_state == PROCESS_GONE,
        Signal::Stop => process_state == PROCESS_STOPPED,
        Signal::Cont => process_state != PROCESS_STOPPED && process_state != PROCESS_GONE,
    };
    if !consistent {
        return Err(format!(
            "mh.sh signal {} reported OK with process_state={process_state}",
            signal.as_str()
        ));
    }
    let seconds = |key: &str| -> Result<Option<std::time::Duration>, String> {
        match json.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .filter(|s| *s > 0)
                .map(|s| Some(std::time::Duration::from_secs(s)))
                .ok_or_else(|| format!("{key} is not a positive integer")),
        }
    };
    let liveness_budget = seconds("liveness_budget_seconds")?;
    let restart_budget = seconds("restart_budget_seconds")?;
    if signal == Signal::Stop && liveness_budget.is_none() {
        return Err("mh.sh signal STOP reported no liveness_budget_seconds".to_string());
    }
    if signal == Signal::Kill && restart_budget.is_none() {
        return Err("mh.sh signal KILL reported no restart_budget_seconds".to_string());
    }
    Ok(SignalOutcome {
        entry,
        process_state,
        liveness_budget,
        restart_budget,
    })
}

/// PURE: the freeze-budget verdict. `elapsed` runs from BEFORE the STOP call
/// to AFTER the CONT call returned: the process is stopped somewhere inside
/// the first call and resumed somewhere inside the second, so this bounds the
/// real frozen time from above (never under-counts). The caller checks it
/// before the incarnation check, so an overrun reports as this, not as the
/// kubelet restart it caused.
///
/// # Errors
/// [`FREEZE_BUDGET_EXCEEDED`] when `elapsed >= budget`.
pub fn freeze_overrun(
    elapsed: std::time::Duration,
    budget: std::time::Duration,
) -> Result<(), String> {
    if elapsed >= budget {
        return Err(format!(
            "{FREEZE_BUDGET_EXCEEDED}: the MH was frozen for {elapsed:?}, at or past the live \
             liveness-probe budget {budget:?} ((failureThreshold-1) x periodSeconds + \
             timeoutSeconds): kubelet may already have restarted it, so a zombie leg would \
             observe a restart. Resume sooner (CONT right after MC declares the handler dead)"
        ));
    }
    Ok(())
}

/// PURE: `after` is the same incarnation as `before` with an unchanged
/// restart count (a freeze/resume never restarts the container).
///
/// # Errors
/// [`RESTART_COUNT_CHANGED`], naming both.
pub fn assert_same_incarnation(before: &SlotEntry, after: &SlotEntry) -> Result<(), String> {
    if before.pod_uid != after.pod_uid || before.restart_count != after.restart_count {
        return Err(format!(
            "{RESTART_COUNT_CHANGED}: slot {} went pod_uid {:?} restart_count {:?} -> pod_uid {:?} \
             restart_count {:?} across freeze/resume: a kubelet restart or another incarnation, \
             not a zombie",
            before.slot, before.pod_uid, before.restart_count, after.pod_uid, after.restart_count
        ));
    }
    Ok(())
}

/// PURE: `after` is `before` restarted IN PLACE (same pod_uid,
/// restart_count + 1) and serving again.
#[must_use]
pub fn restarted_in_place(before: &SlotEntry, after: &SlotEntry) -> bool {
    after.pod_uid == before.pod_uid
        && after.pod_uid.is_some()
        && before.restart_count.map(|c| c + 1) == after.restart_count
        && after.state == SlotState::Serving
}

/// PURE: the baseline predicate (contract §7, §8(c)). Every occupied slot's
/// owner is `baseline` or `run`, and the baseline-owned SERVING count is B
/// (`snapshot.baseline`, never a constant). `stale` still counts: an
/// upgrade-pending MH still serves. A baseline MH that is starting, draining
/// or terminating does not count.
///
/// # Errors
/// [`FOREIGN_MH_ACTIVITY`], naming each foreign owner and slot, B vs observed,
/// and how to reclaim.
pub fn baseline_verdict(
    snapshot: &PoolSnapshot,
    run: &str,
    reclaim: impl Fn(&str) -> String,
    restore: &str,
) -> Result<(), String> {
    let mut problems = Vec::new();
    let mut foreign_owners = std::collections::BTreeSet::new();
    for s in snapshot.occupied() {
        match s.owner.as_deref() {
            Some(BASELINE_OWNER) => {}
            Some(o) if o == run => {}
            other => {
                problems.push(format!(
                    "slot {} ({:?}) owned by {other:?}",
                    s.slot, s.state
                ));
                if let Some(o) = other {
                    foreign_owners.insert(o.to_string());
                }
            }
        }
    }
    let serving = snapshot
        .slots
        .iter()
        .filter(|s| s.state == SlotState::Serving && s.owner.as_deref() == Some(BASELINE_OWNER))
        .count();
    let short = serving < snapshot.baseline as usize;
    if serving != snapshot.baseline as usize {
        problems.push(format!(
            "baseline-owned serving MHs: {serving}, B = {} (mh.sh list `baseline`)",
            snapshot.baseline
        ));
    }
    if problems.is_empty() {
        return Ok(());
    }
    let hints: Vec<String> = foreign_owners.iter().map(|o| reclaim(o)).collect();
    Err(format!(
        "{FOREIGN_MH_ACTIVITY}: the MH pool is not {{baseline B, this run {run}}}: {}. Layer 7 \
         supports one suite per cluster; a concurrent run, a leaked earlier run or a manual MH \
         is mutating the pool. Inspect `$DT_MH_SH list`{}{}",
        problems.join("; "),
        if hints.is_empty() {
            String::new()
        } else {
            format!("; reclaim a leaked run with: {}", hints.join(" ; "))
        },
        if short {
            format!(
                "; the baseline is SHORT: restore with `{restore}` once per missing baseline MH, \
                 or re-run infra/kind/scripts/deploy.sh"
            )
        } else {
            String::new()
        }
    ))
}

/// The owner deploy.sh labels its MHs with (contract §6).
pub const BASELINE_OWNER: &str = "baseline";

/// PURE: nothing is left owned by `run` and nothing is stuck past its grace.
///
/// # Errors
/// [`MH_RUN_LEAK`], naming the slots.
pub fn run_leak_verdict(snapshot: &PoolSnapshot, run: &str) -> Result<(), String> {
    let leaked: Vec<String> = snapshot
        .occupied()
        .into_iter()
        .filter(|s| s.owner.as_deref() == Some(run) || s.terminating_past_grace)
        .map(|s| format!("slot {} ({:?}, owner {:?})", s.slot, s.state, s.owner))
        .collect();
    if leaked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{MH_RUN_LEAK}: after teardown the pool still holds {}",
            leaked.join(", ")
        ))
    }
}

/// PURE: the ONE slot whose `address` is `url`. Exact match, because the two
/// are byte-identical by construction: the set url is MC's projection of the
/// WebTransport endpoint MH registered with GC, which is MH's
/// `MH_WEBTRANSPORT_ADVERTISE_ADDRESS` from its slot ConfigMap (R-28,
/// `mh_address::check_address_invariant`, proves the process advertises
/// exactly that value), and mh.sh copies `address` from the same ConfigMap.
///
/// # Errors
/// No slot, or more than one, has that address.
pub fn slot_for_address<'a>(
    snapshot: &'a PoolSnapshot,
    url: &str,
) -> Result<&'a SlotEntry, String> {
    let hits: Vec<&SlotEntry> = snapshot.slots.iter().filter(|s| s.address == url).collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no mh.sh slot has address {url}")),
        many => Err(format!(
            "{} mh.sh slots share address {url} (slots {:?}): R-28 violated",
            many.len(),
            many.iter().map(|s| s.slot).collect::<Vec<_>>()
        )),
    }
}

/// The live `spec.terminationGracePeriodSeconds` of the MH pod in `slot`
/// (selected by [`SLOT_LABEL`](crate::fixtures::mh_address::SLOT_LABEL)),
/// read from the API server, never retyped: MH's drain window is this grace
/// minus its own shutdown margin (ADR-0039 §7), so it is an UPPER bound on
/// every drain-derived event. Read it BEFORE draining. Blocking.
///
/// # Panics
/// PRECONDITION: no pod in that slot, or the field is absent.
#[must_use]
pub fn slot_termination_grace(slot: u32) -> std::time::Duration {
    let selector = format!("{}={slot}", crate::fixtures::mh_address::SLOT_LABEL);
    let pods = crate::fixtures::kube::pods_json(&selector);
    crate::fixtures::kube::max_termination_grace(&pods, &selector)
        .unwrap_or_else(|e| panic!("PRECONDITION (mh-slot-grace): {e}"))
}

/// The run-scoped MH pool handle (contract §6, §8): every MH this run creates
/// carries `dark-tower.io/mh-owner=<run token>`, so teardown is owner-scoped
/// (`drain --owner <token> --all`) and needs no slot numbers. That matters
/// because `up`'s not-available-timeout leaves its pod in place.
///
/// The mh.sh path, cluster and owner are resolved ONCE here, so neither
/// [`MhPool::teardown`] nor the `Drop` backstop can panic on a missing env
/// var. `Drop` only PRINTS the reclaim command (contract §8(b)); it never
/// cleans up silently, and never panics (a panic in Drop during unwind aborts
/// the process and loses the message).
pub struct MhPool {
    mh_sh: PathBuf,
    cluster: String,
    token: String,
    torn_down: bool,
    /// This handle runs a leg that may drain a BASELINE MH
    /// ([`MhPool::run_baseline_drain_leg`]); only then is
    /// [`MhPool::drain_baseline_no_wait`] allowed.
    baseline_drain_leg: bool,
    /// Set once [`MhPool::restore_baseline`] ran clean. Until then the Drop
    /// backstop of a baseline-drain leg also prints the restore command.
    baseline_restored: bool,
}

impl MhPool {
    /// The run's pool handle. The owner is the per-run org subdomain
    /// (`auth_client::org_subdomain`, `ENV_TEST_ORG_SUBDOMAIN`, exported by
    /// `scripts/layer7.sh`), already validated against the subdomain rule that
    /// `validate-subdomain-regex-sync.sh` keeps byte-identical to mh.sh's
    /// `MH_OWNER_RE`. There is no second owner predicate here.
    ///
    /// # Panics
    /// PRECONDITION: `DT_MH_SH`, `DT_CLUSTER_NAME` or the owner variable is
    /// unset or invalid.
    #[must_use]
    pub fn for_run() -> Self {
        let token = crate::fixtures::auth_client::org_subdomain()
            .unwrap_or_else(|e| panic!("PRECONDITION (run-owner): {e}"));
        let cluster = dt_cluster_name_from(std::env::var(DT_CLUSTER_NAME_ENV).ok())
            .unwrap_or_else(|e| panic!("PRECONDITION (cluster-unset): {e}"));
        Self::with(mh_sh_path(), cluster, token)
    }

    /// A handle with explicit parts (hermetic tests).
    #[must_use]
    pub fn with(mh_sh: PathBuf, cluster: String, token: String) -> Self {
        Self {
            mh_sh,
            cluster,
            token,
            torn_down: false,
            baseline_drain_leg: false,
            baseline_restored: false,
        }
    }

    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The manual reclaim command ([`reclaim_command`]).
    #[must_use]
    pub fn reclaim_command(&self) -> String {
        reclaim_command(&self.mh_sh, &self.cluster, &self.token)
    }

    /// `mh.sh list --json`, parsed ([`parse_list_value`]). Blocking.
    ///
    /// # Panics
    /// PRECONDITION: mh.sh fails or prints a document the reader refuses.
    #[must_use]
    pub fn snapshot(&self) -> PoolSnapshot {
        snapshot_at(&self.mh_sh)
    }

    /// Pool-mutating. `mh.sh up --owner <token>`: the next-fit slot, returned
    /// once the MH is Available (= MH `/ready` = RegisterMH accepted, contract
    /// §5). Blocking. The bound is mh.sh's own derived one.
    ///
    /// # Errors
    /// The contract-conforming FAIL result, for the caller to classify.
    pub fn up(&self) -> Result<MhSlot, MhShResult> {
        up_with(&self.mh_sh, &self.cluster, &self.token)
    }

    /// Pool-mutating. [`MhPool::up`] (in `spawn_blocking`), panicking on any
    /// FAIL with its REASON, then [`MhSlot::wait_scraped`].
    ///
    /// # Panics
    /// `up` failed (`mh-no-free-slot` as a PRECONDITION), or the scrape wait.
    pub async fn up_scraped(&self, prom: &crate::fixtures::metrics::PrometheusClient) -> MhSlot {
        let (path, cluster, token) = (self.mh_sh.clone(), self.cluster.clone(), self.token.clone());
        let slot = tokio::task::spawn_blocking(move || up_with(&path, &cluster, &token))
            .await
            .expect("up task")
            .unwrap_or_else(|r| {
                let lane = if r.reason == REASON_NO_FREE_SLOT {
                    "PRECONDITION"
                } else {
                    "FAIL"
                };
                panic!(
                    "{lane} (mh-up): `mh.sh up --owner {}` failed: {r}. Reclaim with: {}",
                    self.token,
                    self.reclaim_command()
                )
            });
        slot.wait_scraped(prom).await;
        slot
    }

    /// Pool-mutating. `mh.sh drain --all --owner <token>`: the explicit,
    /// asserted run teardown (exit 0 + STATUS=OK), then [`run_leak_verdict`]
    /// on a fresh `list` (nothing left owned by this run, nothing stuck past
    /// its grace). Blocking. The handle stays usable (for the after-leg
    /// [`MhPool::assert_baseline`]); Drop no longer prints.
    ///
    /// # Errors
    /// A non-OK drain or a leak, naming the REASON and the reclaim command.
    pub fn teardown(&mut self) -> Result<(), String> {
        self.torn_down = true;
        teardown_at(&self.mh_sh, &self.cluster, &self.token)
    }

    /// Pool-mutating. [`MhPool::teardown`] (in `spawn_blocking`), then, only
    /// if it drained,
    /// [`gc_fleet::wait_removed_mh_unselectable`](crate::fixtures::gc_fleet::wait_removed_mh_unselectable):
    /// the run's removed MHs are no longer selectable by any GC replica before
    /// this returns, so a later suite is never handed a dead handler.
    ///
    /// # Errors
    /// The teardown's error, or the settle wait's (`removed-mh-still-selectable`).
    pub async fn teardown_and_settle(
        &mut self,
        prom: &crate::fixtures::metrics::PrometheusClient,
    ) -> Result<(), String> {
        self.torn_down = true;
        let (path, cluster, token) = (self.mh_sh.clone(), self.cluster.clone(), self.token.clone());
        tokio::task::spawn_blocking(move || teardown_at(&path, &cluster, &token))
            .await
            .map_err(|e| format!("TEARDOWN (mh-teardown-failed): teardown task: {e}"))??;
        crate::fixtures::gc_fleet::wait_removed_mh_unselectable(prom).await
    }

    /// Pool-mutating. THE leg scaffold (40's legs, 44): a run-owned pool with
    /// [`MhPool::assert_baseline`] and no live MC meeting
    /// ([`await_no_mc_meetings`](crate::fixtures::metrics::await_no_mc_meetings))
    /// before, `body` in its own task (so its
    /// panic is caught), then on EVERY exit path the asserted owner teardown +
    /// GC settle ([`MhPool::teardown_and_settle`]), then the baseline again.
    /// `body` gets the pool and the Prometheus base url. The body's panic is
    /// re-raised unchanged after teardown; a teardown failure alongside it is
    /// printed first.
    ///
    /// # Panics
    /// The body's panic; the teardown's error; a baseline violation.
    pub async fn run_leg<F, Fut>(body: F)
    where
        F: FnOnce(std::sync::Arc<MhPool>, String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        Self::run_leg_with(false, body).await;
    }

    /// Pool-mutating. [`MhPool::run_leg`] for the ONE leg that drains a
    /// BASELINE MH (story 4 MC-F, planned transfer without a target; contract
    /// §8 MC-F amendment). Same scaffold, plus, on EVERY exit path (body
    /// panic included) and BEFORE the owner teardown + GC settle: the
    /// asserted, deficit-driven [`MhPool::restore_baseline`]; then
    /// [`MhSlot::wait_scraped`] on every restored MH (later legs take MH metric
    /// baselines), then [`MhPool::assert_baseline`], then the R-28 address
    /// invariant (`assert_addresses_unique`): the body must NOT call it, since
    /// the baseline is short until the restore. A restore failure beside
    /// a body panic is printed first as `ALSO:`.
    ///
    /// # Panics
    /// The body's panic; the restore's or teardown's error; a baseline
    /// violation.
    pub async fn run_baseline_drain_leg<F, Fut>(body: F)
    where
        F: FnOnce(std::sync::Arc<MhPool>, String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        Self::run_leg_with(true, body).await;
    }

    async fn run_leg_with<F, Fut>(baseline_drain_leg: bool, body: F)
    where
        F: FnOnce(std::sync::Arc<MhPool>, String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let cluster = crate::cluster::ClusterConnection::new()
            .await
            .expect("Failed to connect to cluster - ensure port-forwards are running");
        let prom_url = cluster.prometheus_base_url.clone();
        let prom = crate::fixtures::metrics::PrometheusClient::new(&prom_url);
        let mut pool = MhPool::for_run();
        pool.baseline_drain_leg = baseline_drain_leg;
        let pool = std::sync::Arc::new(pool);
        pool.assert_baseline(&prom).await;
        // Foreign activity also covers MC: no meeting from an earlier suite may
        // be alive, or MC can pull this leg's run-owned MH into it as a
        // replacement (story 4 MC-C) and the leg would count its releases.
        crate::fixtures::metrics::await_no_mc_meetings(
            &prom,
            // Two reads one SCRAPE apart (observability C3): reads closer than
            // the service-job scrape interval compare one sample with itself.
            crate::fixtures::metrics::service_job_scrape_settle(&prom).await,
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));

        let outcome = tokio::spawn(body(std::sync::Arc::clone(&pool), prom_url)).await;

        let mut pool = std::sync::Arc::try_unwrap(pool)
            .unwrap_or_else(|_| panic!("the leg still holds the MhPool after it finished"));
        let mut errors = Vec::new();
        let mut restored = Vec::new();
        if baseline_drain_leg {
            match pool.restore_baseline().await {
                Ok(slots) => restored = slots,
                Err(e) => errors.push(e),
            }
        }
        if let Err(t) = pool.teardown_and_settle(&prom).await {
            errors.push(t);
        }
        for slot in restored {
            // wait_scraped panics on its ceiling; catch it so it cannot mask
            // the body's own panic.
            let prom =
                crate::fixtures::metrics::PrometheusClient::new(&cluster.prometheus_base_url);
            if let Err(e) = tokio::spawn(async move { slot.wait_scraped(&prom).await }).await {
                errors.push(format!("RESTORE (restored-mh-not-scraped): {e}"));
            }
        }
        match outcome {
            Ok(()) => {
                if let Some(first) = errors.first() {
                    for e in &errors[1..] {
                        eprintln!("ALSO: {e}");
                    }
                    panic!("{first}");
                }
            }
            Err(e) => {
                for t in &errors {
                    eprintln!("ALSO: {t}");
                }
                if e.is_panic() {
                    std::panic::resume_unwind(e.into_panic());
                }
                panic!("leg task did not complete: {e}");
            }
        }
        pool.assert_baseline(&prom).await;
        if baseline_drain_leg {
            // R-28 after the leg, at STEADY STATE: the leg deliberately left
            // the baseline short, so the address invariant (whose positive
            // control requires >= B serving MHs) runs here, after the restore,
            // and also covers the restored MH's address.
            crate::fixtures::mh_address::assert_addresses_unique().await;
        }
    }

    /// Pool-mutating. Drain ONE baseline MH, returning once SIGTERM is
    /// delivered (`mh.sh drain <k> --owner baseline --no-wait`,
    /// [`baseline_drain_args`]; never `--force`). Only inside
    /// [`MhPool::run_baseline_drain_leg`], whose scaffold restores the pool.
    /// Blocking.
    ///
    /// # Panics
    /// Called outside that scaffold; the slot is not a baseline MH;
    /// PRECONDITION on `last-serving-mh` (the survivor is not serving, so the
    /// leg has nothing to move to); FAIL on any other non-OK result.
    pub fn drain_baseline_no_wait(&self, slot: u32) {
        assert!(
            self.baseline_drain_leg,
            "drain_baseline_no_wait outside MhPool::run_baseline_drain_leg: nothing would \
             restore the baseline"
        );
        let entry = snapshot_at(&self.mh_sh)
            .slots
            .into_iter()
            .find(|s| s.slot == slot)
            .unwrap_or_else(|| panic!("PRECONDITION (mh-sh-list): slot {slot} not listed"));
        assert_eq!(
            entry.owner.as_deref(),
            Some(BASELINE_OWNER),
            "drain_baseline_no_wait: slot {slot} is not a baseline MH ({:?})",
            entry.owner
        );
        let args = baseline_drain_args(slot);
        let result = run_mh_sh_in(&self.mh_sh, Some(&self.cluster), &as_strs(&args));
        if !result.is_ok() {
            let lane = if result.reason == "last-serving-mh" {
                "PRECONDITION"
            } else {
                "FAIL"
            };
            panic!(
                "{lane} (mh-baseline-drain): `mh.sh {}` failed: {result}",
                args.join(" ")
            );
        }
    }

    /// Pool-mutating. Top the pool back up to B baseline MHs: one
    /// `mh.sh up --owner baseline` per [`baseline_deficit`] on a FRESH
    /// snapshot (0 = nothing to do), each asserted OK and baseline-owned.
    /// Returns the restored MHs (for [`MhSlot::wait_scraped`]). On success the
    /// Drop backstop stops printing the restore command.
    ///
    /// # Errors
    /// `RESTORE (baseline-restore-failed)`, naming mh.sh's REASON and the
    /// manual restore command.
    pub async fn restore_baseline(&mut self) -> Result<Vec<MhSlot>, String> {
        let (path, cluster) = (self.mh_sh.clone(), self.cluster.clone());
        let restored = tokio::task::spawn_blocking(move || restore_baseline_at(&path, &cluster))
            .await
            .map_err(|e| format!("RESTORE (baseline-restore-failed): restore task: {e}"))??;
        self.baseline_restored = true;
        Ok(restored)
    }

    /// [`baseline_verdict`] as an eventual poll, before AND after every
    /// destructive leg. The ceiling is GC's removal-settle budget
    /// ([`gc_fleet::removal_settle_budget`](crate::fixtures::gc_fleet::removal_settle_budget):
    /// the live staleness bound + health ticks + scrape), because a pool that
    /// just changed takes that long to look settled end to end; a single read
    /// flakes.
    ///
    /// # Panics
    /// [`FOREIGN_MH_ACTIVITY`] at the ceiling, with the last verdict.
    pub async fn assert_baseline(&self, prom: &crate::fixtures::metrics::PrometheusClient) {
        let ceiling = crate::fixtures::gc_fleet::removal_settle_budget(prom)
            .await
            .unwrap_or_else(|e| panic!("PRECONDITION (baseline-ceiling): {e}"));
        let path = self.mh_sh.clone();
        let cluster = self.cluster.clone();
        let run = self.token.clone();
        let verdict = crate::eventual::poll_within(ceiling, move || {
            let (path, cluster, run) = (path.clone(), cluster.clone(), run.clone());
            async move {
                tokio::task::spawn_blocking(move || {
                    let snap = snapshot_at(&path);
                    baseline_verdict(
                        &snap,
                        &run,
                        |o| reclaim_command(&path, &cluster, o),
                        &baseline_restore_command(&path, &cluster),
                    )
                })
                .await
                .expect("baseline task")
            }
        })
        .await;
        if let Err(e) = verdict {
            panic!("{e} (polled for {ceiling:?})");
        }
    }

    /// PURE (writer seam): the baseline-drain leg's Drop backstop. It ONLY
    /// prints (contract §8(b)): running mh.sh from Drop would block inside the
    /// runtime during unwind.
    pub fn baseline_drop_notice(&self, w: &mut dyn std::io::Write) {
        let _ = writeln!(
            w,
            "MhPool {} (baseline-drain leg) dropped before its baseline restore; run `mh.sh \
             list`, then once per missing baseline MH: {} (or re-run infra/kind/scripts/deploy.sh, \
             which tops the pool back up to B)",
            self.token,
            baseline_restore_command(&self.mh_sh, &self.cluster)
        );
    }

    /// PURE (writer seam): the Drop backstop's notice.
    pub fn drop_notice(&self, w: &mut dyn std::io::Write) {
        let _ = writeln!(
            w,
            "MhPool {} dropped without teardown; reclaim its MHs with: {}",
            self.token,
            self.reclaim_command()
        );
    }
}

impl Drop for MhPool {
    fn drop(&mut self) {
        if !self.torn_down {
            self.drop_notice(&mut std::io::stderr());
        }
        if self.baseline_drain_leg && !self.baseline_restored {
            self.baseline_drop_notice(&mut std::io::stderr());
        }
    }
}

/// Owner teardown at `path` ([`MhPool::teardown`]). Blocking.
fn teardown_at(path: &std::path::Path, cluster: &str, token: &str) -> Result<(), String> {
    let reclaim = reclaim_command(path, cluster, token);
    let result = run_mh_sh_in(path, Some(cluster), &as_strs(&teardown_args(token)));
    if !result.is_ok() {
        return Err(format!(
            "TEARDOWN (mh-teardown-failed): owner {token} not drained: {result}. Reclaim \
             manually: {reclaim}"
        ));
    }
    run_leak_verdict(&snapshot_at(path), token)
        .map_err(|e| format!("TEARDOWN ({e}). Reclaim manually: {reclaim}"))
}

/// `mh.sh list --json` at `path`. Blocking.
fn snapshot_at(path: &std::path::Path) -> PoolSnapshot {
    let result = run_mh_sh_at(path, &["list"]);
    assert!(
        result.is_ok(),
        "PRECONDITION (mh-sh-list): `mh.sh list --json` failed: {result}"
    );
    parse_list_value(&result.json)
        .unwrap_or_else(|e| panic!("PRECONDITION (mh-sh-list): {e} ({result})"))
}

/// [`MhPool::restore_baseline`] at `path`. Blocking. A sibling of
/// [`up_with`], whose run-owner check stays as it is.
fn restore_baseline_at(path: &std::path::Path, cluster: &str) -> Result<Vec<MhSlot>, String> {
    let manual = baseline_restore_command(path, cluster);
    let deficit = baseline_deficit(&snapshot_at(path));
    let mut restored = Vec::new();
    for _ in 0..deficit {
        let result = run_mh_sh_in(path, Some(cluster), &as_strs(&baseline_restore_args()));
        if !result.is_ok() {
            return Err(format!(
                "RESTORE (baseline-restore-failed): `{manual}` failed: {result}; restored {} of \
                 {deficit}. Restore manually (once per missing baseline MH): {manual}",
                restored.len()
            ));
        }
        let entry: SlotEntry = serde_json::from_value(result.json.clone()).map_err(|e| {
            format!("PRECONDITION (mh-sh-contract): up's OK object is not a slot entry: {e}")
        })?;
        if entry.owner.as_deref() != Some(BASELINE_OWNER) {
            return Err(format!(
                "RESTORE (baseline-restore-failed): up returned slot {} owned by {:?}, not \
                 {BASELINE_OWNER}",
                entry.slot, entry.owner
            ));
        }
        restored.push(MhSlot::from_entry(path, cluster, BASELINE_OWNER, entry));
    }
    Ok(restored)
}

/// `mh.sh up --owner <token>` at `path` ([`MhPool::up`]). Blocking.
fn up_with(path: &std::path::Path, cluster: &str, token: &str) -> Result<MhSlot, MhShResult> {
    let result = run_mh_sh_in(path, Some(cluster), &["up", "--owner", token]);
    if !result.is_ok() {
        return Err(result);
    }
    let entry: SlotEntry = serde_json::from_value(result.json.clone()).unwrap_or_else(|e| {
        panic!("PRECONDITION (mh-sh-contract): up's OK object is not a slot entry: {e}")
    });
    assert_eq!(
        entry.owner.as_deref(),
        Some(token),
        "PRECONDITION (mh-sh-contract): up returned a slot not owned by this run"
    );
    Ok(MhSlot::from_entry(path, cluster, token, entry))
}

/// One run-owned MH (`mh.sh up`'s answer). Carries what a leg needs: the
/// slot, its incarnation (`pod_uid`, `restart_count`), its `pod_ip` (the
/// Prometheus pin) and its client-facing `address`.
#[derive(Clone, Debug)]
pub struct MhSlot {
    pub slot: u32,
    pub deployment: String,
    pub pod: String,
    pub pod_uid: String,
    pub pod_ip: String,
    pub address: String,
    pub restart_count: u32,
    mh_sh: PathBuf,
    cluster: String,
    owner: String,
}

impl MhSlot {
    fn from_entry(path: &std::path::Path, cluster: &str, owner: &str, e: SlotEntry) -> Self {
        fn missing(slot: u32, key: &str) -> ! {
            panic!(
                "PRECONDITION (mh-sh-contract): up's slot {slot} entry has no {key} at Available"
            )
        }
        let slot = e.slot;
        Self {
            slot,
            deployment: e.deployment.unwrap_or_else(|| missing(slot, "deployment")),
            pod: e.pod.unwrap_or_else(|| missing(slot, "pod")),
            pod_uid: e.pod_uid.unwrap_or_else(|| missing(slot, "pod_uid")),
            pod_ip: e.pod_ip.unwrap_or_else(|| missing(slot, "pod_ip")),
            address: e.address,
            restart_count: e
                .restart_count
                .unwrap_or_else(|| missing(slot, "restart_count")),
            mh_sh: path.to_path_buf(),
            cluster: cluster.to_string(),
            owner: owner.to_string(),
        }
    }

    /// This slot's current `list` entry. Blocking.
    ///
    /// # Panics
    /// The slot is absent from `list`.
    #[must_use]
    pub fn entry(&self) -> SlotEntry {
        snapshot_at(&self.mh_sh)
            .slots
            .into_iter()
            .find(|s| s.slot == self.slot)
            .unwrap_or_else(|| panic!("PRECONDITION (mh-sh-list): slot {} not listed", self.slot))
    }

    /// Wait until Prometheus scrapes this MH: `up{job="mh-service"} == 1`
    /// pinned by POD IP (`instance_for_pod_ip`; the job sets no `pod` label).
    /// Call it before taking ANY MH metric baseline, and again after
    /// [`MhSlot::kill`] / [`FreezeGuard::resume`]: a restarted process starts
    /// its counters at zero, and a frozen one fails its scrapes (`up` 0 while
    /// frozen is expected, never asserted). The ceiling is two
    /// [`service_job_scrape_settle`](crate::fixtures::metrics::service_job_scrape_settle)s
    /// of the live scrape config (a failure-only bound).
    ///
    /// # Panics
    /// [`MH_SCRAPE_UP_TIMEOUT`] at the ceiling.
    pub async fn wait_scraped(&self, prom: &crate::fixtures::metrics::PrometheusClient) {
        let ceiling = 2 * crate::fixtures::metrics::service_job_scrape_settle(prom).await;
        crate::fixtures::metrics::poll_until_pinned_instance(
            prom,
            MH_UP_PROMQL,
            &self.pod_ip,
            ceiling,
            std::time::Duration::from_secs(1),
            MH_SCRAPE_UP_TIMEOUT,
            "== 1",
            |v| (v - 1.0).abs() < f64::EPSILON,
        )
        .await;
    }

    /// [`MhSlot::wait_scraped`], but only once the pinned instance's `up`
    /// sample was TAKEN at or after `since_unix_secs`: an in-place restart
    /// keeps the pod IP (so the instance), and the pre-restart `up == 1`
    /// sample would otherwise satisfy the wait before Prometheus ever scraped
    /// the new process. Same failure-only ceiling and token.
    ///
    /// # Panics
    /// [`MH_SCRAPE_UP_TIMEOUT`] at the ceiling.
    pub async fn wait_scraped_since(
        &self,
        prom: &crate::fixtures::metrics::PrometheusClient,
        since_unix_secs: f64,
    ) {
        let ceiling = 2 * crate::fixtures::metrics::service_job_scrape_settle(prom).await;
        crate::fixtures::metrics::poll_until_pinned_instance(
            prom,
            MH_UP_TIMESTAMP_PROMQL,
            &self.pod_ip,
            ceiling,
            std::time::Duration::from_secs(1),
            MH_SCRAPE_UP_TIMEOUT,
            "a sample taken after the restart",
            |t| t >= since_unix_secs,
        )
        .await;
        self.wait_scraped(prom).await;
    }

    fn drain_with(self, no_wait: bool) {
        let slot = self.slot.to_string();
        let mut args = vec!["drain", slot.as_str(), "--owner", self.owner.as_str()];
        if no_wait {
            args.push("--no-wait");
        }
        let result = run_mh_sh_in(&self.mh_sh, Some(&self.cluster), &args);
        assert!(
            result.is_ok(),
            "FAIL (mh-drain): `mh.sh {}` failed: {result}. Reclaim with: {}",
            args.join(" "),
            reclaim_command(&self.mh_sh, &self.cluster, &self.owner)
        );
    }

    /// Pool-mutating. `mh.sh drain <k> --owner <run>`: returns once the pod
    /// and Deployment are gone; asserted. Success is mh.sh's verdict alone,
    /// never the exiting MH's metrics (read drain outcomes from MC). Blocking.
    ///
    /// # Panics
    /// A non-OK result, naming the REASON.
    pub fn drain(self) {
        self.drain_with(false);
    }

    /// Pool-mutating. `mh.sh drain <k> --owner <run> --no-wait`: returns once
    /// SIGTERM is delivered, for mid-drain observation; asserted. Blocking.
    ///
    /// # Panics
    /// A non-OK result, naming the REASON.
    pub fn drain_no_wait(self) {
        self.drain_with(true);
    }

    fn signal(&self, signal: Signal) -> SignalOutcome {
        let args = signal_args(self.slot, signal, &self.owner);
        let result = run_mh_sh_in(&self.mh_sh, Some(&self.cluster), &as_strs(&args));
        assert!(
            result.is_ok(),
            "FAIL (mh-signal): `mh.sh {}` failed: {result}",
            args.join(" ")
        );
        parse_signal(&result.json, signal)
            .unwrap_or_else(|e| panic!("PRECONDITION (mh-sh-contract): {e}"))
    }

    /// Pool-mutating. SIGKILL the mh-service process (`mh.sh signal KILL`,
    /// never a pod delete), then wait for the IN-PLACE restart: same pod_uid,
    /// restart_count + 1, serving (ceiling: mh.sh's own derived `up` bound,
    /// `restart_budget_seconds` on the KILL answer). Then
    /// re-wait the scrape: the new process's counters start at zero, so
    /// callers re-baseline any MH metric after this.
    ///
    /// # Panics
    /// The signal failed, or [`RESTART_NOT_OBSERVED`] at the ceiling.
    pub async fn kill(&mut self, prom: &crate::fixtures::metrics::PrometheusClient) {
        let me = self.clone();
        let (before, ceiling) = tokio::task::spawn_blocking(move || {
            let before = me.entry();
            let outcome = me.signal(Signal::Kill);
            (
                before,
                outcome
                    .restart_budget
                    .expect("parse_signal requires a KILL restart budget"),
            )
        })
        .await
        .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
        let me = self.clone();
        let after = crate::eventual::poll_within(ceiling, move || {
            let (me, before) = (me.clone(), before.clone());
            async move {
                tokio::task::spawn_blocking(move || {
                    let now = me.entry();
                    if restarted_in_place(&before, &now) {
                        Ok(now)
                    } else {
                        Err(format!(
                            "{RESTART_NOT_OBSERVED}: slot {} after KILL: pod_uid {:?} -> {:?}, \
                             restart_count {:?} -> {:?}, state {:?} (want same pod_uid, +1, \
                             serving)",
                            me.slot,
                            before.pod_uid,
                            now.pod_uid,
                            before.restart_count,
                            now.restart_count,
                            now.state
                        ))
                    }
                })
                .await
                .expect("kill poll task")
            }
        })
        .await
        .unwrap_or_else(|e| panic!("{e} (polled for {ceiling:?})"));
        self.refresh(&after);
        // The restart keeps the pod IP, so `up` for this instance may still be
        // the PRE-KILL incarnation's sample: require a scrape taken after the
        // restart was observed before anyone re-baselines (observability C3).
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after the epoch")
            .as_secs_f64();
        self.wait_scraped_since(prom, since).await;
    }

    /// Pool-mutating. SIGSTOP the mh-service process (`mh.sh signal STOP`).
    /// mh.sh polls `/proc/<pid>/stat` until the state is `T` (the freeze TOOK
    /// EFFECT) and derives the budget from the live liveness probe. The
    /// guard's clock starts BEFORE the call. A STOP call that fails (the
    /// signal may already have landed) sends a best-effort CONT and prints the
    /// resume command before re-raising, so nothing is left frozen without a
    /// guard. A fresh post-STOP `list` must show the same incarnation.
    /// Blocking.
    ///
    /// # Panics
    /// The signal failed, or [`RESTART_COUNT_CHANGED`] after the STOP.
    #[must_use]
    pub fn freeze(&self) -> FreezeGuard {
        let started = std::time::Instant::now();
        let before = self.entry();
        let outcome =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.signal(Signal::Stop)
            })) {
                Ok(o) => o,
                Err(panic) => {
                    best_effort_cont(self);
                    eprintln!(
                    "the STOP call for MH slot {} failed; sent a best-effort CONT. If it is still \
                     stopped, resume it with: {}",
                    self.slot,
                    signal_command(&self.mh_sh, &self.cluster, self.slot, Signal::Cont, &self.owner)
                );
                    std::panic::resume_unwind(panic);
                }
            };
        let stop_call = started.elapsed();
        let mut guard = FreezeGuard {
            slot: self.clone(),
            before,
            started,
            stop_call,
            budget: outcome
                .liveness_budget
                .expect("parse_signal requires a STOP budget"),
            resumed: false,
        };
        let after_stop = guard.slot.entry();
        if let Err(e) = assert_same_incarnation(&guard.before, &after_stop) {
            guard.resume_best_effort();
            panic!("{e}");
        }
        guard
    }

    /// Adopt `entry`'s incarnation fields (after a restart or a resume), so
    /// callers read live state, never the `up`-time copy.
    fn refresh(&mut self, entry: &SlotEntry) {
        if let Some(uid) = &entry.pod_uid {
            self.pod_uid.clone_from(uid);
        }
        if let Some(rc) = entry.restart_count {
            self.restart_count = rc;
        }
    }
}

/// Send `mh.sh signal <k> CONT` without parsing or panicking (the backstops).
fn best_effort_cont(slot: &MhSlot) {
    let _ = Command::new(&slot.mh_sh)
        .env(DT_CLUSTER_NAME_ENV, &slot.cluster)
        .args(signal_args(slot.slot, Signal::Cont, &slot.owner))
        .arg("--json")
        .output();
}

/// `up` of the mh-service job, the scrape-liveness series.
pub const MH_UP_PROMQL: &str = r#"up{job="mh-service"}"#;

/// When each MH instance's `up` sample was taken (unix seconds): the
/// freshness half of [`MhSlot::wait_scraped_since`].
pub const MH_UP_TIMESTAMP_PROMQL: &str = r#"timestamp(up{job="mh-service"})"#;

/// A SIGSTOPped run-owned MH. [`FreezeGuard::resume`] is the explicit,
/// asserted end. Dropped while still frozen (a panicking leg), it sends a
/// best-effort CONT and prints the exact resume command; it never panics.
pub struct FreezeGuard {
    slot: MhSlot,
    before: SlotEntry,
    started: std::time::Instant,
    stop_call: std::time::Duration,
    budget: std::time::Duration,
    resumed: bool,
}

impl FreezeGuard {
    /// The live liveness-probe budget this freeze must stay inside.
    #[must_use]
    pub fn budget(&self) -> std::time::Duration {
        self.budget
    }

    /// Time since BEFORE the STOP call.
    #[must_use]
    pub fn elapsed(&self) -> std::time::Duration {
        self.started.elapsed()
    }

    /// How long the STOP call took: the best available estimate of the CONT
    /// call's latency too (same mechanism), so a leg needs
    /// `2 x stop_call < budget` to be able to resume in time at all.
    #[must_use]
    pub fn stop_call(&self) -> std::time::Duration {
        self.stop_call
    }

    /// The manual resume command.
    #[must_use]
    pub fn resume_command(&self) -> String {
        signal_command(
            &self.slot.mh_sh,
            &self.slot.cluster,
            self.slot.slot,
            Signal::Cont,
            &self.slot.owner,
        )
    }

    fn resume_best_effort(&mut self) {
        self.resumed = true;
        best_effort_cont(&self.slot);
    }

    /// Pool-mutating. SIGCONT (`mh.sh signal CONT`), sent unconditionally.
    /// Only once it returned OK is the guard marked resumed: a failed CONT
    /// panics with the guard still armed, so Drop retries and prints the
    /// resume command. Then, in this order: the budget over the whole frozen
    /// span ([`freeze_overrun`], so an overrun is reported as itself, not as
    /// the kubelet restart it caused), the process left state `T`, and the
    /// same incarnation (restart_count, pod_uid). Returns the slot refreshed
    /// from the post-resume `list`, after re-waiting the scrape.
    ///
    /// # Panics
    /// [`FREEZE_BUDGET_EXCEEDED`], [`RESTART_COUNT_CHANGED`], or the signal.
    pub async fn resume(mut self, prom: &crate::fixtures::metrics::PrometheusClient) -> MhSlot {
        let slot = self.slot.clone();
        let after = tokio::task::spawn_blocking(move || {
            slot.signal(Signal::Cont);
            slot.entry()
        })
        .await
        .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()));
        self.resumed = true;
        if let Err(e) = freeze_overrun(self.elapsed(), self.budget) {
            panic!("{e}");
        }
        if let Err(e) = assert_same_incarnation(&self.before, &after) {
            panic!("{e}");
        }
        let mut slot = self.slot.clone();
        slot.refresh(&after);
        slot.wait_scraped(prom).await;
        slot
    }

    /// PURE (writer seam): the Drop backstop's notice.
    pub fn drop_notice(&self, w: &mut dyn std::io::Write) {
        let _ = writeln!(
            w,
            "FreezeGuard for MH slot {} dropped while frozen; sent a best-effort CONT. If it is \
             still stopped, resume it with: {}",
            self.slot.slot,
            self.resume_command()
        );
    }
}

impl Drop for FreezeGuard {
    fn drop(&mut self) {
        if !self.resumed {
            self.resume_best_effort();
            self.drop_notice(&mut std::io::stderr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mh_sh_text() -> String {
        committed_mh_sh_source()
    }

    #[test]
    fn mh_result_parses_ok_and_fail_with_detail_to_end_of_line() {
        let ok = parse_mh_sh_result(
            "human text\nMH_RESULT STATUS=OK REASON=ok SLOT=3 DETAIL=created timeout=90s\n",
            r#"{"ok":true,"slot":3}"#,
            Some(0),
        )
        .unwrap();
        assert!(ok.is_ok());
        assert_eq!(ok.reason, "ok");
        assert_eq!(ok.slot, Some(3));
        assert_eq!(ok.detail.as_deref(), Some("created timeout=90s"));

        let fail = parse_mh_sh_result(
            "MH_RESULT STATUS=FAIL REASON=not-available-timeout SLOT=2 DETAIL=timeout=90s; mh-s2 left in place\n",
            r#"{"ok":false,"reason":"not-available-timeout","slot":2,"detail":"x"}"#,
            Some(1),
        )
        .unwrap();
        assert!(!fail.is_ok());
        assert_eq!(fail.reason, REASON_NOT_AVAILABLE);
        assert_eq!(fail.slot, Some(2));
        assert_eq!(
            fail.to_string(),
            "STATUS=FAIL REASON=not-available-timeout EXIT=1 SLOT=2 DETAIL=timeout=90s; mh-s2 left in place"
        );

        let no_slot = parse_mh_sh_result(
            "MH_RESULT STATUS=FAIL REASON=mh-no-free-slot\n",
            r#"{"ok":false,"reason":"mh-no-free-slot","slot":null,"detail":null}"#,
            Some(3),
        )
        .unwrap();
        assert_eq!(no_slot.slot, None);
        assert_eq!(no_slot.detail, None);
    }

    /// Every named rejection case (the TS twin carries the same names).
    #[test]
    fn mh_result_refusals_are_each_named() {
        let err = |stderr: &str, stdout: &str, code: Option<i32>| {
            parse_mh_sh_result(stderr, stdout, code).unwrap_err()
        };
        assert!(err("", "{}", Some(0)).starts_with("one-line: mh.sh printed nothing"));
        assert!(err("oops\n", "{}", Some(0)).starts_with("one-line: mh.sh's last stderr line"));
        assert!(err(
            "MH_RESULT STATUS=OK REASON=ok\nMH_RESULT STATUS=OK REASON=ok\n",
            r#"{"ok":true}"#,
            Some(0)
        )
        .starts_with("one-line: mh.sh printed 2 MH_RESULT lines"));
        assert!(err(
            "MH_RESULT STATUS=MAYBE REASON=ok\n",
            r#"{"ok":true}"#,
            Some(0)
        )
        .starts_with("fields: MH_RESULT has a malformed field"));
        assert!(err("MH_RESULT STATUS=OK\n", r#"{"ok":true}"#, Some(0))
            .starts_with("fields: MH_RESULT has no REASON"));
        assert!(err(
            "MH_RESULT STATUS=OK REASON=ok SLOT=x\n",
            r#"{"ok":true}"#,
            Some(0)
        )
        .starts_with("fields: MH_RESULT SLOT"));
        assert!(err(
            "MH_RESULT STATUS=FAIL REASON=made-up\n",
            r#"{"ok":false}"#,
            Some(1)
        )
        .starts_with("known-token: REASON=made-up"));
        // exit 0 with a FAIL token, and a FAIL token on the wrong group.
        assert!(err(
            "MH_RESULT STATUS=FAIL REASON=drain-timeout\n",
            r#"{"ok":false}"#,
            Some(0)
        )
        .starts_with("exit-group: REASON=drain-timeout belongs to exit 1, mh.sh exited 0"));
        assert!(err(
            "MH_RESULT STATUS=FAIL REASON=mh-pool-busy\n",
            r#"{"ok":false}"#,
            Some(1)
        )
        .starts_with("exit-group:"));
        assert!(
            err("MH_RESULT STATUS=OK REASON=ok\n", r#"{"ok":true}"#, None)
                .starts_with("exit-group: mh.sh was killed")
        );
        assert!(err(
            "MH_RESULT STATUS=FAIL REASON=ok\n",
            r#"{"ok":false}"#,
            Some(0)
        )
        .starts_with("ok-reason:"));
        assert!(err("MH_RESULT STATUS=OK REASON=ok\n", "not json", Some(0))
            .starts_with("json-ok-agrees: mh.sh --json stdout is not one JSON object"));
        assert!(err(
            "MH_RESULT STATUS=OK REASON=ok\n",
            r#"{"ok":false}"#,
            Some(0)
        )
        .starts_with("json-ok-agrees: mh.sh's stdout ok="));
    }

    /// PURE: mh.sh's `MH_TOKENS=( "tok:code" ... )` entries.
    fn mh_sh_tokens(text: &str) -> Vec<(String, i32)> {
        text.split_once("readonly -a MH_TOKENS=(")
            .and_then(|(_, rest)| rest.split_once("\n)"))
            .map(|(body, _)| body)
            .expect("mh.sh defines `readonly -a MH_TOKENS=( ... )`")
            .lines()
            .filter_map(|l| {
                l.trim()
                    .strip_prefix('"')?
                    .strip_suffix('"')?
                    .split_once(':')
            })
            .map(|(t, c)| (t.to_string(), c.parse().expect("token code is an integer")))
            .collect()
    }

    /// [`MH_TOKENS`] == mh.sh's token array, BOTH directions, codes included
    /// (one home: mh.sh). A renamed or regrouped token fails here, not as a
    /// misclassification at run time.
    #[test]
    fn mh_tokens_match_mh_sh_both_directions() {
        let theirs = mh_sh_tokens(&mh_sh_text());
        // Vacuity control: mh.sh has well over ten tokens.
        assert!(
            theirs.len() > 10,
            "extraction produced too few tokens: {theirs:?}"
        );
        let theirs: std::collections::BTreeSet<(String, i32)> = theirs.into_iter().collect();
        let ours: std::collections::BTreeSet<(String, i32)> = MH_TOKENS
            .iter()
            .map(|(t, c)| ((*t).to_string(), *c))
            .collect();
        let missing: Vec<_> = theirs.difference(&ours).collect();
        let extra: Vec<_> = ours.difference(&theirs).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "MH_TOKENS drifted from mh.sh: in mh.sh only {missing:?}; in Rust only {extra:?}"
        );
        for t in [
            REASON_OK,
            REASON_NOT_AVAILABLE,
            REASON_NO_FREE_SLOT,
            REASON_BUSY,
        ] {
            assert!(
                token_exit_code(t).is_some(),
                "{t} is branched on but unknown"
            );
        }
        // The fixture tokens are never mh.sh tokens.
        for t in FIXTURE_TOKENS {
            assert!(
                token_exit_code(t).is_none(),
                "{t} collides with an mh.sh token"
            );
        }
    }

    /// PURE: an integer `readonly NAME=<int>` from mh.sh.
    fn mh_sh_int(text: &str, name: &str) -> u64 {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("readonly {name}=")))
            .unwrap_or_else(|| panic!("mh.sh has no `readonly {name}=`"))
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("mh.sh {name} is not an integer: {e}"))
    }

    /// The BUSY bound outlasts a crashed holder's Lease (reclaimable after
    /// `MH_LEASE_S + MH_LEASE_FUTURE_SKEW_S`) plus one interval, so exhaustion
    /// always means a LIVE concurrent mutator.
    #[test]
    fn busy_bound_outlasts_a_crashed_holders_lease() {
        let text = mh_sh_text();
        let lease = mh_sh_int(&text, "MH_LEASE_S");
        let skew = mh_sh_int(&text, "MH_LEASE_FUTURE_SKEW_S");
        assert!(
            lease > 0,
            "positive control: MH_LEASE_S extracted as {lease}"
        );
        let floor = std::time::Duration::from_secs(lease + skew) + BUSY_RETRY_INTERVAL;
        assert!(
            BUSY_RETRY_BOUND > floor,
            "BUSY_RETRY_BOUND {BUSY_RETRY_BOUND:?} must exceed lease {lease}s + skew {skew}s + \
             one interval"
        );
    }

    fn run(stderr: &str, stdout: &str, code: i32) -> RawRun {
        RawRun {
            exit_code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    fn busy() -> RawRun {
        run(
            "MH_RESULT STATUS=FAIL REASON=mh-pool-busy DETAIL=holder=e2e-aa@host:1\n",
            r#"{"ok":false}"#,
            5,
        )
    }

    #[test]
    fn busy_is_retried_until_ok_within_the_bound() {
        let mut calls = 0;
        let mut slept = std::time::Duration::ZERO;
        let r = run_with_busy_retry(
            "up",
            || {
                calls += 1;
                if calls < 4 {
                    busy()
                } else {
                    run("MH_RESULT STATUS=OK REASON=ok\n", r#"{"ok":true}"#, 0)
                }
            },
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(2),
            |d| slept += d,
        )
        .unwrap();
        assert!(r.is_ok());
        assert_eq!(calls, 4, "positive control: OK on attempt 4");
        assert_eq!(slept, std::time::Duration::from_secs(6));
    }

    #[test]
    fn busy_exhaustion_is_loud_and_names_the_holder() {
        let mut calls = 0;
        let mut slept = std::time::Duration::ZERO;
        let e = run_with_busy_retry(
            "drain 3",
            || {
                calls += 1;
                busy()
            },
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(2),
            |d| slept += d,
        )
        .unwrap_err();
        assert_eq!(calls, 6, "bound/interval + 1 attempts");
        assert_eq!(
            slept,
            std::time::Duration::from_secs(10),
            "slept the whole bound"
        );
        assert!(e.starts_with("BUSY (mh-pool-busy): `mh.sh drain 3` still busy after 6 attempts"));
        assert!(e.contains("holder=e2e-aa@host:1"), "{e}");
        assert!(e.contains("MH_LEASE_S"), "{e}");
    }

    #[test]
    fn only_busy_is_retried() {
        for (stderr, code) in [
            ("MH_RESULT STATUS=FAIL REASON=mh-no-free-slot\n", 3),
            ("MH_RESULT STATUS=FAIL REASON=foreign-owner\n", 4),
            ("MH_RESULT STATUS=FAIL REASON=drain-timeout\n", 1),
        ] {
            let mut calls = 0;
            let r = run_with_busy_retry(
                "x",
                || {
                    calls += 1;
                    run(stderr, r#"{"ok":false}"#, code)
                },
                std::time::Duration::from_secs(10),
                std::time::Duration::from_secs(2),
                |_| panic!("must not sleep"),
            )
            .unwrap();
            assert_eq!(calls, 1, "{stderr}");
            assert!(!r.is_ok());
        }
        // A contract break is an error, not a retry.
        let e = run_with_busy_retry(
            "x",
            || run("MH_RESULT STATUS=FAIL REASON=nope\n", r#"{"ok":false}"#, 1),
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(2),
            |_| panic!("must not sleep"),
        )
        .unwrap_err();
        assert!(e.contains("known-token"), "{e}");
    }

    fn slot_entry(slot: u32, state: &str, owner: Option<&str>) -> SlotEntry {
        let pod = owner.map(|_| format!("mh-s{slot}-x"));
        serde_json::from_value(entry_owned(slot, state, pod.as_deref(), owner)).unwrap()
    }

    fn entry_owned(slot: u32, state: &str, pod: Option<&str>, owner: Option<&str>) -> Value {
        let mut e = entry(slot, state, pod);
        e["owner"] = json!(owner);
        e
    }

    fn snap(baseline: u32, slots: Vec<SlotEntry>) -> PoolSnapshot {
        PoolSnapshot {
            template_revision: "r1".into(),
            baseline,
            slots,
        }
    }

    fn hint(o: &str) -> String {
        format!("reclaim {o}")
    }

    #[test]
    fn baseline_deficit_counts_from_pool_state_not_from_a_drain_flag() {
        let run = "e2e-0123456789abcdef";
        // Positive arm: one baseline MH draining (the leg-B drain) -> 1.
        let drained = snap(
            2,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "draining", Some("baseline")),
                slot_entry(2, "free", None),
            ],
        );
        assert_eq!(baseline_deficit(&drained), 1);
        // A terminating one does not count either; a run-owned one never fills
        // a baseline hole.
        let gone = snap(
            2,
            vec![
                slot_entry(0, "terminating", Some("baseline")),
                slot_entry(1, "serving", Some(run)),
                slot_entry(2, "free", None),
            ],
        );
        assert_eq!(baseline_deficit(&gone), 2);
        // No drain happened (leg failed first, or the drain was refused) -> 0,
        // so the restore can never add a B+1-th baseline MH.
        let intact = snap(
            2,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "serving", Some("baseline")),
            ],
        );
        assert_eq!(baseline_deficit(&intact), 0);
        // A restore already under way (starting) counts as present.
        let restoring = snap(
            2,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "draining", Some("baseline")),
                slot_entry(2, "starting", Some("baseline")),
            ],
        );
        assert_eq!(baseline_deficit(&restoring), 0);
        // Over B (an extra baseline MH) saturates at 0, never wraps.
        let over = snap(
            1,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "serving", Some("baseline")),
            ],
        );
        assert_eq!(baseline_deficit(&over), 0);
    }

    #[test]
    fn baseline_drain_and_restore_spellings_never_force() {
        let drain = baseline_drain_args(4);
        assert_eq!(
            drain,
            ["drain", "4", "--owner", BASELINE_OWNER, "--no-wait"]
                .map(str::to_string)
                .to_vec()
        );
        assert!(!drain.iter().any(|a| a == "--force"));
        assert_eq!(
            baseline_restore_args(),
            ["up", "--owner", BASELINE_OWNER]
                .map(str::to_string)
                .to_vec()
        );
        // The printed command is byte-for-byte the executed argv.
        let cmd = baseline_restore_command(std::path::Path::new("/x/mh.sh"), "dt");
        assert_eq!(
            cmd,
            format!("DT_CLUSTER_NAME=dt /x/mh.sh up --owner {BASELINE_OWNER}")
        );
    }

    #[test]
    fn baseline_drop_notice_prints_the_restore_command() {
        let mut pool = MhPool::with(
            std::path::PathBuf::from("/x/mh.sh"),
            "dt".into(),
            "e2e-0123456789abcdef".into(),
        );
        let mut out = Vec::new();
        pool.baseline_drop_notice(&mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains(&baseline_restore_command(
                std::path::Path::new("/x/mh.sh"),
                "dt"
            )),
            "{text}"
        );
        assert!(text.contains("deploy.sh"), "{text}");
        // Quiet Drop for this hermetic handle.
        pool.torn_down = true;
    }

    const RESTORE: &str = "RESTORE-CMD up --owner baseline";

    #[test]
    fn baseline_verdict_cases() {
        let run = "e2e-0123456789abcdef";
        let clean = snap(
            2,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "serving", Some("baseline")),
                slot_entry(2, "serving", Some(run)),
                slot_entry(3, "free", None),
            ],
        );
        assert_eq!(baseline_verdict(&clean, run, hint, RESTORE), Ok(()));

        let mut foreign = clean.clone();
        foreign.slots[3] = slot_entry(3, "serving", Some("e2e-ffffffffffffffff"));
        let e = baseline_verdict(&foreign, run, hint, RESTORE).unwrap_err();
        assert!(e.starts_with(FOREIGN_MH_ACTIVITY), "{e}");
        assert!(
            e.contains("slot 3") && e.contains("e2e-ffffffffffffffff"),
            "{e}"
        );
        assert!(e.contains("reclaim e2e-ffffffffffffffff"), "{e}");
        assert!(!e.contains(RESTORE), "foreign-only: no restore hint: {e}");

        let mut manual = clean.clone();
        manual.slots[3] = slot_entry(3, "starting", Some("manual"));
        assert!(baseline_verdict(&manual, run, hint, RESTORE)
            .unwrap_err()
            .contains("\"manual\""));

        let mut short = clean.clone();
        short.slots[1] = slot_entry(1, "terminating", Some("baseline"));
        let e = baseline_verdict(&short, run, hint, RESTORE).unwrap_err();
        assert!(e.contains("baseline-owned serving MHs: 1, B = 2"), "{e}");
        assert!(
            e.contains(RESTORE) && e.contains("deploy.sh"),
            "short: the restore remedy: {e}"
        );

        let mut over = clean.clone();
        over.slots[3] = slot_entry(3, "serving", Some("baseline"));
        let e = baseline_verdict(&over, run, hint, RESTORE).unwrap_err();
        assert!(e.contains("serving MHs: 3, B = 2"), "{e}");
        assert!(!e.contains(RESTORE), "over-B: no restore hint: {e}");

        let mut starting = clean.clone();
        starting.slots[1] = slot_entry(1, "starting", Some("baseline"));
        assert!(baseline_verdict(&starting, run, hint, RESTORE).is_err());

        // A stale (upgrade-pending) baseline MH still serves and still counts.
        let mut stale = clean.clone();
        stale.slots[0].stale = true;
        assert_eq!(baseline_verdict(&stale, run, hint, RESTORE), Ok(()));
    }

    #[test]
    fn run_leak_verdict_cases() {
        let run = "e2e-0123456789abcdef";
        let clean = snap(
            1,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "free", None),
            ],
        );
        assert_eq!(run_leak_verdict(&clean, run), Ok(()));
        let mut leaked = clean.clone();
        leaked.slots[1] = slot_entry(1, "draining", Some(run));
        assert!(run_leak_verdict(&leaked, run)
            .unwrap_err()
            .starts_with(MH_RUN_LEAK));
        let mut stuck = clean.clone();
        stuck.slots[0].terminating_past_grace = true;
        assert!(run_leak_verdict(&stuck, run).is_err());
    }

    #[test]
    fn slot_for_address_is_exact_and_unique() {
        let s = snap(
            1,
            vec![
                slot_entry(0, "serving", Some("baseline")),
                slot_entry(1, "free", None),
            ],
        );
        assert_eq!(
            slot_for_address(&s, "https://127.0.0.1:4441").unwrap().slot,
            1
        );
        assert!(slot_for_address(&s, "https://127.0.0.1:9")
            .unwrap_err()
            .contains("no mh.sh slot"));
        let mut dup = s.clone();
        dup.slots[1].address = dup.slots[0].address.clone();
        assert!(slot_for_address(&dup, &dup.slots[0].address.clone())
            .unwrap_err()
            .contains("2 mh.sh slots share"));
    }

    fn signal_json(state: &str, budget: Option<u64>) -> Value {
        signal_json_with(state, budget, Some(75))
    }

    fn signal_json_with(state: &str, budget: Option<u64>, restart: Option<u64>) -> Value {
        let mut e = entry_owned(2, "serving", Some("mh-s2-x"), Some("e2e-0123456789abcdef"));
        e["ok"] = json!(true);
        e["pid"] = json!(7);
        e["process_state"] = json!(state);
        e["liveness_budget_seconds"] = json!(budget);
        e["restart_budget_seconds"] = json!(restart);
        e
    }

    #[test]
    fn signal_outcomes_must_match_the_signal() {
        let stop = parse_signal(&signal_json("T", Some(25)), Signal::Stop).unwrap();
        assert_eq!(
            stop.liveness_budget,
            Some(std::time::Duration::from_secs(25))
        );
        assert!(parse_signal(&signal_json("T", None), Signal::Stop)
            .unwrap_err()
            .contains("no liveness_budget_seconds"));
        assert!(parse_signal(&signal_json("S", Some(25)), Signal::Stop).is_err());
        assert!(parse_signal(&signal_json("T", Some(0)), Signal::Stop).is_err());
        let kill = parse_signal(&signal_json("gone", None), Signal::Kill).unwrap();
        assert_eq!(
            kill.restart_budget,
            Some(std::time::Duration::from_secs(75))
        );
        assert!(
            parse_signal(&signal_json_with("gone", None, None), Signal::Kill)
                .unwrap_err()
                .contains("no restart_budget_seconds")
        );
        // A KILL must never be read as a state letter.
        assert!(parse_signal(&signal_json("S", None), Signal::Kill).is_err());
        assert!(parse_signal(&signal_json("S", None), Signal::Cont).is_ok());
        assert!(parse_signal(&signal_json("T", None), Signal::Cont).is_err());
        assert!(parse_signal(&signal_json("gone", None), Signal::Cont).is_err());
        let mut no_state = signal_json("T", Some(25));
        no_state.as_object_mut().unwrap().remove("process_state");
        assert!(parse_signal(&no_state, Signal::Stop)
            .unwrap_err()
            .contains("no process_state"));
    }

    #[test]
    fn freeze_overrun_is_failure_only_at_the_budget() {
        let b = std::time::Duration::from_secs(25);
        assert_eq!(
            freeze_overrun(std::time::Duration::from_secs(24), b),
            Ok(())
        );
        assert!(freeze_overrun(b, b)
            .unwrap_err()
            .starts_with(FREEZE_BUDGET_EXCEEDED));
        assert!(freeze_overrun(std::time::Duration::from_secs(40), b).is_err());
    }

    #[test]
    fn incarnation_checks_both_ways() {
        let run = "e2e-0123456789abcdef";
        let before = slot_entry(2, "serving", Some(run));
        assert_eq!(assert_same_incarnation(&before, &before.clone()), Ok(()));
        let mut restarted = before.clone();
        restarted.restart_count = Some(1);
        assert!(assert_same_incarnation(&before, &restarted)
            .unwrap_err()
            .starts_with(RESTART_COUNT_CHANGED));
        let mut other = before.clone();
        other.pod_uid = Some("v".into());
        assert!(assert_same_incarnation(&before, &other).is_err());

        assert!(restarted_in_place(&before, &restarted));
        assert!(
            !restarted_in_place(&before, &before),
            "no restart is not a restart"
        );
        assert!(!restarted_in_place(&before, &other));
        let mut not_serving = restarted.clone();
        not_serving.state = SlotState::Starting;
        assert!(!restarted_in_place(&before, &not_serving));
    }

    #[test]
    fn commands_are_built_in_one_place() {
        let path = PathBuf::from("/repo/infra/kind/scripts/mh.sh");
        assert_eq!(
            reclaim_command(&path, "devloop-x", "e2e-0123456789abcdef"),
            "DT_CLUSTER_NAME=devloop-x /repo/infra/kind/scripts/mh.sh drain --all --owner \
             e2e-0123456789abcdef"
        );
        assert_eq!(
            teardown_args("o"),
            ["drain", "--all", "--owner", "o"],
            "the executed argv is the printed one"
        );
        assert_eq!(
            signal_args(4, Signal::Cont, "o"),
            ["signal", "4", "CONT", "--owner", "o"]
        );
        assert!(dt_cluster_name_from(None)
            .unwrap_err()
            .contains("unset or empty"));
        assert!(dt_cluster_name_from(Some(" ".into())).is_err());
        assert_eq!(dt_cluster_name_from(Some("c".into())).unwrap(), "c");
        assert_eq!(
            signal_command(&path, "c", 4, Signal::Cont, "o"),
            "DT_CLUSTER_NAME=c /repo/infra/kind/scripts/mh.sh signal 4 CONT --owner o"
        );
    }

    /// The Drop backstop prints exactly the reclaim command, and a drop while
    /// already panicking does not abort (no double panic): the path is
    /// resolved at construction, so Drop reads no environment.
    #[test]
    fn drop_prints_the_reclaim_command_and_never_panics() {
        let pool = MhPool::with(
            PathBuf::from("/nonexistent/mh.sh"),
            "c".into(),
            "e2e-0123456789abcdef".into(),
        );
        let mut out = Vec::new();
        pool.drop_notice(&mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "MhPool e2e-0123456789abcdef dropped without teardown; reclaim its MHs with: \
             DT_CLUSTER_NAME=c /nonexistent/mh.sh drain --all --owner e2e-0123456789abcdef\n"
        );
        let unwound = std::panic::catch_unwind(move || {
            let _held = pool;
            panic!("leg failed");
        });
        assert!(
            unwound.is_err(),
            "the leg's panic propagates; Drop did not abort"
        );
    }

    #[test]
    fn freeze_guard_drop_resumes_best_effort_and_never_panics() {
        let slot = MhSlot {
            slot: 4,
            deployment: "mh-s4".into(),
            pod: "mh-s4-x".into(),
            pod_uid: "u".into(),
            pod_ip: "10.0.0.4".into(),
            address: "https://127.0.0.1:4444".into(),
            restart_count: 0,
            mh_sh: PathBuf::from("/nonexistent/mh.sh"),
            cluster: "c".into(),
            owner: "o".into(),
        };
        let guard = FreezeGuard {
            before: slot_entry(4, "serving", Some("o")),
            slot,
            started: std::time::Instant::now(),
            stop_call: std::time::Duration::from_secs(1),
            budget: std::time::Duration::from_secs(25),
            resumed: false,
        };
        let mut out = Vec::new();
        guard.drop_notice(&mut out);
        assert!(String::from_utf8(out).unwrap().contains(
            "resume it with: DT_CLUSTER_NAME=c /nonexistent/mh.sh signal 4 CONT --owner o"
        ));
        // The best-effort CONT fails to spawn here; Drop must swallow that.
        let unwound = std::panic::catch_unwind(move || {
            let _held = guard;
            panic!("leg failed");
        });
        assert!(unwound.is_err());
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

        for key in [
            "ok",
            "schema_version",
            "template_revision",
            "baseline",
            "slots",
        ] {
            let mut d = doc(vec![entry(0, "free", None)]);
            d.as_object_mut().unwrap().remove(key);
            assert!(
                parse_list(&d.to_string()).is_err(),
                "a document without top-level `{key}` must be refused"
            );
        }

        for key in SLOT_KEYS {
            let mut dropped = entry(0, "free", None);
            dropped.as_object_mut().unwrap().remove(*key);
            assert!(
                parse_list(&doc(vec![dropped]).to_string())
                    .unwrap_err()
                    .contains(&format!("no `{key}` key")),
                "dropping {key} must be refused"
            );
        }

        // Additive-only contract: an extra key (top level and per slot) passes.
        let mut extra = entry(0, "free", None);
        extra["future_field"] = json!("x");
        let mut d = doc(vec![extra]);
        d["future_top"] = json!(1);
        assert!(parse_list(&d.to_string()).is_ok());

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
        let real = crate::repo_root().join(MH_SH_REPO_PATH);
        assert_eq!(mh_sh_path_from(Some(real.clone())).unwrap(), real);
    }

    /// [`SLOT_KEYS`] == the key list of mh.sh's `MH_JQ_PUBLIC` (the ONE home
    /// of the v1 slot schema), read from the committed script.
    #[test]
    fn slot_keys_match_mh_sh_public_schema() {
        let text = mh_sh_text();
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

    /// The re-encoded contract values have one home each: `SCHEMA_VERSION`
    /// is mh.sh's `MH_SCHEMA_VERSION`, `BASELINE_OWNER` is deploy.sh's
    /// `MH_BASELINE_OWNER` (the label it stamps on the baseline MHs).
    #[test]
    fn schema_version_and_baseline_owner_match_their_homes() {
        assert_eq!(
            mh_sh_int(&mh_sh_text(), "MH_SCHEMA_VERSION"),
            SCHEMA_VERSION,
            "SCHEMA_VERSION drifted from mh.sh"
        );
        let deploy =
            std::fs::read_to_string(crate::repo_root().join("infra/kind/scripts/deploy.sh"))
                .expect("read deploy.sh");
        let owner = deploy
            .lines()
            .find_map(|l| l.trim().strip_prefix("MH_BASELINE_OWNER=\""))
            .and_then(|r| r.strip_suffix('"'))
            .expect("positive control: deploy.sh defines MH_BASELINE_OWNER=\"...\"");
        assert_eq!(
            owner, BASELINE_OWNER,
            "BASELINE_OWNER drifted from deploy.sh"
        );
    }
}
