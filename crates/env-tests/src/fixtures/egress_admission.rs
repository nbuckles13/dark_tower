//! Kind egress sizing — ONE home for every constant of the two-sided window
//! the Kind MH egress budget is sized into
//! (`infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`),
//! shared by the deployment-config test that asserts the window
//! (`tests/01_mh_deployment_config.rs`) and the S9 exhaustion test that
//! defines its upper side (`tests/28_mh_egress_admission.rs`). Those are
//! separate test binaries, so without this module each side would rest on a
//! COPY of the other's numbers.
//!
//! - LOWER side ([`kind_window_lower_bound`]): the largest load that must fit
//!   on ONE handler at once — the demo meeting or the largest per-file
//!   concurrent edge sum, whichever is larger — plus the edges a previous
//!   file's disconnected participants still hold. Counted per handler; that
//!   is conservative, since one file's meetings may spread over both MHs.
//! - UPPER side ([`max_s9_feasible_ceiling`]): two different kinds of bound,
//!   kept textually apart. [`MAX_S9_PARTICIPANTS`] is an ECONOMIC feasibility
//!   limit — how many real AC registrations and GC/MC/MH joins one test may
//!   spend. [`size_s9_meeting`]'s `p * s > 2 * ceiling` is the CORRECTNESS
//!   bound — the pigeonhole condition that makes exhaustion certain for ANY
//!   edge chooser MC uses across two handlers.

/// The story-3 demo meeting the Kind ceiling must carry on one handler: P
/// participants each filling N audio plus M video receive slots. The
/// REQUIREMENT, stated once here.
///
/// ANCHOR (DRY): the same three numbers are MH's `DEMO_PARTICIPANTS`,
/// `DEMO_AUDIO_SLOTS`, `DEMO_VIDEO_SLOTS` behind
/// `EGRESS_STREAM_CEILING_RECOMMENDED_MIN` in `crates/mh-service/src/config.rs`
/// (env-tests links no service crate);
/// `01::test_running_mh_publishes_the_deployed_stream_ceiling` checks the two
/// agree through the published recommended minimum. NOT dev-web.sh's demo
/// defaults, which are a smaller launcher topology.
pub const DEMO_P: u64 = 6;
/// See [`DEMO_P`].
pub const DEMO_N: u64 = 5;
/// See [`DEMO_P`].
pub const DEMO_M: u64 = 4;

/// Egress streams for P participants each filling N audio + M video slots:
/// `P x (N + M)`. ANCHOR (DRY): MH's `demo_streams` (see [`DEMO_P`]).
#[must_use]
pub const fn required_edges(p: u64, n: u64, m: u64) -> u64 {
    p * (n + m)
}

/// The PLANNED largest concurrent edge sum of any one env-test file: story 3's
/// planning budget for `27_mc_slot_placement` (the largest video file). A CAP
/// every file stays under, not a measurement: each file's ACTUAL sum is the
/// `Edge-sum` line in its own header (the one home of that number), and the
/// hermetic check below fails when any header exceeds this. A change that
/// pushes a file past it must, in the same change, serialise that file's fns or
/// raise this cap — which moves the window's lower side, so re-check the Kind
/// budget.
pub const MAX_PER_FILE_EDGE_SUM: u64 = 72;

/// Edges still installed from the PREVIOUS file when the next one starts:
/// disconnected participants hold their edges through MC's reconnect grace
/// window (`MC_DISCONNECT_GRACE_PERIOD_SECONDS`) while Layer-7 files run back
/// to back. Observed, not derived: mh-0 at 39 installed streams with no live
/// load during story 2 task 20's Gate 2 (the history is in the Kind budget
/// patch header). Shrinking it, shortening the grace period, or serialising the
/// env-tests to make a window fit was considered and rejected — an empty
/// window is an escalation.
pub const RETAINED_EDGE_FLOOR: u64 = 39;

/// The LOWER side of the Kind window: `max(demo, per-file cap) + retained
/// floor`, per handler.
#[must_use]
pub const fn kind_window_lower_bound() -> u64 {
    let demo = required_edges(DEMO_P, DEMO_N, DEMO_M);
    let largest = if demo > MAX_PER_FILE_EDGE_SUM {
        demo
    } else {
        MAX_PER_FILE_EDGE_SUM
    };
    largest + RETAINED_EDGE_FLOOR
}

