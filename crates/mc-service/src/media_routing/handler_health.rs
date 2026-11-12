//! The ONE health decision per (meeting, handler) (ADR-0039 §2).
//!
//! [`assess`] is pure: no I/O, no timers, no clock read. The meeting actor
//! owns it — it keeps the evidence (the liveness record beside the handler
//! set, the first-confirmed instant on the set member), calls `assess` on
//! every heartbeat from a member and on a `sleep_until` arm at
//! [`next_deadline`], and acts ONLY on transitions.
//!
//! # States
//!
//! - **Live** (reversible): healthy, or any combination of
//!   - *lagging* — the applied generation is behind the generation MH OWES
//!     (what MC had sent by the previous accepted heartbeat, see
//!     `HandlerLiveness::owed_generation`). While lagging the actor
//!     re-publishes the current snapshot to the handler's push worker;
//!   - *contract fault* — MH refused MC's policy as `INVALID` or
//!     `TRANSPORT_INCONSISTENT`. MC's own bug: loud, counted, NEVER a
//!     declaration, and it suppresses the lag clock (the lag is explained by
//!     MC's fault; a replacement would refuse the same policy);
//!   - *draining* — MH reported DRAINING. Recorded for the planned-transfer
//!     path (MC-F); never a removal here.
//! - **Declared** (one-way, absorbing): **Dead** (no accepted heartbeat for
//!   longer than N, the clock starting at the first confirmed
//!   `RegisterMeeting`) or **Impaired** (lag held longer than L = 2N, or a
//!   load-dependent `STREAM_CEILING` / `EDGE_CAP` refusal). A declaration is a
//!   removal; nothing un-declares it.
//!
//! **A refusal counts only while MC's policy is pending** (the accepted
//! generation is behind what MH owes): with MC's current policy installed, a
//! refusal can only concern a policy MC never sent — an out-of-band writer —
//! and is ignored. Likewise a heartbeat AHEAD of MC's sent generation is
//! liveness with its generation clamped to `sent` (`liveness.rs`).
//!
//! Precedence: Dead, then capacity refusal, then lag held. gRPC channel
//! connectivity is NOT an input (not deterministically testable). Client loss
//! reports ARE an input with weight 0 in this story (MC-E reads them; a
//! future story may promote them by policy, not by a restructure).
//!
//! # One boundary
//!
//! "Longer than N" is [`expired`]: at least [`DEADLINE_RESOLUTION`] past the
//! span. [`expiry`] is the instant that first satisfies it, and is what
//! [`next_deadline`] returns — the comparison and the wake can never drift,
//! and a wake never re-arms the instant it fired at.
//!
//! # No identity, no key material
//!
//! Every type here holds instants, generations and bounded enums. Nothing is a
//! url, a token or a connection id, so a log line built from them cannot leak
//! one.

use super::liveness::HeartbeatTiming;
use super::placement::RemovalTrigger;
use proto_gen::dark_tower::internal::v1::{ApplyRefusal, ServingState};
use std::time::Duration;
use tokio::time::Instant;

/// The resolution of every health boundary: "longer than X" means at least
/// this much past X. One millisecond — the tokio timer's granularity.
pub const DEADLINE_RESOLUTION: Duration = Duration::from_millis(1);

/// The first instant at which `span` measured from `origin` has been
/// EXCEEDED — the wake [`next_deadline`] arms.
#[must_use]
pub fn expiry(origin: Instant, span: Duration) -> Instant {
    origin + span + DEADLINE_RESOLUTION
}

/// Whether `span` measured from `origin` has been exceeded at `now`. The ONE
/// comparison both `assess` and [`next_deadline`] rest on.
#[must_use]
pub fn expired(origin: Instant, span: Duration, now: Instant) -> bool {
    now >= expiry(origin, span)
}

/// The two spans `assess` judges against, both derived from N.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthConfig {
    /// N: silence longer than this is Dead.
    pub timeout: Duration,
    /// L = `APPLY_LAG_FACTOR` x N: lag held longer than this is Impaired.
    pub lag_limit: Duration,
}

