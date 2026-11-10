//! R-28 (story 4; ADR-0039 §7 "Kind"): every live MH pod's client-facing
//! address is UNIQUE and is ITS SLOT's, on the live cluster.
//!
//! The pure verdict, its evidence model (why `mh.sh list` is only an
//! enumeration, never the evidence), every violation token AND the live gather
//! (`mh_address::assert_addresses_unique`, hoisted here from this file at T-1 so
//! the resilience legs share it) live in `env_tests::fixtures::mh_address`; the
//! verdict is unit-tested at Layer 4 against adverse inputs. The gather reads:
//!
//! - the pool, `mh.sh list --json` (path from `DT_MH_SH`, exported by
//!   `scripts/layer7.sh`; unset is a loud PRECONDITION);
//! - every non-terminating `app=mh-service` pod (kubectl), after the pool has
//!   settled (every MH pod Ready, none Terminating);
//! - the slot ConfigMaps, by label (kubectl, not mh.sh);
//! - each pod's current-container log, for the address the PROCESS advertises;
//! - `networkpolicy/mh-service`'s podSelector (security S1: the policy governs
//!   every pool pod).
//!
//! Whether each address actually ROUTES to its pod is R-27
//! (`43_mh_slot_publish_udp.rs`). Steady state only: the invariant during
//! upgrade / drain / crash restart is T-1's resilience legs.
//!
//! # Residual
//!
//! The process-observed address comes from the MH startup log line. On a pod
//! that has lived through enough log volume for kubelet to rotate that line
//! away, this fails with `startup-line-missing` — loudly, and distinct from an
//! address mismatch. Remedy: re-incarnate the pool (`mh.sh upgrade` after a
//! template change, or drain + up a slot). MH publishes its address nowhere
//! else a black-box test can read.
//!
//! # Diagnostic output policy (PII)
//!
//! Pod names, slot numbers, ConfigMap names, addresses (dev-cluster host
//! gateway IP:port, not personal data), label key=value pairs. Never a log
//! line, never a raw kubectl document.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "smoke")]

use env_tests::fixtures::mh_address::assert_addresses_unique;

#[tokio::test]
async fn every_live_mh_pod_advertises_its_own_unique_slot_address() {
    // The whole live gather + verdict lives in the fixture (one home; the
    // resilience legs call it after each leg).
    let (pods, slots) = assert_addresses_unique().await;
    println!(
        "R28 PASS: {pods} MH pod(s) on {slots} slot(s), each advertising its own slot address"
    );
}
