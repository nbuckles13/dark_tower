//! In-memory Redis mock for MC state testing.
//!
//! Provides an in-memory implementation of Redis operations used by MC:
//! - Session binding state (correlation_id, nonce, binding_token)
//! - Meeting state (participants, subscriptions)
//! - Fencing token validation (generation-based)
//!
//! # Example
//!
//! ```rust,ignore
//! use mc_test_utils::MockRedis;
//!
//! let redis = MockRedis::new()
//!     .with_session("corr-123", SessionState {
//!         user_id: "user-456",
//!         participant_id: "part-789",
//!         nonce: "abc123",
//!     })
//!     .with_fencing_generation("meeting-123", 5);
//!
//! // Test fencing
//! assert!(redis.validate_fencing("meeting-123", 5).is_ok());
//! assert!(redis.validate_fencing("meeting-123", 4).is_err());
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// TODO (Phase 6b): Full implementation with async traits

/// Mock Redis for testing MC state management.
#[derive(Debug, Clone)]
pub struct MockRedis {
    inner: Arc<Mutex<MockRedisInner>>,
}

#[derive(Debug, Default)]
struct MockRedisInner {
    /// Key-value store for simple values
    kv: HashMap<String, String>,
    /// Fencing generation per meeting
    fencing_generations: HashMap<String, u64>,
    /// Session binding state per correlation_id
    sessions: HashMap<String, SessionState>,
    /// Used nonces (for replay prevention)
    used_nonces: HashMap<String, bool>,
}

/// Session binding state stored in Redis.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// User ID from JWT.
    pub user_id: String,
    /// Participant ID assigned by MC.
    pub participant_id: String,
    /// Meeting ID.
    pub meeting_id: String,
    /// Current nonce for binding token.
    pub nonce: String,
    /// Timestamp when session was created.
    pub created_at: i64,
}

/// Error from mock Redis operations.
#[derive(Debug, thiserror::Error)]
pub enum MockRedisError {
    #[error("Key not found: {0}")]
    NotFound(String),
    #[error("Fenced out: expected generation {expected}, got {actual}")]
    FencedOut { expected: u64, actual: u64 },
    #[error("Nonce already used")]
    NonceReused,
}

impl Default for MockRedis {
    fn default() -> Self {
        Self::new()
    }
}