impl HealthConfig {
    /// The ONE derivation, from MC's heartbeat timing.
    #[must_use]
    pub fn from_timing(timing: HeartbeatTiming) -> Self {
        Self {
            timeout: timing.timeout(),
            lag_limit: timing.apply_lag_limit(),
        }
    }
}

/// An MC contract-fault refusal (ADR-0039 §1): never a replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractFault {
    /// `INVALID`.
    Invalid,
    /// `TRANSPORT_INCONSISTENT`.
    TransportInconsistent,
}

impl ContractFault {
    /// Bounded log label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::TransportInconsistent => "transport_inconsistent",
        }
    }
}

/// A load-dependent refusal (ADR-0039 §1): feeds the replace path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityRefusal {
    /// `STREAM_CEILING`.
    StreamCeiling,
    /// `EDGE_CAP`.
    EdgeCap,
}

impl CapacityRefusal {
    /// Bounded log label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::StreamCeiling => "stream_ceiling",
            Self::EdgeCap => "edge_cap",
        }
    }
}

/// The latest `apply_refusal`, projected ONCE at the edge into what `assess`
/// does with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalClass {
    /// No refusal (the latest apply succeeded).
    None,
    /// An MC contract fault.
    Contract(ContractFault),
    /// A capacity refusal.
    Capacity(CapacityRefusal),
}

impl RefusalClass {
    /// Bounded log label of the latest refusal (`none` when the last apply
    /// succeeded).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Contract(f) => f.label(),
            Self::Capacity(c) => c.label(),
        }
    }

    /// The contract fault, if this refusal is one.
    #[must_use]
    pub const fn contract_fault(self) -> Option<ContractFault> {
        match self {
            Self::Contract(f) => Some(f),
            Self::None | Self::Capacity(_) => None,
        }
    }

    /// The ONE projection of the wire enum.
    #[must_use]
    pub const fn from_wire(refusal: ApplyRefusal) -> Self {
        match refusal {
            ApplyRefusal::Unspecified => Self::None,
            ApplyRefusal::Invalid => Self::Contract(ContractFault::Invalid),
            ApplyRefusal::TransportInconsistent => {
                Self::Contract(ContractFault::TransportInconsistent)
            }
            ApplyRefusal::StreamCeiling => Self::Capacity(CapacityRefusal::StreamCeiling),
            ApplyRefusal::EdgeCap => Self::Capacity(CapacityRefusal::EdgeCap),
        }
    }
}

/// What the actor knows about one pair, at the instant of an assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observations {
    /// When the handler's first `RegisterMeeting` was confirmed (it became
    /// Active). `None` while Registering: an unconfirmed handler is never Dead.
    pub confirmed_at: Option<Instant>,
    /// Receipt (gRPC arrival) of the last ACCEPTED heartbeat. Stale and
    /// ahead-of-sent heartbeats never move it.
    pub last_accepted: Option<Instant>,
    /// The last accepted `applied_generation`.
    pub applied: Option<u64>,
    /// The generation MH owes (sent by the previous accepted heartbeat).
    pub owed: Option<u64>,
    /// The latest refusal, projected.
    pub refusal: RefusalClass,
    /// Whether the latest serving state is DRAINING.
    pub draining: bool,
    /// Client loss reports for this handler. WEIGHT 0 in this story: read,
    /// never acted on (MC-E computes its metric from them).
    pub client_loss_reports: u32,
}

impl Observations {
    /// Whether the accepted generation is behind what MH owes: MC's policy is
    /// PENDING on the handler.
    #[must_use]
    pub fn behind(&self) -> bool {
        matches!((self.applied, self.owed), (Some(a), Some(o)) if a < o)
    }

    /// THE gate on whether the latest refusal is evidence about MC's policy —
    /// used by `assess` AND by the actor's contract-fault anomaly, so the two
    /// can never disagree. Only while MC's policy is pending ([`Self::behind`]);
    /// with MC's current policy installed a refusal can only concern a policy
    /// MC never sent (an out-of-band writer) and reads as no refusal.
    #[must_use]
    pub fn effective_refusal(&self) -> RefusalClass {
        if self.behind() {
            self.refusal
        } else {
            RefusalClass::None
        }
    }
}

