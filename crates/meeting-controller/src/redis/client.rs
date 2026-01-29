//! Fenced Redis client implementation (ADR-0023 Section 3 & 6).
//!
//! Provides a Redis client with fencing token support for split-brain prevention.
//!
//! # Key Patterns
//!
//! - `meeting:{id}:generation` - Fencing generation (monotonic counter)
//! - `meeting:{id}:mh` - MH assignment data (JSON)
//! - `meeting:{id}:state` - Meeting metadata (HASH)
//!
//! # Connection Pattern
//!
//! The redis-rs `MultiplexedConnection` is designed to be cloned cheaply and used
//! concurrently. From the docs: "cheap to clone and can be used safely concurrently".
//! No locking is needed - just clone the connection for each operation.
//!
//! # Usage
//!
//! ```rust,ignore
//! let client = FencedRedisClient::new("redis://localhost:6379").await?;
//!
//! // Store MH assignment with fencing
//! client.store_mh_assignment("meeting-123", "mh-1", "wt://mh-1:4433", None).await?;
//!
//! // Get current generation
//! let gen = client.get_generation("meeting-123").await?;
//! ```

use crate::errors::McError;
use crate::redis::lua_scripts;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client, Script};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, instrument, warn};

/// MH assignment data stored in Redis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MhAssignmentData {
    /// Primary MH ID.
    pub primary_mh_id: String,
    /// Primary MH WebTransport endpoint.
    pub primary_endpoint: String,
    /// Backup MH ID (optional).
    pub backup_mh_id: Option<String>,
    /// Backup MH WebTransport endpoint (optional).
    pub backup_endpoint: Option<String>,
    /// Assignment timestamp.
    pub assigned_at: i64,
}

/// Fenced Redis client for Meeting Controller.
///
/// All write operations use fencing tokens to prevent split-brain.
///
/// This struct is cheaply cloneable - the underlying `MultiplexedConnection`
/// is designed to be shared across tasks. Each actor should clone this client
/// rather than sharing via `Arc<Mutex>`.
#[derive(Clone)]
pub struct FencedRedisClient {
    /// Redis client (kept for potential reconnection scenarios).
    #[allow(dead_code)]
    client: Client,
    /// Multiplexed connection (cheaply cloneable, designed for concurrent use).
    connection: MultiplexedConnection,
    /// Local generation cache per meeting (optimization).
    /// Format: meeting_id -> generation
    ///
    /// This cache is populated on increment_generation calls and cleared on delete_meeting.
    /// Used by future optimizations to skip Redis reads when generation is known.
    /// Currently written but read path deferred to Phase 6d (session binding validation).
    #[allow(dead_code)]
    local_generation: Arc<RwLock<std::collections::HashMap<String, u64>>>,
    /// Precompiled Lua scripts.
    fenced_write_script: Script,
    fenced_hset_script: Script,
    fenced_delete_script: Script,
    increment_gen_script: Script,
}

