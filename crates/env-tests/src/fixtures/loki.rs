//! Loki `query_range` — THE env-test Loki client (hoisted on its second
//! consumer: `tests/30_observability.rs`, `tests/47_mc_join_window.rs`), and
//! THE meeting-scoped log evidence built on it ([`MeetingScope`], hoisted from
//! `tests/47_mc_join_window.rs` when `fixtures/meeting_end.rs` became its
//! second consumer). One home for every MC/GC log line an env-test matches
//! ([`LogLine`] consts below, each citing its emitter and pinned to it by a
//! hermetic test, [`EMITTERS`]).
//!
//! Every failure is an `Err`, never an empty result: a transport error, a
//! non-2xx (status and a truncated body), an unparseable body, or a 200 whose
//! shape lacks `data.result`. An empty `Vec` therefore means exactly "Loki
//! answered and no line matched" — so a caller polling for a line can never
//! report a query or infrastructure failure as "line never seen".

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::cluster::ClusterConnection;

/// One matched log entry: its Loki timestamp (ns since the epoch) and the raw
/// line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LokiEntry {
    pub ts_ns: u128,
    pub line: String,
}

/// Why a Loki query did not produce an answer. Its `Display` is the evidence
/// a failing test prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LokiError {
    /// The request never got a response.
    Transport(String),
    /// Loki answered non-2xx (e.g. a LogQL 400).
    Status { status: u16, body: String },
    /// A 2xx whose body is not JSON.
    Unparseable(String),
    /// A 2xx JSON body without the `status: success` / `data.result` shape.
    UnexpectedShape(String),
}

impl std::fmt::Display for LokiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::Status { status, body } => write!(f, "status {status}, body {body}"),
            Self::Unparseable(e) => write!(f, "unparseable body: {e}"),
            Self::UnexpectedShape(b) => write!(f, "unexpected response shape: {b}"),
        }
    }
}

/// At most this much of a body goes into an error.
const BODY_EXCERPT: usize = 300;

fn excerpt(body: &str) -> String {
    body.chars().take(BODY_EXCERPT).collect()
}

/// PURE: classify one `query_range` response. Separated from the HTTP call so
/// the error paths are unit-tested hermetically.
///
/// # Errors
/// See [`LokiError`].
pub fn classify_query_range(status: u16, body: &str) -> Result<Vec<LokiEntry>, LokiError> {
    if !(200..300).contains(&status) {
        return Err(LokiError::Status {
            status,
            body: excerpt(body),
        });
    }
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| LokiError::Unparseable(e.to_string()))?;
    if json.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return Err(LokiError::UnexpectedShape(excerpt(body)));
    }
    let streams = json
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| LokiError::UnexpectedShape(excerpt(body)))?;
    let mut out = Vec::new();
    for stream in streams {
        let values = stream
            .get("values")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| LokiError::UnexpectedShape(excerpt(body)))?;
        for v in values {
            let ts_ns = v
                .get(0)
                .and_then(serde_json::Value::as_str)
                .and_then(|t| t.parse::<u128>().ok())
                .ok_or_else(|| LokiError::UnexpectedShape(excerpt(body)))?;
            let line = v
                .get(1)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| LokiError::UnexpectedShape(excerpt(body)))?
                .to_string();
            out.push(LokiEntry { ts_ns, line });
        }
    }
    out.sort_by_key(|e| e.ts_ns);
    Ok(out)
}

/// `GET {loki}/loki/api/v1/query_range` for `logql` over `[start_ns, end_ns]`,
/// oldest first.
///
/// # Errors
/// See [`LokiError`]; never an empty `Ok` for a failure.
pub async fn query_range(
    cluster: &ClusterConnection,
    loki_base_url: &str,
    logql: &str,
    start_ns: u128,
    end_ns: u128,
    limit: u32,
) -> Result<Vec<LokiEntry>, LokiError> {
    let response = cluster
        .http_client()
        .get(format!("{loki_base_url}/loki/api/v1/query_range"))
        .query(&[
            ("query", logql),
            ("start", &start_ns.to_string()),
            ("end", &end_ns.to_string()),
            ("limit", &limit.to_string()),
            ("direction", "forward"),
        ])
        .send()
        .await
        .map_err(|e| LokiError::Transport(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| LokiError::Transport(e.to_string()))?;
    classify_query_range(status, &body)
}

// ---------------------------------------------------------------------------
// Meeting-scoped log evidence (hoisted from tests/47_mc_join_window.rs).
// ---------------------------------------------------------------------------

/// One log line an env-test matches: Loki `app` label, JSON `target`, and the
/// exact `fields.message`. A reworded or retargeted line fails the matching
/// test with the caller's token and a pointer to these consts, never as a
/// vacuous pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogLine {
    pub app: &'static str,
    pub target: &'static str,
    pub message: &'static str,
}

