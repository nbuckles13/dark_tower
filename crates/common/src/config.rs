//! Common configuration types for Dark Tower components.

use serde::{Deserialize, Serialize};

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection string
    pub postgres_url: String,
    /// Maximum number of connections in the pool
    pub max_connections: u32,
}

/// Redis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    /// Connection pool size
    pub pool_size: usize,
}

/// Observability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// OpenTelemetry collector endpoint
    pub otlp_endpoint: Option<String>,
    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,
    /// Enable JSON-formatted logs
    pub json_logs: bool,
}