impl MockRedis {
    /// Create a new empty MockRedis.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockRedisInner::default())),
        }
    }

    /// Add a session to the mock.
    #[must_use]
    pub fn with_session(self, correlation_id: &str, state: SessionState) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner
                .sessions
                .insert(correlation_id.to_string(), state.clone());
        }
        self
    }

    /// Set the fencing generation for a meeting.
    #[must_use]
    pub fn with_fencing_generation(self, meeting_id: &str, generation: u64) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner
                .fencing_generations
                .insert(meeting_id.to_string(), generation);
        }
        self
    }

    /// Get a session by correlation ID.
    pub fn get_session(&self, correlation_id: &str) -> Option<SessionState> {
        let inner = self.inner.lock().unwrap();
        inner.sessions.get(correlation_id).cloned()
    }

    /// Store a session.
    pub fn set_session(&self, correlation_id: &str, state: SessionState) {
        let mut inner = self.inner.lock().unwrap();
        inner.sessions.insert(correlation_id.to_string(), state);
    }

    /// Delete a session.
    pub fn delete_session(&self, correlation_id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        inner.sessions.remove(correlation_id).is_some()
    }

    /// Get the current fencing generation for a meeting.
    pub fn get_fencing_generation(&self, meeting_id: &str) -> Option<u64> {
        let inner = self.inner.lock().unwrap();
        inner.fencing_generations.get(meeting_id).copied()
    }

    /// Validate fencing generation (returns error if stale).
    pub fn validate_fencing(&self, meeting_id: &str, expected: u64) -> Result<(), MockRedisError> {
        let inner = self.inner.lock().unwrap();
        if let Some(&current) = inner.fencing_generations.get(meeting_id) {
            if current > expected {
                return Err(MockRedisError::FencedOut {
                    expected,
                    actual: current,
                });
            }
        }
        Ok(())
    }

    /// Perform a fenced write (validate generation, then write).
    pub fn fenced_set(
        &self,
        meeting_id: &str,
        expected_generation: u64,
        key: &str,
        value: &str,
    ) -> Result<(), MockRedisError> {
        let mut inner = self.inner.lock().unwrap();

        // Validate fencing
        if let Some(&current) = inner.fencing_generations.get(meeting_id) {
            if current > expected_generation {
                return Err(MockRedisError::FencedOut {
                    expected: expected_generation,
                    actual: current,
                });
            }
        }

        // Write value
        inner.kv.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Check and consume a nonce (SETNX pattern).
    /// Returns Ok if nonce was not used, Err if already used.
    pub fn consume_nonce(&self, nonce: &str) -> Result<(), MockRedisError> {
        let mut inner = self.inner.lock().unwrap();
        if inner.used_nonces.contains_key(nonce) {
            return Err(MockRedisError::NonceReused);
        }
        inner.used_nonces.insert(nonce.to_string(), true);
        Ok(())
    }

    /// Check if a nonce has been used.
    pub fn is_nonce_used(&self, nonce: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.used_nonces.contains_key(nonce)
    }

    /// Get a key-value.
    pub fn get(&self, key: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        inner.kv.get(key).cloned()
    }

    /// Set a key-value.
    pub fn set(&self, key: &str, value: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.kv.insert(key.to_string(), value.to_string());
    }

    /// Clear all state.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.kv.clear();
        inner.fencing_generations.clear();
        inner.sessions.clear();
        inner.used_nonces.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_storage() {
        let redis = MockRedis::new();

        let session = SessionState {
            user_id: "user-123".to_string(),
            participant_id: "part-456".to_string(),
            meeting_id: "meeting-789".to_string(),
            nonce: "nonce-abc".to_string(),
            created_at: 1234567890,
        };

        redis.set_session("corr-123", session.clone());

        let retrieved = redis.get_session("corr-123");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.user_id, "user-123");
        assert_eq!(retrieved.participant_id, "part-456");
    }

    #[test]
    fn test_fencing_validation() {
        let redis = MockRedis::new().with_fencing_generation("meeting-123", 5);

        // Current generation matches
        assert!(redis.validate_fencing("meeting-123", 5).is_ok());

        // Higher generation is ok (we're ahead)
        assert!(redis.validate_fencing("meeting-123", 6).is_ok());

        // Lower generation is rejected (we're stale)
        let result = redis.validate_fencing("meeting-123", 4);
        assert!(matches!(result, Err(MockRedisError::FencedOut { .. })));
    }

    #[test]
    fn test_nonce_consumption() {
        let redis = MockRedis::new();

        // First use succeeds
        assert!(redis.consume_nonce("nonce-123").is_ok());

        // Second use fails (replay prevention)
        assert!(matches!(
            redis.consume_nonce("nonce-123"),
            Err(MockRedisError::NonceReused)
        ));

        // Different nonce succeeds
        assert!(redis.consume_nonce("nonce-456").is_ok());
    }

    #[test]
    fn test_fenced_write() {
        let redis = MockRedis::new().with_fencing_generation("meeting-123", 5);

        // Write with current generation succeeds
        assert!(redis.fenced_set("meeting-123", 5, "key1", "value1").is_ok());
        assert_eq!(redis.get("key1"), Some("value1".to_string()));

        // Write with stale generation fails
        let result = redis.fenced_set("meeting-123", 4, "key2", "value2");
        assert!(matches!(result, Err(MockRedisError::FencedOut { .. })));
        assert!(redis.get("key2").is_none());
    }

    #[test]
    fn test_builder_pattern() {
        let session = SessionState {
            user_id: "user-1".to_string(),
            participant_id: "part-1".to_string(),
            meeting_id: "meeting-1".to_string(),
            nonce: "nonce-1".to_string(),
            created_at: 0,
        };

        let redis = MockRedis::new()
            .with_session("corr-1", session)
            .with_fencing_generation("meeting-1", 10);

        assert!(redis.get_session("corr-1").is_some());
        assert_eq!(redis.get_fencing_generation("meeting-1"), Some(10));
    }
}
