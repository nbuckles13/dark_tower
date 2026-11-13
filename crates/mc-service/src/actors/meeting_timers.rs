//! The meeting actor's lifetime timers: the join window and the disconnect
//! grace period, carried config → controller → every meeting actor in ONE
//! value ([`MeetingTimers`]).
//!
//! RULING (2026-11-12): an MC meeting exists only while it has participants.
//! The last leave ends it at once (the existing choke point); a meeting that
//! no participant joins within the JOIN WINDOW — measured from actor creation,
//! i.e. GC's assignment — ends through the same path
//! (`MeetingEndCause::Empty(EmptyReason::JoinWindowExpired)`).

use super::join_park::SDK_JOIN_DEADLINE;
use crate::config::DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS;
use common::jwt::JWT_EXP_LEEWAY_SECONDS;
use common::meeting_token::MEETING_TOKEN_MAX_TTL_SECONDS;
use std::time::Duration;

/// The period of the meeting actor's lifetime check (disconnect-grace
/// expiry). The documented worst-case crash roster-removal latency is
/// `quic idle timeout + disconnect grace + LIFETIME_TICK`. The join window
/// does NOT ride this tick — it is an exact deadline.
pub const LIFETIME_TICK: Duration = Duration::from_secs(5);

/// The DERIVED default join window, and its ceiling:
/// `MEETING_TOKEN_MAX_TTL_SECONDS + JWT_EXP_LEEWAY_SECONDS + SDK_JOIN_DEADLINE`.
///
/// Why: GC assigns the meeting (creating this actor) and then mints the
/// assigning joiner's token. MC accepts that token until
/// `iat + TTL + exp leeway`; the joiner then has at most the SDK's join
/// deadline to be admitted (the few ms between assignment and mint fall inside
/// that term). Past this, no credential issued at assignment can still produce
/// a participant, so a still-empty meeting can only be a leak — and a window
/// configured longer only re-opens the leak, so this is also the maximum.
///
/// Assumes AC's and MC's wall clocks are synced: `exp` is set on AC's clock
/// and checked on MC's. If AC runs ahead, a token can outlive the window by
/// the skew; the result is a refused join that the SDK's GC re-join recovers.
/// Do NOT "fix" a skew-induced refusal by adding `DEFAULT_CLOCK_SKEW` here.
pub const MAX_JOIN_WINDOW: Duration =
    Duration::from_secs(MEETING_TOKEN_MAX_TTL_SECONDS as u64 + JWT_EXP_LEEWAY_SECONDS)
        .saturating_add(SDK_JOIN_DEADLINE);

/// The smallest legal join window: [`LIFETIME_TICK`].
///
/// Re-derived 2026-11-13 (operator direction: short configured windows in
/// production and Kind; the previous floor `FIRST_JOIN_PARK_DEADLINE + SDK_JOIN_DEADLINE` = 23 s
/// over-counted). What the window can actually cut short, and why neither is
/// a reason for a larger floor:
///
/// - **The first-join park is NOT exposed.** `handle_join` inserts the joiner
///   into the roster BEFORE parking it for the initial handlers, and a
///   non-empty roster disarms the window — so the park deadline is not part
///   of the race at all.
/// - **The only exposure is the GC-join → MC-admission gap** (AC token mint,
///   client identity/VP8 probe, WebTransport connect, JoinRequest). A join
///   that loses that race is refused `NOT_FOUND` / `CONFLICT` — fail-closed
///   and keyless — and the SDK's bounded GC re-join (`JOIN_RETRY`) gets a
///   fresh assignment with a fresh window. So the window is not a
///   join-correctness bound; the gap is environment latency (well under a
///   second to a couple of seconds in-cluster), not a code constant, and a
///   window must simply sit clearly above it for the retry to converge.
///
/// MC ends a never-joined meeting AT the window (an exact deadline); an
/// observer should allow up to one `LIFETIME_TICK` of tolerance and can
/// check `age_ms` on the end line.
///
/// What the floor DOES guard. The window is enforced as an EXACT deadline
/// (`sleep_until(created + window)` in the actor loop), so the enforced
/// bound is the window itself, not the tick. [`LIFETIME_TICK`] is only the
/// OBSERVER tolerance: the task's "window + one tick" bound, which the
/// env-test and the expiry line's `lifetime_tick_ms` field allow. Below that
/// tolerance, an end at the window could not be told apart from a much later
/// end, and a 0 / sub-tick window is the degenerate "end every meeting at
/// once" configuration. [`LIFETIME_TICK`] is therefore the floor; it is also
/// above any normal GC-join → admission latency.
pub const MIN_JOIN_WINDOW: Duration = LIFETIME_TICK;

