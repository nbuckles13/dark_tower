//! Prometheus client fixture for querying metrics.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Prometheus client errors.
#[derive(Debug, Error)]
pub enum PrometheusError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Prometheus query response.
#[derive(Debug, Deserialize)]
pub struct QueryResponse {
    pub status: String,
    pub data: QueryData,
}

/// Query response data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryData {
    pub result_type: String,
    pub result: Vec<QueryResult>,
}

/// A single query result.
#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub metric: std::collections::HashMap<String, String>,
    pub value: Option<(f64, String)>,
    pub values: Option<Vec<(f64, String)>>,
}

/// Request parameters for a Prometheus query.
#[derive(Debug, Serialize)]
pub struct QueryRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<i64>,
}

impl QueryRequest {
    /// Create a new instant query.
    pub fn instant(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            time: None,
        }
    }

    /// Create a new instant query at a specific time.
    pub fn instant_at(query: impl Into<String>, time: i64) -> Self {
        Self {
            query: query.into(),
            time: Some(time),
        }
    }
}

/// Client for querying Prometheus.
pub struct PrometheusClient {
    base_url: String,
    http_client: Client,
}

impl PrometheusClient {
    /// Create a new Prometheus client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http_client: Client::new(),
        }
    }

    /// Execute an instant query.
    pub async fn query(&self, request: QueryRequest) -> Result<QueryResponse, PrometheusError> {
        let query_url = format!("{}/api/v1/query", self.base_url);

        let response = self
            .http_client
            .get(&query_url)
            .query(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(PrometheusError::QueryFailed(format!(
                "Status: {}",
                response.status()
            )));
        }

        let query_response = response.json::<QueryResponse>().await?;

        if query_response.status != "success" {
            return Err(PrometheusError::QueryFailed(format!(
                "Query status: {}",
                query_response.status
            )));
        }

        Ok(query_response)
    }

    /// Execute a PromQL query and return the results.
    pub async fn query_promql(&self, promql: &str) -> Result<QueryResponse, PrometheusError> {
        self.query(QueryRequest::instant(promql)).await
    }

    /// Get the raw metrics from a specific endpoint.
    ///
    /// This bypasses Prometheus storage and queries the service's /metrics endpoint directly.
    pub async fn fetch_metrics(&self, metrics_url: &str) -> Result<String, PrometheusError> {
        let response = self.http_client.get(metrics_url).send().await?;

        if !response.status().is_success() {
            return Err(PrometheusError::QueryFailed(format!(
                "Metrics endpoint returned status: {}",
                response.status()
            )));
        }

        Ok(response.text().await?)
    }

    /// Get the HTTP client for custom requests.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }
}