/// The largest participant count S9 will spend before calling the deployment
/// unsuitable (a ceiling S9 cannot exceed economically). An economic limit,
/// NOT a correctness bound: raising it buys a larger feasible Kind ceiling at
/// the cost of that many more registrations and joins in one test.
pub const MAX_S9_PARTICIPANTS: u64 = 40;

/// Smallest `p` (with its per-participant slot count `s = min(p - 1,
/// slot_cap)`) satisfying the pigeonhole condition `p * s > 2 * ceiling`
/// within [`MAX_S9_PARTICIPANTS`]; `None` when the deployed ceiling is too
/// large for S9 to exceed at any affordable size.
#[must_use]
pub fn size_s9_meeting(ceiling: u64, slot_cap: u64) -> Option<(u64, u64)> {
    (2..=MAX_S9_PARTICIPANTS)
        .map(|p| (p, (p - 1).min(slot_cap)))
        .find(|(p, s)| p * s > 2 * ceiling)
}

/// The largest stream ceiling S9 can still exceed, for `slot_cap`: the
/// deployment-config test's UPPER bound on the Kind budget.
#[must_use]
pub fn max_s9_feasible_ceiling(slot_cap: u64) -> u64 {
    let most_streams = MAX_S9_PARTICIPANTS * (MAX_S9_PARTICIPANTS - 1).min(slot_cap);
    // p*s > 2*ceiling  <=>  ceiling < p*s / 2  <=>  ceiling <= (p*s - 1) / 2.
    most_streams.saturating_sub(1) / 2
}

/// The header-line prefix every env-test file carries (README "Per-file
/// edge-sum header rule").
pub const EDGE_SUM_PREFIX: &str = "//! Edge-sum (concurrent Σ P×(N+M)): ";

/// The ONE value a file may carry in place of a number: the S9 exhaustion
/// test, which sizes its meeting at run time from the live ceiling and
/// deliberately exceeds it (that is what it tests), so a recorded number would
/// be a restated derived figure. Exactly one file may carry it.
pub const EDGE_SUM_EXEMPT: &str =
    "exempt — sized from the live ceiling by fixtures::egress_admission::size_s9_meeting";

/// One file's parsed `Edge-sum` header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeSum {
    /// A recorded concurrent edge sum.
    Sum(u64),
    /// [`EDGE_SUM_EXEMPT`].
    Exempt,
}

/// Parse one file's `Edge-sum` header STRICTLY: exactly one line starting
/// `//! Edge-sum`, of the exact form `EDGE_SUM_PREFIX<n>` or
/// `EDGE_SUM_PREFIX` + [`EDGE_SUM_EXEMPT`].
///
/// # Errors
/// No line, more than one, or one that does not parse.
pub fn parse_edge_sum(text: &str, file: &str) -> Result<EdgeSum, String> {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("//! Edge-sum"))
        .collect();
    match lines.as_slice() {
        [] => Err(format!(
            "{file} has no `{EDGE_SUM_PREFIX}<n>` header line. EVERY env-test file records the \
             concurrent Σ P×(N+M) of its test fns (0 when it opens no media edges) — README \
             \"Per-file edge-sum header rule\"."
        )),
        [line] => {
            let rest = line.strip_prefix(EDGE_SUM_PREFIX).ok_or_else(|| {
                format!("{file}: `{line}` is not of the form `{EDGE_SUM_PREFIX}<n>`")
            })?;
            if rest == EDGE_SUM_EXEMPT {
                return Ok(EdgeSum::Exempt);
            }
            rest.parse().map(EdgeSum::Sum).map_err(|_| {
                format!(
                    "{file}: `{line}` — the value must be an unsigned integer or exactly \
                     `{EDGE_SUM_EXEMPT}`"
                )
            })
        }
        many => Err(format!(
            "{file} carries {} `Edge-sum` lines; exactly one is allowed",
            many.len()
        )),
    }
}