/// The two lifetime timers a meeting actor enforces. Built ONCE from config
/// (`Config::meeting_timers`) and handed to every meeting actor; the config
/// gauges echo the same value, so published and enforced cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeetingTimers {
    /// End a meeting no participant was admitted to within this long of its
    /// creation (`MC_MEETING_JOIN_WINDOW_SECONDS`).
    pub join_window: Duration,
    /// How long a participant whose connection dropped abruptly stays on the
    /// roster for a reconnect (`MC_DISCONNECT_GRACE_PERIOD_SECONDS`).
    pub disconnect_grace: Duration,
}

impl Default for MeetingTimers {
    /// The same defaults `Config` applies to unset keys.
    fn default() -> Self {
        Self {
            join_window: MAX_JOIN_WINDOW,
            disconnect_grace: Duration::from_secs(DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The default is the constant sum, AND pinned as a literal so an
    /// unnoticed upstream change (token cap, leeway, SDK deadline) shows here.
    #[test]
    fn the_join_window_bounds_are_derived_and_pinned() {
        assert_eq!(
            MAX_JOIN_WINDOW,
            Duration::from_secs(u64::from(MEETING_TOKEN_MAX_TTL_SECONDS) + JWT_EXP_LEEWAY_SECONDS)
                + SDK_JOIN_DEADLINE
        );
        assert_eq!(MAX_JOIN_WINDOW, Duration::from_secs(975));
        assert_eq!(MIN_JOIN_WINDOW, LIFETIME_TICK);
        assert_eq!(MIN_JOIN_WINDOW, Duration::from_secs(5));
        assert_eq!(MeetingTimers::default().join_window, MAX_JOIN_WINDOW);
    }

    const MC_ALERT_RULES: &str =
        include_str!("../../../../infra/docker/prometheus/rules/mc-alerts.yaml");

    #[derive(serde::Deserialize)]
    struct RulesDoc {
        groups: Vec<RuleGroup>,
    }
    #[derive(serde::Deserialize)]
    struct RuleGroup {
        rules: Vec<Rule>,
    }
    #[derive(serde::Deserialize)]
    struct Rule {
        alert: Option<String>,
        #[serde(rename = "for")]
        for_: Option<String>,
    }

    /// A Prometheus duration of the forms this rule uses (`17m`, `900s`, `1h`).
    fn prom_duration(raw: &str) -> Duration {
        let (digits, unit) = raw.split_at(raw.len() - 1);
        let n: u64 = digits
            .parse()
            .unwrap_or_else(|e| panic!("unparseable `for:` {raw:?}: {e}"));
        match unit {
            "s" => Duration::from_secs(n),
            "m" => Duration::from_secs(n * 60),
            "h" => Duration::from_secs(n * 3600),
            other => panic!("unsupported `for:` unit {other:?} in {raw:?}"),
        }
    }

    /// `MCLowConnectionCount` is the never-joined leak detector: a meeting
    /// with zero connections is LEGAL for up to the join window, so the rule's
    /// `for:` must outlast the largest legal window plus the lifetime tick —
    /// read from the SAME ceiling `config.rs` enforces, never a re-derived
    /// number — or every abandoned GC join on an idle MC pages.
    #[test]
    fn low_connection_alert_outlasts_every_legal_join_window() {
        let doc: RulesDoc = serde_norway::from_str(MC_ALERT_RULES).expect("mc-alerts.yaml parses");
        let rule_for = doc
            .groups
            .into_iter()
            .flat_map(|g| g.rules)
            .find(|r| r.alert.as_deref() == Some("MCLowConnectionCount"))
            .expect("MCLowConnectionCount exists in mc-alerts.yaml")
            .for_
            .expect("MCLowConnectionCount has a `for:`");
        let held = prom_duration(rule_for.trim());
        assert!(
            held >= MAX_JOIN_WINDOW + LIFETIME_TICK,
            "MCLowConnectionCount `for: {rule_for}` ({held:?}) must be >= MAX_JOIN_WINDOW \
             {MAX_JOIN_WINDOW:?} + LIFETIME_TICK {LIFETIME_TICK:?}"
        );
    }
}