/// Whether a serving state is DRAINING: the ONE projection of the wire
/// serving state `assess` reads (beside [`RefusalClass::from_wire`]).
#[must_use]
pub fn is_draining(state: ServingState) -> bool {
    state == ServingState::Draining
}

/// Why a handler was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclareCause {
    /// No accepted heartbeat for longer than N (Dead).
    Silence,
    /// The applied generation stayed behind for longer than L (Impaired).
    LagHeld,
    /// A load-dependent refusal (Impaired).
    Capacity(CapacityRefusal),
}

impl DeclareCause {
    /// Bounded log label: WHY the trigger fired (`apply_lag` has two causes
    /// with different remedies — an MH that stopped applying vs a capacity
    /// refusal).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::LagHeld => "lag_held",
            Self::Capacity(c) => c.label(),
        }
    }
}

/// A one-way declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declaration {
    /// Why.
    pub cause: DeclareCause,
    /// Where the detection clock started: the last accepted heartbeat (or the
    /// first confirmation) for Silence, the lag onset for LagHeld, the
    /// refusing heartbeat's receipt for Capacity.
    pub evidence_at: Instant,
}

impl Declaration {
    /// The removal trigger (the shared `trigger` vocabulary).
    #[must_use]
    pub const fn trigger(self) -> RemovalTrigger {
        match self.cause {
            DeclareCause::Silence => RemovalTrigger::HeartbeatTimeout,
            DeclareCause::LagHeld | DeclareCause::Capacity(_) => RemovalTrigger::ApplyLag,
        }
    }

    /// `mc_media_handler_detection_seconds`: evidence start → declaration.
    #[must_use]
    pub fn detection(self, now: Instant) -> Duration {
        now.saturating_duration_since(self.evidence_at)
    }
}

/// The reversible part of a handler's health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LiveHealth {
    /// Lag onset: when the pair was first seen behind what it owes.
    pub lag_since: Option<Instant>,
    /// The active contract-fault refusal, if any (suppresses the lag clock).
    pub contract_fault: Option<ContractFault>,
    /// When DRAINING was first seen (sticky: MH never un-drains).
    pub draining_since: Option<Instant>,
}

/// One pair's health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerHealth {
    /// Reversible states.
    Live(LiveHealth),
    /// Dead or Impaired (one-way).
    Declared(Declaration),
}

impl Default for HandlerHealth {
    fn default() -> Self {
        Self::Live(LiveHealth::default())
    }
}

impl HandlerHealth {
    /// The headline state, for log lines only.
    #[must_use]
    pub const fn state_label(&self) -> &'static str {
        match self {
            Self::Declared(Declaration {
                cause: DeclareCause::Silence,
                ..
            }) => "dead",
            Self::Declared(_) => "impaired",
            Self::Live(live) => {
                if live.contract_fault.is_some() {
                    "contract_fault"
                } else if live.lag_since.is_some() {
                    "lagging"
                } else if live.draining_since.is_some() {
                    "draining"
                } else {
                    "healthy"
                }
            }
        }
    }

    /// Whether the pair is lagging (the re-publish condition).
    #[must_use]
    pub const fn is_lagging(&self) -> bool {
        matches!(
            self,
            Self::Live(LiveHealth {
                lag_since: Some(_),
                ..
            })
        )
    }

    /// Whether DRAINING has been seen (exposed for the planned path, MC-F).
    #[must_use]
    pub const fn is_draining(&self) -> bool {
        matches!(
            self,
            Self::Live(LiveHealth {
                draining_since: Some(_),
                ..
            })
        )
    }

    /// The declaration, if any.
    #[must_use]
    pub const fn declaration(&self) -> Option<Declaration> {
        match self {
            Self::Declared(d) => Some(*d),
            Self::Live(_) => None,
        }
    }

    /// What a transition is judged on (instants excluded: a lag onset that
    /// stays the same onset is no transition).
    const fn shape(&self) -> HealthShape {
        match self {
            Self::Declared(d) => HealthShape {
                declared: Some(d.cause),
                lagging: false,
                draining: false,
                contract_fault: None,
            },
            Self::Live(l) => HealthShape {
                declared: None,
                lagging: l.lag_since.is_some(),
                draining: l.draining_since.is_some(),
                contract_fault: l.contract_fault,
            },
        }
    }

    /// Whether `previous` -> `next` is a health TRANSITION (the only thing
    /// the actor acts on and logs): lag entered or cleared, a contract fault
    /// entered or cleared, DRAINING first seen, or a declaration.
    #[must_use]
    pub fn is_transition(previous: &Self, next: &Self) -> bool {
        previous.shape() != next.shape()
    }
}