/// `crates/mc-service/src/actors/controller.rs` (`create_meeting`): one per
/// meeting-actor incarnation.
pub const MC_CREATED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.controller",
    message: "Meeting actor created",
};

/// `crates/mc-service/src/actors/controller.rs` (`reap_ended`): one per
/// meeting end, carrying `cause`, `reason` and `age_ms`. Logged at the REAP;
/// the MH releases are still in flight ([`MC_MEETING_RELEASED`] is their end).
pub const MC_MEETING_ENDED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.controller",
    message: "Meeting ended; releasing it on its media handlers",
};

/// `crates/mc-service/src/media_routing/teardown.rs` (`run`): the meeting's
/// MH-side teardown completed, carrying `handlers`, `released`, `failed` and
/// `quiesce_timed_out`. Emitted only after the meeting's outstanding
/// handler-removal releases finished too.
pub const MC_MEETING_RELEASED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.teardown",
    message: "Meeting released on its media handlers",
};

/// `crates/mc-service/src/actors/meeting.rs` (the `ClientClosed` disconnect
/// arm): a clean transport close removed `participant_id` at once (no grace).
pub const MC_CLEAN_CLOSE: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.meeting",
    message: "Clean transport close — removing participant immediately (grace skipped)",
};

/// `crates/mc-service/src/actors/meeting.rs` (`check_disconnect_timeouts`):
/// `participant_id` was removed only when its disconnect grace expired.
pub const MC_GRACE_EXPIRED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.meeting",
    message: "Disconnect grace period expired, removing participant",
};

/// `crates/mc-service/src/actors/meeting.rs` (`on_join_window_deadline`): the
/// actor's own join-window end, carrying `window_seconds`, `age_ms` and
/// `lifetime_tick_ms`.
pub const MC_WINDOW_FIRED: LogLine = LogLine {
    app: "mc-service",
    target: "mc.actor.meeting",
    message: "No participant joined within the join window; ending the meeting",
};

/// `crates/gc-service/src/services/mc_assignment.rs` (`end_assignment`):
/// emitted only when GC actually ended a live assignment row for the meeting.
pub const GC_ENDED: LogLine = LogLine {
    app: "gc-service",
    target: "gc.service.assignment",
    message: "Ended meeting assignment(s)",
};

/// Every [`LogLine`] above with its emitter file (repo-relative), for the
/// hermetic pin test: a reword or retarget at the emitter fails Layer 4, not
/// only Layer 7 (and never silently, for an absence check).
pub const EMITTERS: &[(LogLine, &str)] = &[
    (MC_CREATED, "crates/mc-service/src/actors/controller.rs"),
    (
        MC_MEETING_ENDED,
        "crates/mc-service/src/actors/controller.rs",
    ),
    (
        MC_MEETING_RELEASED,
        "crates/mc-service/src/media_routing/teardown.rs",
    ),
    (MC_CLEAN_CLOSE, "crates/mc-service/src/actors/meeting.rs"),
    (MC_GRACE_EXPIRED, "crates/mc-service/src/actors/meeting.rs"),
    (MC_WINDOW_FIRED, "crates/mc-service/src/actors/meeting.rs"),
    (GC_ENDED, "crates/gc-service/src/services/mc_assignment.rs"),
];

/// Backward slack on a scope's start: host and MC-pod clocks can disagree by
/// seconds (WSL2). It only WIDENS the scanned range; ordering is always
/// decided on MC's own Loki timestamps, never on the host clock.
pub const SCOPE_SKEW_SLACK: Duration = Duration::from_secs(5);

/// The host wall clock as Loki nanoseconds (scope bounds only).
#[must_use]
pub fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos()
}

/// `now_ns() - SCOPE_SKEW_SLACK`: a scope start taken "now".
#[must_use]
pub fn since_now_ns() -> u128 {
    now_ns().saturating_sub(SCOPE_SKEW_SLACK.as_nanos())
}

/// One matched entry: its Loki timestamp (ns) and the parsed JSON line.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub ts_ns: u128,
    pub json: serde_json::Value,
}

