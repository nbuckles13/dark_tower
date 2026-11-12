//! Placing a meeting on a RUN-OWNED MH, for the story 4 resilience legs that
//! kill or freeze a handler carrying a meeting (MC-B2 crash-to-survivor and
//! zombie; ADR-0039 §9, R-39).
//!
//! # Why a bounded search, and why its bound is sound
//!
//! `mh.sh signal` is owner-gated (`foreign-owner`): a leg may KILL or STOP
//! only an MH it brought up itself. GC places a new meeting on
//! `GC_MH_INITIAL_HANDLERS_PER_MEETING` handlers chosen weighted-random over
//! every registered MH, baseline and run-owned alike
//! (`gc-service/src/services/weighted_pick.rs`, weight `1 - load`). No GC knob
//! pins a meeting, and the baseline MHs are foreign-owned, so they cannot be
//! drained out of the way. So the leg creates meetings until one is placed on
//! its MH.
//!
//! The attempt count is DERIVED, never a literal ([`placement_attempts`]). With
//! `m` selectable MHs and `s` picks per meeting, drawn without replacement,
//! the uniform miss probability is `(m - s) / m`. A fresh run-owned MH carries
//! no load (weight 1.0, the maximum), so its real miss probability is at most
//! that. Every miss leaves the owned MH untouched (the missed meeting is not
//! on it), so the bound holds on every attempt. `k` attempts then miss with
//! probability at most `((m - s) / m)^k`, and `k` is the smallest value that
//! puts this at or under [`PLACEMENT_MISS_CEILING`]. Running out is a loud
//! [`OWNED_MH_NOT_PLACED`] failure that lists every observed set. It is never
//! retried and never skipped.
//!
//! # Cost
//!
//! One attempt is one `create_meeting` plus ONE MC join (the first joiner's
//! `JoinResponse.media_handler_set` IS the meeting's set; later joins never
//! change membership). The users are registered once per search, not per
//! attempt. A missed meeting's participant is dropped at once, which closes
//! its MC session. The meeting is on baseline MHs only and ends by MC's normal
//! empty-meeting path.

use crate::fixtures::auth_client::UserRegistrationRequest;
use crate::fixtures::gc_client::{CreateMeetingRequest, JoinMeetingResponse};
use crate::fixtures::mc_session::JoinDeclaration;
use crate::fixtures::mh_pool::MhPool;
use crate::fixtures::participant::{mc_join_with, Participant};
use crate::fixtures::{AuthClient, GcClient};

/// Reason token: the bounded search never placed a meeting on the run-owned
/// MH (or never with the required member count).
pub const OWNED_MH_NOT_PLACED: &str = "owned-mh-not-placed";

/// The largest acceptable probability that the search misses on a healthy
/// cluster. It is a flake budget, not a timing value: a run fails spuriously
/// at most once in this many.
pub const PLACEMENT_MISS_CEILING: f64 = 1e-6;

pub use super::gc_fleet::INITIAL_HANDLERS_KEY;

/// PURE: the attempt count `k` for a search over `selectable` MHs with
/// `per_meeting` picks each, so that the miss probability (see the module
/// doc) is at most `miss_ceiling`. One attempt when every meeting takes every
/// MH.
///
/// # Errors
/// `selectable` or `per_meeting` is 0, or `miss_ceiling` is not in (0, 1).
pub fn placement_attempts(
    selectable: usize,
    per_meeting: usize,
    miss_ceiling: f64,
) -> Result<u32, String> {
    if selectable == 0 {
        return Err("no selectable MH: the owned MH is not even registered".to_string());
    }
    if per_meeting == 0 {
        return Err(format!("{INITIAL_HANDLERS_KEY}=0 places no handler"));
    }
    if !(miss_ceiling > 0.0 && miss_ceiling < 1.0) {
        return Err(format!("miss ceiling {miss_ceiling} is not in (0, 1)"));
    }
    if per_meeting >= selectable {
        return Ok(1);
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "pool sizes are tiny (K = 6 slots)"
    )]
    let miss = (selectable - per_meeting) as f64 / selectable as f64;
    let k = (miss_ceiling.ln() / miss.ln()).ceil().max(1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "k is a small positive integer: ln(1e-6) / ln(5/6) < 76"
    )]
    Ok(k as u32)
}

