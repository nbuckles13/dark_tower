//! Mock Global Controller for MC testing.
//!
//! Provides a mock GC implementation that can be configured to:
//! - Accept or reject MC registration
//! - Return specific MH assignments for meetings
//! - Simulate various failure scenarios
//!
//! # Example
//!
//! ```rust,ignore
//! use mc_test_utils::MockGc;
//!
//! let mock_gc = MockGc::builder()
//!     .accept_registration()
//!     .with_mh_assignments("meeting-123", vec!["mh-1", "mh-2"])
//!     .build();
//!
//! // Use mock_gc in your tests...
//! ```

// TODO (Phase 6b): Full implementation

/// Mock Global Controller for testing MC registration and heartbeats.
#[derive(Debug, Default)]
pub struct MockGc {
    accept_registration: bool,
    // TODO: Add more configuration fields
}

impl MockGc {
    /// Create a new MockGc builder.
    #[must_use]
    pub fn builder() -> MockGcBuilder {
        MockGcBuilder::default()
    }

    /// Create a MockGc that accepts all registrations.
    #[must_use]
    pub fn accepting() -> Self {
        Self {
            accept_registration: true,
        }
    }

    /// Create a MockGc that rejects all registrations.
    #[must_use]
    pub fn rejecting() -> Self {
        Self {
            accept_registration: false,
        }
    }

    /// Check if this mock accepts registrations.
    #[must_use]
    pub fn accepts_registration(&self) -> bool {
        self.accept_registration
    }
}

/// Builder for MockGc configuration.
#[derive(Debug, Default)]
pub struct MockGcBuilder {
    accept_registration: bool,
}

impl MockGcBuilder {
    /// Configure the mock to accept MC registration.
    #[must_use]
    pub fn accept_registration(mut self) -> Self {
        self.accept_registration = true;
        self
    }

    /// Configure the mock to reject MC registration.
    #[must_use]
    pub fn reject_registration(mut self) -> Self {
        self.accept_registration = false;
        self
    }

    /// Build the MockGc.
    #[must_use]
    pub fn build(self) -> MockGc {
        MockGc {
            accept_registration: self.accept_registration,
        }
    }
}

// =============================================================================
// Scripted GC for AcquireMediaHandler (ADR-0039 §3; story 4 MC-C)
// =============================================================================

use mc_service::grpc::gc_client::{AcquireCall, AcquireResult, HandlerAcquirer};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// A scripted `AcquireMediaHandler` endpoint: every call is RECORDED (the
/// call log a test counts attempts against), then answered with the next
/// scripted result or, once the script is empty, the default (no capacity).
/// One call can be HELD (recorded, not answered) until [`Self::release`], so
/// a test can change the meeting while GC is "choosing".
pub struct ScriptedGc {
    cap: Mutex<Option<NonZeroU32>>,
    calls: Mutex<Vec<AcquireCall>>,
    /// When each call arrived (`tokio::time::Instant`: deterministic under
    /// paused time).
    times: Mutex<Vec<tokio::time::Instant>>,
    script: Mutex<VecDeque<AcquireResult>>,
    default: Mutex<AcquireResult>,
    hold: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    notify: Arc<Notify>,
}

impl ScriptedGc {
    /// A GC advertising `cap` (`max_exclude_handler_ids`) with no capacity.
    #[must_use]
    pub fn with_cap(cap: u32) -> Self {
        Self {
            cap: Mutex::new(NonZeroU32::new(cap)),
            calls: Mutex::new(Vec::new()),
            times: Mutex::new(Vec::new()),
            script: Mutex::new(VecDeque::new()),
            default: Mutex::new(AcquireResult::NoCapacity {
                message: "no capacity (fixture)".to_string(),
            }),
            hold: Mutex::new(None),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Answer the next unscripted call with `result`.
    pub fn push(&self, result: AcquireResult) {
        self.script.lock().unwrap().push_back(result);
    }

    /// Answer every unscripted call with `result` from now on.
    pub fn set_default(&self, result: AcquireResult) {
        *self.default.lock().unwrap() = result;
    }

    /// Hold every call from now on (recorded, unanswered) until
    /// [`Self::release`].
    pub fn hold(&self) {
        *self.hold.lock().unwrap() = Some(Arc::new(tokio::sync::Semaphore::new(0)));
    }

    /// Answer every held call and stop holding.
    pub fn release(&self) {
        if let Some(gate) = self.hold.lock().unwrap().take() {
            gate.close();
        }
    }

    /// Every call so far, in arrival order.
    #[must_use]
    pub fn calls(&self) -> Vec<AcquireCall> {
        self.calls.lock().unwrap().clone()
    }

    /// When each call arrived, in arrival order (paired with [`Self::calls`]).
    #[must_use]
    pub fn call_times(&self) -> Vec<tokio::time::Instant> {
        self.times.lock().unwrap().clone()
    }

    /// Wait (paused-time friendly) until at least `n` calls were made.
    pub async fn wait_for_calls(&self, n: usize, timeout: std::time::Duration) -> Vec<AcquireCall> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let calls = self.calls();
            if calls.len() >= n {
                return calls;
            }
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !left.is_zero(),
                "timed out waiting for {n} Acquire calls; got {}",
                calls.len()
            );
            let _ = tokio::time::timeout(left, self.notify.notified()).await;
        }
    }
}

impl HandlerAcquirer for ScriptedGc {
    fn acquire<'a>(
        &'a self,
        call: &'a AcquireCall,
    ) -> Pin<Box<dyn std::future::Future<Output = AcquireResult> + Send + 'a>> {
        self.calls.lock().unwrap().push(call.clone());
        self.times.lock().unwrap().push(tokio::time::Instant::now());
        self.notify.notify_one();
        let result = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.default.lock().unwrap().clone());
        let gate = self.hold.lock().unwrap().clone();
        Box::pin(async move {
            if let Some(gate) = gate {
                let _ = gate.acquire().await;
            }
            result
        })
    }

    fn max_exclude_handler_ids(&self) -> Option<NonZeroU32> {
        *self.cap.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_gc_builder() {
        let gc = MockGc::builder().accept_registration().build();
        assert!(gc.accepts_registration());

        let gc = MockGc::builder().reject_registration().build();
        assert!(!gc.accepts_registration());
    }

    #[test]
    fn test_mock_gc_shortcuts() {
        assert!(MockGc::accepting().accepts_registration());
        assert!(!MockGc::rejecting().accepts_registration());
    }
}