impl Entry {
    fn fields(&self) -> Option<&serde_json::Value> {
        self.json.get("fields")
    }

    /// A string field (`fields.<name>`).
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields()
            .and_then(|f| f.get(name))
            .and_then(serde_json::Value::as_str)
    }

    /// An unsigned integer field.
    #[must_use]
    pub fn field_u64(&self, name: &str) -> Option<u64> {
        self.fields()
            .and_then(|f| f.get(name))
            .and_then(serde_json::Value::as_u64)
    }

    /// A boolean field.
    #[must_use]
    pub fn field_bool(&self, name: &str) -> Option<bool> {
        self.fields()
            .and_then(|f| f.get(name))
            .and_then(serde_json::Value::as_bool)
    }
}

/// PURE: the longest prefix of `message` that is plain printable ASCII and
/// free of LogQL string metacharacters, trimmed — the LogQL line filter. It
/// only NARROWS the scan; [`matching_entries`] matches the full message
/// structurally, so a non-ASCII character (e.g. an em dash, however the log
/// encoder escapes it) can never make the filter miss the line.
#[must_use]
pub fn logql_filter(message: &str) -> &str {
    let end = message
        .char_indices()
        .find(|(_, c)| !c.is_ascii() || c.is_ascii_control() || *c == '"' || *c == '\\')
        .map_or(message.len(), |(i, _)| i);
    message[..end].trim_end()
}

/// PURE: the LogQL for `line` scoped to `meeting_id`. The id is a [`Uuid`],
/// whose Display cannot carry a quote or a pipe, so the query cannot be
/// broken by construction.
#[must_use]
pub fn meeting_logql(line: LogLine, meeting_id: Uuid) -> String {
    format!(
        "{{app=\"{}\"}} |= \"{meeting_id}\" |= \"{}\"",
        line.app,
        logql_filter(line.message)
    )
}

/// PURE: the entries of `raw` that ARE `line` for `meeting_id`, structurally
/// (JSON `target`, `fields.message`, `fields.meeting_id` — the hyphenated
/// lowercase Uuid form MC logs). A non-JSON line is not a match.
#[must_use]
pub fn matching_entries(line: LogLine, meeting_id: Uuid, raw: Vec<LokiEntry>) -> Vec<Entry> {
    let id = meeting_id.to_string();
    raw.into_iter()
        .filter_map(|LokiEntry { ts_ns, line }| {
            serde_json::from_str(&line)
                .ok()
                .map(|json| Entry { ts_ns, json })
        })
        .filter(|e| {
            e.json.get("target").and_then(serde_json::Value::as_str) == Some(line.target)
                && e.field("message") == Some(line.message)
                && e.field("meeting_id") == Some(id.as_str())
        })
        .collect()
}

/// Reason token: Loki is not up, so log evidence cannot be read.
pub const PRECONDITION_LOKI_UNAVAILABLE: &str = "PRECONDITION-loki-unavailable";

/// PRECONDITION: Loki is up; returns its base URL. Call at the START of a test
/// whose evidence is Loki lines, so a missing Loki fails before the scenario
/// runs. A FAILURE, never a skip.
///
/// # Panics
/// [`PRECONDITION_LOKI_UNAVAILABLE`].
pub async fn require_loki(cluster: &ClusterConnection) -> String {
    assert!(
        cluster.is_loki_available().await,
        "{PRECONDITION_LOKI_UNAVAILABLE}: this test's evidence is log lines in Loki; Loki must \
         be up (an unavailable Loki FAILS the test, never skips it)"
    );
    cluster.loki_base_url.clone().expect("Loki URL is set")
}

/// Reason token: a Loki query FAILED (transport, non-2xx, unparseable, bad
/// shape). Never read as "no line yet".
pub const LOKI_QUERY_FAILED: &str = "loki-query-failed";

/// PURE: a query outcome as matched entries, or the [`LOKI_QUERY_FAILED`]
/// message. Separated so "a Loki error is its own token" is tested
/// hermetically.
///
/// # Errors
/// [`LOKI_QUERY_FAILED`] naming the query, the meeting and the error.
pub fn classify_scoped(
    line: LogLine,
    meeting_id: Uuid,
    query: &str,
    outcome: Result<Vec<LokiEntry>, LokiError>,
) -> Result<Vec<Entry>, String> {
    outcome
        .map(|raw| matching_entries(line, meeting_id, raw))
        .map_err(|e| format!("{LOKI_QUERY_FAILED}: `{query}` for meeting {meeting_id}: {e}"))
}