/// PURE: a placement HIT. The set names `owned`, and it has exactly
/// `members` handlers when a member count is required (a crash-to-survivor
/// leg needs `owned` plus one survivor).
#[must_use]
pub fn placement_hit(set_urls: &[String], owned: &str, members: Option<usize>) -> bool {
    set_urls.iter().any(|u| u == owned) && members.is_none_or(|n| set_urls.len() == n)
}

/// A meeting GC placed on the run-owned MH: the remaining registered users,
/// ready to join it. The first joiner (user 0, whose JoinResponse proved the
/// placement) is returned beside it by [`place_on_owned`].
pub struct OwnedMeeting<'a> {
    gc: &'a GcClient,
    meeting_code: String,
    /// `(access token, display name)` of users 1.., not yet joined.
    rest: Vec<(String, String)>,
    /// Attempts the search used (logged by the leg).
    pub attempts: u32,
}

impl OwnedMeeting<'_> {
    /// A fresh GC meeting token for registered user `i` (1-based over the
    /// users that are not `first`).
    pub async fn gc_join(&self, i: usize) -> JoinMeetingResponse {
        self.gc
            .join_meeting(&self.meeting_code, &self.rest[i - 1].0)
            .await
            .expect("GC should issue a meeting token")
    }

    /// Join registered user `i` (>= 1) through GC -> MC.
    pub async fn join(
        &self,
        i: usize,
        label: &'static str,
        declaration: &JoinDeclaration,
    ) -> Participant {
        let gc_join = self.gc_join(i).await;
        mc_join_with(label, &gc_join, &self.rest[i - 1].1, declaration).await
    }
}

/// [`placement_attempts`] over the LIVE pool: `m` = the serving slots in
/// `mh.sh list` (baseline plus run-owned), `s` = GC's deployed
/// [`INITIAL_HANDLERS_KEY`] from the generation the serving GC pods run.
///
/// # Panics
/// PRECONDITION (`owned-mh-not-placed`) on a degenerate pool or key.
pub async fn live_placement_attempts(pool: &std::sync::Arc<MhPool>) -> u32 {
    let p = std::sync::Arc::clone(pool);
    let selectable = tokio::task::spawn_blocking(move || p.snapshot().serving().len())
        .await
        .expect("mh.sh list task");
    let gc = crate::fixtures::gc_fleet::live_gc_pods().await;
    let per_meeting = crate::fixtures::kube::configmap_u64(&gc.configmap, INITIAL_HANDLERS_KEY);
    let per_meeting = usize::try_from(per_meeting).unwrap_or(usize::MAX);
    placement_attempts(selectable, per_meeting, PLACEMENT_MISS_CEILING).unwrap_or_else(|e| {
        panic!("PRECONDITION ({OWNED_MH_NOT_PLACED}): {e} (selectable {selectable})")
    })
}