/// The per-file rule over `(file, parsed)` pairs: every sum within
/// [`MAX_PER_FILE_EDGE_SUM`], exactly one exemption, at least one file.
///
/// # Errors
/// Each violation, all reported together.
pub fn check_edge_sums(files: &[(String, EdgeSum)]) -> Result<(), String> {
    if files.is_empty() {
        return Err("no env-test file was read: the edge-sum check would pass vacuously".into());
    }
    let mut problems: Vec<String> = files
        .iter()
        .filter_map(|(file, sum)| match sum {
            EdgeSum::Sum(n) if *n > MAX_PER_FILE_EDGE_SUM => Some(format!(
                "{file} records an edge sum of {n}, above MAX_PER_FILE_EDGE_SUM \
                 ({MAX_PER_FILE_EDGE_SUM}): serialise its fns, or raise the cap AND re-check the \
                 Kind egress window it feeds"
            )),
            _ => None,
        })
        .collect();
    let exempt: Vec<&str> = files
        .iter()
        .filter(|(_, sum)| *sum == EdgeSum::Exempt)
        .map(|(file, _)| file.as_str())
        .collect();
    if exempt.len() != 1 {
        problems.push(format!(
            "exactly one file (the S9 exhaustion test) may carry the exempt edge sum; found \
             {exempt:?}"
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s9_sizing_arithmetic() {
        // Pure arithmetic, no deployment claim: the smallest p with
        // p * min(p - 1, cap) > 2 * ceiling.
        assert_eq!(size_s9_meeting(100, 8), Some((26, 8)));
        assert_eq!(size_s9_meeting(135, 8), Some((34, 8)));
        assert_eq!(size_s9_meeting(2, 8), Some((3, 2)));
    }

    #[test]
    fn the_feasible_ceiling_is_exactly_the_last_one_size_can_satisfy() {
        let edge = max_s9_feasible_ceiling(8);
        assert_eq!(edge, 159);
        assert!(size_s9_meeting(edge, 8).is_some());
        assert!(size_s9_meeting(edge + 1, 8).is_none());
    }

    #[test]
    fn the_lower_bound_is_the_larger_load_plus_the_retained_floor() {
        assert_eq!(required_edges(DEMO_P, DEMO_N, DEMO_M), 54);
        assert_eq!(kind_window_lower_bound(), 72 + 39);
    }

    #[test]
    fn parse_edge_sum_is_strict() {
        let line = |v: &str| format!("//! x\n{EDGE_SUM_PREFIX}{v}\n");
        assert_eq!(parse_edge_sum(&line("12"), "f"), Ok(EdgeSum::Sum(12)));
        assert_eq!(
            parse_edge_sum(&line(EDGE_SUM_EXEMPT), "f"),
            Ok(EdgeSum::Exempt)
        );
        assert!(parse_edge_sum("//! x\n", "f").unwrap_err().contains("no `"));
        assert!(parse_edge_sum(&line("about 9"), "f")
            .unwrap_err()
            .contains("unsigned integer"));
        assert!(parse_edge_sum("//! Edge-sum: 3\n", "f")
            .unwrap_err()
            .contains("not of the form"));
        let twice = format!("{}{}", line("1"), line("2"));
        assert!(parse_edge_sum(&twice, "f")
            .unwrap_err()
            .contains("2 `Edge-sum` lines"));
    }

    #[test]
    fn check_edge_sums_rules() {
        let f = |n: &str, s| (n.to_string(), s);
        assert!(check_edge_sums(&[f("a", EdgeSum::Sum(72)), f("s9", EdgeSum::Exempt)]).is_ok());
        assert!(
            check_edge_sums(&[f("a", EdgeSum::Sum(73)), f("s9", EdgeSum::Exempt)])
                .unwrap_err()
                .contains("above MAX_PER_FILE_EDGE_SUM")
        );
        assert!(check_edge_sums(&[f("a", EdgeSum::Sum(1))])
            .unwrap_err()
            .contains("exactly one file"));
        assert!(
            check_edge_sums(&[f("a", EdgeSum::Exempt), f("b", EdgeSum::Exempt)])
                .unwrap_err()
                .contains("exactly one file")
        );
        assert!(check_edge_sums(&[]).unwrap_err().contains("vacuously"));
    }

    /// The README per-file edge-sum header rule, over the REAL tree: every
    /// `tests/*.rs` carries exactly one valid `Edge-sum` line (fail-closed —
    /// no guess at which files open media), each within
    /// `MAX_PER_FILE_EDGE_SUM`, and exactly one carries the S9 exemption.
    #[test]
    fn every_env_test_file_records_an_edge_sum_within_the_cap() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
        let mut files = Vec::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).expect("readable test file");
            match parse_edge_sum(&text, &name) {
                Ok(sum) => files.push((name, sum)),
                Err(e) => errors.push(e),
            }
        }
        if let Err(e) = check_edge_sums(&files) {
            errors.push(e);
        }
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }
}
