//! Actor metrics and mailbox monitoring (ADR-0023 Section 2).
//!
//! Provides mailbox depth monitoring with configurable thresholds:
//!
//! | Actor Type | Normal | Warning | Critical |
//! |------------|--------|---------|----------|
//! | Meeting    | < 100  | 100-500 | > 500    |
//! | Connection | < 50   | 50-200  | > 200    |
//!
//! All metrics are emitted with the `mc_` prefix per ADR-0023 naming conventions.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{debug, warn};

/// Mailbox depth thresholds for meeting actors.
pub const MEETING_MAILBOX_NORMAL: usize = 100;
pub const MEETING_MAILBOX_WARNING: usize = 500;

/// Mailbox depth thresholds for connection actors.
pub const CONNECTION_MAILBOX_NORMAL: usize = 50;
pub const CONNECTION_MAILBOX_WARNING: usize = 200;

/// Actor type for metrics labeling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorType {
    /// MeetingControllerActor (singleton).
    Controller,
    /// MeetingActor (one per meeting).
    Meeting,
    /// ConnectionActor (one per WebTransport connection).
    Connection,
}

impl ActorType {
    /// Returns the actor type as a string for metric labels.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            ActorType::Controller => "controller",
            ActorType::Meeting => "meeting",
            ActorType::Connection => "connection",
        }
    }

    /// Returns the warning threshold for this actor type.
    #[must_use]
    pub const fn warning_threshold(&self) -> usize {
        match self {
            ActorType::Controller => MEETING_MAILBOX_WARNING, // Use meeting thresholds
            ActorType::Meeting => MEETING_MAILBOX_WARNING,
            ActorType::Connection => CONNECTION_MAILBOX_WARNING,
        }
    }

    /// Returns the normal threshold for this actor type.
    #[must_use]
    pub const fn normal_threshold(&self) -> usize {
        match self {
            ActorType::Controller => MEETING_MAILBOX_NORMAL,
            ActorType::Meeting => MEETING_MAILBOX_NORMAL,
            ActorType::Connection => CONNECTION_MAILBOX_NORMAL,
        }
    }
}

/// Mailbox depth level for alerting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxLevel {
    /// Below normal threshold.
    Normal,
    /// Between normal and warning thresholds.
    Warning,
    /// Above warning threshold.
    Critical,
}

/// Mailbox monitor for tracking queue depth and emitting metrics.
#[derive(Debug)]
pub struct MailboxMonitor {
    /// Actor type for labeling.
    actor_type: ActorType,
    /// Actor identifier (meeting_id, connection_id, etc.).
    actor_id: String,
    /// Current mailbox depth.
    depth: AtomicUsize,
    /// Peak mailbox depth since last reset.
    peak_depth: AtomicUsize,
    /// Total messages processed.
    messages_processed: AtomicU64,
    /// Messages dropped due to backpressure.
    messages_dropped: AtomicU64,
}

impl MailboxMonitor {
    /// Create a new mailbox monitor for the given actor.
    #[must_use]
    pub fn new(actor_type: ActorType, actor_id: impl Into<String>) -> Self {
        Self {
            actor_type,
            actor_id: actor_id.into(),
            depth: AtomicUsize::new(0),
            peak_depth: AtomicUsize::new(0),
            messages_processed: AtomicU64::new(0),
            messages_dropped: AtomicU64::new(0),
        }
    }

    /// Record a message being added to the mailbox.
    pub fn record_enqueue(&self) {
        let new_depth = self.depth.fetch_add(1, Ordering::Relaxed) + 1;

        // Update peak if necessary
        let mut current_peak = self.peak_depth.load(Ordering::Relaxed);
        while new_depth > current_peak {
            match self.peak_depth.compare_exchange_weak(
                current_peak,
                new_depth,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_peak = actual,
            }
        }

        // Check thresholds and log warnings
        let level = self.level_for_depth(new_depth);
        if level == MailboxLevel::Critical {
            warn!(
                target: "mc.actor.mailbox",
                actor_type = self.actor_type.as_str(),
                actor_id = %self.actor_id,
                depth = new_depth,
                threshold = self.actor_type.warning_threshold(),
                "Mailbox depth critical"
            );
        } else if level == MailboxLevel::Warning && new_depth == self.actor_type.normal_threshold()
        {
            // Log once when crossing the warning threshold
            debug!(
                target: "mc.actor.mailbox",
                actor_type = self.actor_type.as_str(),
                actor_id = %self.actor_id,
                depth = new_depth,
                "Mailbox depth elevated"
            );
        }
    }