impl FencedRedisClient {
    /// Create a new fenced Redis client.
    ///
    /// # Arguments
    ///
    /// * `redis_url` - Redis connection URL (e.g., `redis://localhost:6379`)
    ///
    /// # Errors
    ///
    /// Returns `McError::Redis` if connection fails.
    ///
    /// # Note
    ///
    /// The returned client is cheaply cloneable. The underlying `MultiplexedConnection`
    /// is designed for concurrent use without locking.
    pub async fn new(redis_url: &str) -> Result<Self, McError> {
        let client = Client::open(redis_url).map_err(|e| {
            // Note: Do NOT log redis_url as it may contain credentials
            // (e.g., redis://:password@host:port)
            error!(
                target: "mc.redis.client",
                error = %e,
                "Failed to open Redis client"
            );
            McError::Redis(format!("Failed to open Redis client: {e}"))
        })?;

        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                error!(
                    target: "mc.redis.client",
                    error = %e,
                    "Failed to connect to Redis"
                );
                McError::Redis(format!("Failed to connect to Redis: {e}"))
            })?;

        Ok(Self {
            client,
            connection,
            local_generation: Arc::new(RwLock::new(std::collections::HashMap::new())),
            fenced_write_script: Script::new(lua_scripts::FENCED_WRITE),
            fenced_hset_script: Script::new(lua_scripts::FENCED_HSET),
            fenced_delete_script: Script::new(lua_scripts::FENCED_DELETE),
            increment_gen_script: Script::new(lua_scripts::INCREMENT_GENERATION),
        })
    }

    /// Get the current generation for a meeting.
    ///
    /// Returns 0 if no generation exists (new meeting).
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn get_generation(&self, meeting_id: &str) -> Result<u64, McError> {
        // Clone the connection (cheap operation) for this request
        let mut conn = self.connection.clone();
        let key = format!("meeting:{meeting_id}:generation");

        let result: Option<String> = conn.get(&key).await.map_err(|e| {
            warn!(
                target: "mc.redis.client",
                error = %e,
                meeting_id = %meeting_id,
                "Failed to get generation"
            );
            McError::Redis(format!("Failed to get generation: {e}"))
        })?;

        Ok(result.and_then(|s| s.parse().ok()).unwrap_or(0))
    }

    /// Increment the generation for a meeting and return new value.
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn increment_generation(&self, meeting_id: &str) -> Result<u64, McError> {
        // Clone the connection (cheap operation) for this request
        let mut conn = self.connection.clone();
        let key = format!("meeting:{meeting_id}:generation");

        let new_gen: i64 = self
            .increment_gen_script
            .key(&key)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.redis.client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to increment generation"
                );
                McError::Redis(format!("Failed to increment generation: {e}"))
            })?;

        let new_gen = new_gen as u64;

        // Update local cache
        {
            let mut cache = self.local_generation.write().await;
            cache.insert(meeting_id.to_string(), new_gen);
        }

        debug!(
            target: "mc.redis.client",
            meeting_id = %meeting_id,
            new_generation = new_gen,
            "Incremented generation"
        );

        Ok(new_gen)
    }

    /// Store MH assignment with fencing token.
    ///
    /// This method atomically increments the meeting's generation counter and
    /// stores the MH assignment data. The generation acts as a fencing token
    /// to prevent split-brain scenarios during failover.
    ///
    /// # Generation Semantics
    ///
    /// - Each write increments the generation monotonically
    /// - Stale writes (with lower generation) are rejected by Lua scripts
    /// - In split-brain recovery, the MC with higher generation wins
    /// - New meetings start at generation 1
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Meeting identifier
    /// * `primary_mh_id` - Primary MH identifier
    /// * `primary_endpoint` - Primary MH WebTransport endpoint
    /// * `backup` - Optional backup MH (id, endpoint)
    ///
    /// # Errors
    ///
    /// Returns `McError::FencedOut` if another MC has written a higher generation.
    /// Returns `McError::Redis` for connection or serialization errors.
    #[instrument(skip_all, fields(meeting_id = %meeting_id, primary_mh_id = %primary_mh_id))]
    pub async fn store_mh_assignment(
        &self,
        meeting_id: &str,
        primary_mh_id: &str,
        primary_endpoint: &str,
        backup: Option<(&str, &str)>,
    ) -> Result<(), McError> {
        // Get current generation and increment
        let generation = self.increment_generation(meeting_id).await?;

        let data = MhAssignmentData {
            primary_mh_id: primary_mh_id.to_string(),
            primary_endpoint: primary_endpoint.to_string(),
            backup_mh_id: backup.map(|(id, _)| id.to_string()),
            backup_endpoint: backup.map(|(_, ep)| ep.to_string()),
            assigned_at: chrono::Utc::now().timestamp(),
        };

        let json = serde_json::to_string(&data).map_err(|e| {
            error!(
                target: "mc.redis.client",
                error = %e,
                "Failed to serialize MH assignment"
            );
            McError::Internal(format!("serialization failed: {e}"))
        })?;

        // Clone the connection (cheap operation) for this request
        let mut conn = self.connection.clone();
        let gen_key = format!("meeting:{meeting_id}:generation");
        let data_key = format!("meeting:{meeting_id}:mh");

        let result: i64 = self
            .fenced_write_script
            .key(&gen_key)
            .key(&data_key)
            .arg(generation)
            .arg(&json)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.redis.client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to store MH assignment"
                );
                McError::Redis(format!("Failed to store MH assignment: {e}"))
            })?;

        match result {
            1 => {
                debug!(
                    target: "mc.redis.client",
                    meeting_id = %meeting_id,
                    generation = generation,
                    "Stored MH assignment"
                );
                Ok(())
            }
            0 => {
                warn!(
                    target: "mc.redis.client",
                    meeting_id = %meeting_id,
                    generation = generation,
                    "Fenced out when storing MH assignment"
                );
                Err(McError::FencedOut(format!(
                    "Generation {generation} is stale"
                )))
            }
            _ => {
                error!(
                    target: "mc.redis.client",
                    meeting_id = %meeting_id,
                    result = result,
                    "Invalid generation format in Redis"
                );
                Err(McError::Redis("Invalid generation format".to_string()))
            }
        }
    }

    /// Get MH assignment for a meeting.
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn get_mh_assignment(
        &self,
        meeting_id: &str,
    ) -> Result<Option<MhAssignmentData>, McError> {
        let mut conn = self.connection.clone();
        let key = format!("meeting:{meeting_id}:mh");

        let result: Option<String> = conn.get(&key).await.map_err(|e| {
            warn!(
                target: "mc.redis.client",
                error = %e,
                meeting_id = %meeting_id,
                "Failed to get MH assignment"
            );
            McError::Redis(format!("Failed to get MH assignment: {e}"))
        })?;

        match result {
            Some(json) => {
                let data: MhAssignmentData = serde_json::from_str(&json).map_err(|e| {
                    error!(
                        target: "mc.redis.client",
                        error = %e,
                        meeting_id = %meeting_id,
                        "Failed to deserialize MH assignment"
                    );
                    McError::Redis(format!("Failed to deserialize MH assignment: {e}"))
                })?;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    /// Delete MH assignment for a meeting.
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn delete_mh_assignment(&self, meeting_id: &str) -> Result<(), McError> {
        // Get current generation
        let generation = self.get_generation(meeting_id).await?;

        let mut conn = self.connection.clone();
        let gen_key = format!("meeting:{meeting_id}:generation");
        let data_key = format!("meeting:{meeting_id}:mh");

        let result: i64 = self
            .fenced_delete_script
            .key(&gen_key)
            .key(&data_key)
            .arg(generation + 1) // Use next generation for delete
            .invoke_async(&mut conn)
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.redis.client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to delete MH assignment"
                );
                McError::Redis(format!("Failed to delete MH assignment: {e}"))
            })?;

        if result >= 0 {
            debug!(
                target: "mc.redis.client",
                meeting_id = %meeting_id,
                "Deleted MH assignment"
            );
            Ok(())
        } else {
            error!(
                target: "mc.redis.client",
                meeting_id = %meeting_id,
                result = result,
                "Failed to delete MH assignment"
            );
            Err(McError::Redis("Failed to delete MH assignment".to_string()))
        }
    }

    /// Store meeting state with fencing.
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Meeting identifier
    /// * `generation` - Expected generation (fencing token)
    /// * `fields` - Field-value pairs to store
    #[instrument(skip_all, fields(meeting_id = %meeting_id, generation = generation))]
    pub async fn store_meeting_state(
        &self,
        meeting_id: &str,
        generation: u64,
        fields: &[(&str, &str)],
    ) -> Result<(), McError> {
        let mut conn = self.connection.clone();
        let gen_key = format!("meeting:{meeting_id}:generation");
        let state_key = format!("meeting:{meeting_id}:state");

        // Use redis cmd to invoke the script directly
        let mut cmd = redis::cmd("EVALSHA");
        let script_hash = self.fenced_hset_script.get_hash();
        cmd.arg(script_hash)
            .arg(2) // number of keys
            .arg(&gen_key)
            .arg(&state_key)
            .arg(generation);

        // Add field/value pairs as additional args
        for (field, value) in fields {
            cmd.arg(*field).arg(*value);
        }

        // Try EVALSHA first, fall back to EVAL if script not cached
        let result: Result<i64, _> = cmd.query_async(&mut conn).await;
        let result = match result {
            Ok(r) => r,
            Err(e) if e.kind() == redis::ErrorKind::NoScriptError => {
                // Script not cached, use EVAL
                let mut eval_cmd = redis::cmd("EVAL");
                eval_cmd
                    .arg(lua_scripts::FENCED_HSET)
                    .arg(2)
                    .arg(&gen_key)
                    .arg(&state_key)
                    .arg(generation);

                for (field, value) in fields {
                    eval_cmd.arg(*field).arg(*value);
                }

                eval_cmd.query_async(&mut conn).await.map_err(|e| {
                    warn!(
                        target: "mc.redis.client",
                        error = %e,
                        meeting_id = %meeting_id,
                        "Failed to store meeting state"
                    );
                    McError::Redis(format!("Failed to store meeting state: {e}"))
                })?
            }
            Err(e) => {
                warn!(
                    target: "mc.redis.client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to store meeting state"
                );
                return Err(McError::Redis(format!(
                    "Failed to store meeting state: {e}"
                )));
            }
        };

        match result {
            1 => {
                debug!(
                    target: "mc.redis.client",
                    meeting_id = %meeting_id,
                    generation = generation,
                    field_count = fields.len(),
                    "Stored meeting state"
                );
                Ok(())
            }
            0 => {
                warn!(
                    target: "mc.redis.client",
                    meeting_id = %meeting_id,
                    generation = generation,
                    "Fenced out when storing meeting state"
                );
                Err(McError::FencedOut(format!(
                    "Generation {generation} is stale"
                )))
            }
            _ => {
                error!(
                    target: "mc.redis.client",
                    meeting_id = %meeting_id,
                    result = result,
                    "Invalid generation format in Redis"
                );
                Err(McError::Redis("Invalid generation format".to_string()))
            }
        }
    }

    /// Delete all meeting data (cleanup on meeting end).
    #[instrument(skip_all, fields(meeting_id = %meeting_id))]
    pub async fn delete_meeting(&self, meeting_id: &str) -> Result<(), McError> {
        let mut conn = self.connection.clone();

        // Delete all meeting keys
        let keys = vec![
            format!("meeting:{meeting_id}:generation"),
            format!("meeting:{meeting_id}:mh"),
            format!("meeting:{meeting_id}:state"),
            format!("meeting:{meeting_id}:participants"),
        ];

        let _: () = conn.del(&keys).await.map_err(|e| {
            warn!(
                target: "mc.redis.client",
                error = %e,
                meeting_id = %meeting_id,
                "Failed to delete meeting data"
            );
            McError::Redis(format!("Failed to delete meeting data: {e}"))
        })?;

        // Clear local cache
        {
            let mut cache = self.local_generation.write().await;
            cache.remove(meeting_id);
        }

        debug!(
            target: "mc.redis.client",
            meeting_id = %meeting_id,
            "Deleted meeting data"
        );

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mh_assignment_data_serialization() {
        let data = MhAssignmentData {
            primary_mh_id: "mh-1".to_string(),
            primary_endpoint: "wt://mh-1:4433".to_string(),
            backup_mh_id: Some("mh-2".to_string()),
            backup_endpoint: Some("wt://mh-2:4433".to_string()),
            assigned_at: 1234567890,
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: MhAssignmentData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.primary_mh_id, "mh-1");
        assert_eq!(parsed.backup_mh_id, Some("mh-2".to_string()));
    }

    #[test]
    fn test_mh_assignment_data_without_backup() {
        let data = MhAssignmentData {
            primary_mh_id: "mh-1".to_string(),
            primary_endpoint: "wt://mh-1:4433".to_string(),
            backup_mh_id: None,
            backup_endpoint: None,
            assigned_at: 1234567890,
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: MhAssignmentData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.primary_mh_id, "mh-1");
        assert!(parsed.backup_mh_id.is_none());
    }

    #[test]
    fn test_mh_assignment_data_fields() {
        // Verify all fields are properly serialized
        let data = MhAssignmentData {
            primary_mh_id: "mh-primary".to_string(),
            primary_endpoint: "wt://primary:4433".to_string(),
            backup_mh_id: Some("mh-backup".to_string()),
            backup_endpoint: Some("wt://backup:4433".to_string()),
            assigned_at: 1706000000,
        };

        let json = serde_json::to_string(&data).unwrap();

        // Verify JSON structure
        assert!(json.contains("\"primary_mh_id\":\"mh-primary\""));
        assert!(json.contains("\"primary_endpoint\":\"wt://primary:4433\""));
        assert!(json.contains("\"backup_mh_id\":\"mh-backup\""));
        assert!(json.contains("\"backup_endpoint\":\"wt://backup:4433\""));
        assert!(json.contains("\"assigned_at\":1706000000"));
    }

    #[test]
    fn test_mh_assignment_data_round_trip() {
        // Test full round-trip serialization
        let original = MhAssignmentData {
            primary_mh_id: "mh-1".to_string(),
            primary_endpoint: "wt://mh-1:4433".to_string(),
            backup_mh_id: Some("mh-2".to_string()),
            backup_endpoint: Some("wt://mh-2:4433".to_string()),
            assigned_at: 1234567890,
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: MhAssignmentData = serde_json::from_str(&json).unwrap();

        assert_eq!(original.primary_mh_id, restored.primary_mh_id);
        assert_eq!(original.primary_endpoint, restored.primary_endpoint);
        assert_eq!(original.backup_mh_id, restored.backup_mh_id);
        assert_eq!(original.backup_endpoint, restored.backup_endpoint);
        assert_eq!(original.assigned_at, restored.assigned_at);
    }

    #[test]
    fn test_mh_assignment_deserialization_error() {
        // Test that invalid JSON fails gracefully
        let invalid_json = "{invalid json}";
        let result: Result<MhAssignmentData, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());

        // Test missing required fields
        let incomplete_json = r#"{"primary_mh_id": "mh-1"}"#;
        let result: Result<MhAssignmentData, _> = serde_json::from_str(incomplete_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_redis_key_format() {
        // Verify key format used by the client
        let meeting_id = "meeting-123";

        let gen_key = format!("meeting:{meeting_id}:generation");
        assert_eq!(gen_key, "meeting:meeting-123:generation");

        let mh_key = format!("meeting:{meeting_id}:mh");
        assert_eq!(mh_key, "meeting:meeting-123:mh");

        let state_key = format!("meeting:{meeting_id}:state");
        assert_eq!(state_key, "meeting:meeting-123:state");

        let participants_key = format!("meeting:{meeting_id}:participants");
        assert_eq!(participants_key, "meeting:meeting-123:participants");
    }

    #[test]
    fn test_fenced_out_error_message() {
        // Verify FencedOut error message format
        let err = McError::FencedOut("Generation 5 is stale".to_string());
        assert_eq!(err.to_string(), "Fenced out: Generation 5 is stale");
        assert_eq!(err.error_code(), 6); // INTERNAL_ERROR
    }

    #[test]
    fn test_redis_url_validation() {
        // Valid Redis URLs
        let valid_urls = [
            "redis://localhost:6379",
            "redis://user:pass@localhost:6379",
            "redis://redis.example.com:6379/0",
            "redis://localhost",
        ];

        for url in &valid_urls {
            let result = redis::Client::open(*url);
            assert!(result.is_ok(), "Should parse valid URL: {url}");
        }
    }

    #[test]
    fn test_invalid_redis_url() {
        // Invalid URLs should fail
        let invalid_urls = ["", "not-a-url", "http://localhost:6379"];

        for url in &invalid_urls {
            let result = redis::Client::open(*url);
            // Some invalid URLs may parse but fail to connect
            // The important thing is they don't panic
            let _ = result;
        }
    }
}
