//! Common configuration types for Dark Tower components.

use serde::{Deserialize, Serialize};

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// `PostgreSQL` connection string
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

/// The largest legal MC→MH heartbeat timeout N (`MC_MH_HEARTBEAT_TIMEOUT_MS`),
/// in milliseconds (ADR-0039 §1, §6, §7). The ONE home of this value: MC and MH
/// are separate crates, and several bounds on both sides rest on it.
///
/// - MC: the upper end of `MC_MH_HEARTBEAT_TIMEOUT_MS`
///   (`mc_service::config::MAX_MH_HEARTBEAT_TIMEOUT_MS` is defined from it).
/// - MH: the cap of `MH_MAX_IDLE_TIMEOUT_MS` (the QUIC idle timeout is sized
///   ≈ N, so it never needs to exceed the largest N); the floor of
///   `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (const-asserted to be ≥ 30 of
///   MC's heartbeat intervals at this ceiling); and the top bucket of
///   `mh_mc_heartbeat_latency` (≥ the largest interval, N/3).
///
/// Raising it is a cross-service change: each MH const-assert above fails at
/// compile time until its bound is re-derived.
pub const MH_HEARTBEAT_TIMEOUT_CEILING_MS: u32 = 30_000;