    /// Record a message being removed from the mailbox (processed).
    pub fn record_dequeue(&self) {
        self.depth.fetch_sub(1, Ordering::Relaxed);
        self.messages_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a message being dropped due to backpressure.
    pub fn record_drop(&self) {
        self.messages_dropped.fetch_add(1, Ordering::Relaxed);
        warn!(
            target: "mc.actor.mailbox",
            actor_type = self.actor_type.as_str(),
            actor_id = %self.actor_id,
            dropped = self.messages_dropped.load(Ordering::Relaxed),
            "Message dropped due to backpressure"
        );
    }

    /// Get the current mailbox depth.
    #[must_use]
    pub fn current_depth(&self) -> usize {
        self.depth.load(Ordering::Relaxed)
    }

    /// Get the peak mailbox depth.
    #[must_use]
    pub fn peak_depth(&self) -> usize {
        self.peak_depth.load(Ordering::Relaxed)
    }

    /// Get total messages processed.
    #[must_use]
    pub fn messages_processed(&self) -> u64 {
        self.messages_processed.load(Ordering::Relaxed)
    }

    /// Get total messages dropped.
    #[must_use]
    pub fn messages_dropped(&self) -> u64 {
        self.messages_dropped.load(Ordering::Relaxed)
    }

    /// Get the current mailbox level.
    #[must_use]
    pub fn current_level(&self) -> MailboxLevel {
        self.level_for_depth(self.current_depth())
    }

    /// Reset peak depth counter.
    pub fn reset_peak(&self) {
        self.peak_depth
            .store(self.current_depth(), Ordering::Relaxed);
    }

    /// Determine mailbox level for a given depth.
    fn level_for_depth(&self, depth: usize) -> MailboxLevel {
        if depth > self.actor_type.warning_threshold() {
            MailboxLevel::Critical
        } else if depth > self.actor_type.normal_threshold() {
            MailboxLevel::Warning
        } else {
            MailboxLevel::Normal
        }
    }
}

/// Metrics for heartbeat reporting to Global Controller.
///
/// This struct is shared between the actor system (which updates values)
/// and heartbeat tasks (which read values for reporting to GC).
/// All fields are atomic for lock-free concurrent access.
#[derive(Debug, Default)]
pub struct ControllerMetrics {
    /// Current number of active meetings on this MC.
    current_meetings: AtomicU32,
    /// Current number of active participants across all meetings.
    current_participants: AtomicU32,
}

/// Snapshot of controller metrics at a point in time.
#[derive(Debug, Clone, Copy)]
pub struct ControllerMetricsSnapshot {
    /// Current number of active meetings.
    pub meetings: u32,
    /// Current number of active participants.
    pub participants: u32,
}

impl ControllerMetrics {
    /// Create a new shared metrics instance.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Update the current meeting count.
    pub fn set_meetings(&self, count: u32) {
        self.current_meetings.store(count, Ordering::SeqCst);
    }

    /// Update the current participant count.
    pub fn set_participants(&self, count: u32) {
        self.current_participants.store(count, Ordering::SeqCst);
    }

    /// Get the current meeting count.
    #[must_use]
    pub fn meetings(&self) -> u32 {
        self.current_meetings.load(Ordering::SeqCst)
    }

    /// Get the current participant count.
    #[must_use]
    pub fn participants(&self) -> u32 {
        self.current_participants.load(Ordering::SeqCst)
    }

