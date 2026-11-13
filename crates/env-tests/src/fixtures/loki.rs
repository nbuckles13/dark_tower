//! Loki `query_range` — THE env-test Loki client (hoisted on its second
//! consumer: `tests/30_observability.rs`, `tests/47_mc_join_window.rs`).
//!
//! Every failure is an `Err`, never an empty result: a transport error, a
//! non-2xx (status and a truncated body), an unparseable body, or a 200 whose
//! shape lacks `data.result`. An empty `Vec` therefore means exactly "Loki
//! answered and no line matched" — so a caller polling for a line can never
//! report a query or infrastructure failure as "line never seen".

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
}