/// The comparable part of a [`HandlerHealth`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HealthShape {
    declared: Option<DeclareCause>,
    lagging: bool,
    draining: bool,
    contract_fault: Option<ContractFault>,
}

/// `mc_media_handler_suspicions_total{outcome}`: how a LAG suspicion ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspicionOutcome {
    /// Lagging → Impaired (lag held past L, or a capacity refusal while
    /// lagging).
    Declared,
    /// Lagging → caught up (the near-miss).
    Cleared,
}

impl SuspicionOutcome {
    /// Every value, for zero-initialisation.
    pub const ALL: [Self; 2] = [Self::Declared, Self::Cleared];

    /// The metric label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Cleared => "cleared",
        }
    }

    /// The suspicion outcome of one transition, if it ended a lag suspicion.
    ///
    /// - Lagging → Impaired: `Declared`.
    /// - Lagging → not lagging, no contract fault: `Cleared`.
    /// - Lagging → Dead: NONE (a silence declaration, counted on
    ///   `declarations_total{trigger="heartbeat_timeout"}`, not a lag outcome).
    /// - Lagging → contract fault: NONE (the lag clock is suppressed, not
    ///   cleared; the fault is MC's).
    /// - Healthy → Impaired on a refusal (no Lagging): NONE.
    #[must_use]
    pub fn of(previous: &HandlerHealth, next: &HandlerHealth) -> Option<Self> {
        if !previous.is_lagging() {
            return None;
        }
        match next {
            HandlerHealth::Declared(d) if d.trigger() == RemovalTrigger::ApplyLag => {
                Some(Self::Declared)
            }
            HandlerHealth::Declared(_) => None,
            HandlerHealth::Live(l) if l.lag_since.is_none() && l.contract_fault.is_none() => {
                Some(Self::Cleared)
            }
            HandlerHealth::Live(_) => None,
        }
    }
}

/// THE health decision (see the module doc). Pure.
#[must_use]
pub fn assess(
    now: Instant,
    obs: &Observations,
    previous: &HandlerHealth,
    cfg: &HealthConfig,
) -> HandlerHealth {
    let live = match previous {
        HandlerHealth::Declared(d) => return HandlerHealth::Declared(*d),
        HandlerHealth::Live(live) => *live,
    };
    // Weight 0 in this story (ADR-0039 §2: a second opinion only).
    let _ = obs.client_loss_reports;

    // Dead: age measured from the later of the first confirmation and the
    // last accepted heartbeat. Unconfirmed: never Dead.
    if let Some(origin) = silence_origin(obs) {
        if expired(origin, cfg.timeout, now) {
            return declared(DeclareCause::Silence, origin);
        }
    }

    // A refusal is evidence about MC's policy only while MC's policy is
    // PENDING: the accepted generation is behind what MH owes. With
    // `applied >= owed` MC's current policy IS installed, so a refusal can only
    // concern a policy MC never sent (an out-of-band writer); it is ignored.
    // A real refusal of MC's push is acted on at most one interval later, when
    // `owed` catches up with the refused generation.
    let behind = obs.behind();
    let contract_fault = match obs.effective_refusal() {
        RefusalClass::Capacity(c) => {
            return declared(DeclareCause::Capacity(c), obs.last_accepted.unwrap_or(now));
        }
        RefusalClass::Contract(f) => Some(f),
        RefusalClass::None => None,
    };

    let draining_since = live
        .draining_since
        .or_else(|| obs.draining.then(|| obs.last_accepted.unwrap_or(now)));

    let lag_since = if contract_fault.is_some() || !behind {
        None
    } else {
        Some(
            live.lag_since
                .unwrap_or_else(|| obs.last_accepted.unwrap_or(now)),
        )
    };
    if let Some(since) = lag_since {
        if expired(since, cfg.lag_limit, now) {
            return declared(DeclareCause::LagHeld, since);
        }
    }

    HandlerHealth::Live(LiveHealth {
        lag_since,
        contract_fault,
        draining_since,
    })
}