    /// Increment the meeting count atomically.
    pub fn increment_meetings(&self) {
        self.current_meetings.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement the meeting count atomically.
    pub fn decrement_meetings(&self) {
        self.current_meetings.fetch_sub(1, Ordering::SeqCst);
    }

    /// Increment the participant count atomically.
    pub fn increment_participants(&self) {
        self.current_participants.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement the participant count atomically.
    pub fn decrement_participants(&self) {
        self.current_participants.fetch_sub(1, Ordering::SeqCst);
    }

    /// Take an atomic snapshot of current metrics.
    ///
    /// This reads both counters atomically for consistent reporting in heartbeats.
    #[must_use]
    pub fn snapshot(&self) -> ControllerMetricsSnapshot {
        ControllerMetricsSnapshot {
            meetings: self.current_meetings.load(Ordering::SeqCst),
            participants: self.current_participants.load(Ordering::SeqCst),
        }
    }
}

/// Aggregated metrics for the actor system.
#[derive(Debug, Default)]
pub struct ActorMetrics {
    /// Total meetings currently active.
    pub active_meetings: AtomicUsize,
    /// Total connections currently active.
    pub active_connections: AtomicUsize,
    /// Total actor panics (indicates bugs).
    pub actor_panics: AtomicU64,
    /// Total messages processed across all actors.
    pub total_messages_processed: AtomicU64,
}

impl ActorMetrics {
    /// Create a new shared metrics instance.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Increment active meeting count.
    pub fn meeting_created(&self) {
        self.active_meetings.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active meeting count.
    pub fn meeting_removed(&self) {
        self.active_meetings.fetch_sub(1, Ordering::Relaxed);
    }

    /// Increment active connection count.
    pub fn connection_created(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connection count.
    pub fn connection_closed(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Record an actor panic.
    pub fn record_panic(&self, actor_type: ActorType) {
        self.actor_panics.fetch_add(1, Ordering::Relaxed);
        tracing::error!(
            target: "mc.actor.panic",
            actor_type = actor_type.as_str(),
            total_panics = self.actor_panics.load(Ordering::Relaxed),
            "Actor panic detected - indicates bug, investigation required"
        );
    }

    /// Record a message being processed.
    pub fn record_message_processed(&self) {
        self.total_messages_processed
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Get current meeting count.
    #[must_use]
    pub fn meeting_count(&self) -> usize {
        self.active_meetings.load(Ordering::Relaxed)
    }

    /// Get current connection count.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_type_as_str() {
        assert_eq!(ActorType::Controller.as_str(), "controller");
        assert_eq!(ActorType::Meeting.as_str(), "meeting");
        assert_eq!(ActorType::Connection.as_str(), "connection");
    }

    #[test]
    fn test_actor_type_thresholds() {
        assert_eq!(ActorType::Meeting.normal_threshold(), 100);
        assert_eq!(ActorType::Meeting.warning_threshold(), 500);
        assert_eq!(ActorType::Connection.normal_threshold(), 50);
        assert_eq!(ActorType::Connection.warning_threshold(), 200);
    }

    #[test]
    fn test_mailbox_monitor_enqueue_dequeue() {
        let monitor = MailboxMonitor::new(ActorType::Meeting, "meeting-123");

        assert_eq!(monitor.current_depth(), 0);

        monitor.record_enqueue();
        assert_eq!(monitor.current_depth(), 1);
        assert_eq!(monitor.peak_depth(), 1);

        monitor.record_enqueue();
        monitor.record_enqueue();
        assert_eq!(monitor.current_depth(), 3);
        assert_eq!(monitor.peak_depth(), 3);

        monitor.record_dequeue();
        assert_eq!(monitor.current_depth(), 2);
        assert_eq!(monitor.peak_depth(), 3); // Peak stays at 3
        assert_eq!(monitor.messages_processed(), 1);
    }

    #[test]
    fn test_mailbox_monitor_levels() {
        let monitor = MailboxMonitor::new(ActorType::Meeting, "meeting-123");

        // Normal level (< 100)
        assert_eq!(monitor.current_level(), MailboxLevel::Normal);

        // Simulate high depth
        for _ in 0..150 {
            monitor.record_enqueue();
        }
        assert_eq!(monitor.current_level(), MailboxLevel::Warning);

        // Simulate critical depth (> 500)
        for _ in 0..400 {
            monitor.record_enqueue();
        }
        assert_eq!(monitor.current_level(), MailboxLevel::Critical);
    }

    #[test]
    fn test_mailbox_monitor_connection_thresholds() {
        let monitor = MailboxMonitor::new(ActorType::Connection, "conn-456");

        // Normal (< 50)
        assert_eq!(monitor.current_level(), MailboxLevel::Normal);

        // Warning (50-200)
        for _ in 0..75 {
            monitor.record_enqueue();
        }
        assert_eq!(monitor.current_level(), MailboxLevel::Warning);

        // Critical (> 200)
        for _ in 0..150 {
            monitor.record_enqueue();
        }
        assert_eq!(monitor.current_level(), MailboxLevel::Critical);
    }

    #[test]
    fn test_mailbox_monitor_drop() {
        let monitor = MailboxMonitor::new(ActorType::Meeting, "meeting-123");

        monitor.record_drop();
        assert_eq!(monitor.messages_dropped(), 1);

        monitor.record_drop();
        assert_eq!(monitor.messages_dropped(), 2);
    }

    #[test]
    fn test_mailbox_monitor_reset_peak() {
        let monitor = MailboxMonitor::new(ActorType::Meeting, "meeting-123");

        for _ in 0..10 {
            monitor.record_enqueue();
        }
        assert_eq!(monitor.peak_depth(), 10);

        for _ in 0..5 {
            monitor.record_dequeue();
        }
        assert_eq!(monitor.peak_depth(), 10); // Still 10
        assert_eq!(monitor.current_depth(), 5);

        monitor.reset_peak();
        assert_eq!(monitor.peak_depth(), 5); // Reset to current
    }

    #[test]
    fn test_actor_metrics() {
        let metrics = ActorMetrics::new();

        assert_eq!(metrics.meeting_count(), 0);
        assert_eq!(metrics.connection_count(), 0);

        metrics.meeting_created();
        metrics.meeting_created();
        assert_eq!(metrics.meeting_count(), 2);

        metrics.connection_created();
        metrics.connection_created();
        metrics.connection_created();
        assert_eq!(metrics.connection_count(), 3);

        metrics.meeting_removed();
        assert_eq!(metrics.meeting_count(), 1);

        metrics.connection_closed();
        assert_eq!(metrics.connection_count(), 2);
    }

    #[test]
    fn test_actor_metrics_panics() {
        let metrics = ActorMetrics::new();

        metrics.record_panic(ActorType::Meeting);
        assert_eq!(metrics.actor_panics.load(Ordering::Relaxed), 1);

        metrics.record_panic(ActorType::Connection);
        assert_eq!(metrics.actor_panics.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_mailbox_level_equality() {
        assert_eq!(MailboxLevel::Normal, MailboxLevel::Normal);
        assert_ne!(MailboxLevel::Normal, MailboxLevel::Warning);
        assert_ne!(MailboxLevel::Warning, MailboxLevel::Critical);
    }

    #[test]
    fn test_controller_metrics_meetings() {
        let metrics = ControllerMetrics::new();

        assert_eq!(metrics.meetings(), 0);

        metrics.set_meetings(10);
        assert_eq!(metrics.meetings(), 10);

        metrics.increment_meetings();
        assert_eq!(metrics.meetings(), 11);

        metrics.decrement_meetings();
        assert_eq!(metrics.meetings(), 10);
    }

    #[test]
    fn test_controller_metrics_snapshot() {
        let metrics = ControllerMetrics::new();

        // Initial snapshot should be zero
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.meetings, 0);
        assert_eq!(snapshot.participants, 0);

        // Update metrics
        metrics.set_meetings(5);
        metrics.set_participants(42);

        // Snapshot should reflect current values
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.meetings, 5);
        assert_eq!(snapshot.participants, 42);

        // Test atomic operations through snapshot
        metrics.increment_meetings();
        metrics.increment_participants();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.meetings, 6);
        assert_eq!(snapshot.participants, 43);
    }

    #[test]
    fn test_controller_metrics_participants() {
        let metrics = ControllerMetrics::new();

        assert_eq!(metrics.participants(), 0);

        metrics.set_participants(100);
        assert_eq!(metrics.participants(), 100);

        metrics.increment_participants();
        assert_eq!(metrics.participants(), 101);

        metrics.decrement_participants();
        assert_eq!(metrics.participants(), 100);
    }

    #[test]
    fn test_controller_metrics_default() {
        let metrics = ControllerMetrics::default();
        assert_eq!(metrics.meetings(), 0);
        assert_eq!(metrics.participants(), 0);
    }
}