/// What a meeting-scoped Loki read is scoped to: ONE meeting id, from
/// `since_ns` (a scan bound only) to now.
pub struct MeetingScope<'a> {
    pub cluster: &'a ClusterConnection,
    pub loki: String,
    pub meeting_id: Uuid,
    pub since_ns: u128,
}

impl MeetingScope<'_> {
    /// Every entry of `line` for this meeting, oldest first.
    ///
    /// # Panics
    /// [`LOKI_QUERY_FAILED`] at once on a query failure.
    pub async fn entries(&self, line: LogLine) -> Vec<Entry> {
        let query = meeting_logql(line, self.meeting_id);
        let outcome = query_range(
            self.cluster,
            &self.loki,
            &query,
            self.since_ns,
            now_ns(),
            1000,
        )
        .await;
        classify_scoped(line, self.meeting_id, &query, outcome).unwrap_or_else(|e| panic!("{e}"))
    }

    /// THE poll kernel: read `line` every `poll` until `verdict` decides
    /// (`Some`), or return `Err(last entries read)` once `deadline` has passed.
    ///
    /// # Panics
    /// [`LOKI_QUERY_FAILED`] on a query failure (never read as "not yet").
    pub async fn poll_until<T>(
        &self,
        line: LogLine,
        deadline: Instant,
        poll: Duration,
        verdict: impl Fn(&[Entry]) -> Option<T>,
    ) -> Result<T, Vec<Entry>> {
        loop {
            let entries = self.entries(line).await;
            if let Some(v) = verdict(&entries) {
                return Ok(v);
            }
            if Instant::now() >= deadline {
                return Err(entries);
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// [`Self::poll_until`] for one entry `pick` finds, failing with `token`
    /// after `ceiling` (a FAILURE bound only, never the correctness bound).
    ///
    /// # Panics
    /// `token` at the ceiling; [`LOKI_QUERY_FAILED`] on a query failure.
    pub async fn await_entry(
        &self,
        line: LogLine,
        ceiling: Duration,
        poll: Duration,
        token: &str,
        pick: impl Fn(&[Entry]) -> Option<usize>,
    ) -> Entry {
        self.poll_until(line, Instant::now() + ceiling, poll, |es| {
            pick(es).map(|i| es[i].clone())
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{token}: no `{}` line (target `{}`, app `{}`) for meeting {} within {ceiling:?} \
                 (LogQL `{}`). If the line was reworded or retargeted, update its LogLine const \
                 in crates/env-tests/src/fixtures/loki.rs; otherwise see the token.",
                line.message,
                line.target,
                line.app,
                self.meeting_id,
                meeting_logql(line, self.meeting_id)
            )
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_success_with_streams_is_entries_oldest_first() {
        let body = r#"{"status":"success","data":{"resultType":"streams","result":[
            {"stream":{"app":"mc-service"},"values":[["20","b"],["10","a"]]}]}}"#;
        assert_eq!(
            classify_query_range(200, body).unwrap(),
            vec![
                LokiEntry {
                    ts_ns: 10,
                    line: "a".to_string()
                },
                LokiEntry {
                    ts_ns: 20,
                    line: "b".to_string()
                },
            ]
        );
    }

    #[test]
    fn a_success_with_no_streams_is_an_empty_ok() {
        let body = r#"{"status":"success","data":{"resultType":"streams","result":[]}}"#;
        assert_eq!(classify_query_range(200, body).unwrap(), Vec::new());
    }

    #[test]
    fn a_non_2xx_is_a_status_error_with_the_body() {
        let err = classify_query_range(400, "parse error at line 1").unwrap_err();
        assert_eq!(
            err,
            LokiError::Status {
                status: 400,
                body: "parse error at line 1".to_string()
            }
        );
    }

    #[test]
    fn a_200_without_data_result_is_an_error_never_no_lines() {
        assert!(matches!(
            classify_query_range(200, r#"{"status":"success","data":{}}"#),
            Err(LokiError::UnexpectedShape(_))
        ));
        assert!(matches!(
            classify_query_range(200, r#"{"status":"error"}"#),
            Err(LokiError::UnexpectedShape(_))
        ));
        assert!(matches!(
            classify_query_range(200, "<html>"),
            Err(LokiError::Unparseable(_))
        ));
    }

    const ID: &str = "4b1e9f2a-0c7d-4e8b-9a35-6f0d2c1b7e44";

    fn id() -> Uuid {
        Uuid::parse_str(ID).unwrap()
    }

    fn raw(ts: u128, target: &str, message: &str, meeting: &str) -> LokiEntry {
        LokiEntry {
            ts_ns: ts,
            line: serde_json::json!({
                "target": target,
                "fields": {"message": message, "meeting_id": meeting}
            })
            .to_string(),
        }
    }

    #[test]
    fn the_matcher_takes_only_this_line_for_this_meeting() {
        let l = MC_MEETING_ENDED;
        let got = matching_entries(
            l,
            id(),
            vec![
                raw(1, l.target, l.message, ID),
                // Right message, wrong meeting.
                raw(
                    2,
                    l.target,
                    l.message,
                    "00000000-0000-0000-0000-000000000000",
                ),
                // Right meeting, wrong target.
                raw(3, "mc.actor.meeting", l.message, ID),
                // Right meeting, other message.
                raw(4, l.target, "Meeting actor created", ID),
                // Not JSON.
                LokiEntry {
                    ts_ns: 5,
                    line: format!("{ID} {}", l.message),
                },
            ],
        );
        assert_eq!(got.iter().map(|e| e.ts_ns).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn the_logql_filter_stops_before_non_ascii_and_metacharacters() {
        assert_eq!(
            logql_filter(MC_CLEAN_CLOSE.message),
            "Clean transport close"
        );
        assert_eq!(
            logql_filter(MC_MEETING_ENDED.message),
            MC_MEETING_ENDED.message
        );
        assert_eq!(logql_filter("a \"quoted\" line"), "a");
        let q = meeting_logql(MC_CLEAN_CLOSE, id());
        assert_eq!(
            q,
            format!("{{app=\"mc-service\"}} |= \"{ID}\" |= \"Clean transport close\"")
        );
    }

    #[test]
    fn a_loki_error_is_its_own_token_never_no_lines() {
        let err = classify_scoped(
            MC_MEETING_ENDED,
            id(),
            "q",
            Err(LokiError::Status {
                status: 400,
                body: "parse error".to_string(),
            }),
        )
        .unwrap_err();
        assert!(err.starts_with(LOKI_QUERY_FAILED), "{err}");
        assert!(err.contains(ID), "{err}");
        // And an empty answer is an empty Ok, not an error.
        assert_eq!(
            classify_scoped(MC_MEETING_ENDED, id(), "q", Ok(vec![])),
            Ok(vec![])
        );
    }

    /// PURE: does `src` emit `line` — the exact `"{message}"` literal, with
    /// the nearest preceding `target: "…"` (same tracing macro) equal to
    /// `line.target`?
    fn emits(src: &str, line: LogLine) -> Result<(), String> {
        let lit = format!("\"{}\"", line.message);
        let at = src.find(&lit).ok_or_else(|| format!("no literal {lit}"))?;
        let before = &src[..at];
        let t = before
            .rfind("target: \"")
            .ok_or_else(|| format!("no `target:` before {lit}"))?;
        let rest = &before[t + "target: \"".len()..];
        let target = &rest[..rest.find('"').unwrap_or(0)];
        if target == line.target {
            Ok(())
        } else {
            Err(format!(
                "{lit} is emitted with target `{target}`, not `{}`",
                line.target
            ))
        }
    }

    #[test]
    fn every_log_line_const_matches_its_emitter() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (line, file) in EMITTERS {
            let src = std::fs::read_to_string(repo.join(file))
                .unwrap_or_else(|e| panic!("read emitter {file}: {e}"));
            emits(&src, *line).unwrap_or_else(|e| {
                panic!(
                    "log-line-drift: {file}: {e}. Update the LogLine const in \
                     fixtures/loki.rs (and its TS twin in packages/web-app/e2e/mcLoki.ts)"
                )
            });
        }
    }

    #[test]
    fn the_emitter_pin_fails_on_a_reword_or_retarget() {
        let line = MC_GRACE_EXPIRED;
        let ok = format!(
            "info!(target: \"{}\", x = 1, \"{}\");",
            line.target, line.message
        );
        assert!(emits(&ok, line).is_ok());
        let reworded = ok.replace("expired", "lapsed");
        assert!(emits(&reworded, line).is_err());
        let retargeted = ok.replace(line.target, "mc.other");
        assert!(emits(&retargeted, line).is_err());
    }
}