/// When `assess` could next change its answer WITHOUT a new heartbeat: the
/// earliest of the silence expiry and the lag expiry. `None` once declared
/// (or with nothing to wait for).
#[must_use]
pub fn next_deadline(
    health: &HandlerHealth,
    obs: &Observations,
    cfg: &HealthConfig,
) -> Option<Instant> {
    let HandlerHealth::Live(live) = health else {
        return None;
    };
    let silence = silence_origin(obs).map(|origin| expiry(origin, cfg.timeout));
    let lag = live.lag_since.map(|since| expiry(since, cfg.lag_limit));
    match (silence, lag) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// The silence clock's origin: the later of the first confirmation and the
/// last accepted heartbeat; `None` while unconfirmed.
fn silence_origin(obs: &Observations) -> Option<Instant> {
    obs.confirmed_at
        .map(|confirmed| obs.last_accepted.map_or(confirmed, |l| l.max(confirmed)))
}

fn declared(cause: DeclareCause, evidence_at: Instant) -> HandlerHealth {
    HandlerHealth::Declared(Declaration { cause, evidence_at })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> HealthConfig {
        HealthConfig::from_timing(HeartbeatTiming::from_timeout_ms(6_000))
    }

    /// One table row: an observation sequence and what it must yield.
    struct Row {
        name: &'static str,
        /// `(offset from t0, observations, expected state label)` in order;
        /// each step's `previous` is the step before's result.
        steps: Vec<(Duration, Observations, &'static str)>,
        /// The expected `next_deadline` after the LAST step, as an offset
        /// from t0 (`None` = no deadline).
        deadline: Option<Duration>,
    }

    fn base(t0: Instant) -> Observations {
        Observations {
            confirmed_at: Some(t0),
            last_accepted: Some(t0),
            applied: Some(1),
            owed: Some(1),
            refusal: RefusalClass::None,
            draining: false,
            client_loss_reports: 0,
        }
    }

    fn run(row: &Row, t0: Instant) {
        let cfg = cfg();
        let mut health = HandlerHealth::default();
        let mut last_obs = base(t0);
        for (i, (at, obs, want)) in row.steps.iter().enumerate() {
            health = assess(t0 + *at, obs, &health, &cfg);
            assert_eq!(
                health.state_label(),
                *want,
                "row `{}` step {i}: got {:?}",
                row.name,
                health
            );
            last_obs = *obs;
        }
        assert_eq!(
            next_deadline(&health, &last_obs, &cfg),
            row.deadline.map(|d| t0 + d),
            "row `{}`: next_deadline",
            row.name
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one named row per transition reads better as one table than split"
    )]
    fn assess_table() {
        let t0 = Instant::now();
        let c = cfg();
        let (n, l, ms) = (c.timeout, c.lag_limit, DEADLINE_RESOLUTION);
        let at = |d: Duration| Some(t0 + d);
        let b = base(t0);
        let lagging = |accepted: Duration| Observations {
            last_accepted: at(accepted),
            applied: Some(1),
            owed: Some(2),
            ..b
        };
        let rows = vec![
            Row {
                name: "healthy: deadline is the silence expiry",
                steps: vec![(Duration::ZERO, b, "healthy")],
                deadline: Some(n + ms),
            },
            Row {
                name: "Healthy -> Lagging -> Healthy (reversible)",
                steps: vec![
                    (Duration::ZERO, b, "healthy"),
                    (ms * 2000, lagging(ms * 2000), "lagging"),
                    (
                        ms * 4000,
                        Observations {
                            last_accepted: at(ms * 4000),
                            applied: Some(2),
                            owed: Some(2),
                            ..b
                        },
                        "healthy",
                    ),
                ],
                deadline: Some(ms * 4000 + n + ms),
            },
            // "A push racing one heartbeat is not lag" is decided by WHAT is
            // owed, not here: `liveness::owed_is_what_was_sent_by_the_previous_accepted_heartbeat`.
            Row {
                name: "Lagging held exactly L stays Lagging; L + 1ms is Impaired",
                steps: vec![
                    (Duration::ZERO, lagging(Duration::ZERO), "lagging"),
                    // Heartbeats keep arriving (no silence), lag persists.
                    (l - n, lagging(l - n), "lagging"),
                    (l, lagging(l), "lagging"),
                    (l + ms, lagging(l), "impaired"),
                ],
                deadline: None,
            },
            Row {
                name: "lagging: deadline is the earlier of lag and silence expiry",
                steps: vec![
                    (Duration::ZERO, lagging(Duration::ZERO), "lagging"),
                    (l - ms * 1000, lagging(l - ms * 1000), "lagging"),
                ],
                deadline: Some(l + ms),
            },
            Row {
                name: "age exactly N is alive; N + 1ms is Dead",
                steps: vec![(n, b, "healthy"), (n + ms, b, "dead")],
                deadline: None,
            },
            Row {
                name: "Dead is one-way: a later good heartbeat does not revive",
                steps: vec![
                    (n + ms, b, "dead"),
                    (
                        n + ms * 2,
                        Observations {
                            last_accepted: at(n + ms * 2),
                            ..b
                        },
                        "dead",
                    ),
                ],
                deadline: None,
            },
            Row {
                name: "unconfirmed at age >> N is not Dead",
                steps: vec![(
                    n * 10,
                    Observations {
                        confirmed_at: None,
                        ..b
                    },
                    "healthy",
                )],
                deadline: None,
            },
            Row {
                name: "...and the same handler is Dead at N + 1ms after its first confirmation",
                steps: vec![
                    (
                        n * 10,
                        Observations {
                            confirmed_at: at(n * 10),
                            ..b
                        },
                        "healthy",
                    ),
                    (
                        n * 11,
                        Observations {
                            confirmed_at: at(n * 10),
                            ..b
                        },
                        "healthy",
                    ),
                    (
                        n * 11 + ms,
                        Observations {
                            confirmed_at: at(n * 10),
                            ..b
                        },
                        "dead",
                    ),
                ],
                deadline: None,
            },
            Row {
                name: "client loss reports alone never reach Dead, at any count",
                steps: vec![(
                    n,
                    Observations {
                        client_loss_reports: u32::MAX,
                        ..b
                    },
                    "healthy",
                )],
                deadline: Some(n + ms),
            },
            Row {
                name: "DRAINING -> draining, no declaration",
                steps: vec![(
                    ms * 10,
                    Observations {
                        last_accepted: at(ms * 10),
                        draining: true,
                        ..b
                    },
                    "draining",
                )],
                deadline: Some(ms * 10 + n + ms),
            },
            Row {
                name: "STREAM_CEILING while MC's policy is pending -> Impaired",
                steps: vec![(
                    Duration::ZERO,
                    Observations {
                        refusal: RefusalClass::Capacity(CapacityRefusal::StreamCeiling),
                        ..lagging(Duration::ZERO)
                    },
                    "impaired",
                )],
                deadline: None,
            },
            Row {
                name: "EDGE_CAP while MC's policy is pending -> Impaired",
                steps: vec![(
                    Duration::ZERO,
                    Observations {
                        refusal: RefusalClass::Capacity(CapacityRefusal::EdgeCap),
                        ..lagging(Duration::ZERO)
                    },
                    "impaired",
                )],
                deadline: None,
            },
            Row {
                name: "a refusal with MC's policy INSTALLED (applied == owed) concerns a policy \
                       MC never sent: ignored (Layer-7 regression, out-of-band writer)",
                steps: vec![
                    (
                        Duration::ZERO,
                        Observations {
                            refusal: RefusalClass::Capacity(CapacityRefusal::StreamCeiling),
                            ..b
                        },
                        "healthy",
                    ),
                    (
                        Duration::ZERO,
                        Observations {
                            refusal: RefusalClass::Contract(ContractFault::Invalid),
                            ..b
                        },
                        "healthy",
                    ),
                ],
                deadline: Some(n + ms),
            },
            Row {
                name: "INVALID: contract fault, never Impaired even with lag held past L",
                steps: vec![
                    (
                        Duration::ZERO,
                        Observations {
                            refusal: RefusalClass::Contract(ContractFault::Invalid),
                            ..lagging(Duration::ZERO)
                        },
                        "contract_fault",
                    ),
                    (
                        l * 2,
                        Observations {
                            refusal: RefusalClass::Contract(ContractFault::Invalid),
                            ..lagging(l * 2)
                        },
                        "contract_fault",
                    ),
                ],
                // Only the silence clock runs while a fault suppresses lag.
                deadline: Some(l * 2 + n + ms),
            },
            Row {
                name:
                    "TRANSPORT_INCONSISTENT: contract fault; the lag clock restarts when it clears",
                steps: vec![
                    (
                        Duration::ZERO,
                        Observations {
                            refusal: RefusalClass::Contract(ContractFault::TransportInconsistent),
                            ..lagging(Duration::ZERO)
                        },
                        "contract_fault",
                    ),
                    (l * 2, lagging(l * 2), "lagging"),
                ],
                deadline: Some(l * 2 + n + ms),
            },
        ];
        for row in &rows {
            run(row, t0);
        }
    }

    /// The negative rows' positive twins: an `assess` that always answered
    /// Healthy fails here.
    #[test]
    fn a_constant_healthy_assess_would_fail_the_table() {
        let t0 = Instant::now();
        let c = cfg();
        let dead = assess(
            t0 + c.timeout + DEADLINE_RESOLUTION,
            &base(t0),
            &HandlerHealth::default(),
            &c,
        );
        assert_eq!(
            dead.declaration().map(Declaration::trigger),
            Some(RemovalTrigger::HeartbeatTimeout)
        );
        let lag = Observations {
            owed: Some(2),
            ..base(t0)
        };
        let lagging = assess(t0, &lag, &HandlerHealth::default(), &c);
        assert!(lagging.is_lagging());
        let impaired = assess(
            t0 + c.lag_limit + DEADLINE_RESOLUTION,
            &Observations {
                last_accepted: Some(t0 + c.lag_limit),
                ..lag
            },
            &lagging,
            &c,
        );
        assert_eq!(
            impaired.declaration().map(Declaration::trigger),
            Some(RemovalTrigger::ApplyLag)
        );
    }

    #[test]
    fn detection_starts_at_the_evidence_of_each_cause() {
        let t0 = Instant::now();
        let c = cfg();
        let b = base(t0);
        // Silence: from the last accepted heartbeat.
        let silent = Observations {
            last_accepted: Some(t0 + Duration::from_secs(1)),
            ..b
        };
        let at = t0 + Duration::from_secs(1) + c.timeout + DEADLINE_RESOLUTION;
        let d = assess(at, &silent, &HandlerHealth::default(), &c)
            .declaration()
            .unwrap();
        assert_eq!(d.detection(at), c.timeout + DEADLINE_RESOLUTION);
        // Lag held: from the lag onset.
        let lag = Observations { owed: Some(2), ..b };
        let lagging = assess(t0, &lag, &HandlerHealth::default(), &c);
        let at = t0 + c.lag_limit + DEADLINE_RESOLUTION;
        let d = assess(
            at,
            &Observations {
                last_accepted: Some(t0 + c.lag_limit),
                ..lag
            },
            &lagging,
            &c,
        )
        .declaration()
        .unwrap();
        assert_eq!(d.cause, DeclareCause::LagHeld);
        assert_eq!(d.detection(at), c.lag_limit + DEADLINE_RESOLUTION);
        // Capacity: from the refusing heartbeat.
        let refused = Observations {
            last_accepted: Some(t0 + Duration::from_secs(2)),
            refusal: RefusalClass::Capacity(CapacityRefusal::EdgeCap),
            owed: Some(2), // MC's policy pending
            ..b
        };
        let at = t0 + Duration::from_secs(2);
        let d = assess(at, &refused, &HandlerHealth::default(), &c)
            .declaration()
            .unwrap();
        assert_eq!(d.detection(at), Duration::ZERO);
    }

    #[test]
    fn suspicion_outcomes_are_lag_endings_only() {
        let t0 = Instant::now();
        let lagging = HandlerHealth::Live(LiveHealth {
            lag_since: Some(t0),
            ..LiveHealth::default()
        });
        let healthy = HandlerHealth::default();
        let impaired = |cause| {
            HandlerHealth::Declared(Declaration {
                cause,
                evidence_at: t0,
            })
        };
        let faulted = HandlerHealth::Live(LiveHealth {
            contract_fault: Some(ContractFault::Invalid),
            ..LiveHealth::default()
        });
        assert_eq!(
            SuspicionOutcome::of(&lagging, &impaired(DeclareCause::LagHeld)),
            Some(SuspicionOutcome::Declared)
        );
        assert_eq!(
            SuspicionOutcome::of(
                &lagging,
                &impaired(DeclareCause::Capacity(CapacityRefusal::EdgeCap))
            ),
            Some(SuspicionOutcome::Declared)
        );
        assert_eq!(
            SuspicionOutcome::of(&lagging, &healthy),
            Some(SuspicionOutcome::Cleared)
        );
        assert_eq!(
            SuspicionOutcome::of(&lagging, &impaired(DeclareCause::Silence)),
            None,
            "Lagging -> Dead is not a lag outcome"
        );
        assert_eq!(SuspicionOutcome::of(&lagging, &faulted), None);
        assert_eq!(
            SuspicionOutcome::of(&healthy, &impaired(DeclareCause::LagHeld)),
            None,
            "refusal-direct Healthy -> Impaired is not a suspicion"
        );
        assert_eq!(SuspicionOutcome::of(&lagging, &lagging), None);
    }

    #[test]
    fn the_refusal_projection_is_total() {
        for (wire, want) in [
            (ApplyRefusal::Unspecified, RefusalClass::None),
            (
                ApplyRefusal::Invalid,
                RefusalClass::Contract(ContractFault::Invalid),
            ),
            (
                ApplyRefusal::TransportInconsistent,
                RefusalClass::Contract(ContractFault::TransportInconsistent),
            ),
            (
                ApplyRefusal::StreamCeiling,
                RefusalClass::Capacity(CapacityRefusal::StreamCeiling),
            ),
            (
                ApplyRefusal::EdgeCap,
                RefusalClass::Capacity(CapacityRefusal::EdgeCap),
            ),
        ] {
            assert_eq!(RefusalClass::from_wire(wire), want);
        }
    }

    /// The ONE refusal gate: a refusal reported with MC's policy installed is
    /// no refusal (for assess AND the contract-fault anomaly); pending, it is.
    #[test]
    fn a_refusal_counts_only_while_mcs_policy_is_pending() {
        let b = base(Instant::now());
        let invalid = RefusalClass::Contract(ContractFault::Invalid);
        let caught_up = Observations {
            refusal: invalid,
            ..b
        };
        assert_eq!(caught_up.effective_refusal(), RefusalClass::None);
        let pending = Observations {
            refusal: invalid,
            owed: Some(2),
            ..b
        };
        assert_eq!(pending.effective_refusal(), invalid);
        let ahead_clamped = Observations {
            refusal: invalid,
            applied: Some(2),
            owed: Some(2),
            ..b
        };
        assert_eq!(ahead_clamped.effective_refusal(), RefusalClass::None);
    }

    #[test]
    fn the_boundary_helpers_agree() {
        let t0 = Instant::now();
        let span = Duration::from_secs(6);
        assert!(!expired(t0, span, t0 + span));
        assert!(expired(t0, span, expiry(t0, span)));
        assert_eq!(expiry(t0, span), t0 + span + DEADLINE_RESOLUTION);
    }
}