/// Create meetings until one is placed on the MH at `owned_address` (and has
/// exactly `members` handlers, if given). Registers `users` users (>= 1)
/// ONCE. User 0 creates each meeting and joins it as `first_label`; that
/// participant is returned first. `attempts` comes from
/// [`placement_attempts`] over the live pool.
///
/// # Panics
/// [`OWNED_MH_NOT_PLACED`] after `attempts` misses, listing every observed
/// set. An AC/GC failure panics as itself.
#[allow(clippy::too_many_arguments)]
pub async fn place_on_owned<'a>(
    auth: &AuthClient,
    gc: &'a GcClient,
    title: &str,
    users: usize,
    first_label: &'static str,
    owned_address: &str,
    members: Option<usize>,
    attempts: u32,
) -> (Participant, OwnedMeeting<'a>) {
    assert!(
        users >= 1,
        "place_on_owned needs at least the creating user"
    );
    let mut registered = Vec::new();
    for i in 0..users {
        let request = UserRegistrationRequest::unique(format!("{title} {i}"));
        let display = request.display_name.clone();
        let r = auth
            .register_user(&request)
            .await
            .expect("AC should register a test user");
        registered.push((r.access_token, display));
    }
    let mut observed: Vec<Vec<String>> = Vec::new();
    for attempt in 1..=attempts {
        let created = gc
            .create_meeting(&registered[0].0, &CreateMeetingRequest::new(title))
            .await
            .expect("GC should create the meeting");
        let gc_join = gc
            .join_meeting(&created.meeting_code, &registered[0].0)
            .await
            .expect("GC should issue a meeting token");
        let first = mc_join_with(
            first_label,
            &gc_join,
            &registered[0].1,
            &JoinDeclaration::default(),
        )
        .await;
        let urls: Vec<String> = first
            .handler_set()
            .handlers
            .iter()
            .map(|h| h.media_handler_url.clone())
            .collect();
        if placement_hit(&urls, owned_address, members) {
            let rest = registered.split_off(1);
            return (
                first,
                OwnedMeeting {
                    gc,
                    meeting_code: created.meeting_code,
                    rest,
                    attempts: attempt,
                },
            );
        }
        observed.push(urls);
        // A miss: dropping the participant closes its MC session. The meeting
        // is not on the owned MH, so the owned MH stays load-free.
        drop(first);
    }
    panic!(
        "{OWNED_MH_NOT_PLACED}: {attempts} meetings (derived for a miss probability <= \
         {PLACEMENT_MISS_CEILING}) and none was placed on the run-owned MH at {owned_address}{}. \
         Observed sets: {observed:?}. Check that the owned MH is GC-selectable \
         (gc_mh_available) and that {INITIAL_HANDLERS_KEY} matches the expected member count",
        members.map_or_else(String::new, |n| format!(" with exactly {n} members"))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_meet_the_ceiling_and_are_minimal() {
        for (m, s) in [(3_usize, 2_usize), (4, 2), (6, 2), (7, 2), (3, 1), (6, 1)] {
            let k = placement_attempts(m, s, PLACEMENT_MISS_CEILING).unwrap();
            #[expect(clippy::cast_precision_loss, reason = "tiny values")]
            let miss = (m - s) as f64 / m as f64;
            let p = |k: u32| miss.powi(i32::try_from(k).unwrap());
            assert!(
                p(k) <= PLACEMENT_MISS_CEILING,
                "m={m} s={s}: k={k} misses with {}",
                p(k)
            );
            assert!(
                p(k - 1) > PLACEMENT_MISS_CEILING,
                "m={m} s={s}: k={k} is not minimal"
            );
        }
        // Known value: m=3, s=2 misses with 1/3 per attempt -> 13 attempts.
        assert_eq!(placement_attempts(3, 2, PLACEMENT_MISS_CEILING), Ok(13));
    }

    #[test]
    fn every_meeting_on_every_mh_needs_one_attempt() {
        assert_eq!(placement_attempts(2, 2, PLACEMENT_MISS_CEILING), Ok(1));
        assert_eq!(placement_attempts(1, 2, PLACEMENT_MISS_CEILING), Ok(1));
    }

    #[test]
    fn degenerate_inputs_are_errors_not_zero_attempts() {
        assert!(placement_attempts(0, 2, PLACEMENT_MISS_CEILING).is_err());
        assert!(placement_attempts(3, 0, PLACEMENT_MISS_CEILING).is_err());
        assert!(placement_attempts(3, 2, 0.0).is_err());
        assert!(placement_attempts(3, 2, 1.0).is_err());
    }

    #[test]
    fn hit_needs_the_owned_url_and_the_member_count() {
        let owned = "https://172.18.0.1:24423";
        let set = |u: &[&str]| u.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        // Positive arms.
        assert!(placement_hit(&set(&[owned]), owned, None));
        assert!(placement_hit(&set(&["https://x:1", owned]), owned, Some(2)));
        // Negative arms: owned absent; owned present at the wrong count.
        assert!(!placement_hit(
            &set(&["https://x:1", "https://x:2"]),
            owned,
            Some(2)
        ));
        assert!(!placement_hit(&set(&[owned]), owned, Some(2)));
        assert!(!placement_hit(&set(&[]), owned, None));
        // Exact match only: a prefix of the owned url is not it.
        assert!(!placement_hit(
            &set(&["https://172.18.0.1:2442"]),
            owned,
            None
        ));
    }
}
