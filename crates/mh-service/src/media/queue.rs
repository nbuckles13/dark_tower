//! One bounded drop-oldest ring, used twice: ingress and egress.
//!
//! ADR-0036 §11 names "a bounded datagram queue with drop-oldest" among the
//! ingress denial-of-service caps, and §1 requires MH's own application-level
//! egress bound to trip *before* the transport ceiling so back-pressure is
//! observable in our code rather than inside quinn. Those are two instances of
//! one mechanism — *bound the resource before allocating for it* — so this is
//! **one** generic ring used twice, not two hand-rolled rings that can drift.
//!
//! # Drop-OLDEST is the property, and it is why the queue exists
//!
//! Under a slow subscriber MH sheds backlog and delivers the **freshest**
//! frames: current audio with gaps, not growing delay. That is what makes this
//! queue a **latency ceiling rather than a buffer** (§1: "a realtime path must
//! prefer loss to unbounded latency"). A drop-*newest* implementation increments
//! the same counter, makes the same forward progress, and delivers staler audio
//! — the two are separable only by frame identity, which is what the
//! drop-direction gate in `tests/media_backpressure_integration.rs` asserts.
//!
//! # This is NOT `mc-service`'s bounded participant mailbox
//!
//! Recorded so the resemblance is not re-opened as duplication:
//! `crates/mc-service/src/actors/participant.rs`'s mailbox is **drop-newest**
//! (the opposite policy), it is a signalling mailbox rather than a media path,
//! and its metrics deliberately carry no `key_custody` label and a
//! non-exhaustive label domain. All three of those are wrong here. Same word,
//! different mechanism.
//!
//! # The ring returns the evicted item; the CALLER counts it
//!
//! This type holds no metric handle and has no observability coupling at all.
//! The two call sites keep their own `(reason, direction)` pairs — which they
//! must, since ingress and egress overflow are different tokens with different
//! directions — and an injected handle would have to carry that label pair as
//! state, which is the same two-configurations problem wearing a different hat.

use std::collections::VecDeque;
use std::sync::Mutex;
use tokio::sync::Notify;

/// A fixed-capacity ring that evicts its oldest element on overflow.
///
/// Pure data structure: no clock, no metrics, no async.
#[derive(Debug)]
pub struct BoundedDropOldest<T> {
    ring: VecDeque<T>,
    capacity: usize,
}

impl<T> BoundedDropOldest<T> {
    /// A ring bounded at `capacity` items.
    ///
    /// `capacity` is clamped to at least 1: a zero-capacity ring would drop
    /// every frame while reporting a healthy bound, which is the silent failure
    /// this whole module exists to make loud. The clamp is not a fallback for a
    /// configuration mistake — both call sites pass a compile-time constant —
    /// it is what makes `push` total.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            ring: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push `item`, returning the evicted oldest item if the ring was full.
    ///
    /// The eviction is returned rather than counted here; see the module docs.
    pub fn push(&mut self, item: T) -> Option<T> {
        let evicted = if self.ring.len() >= self.capacity {
            self.ring.pop_front()
        } else {
            None
        };
        self.ring.push_back(item);
        evicted
    }

    /// Pop the oldest item.
    pub fn pop(&mut self) -> Option<T> {
        self.ring.pop_front()
    }

    /// Current occupancy.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    /// Whether the ring holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// The declared bound. Tests assert against this rather than a literal.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
}

/// A [`BoundedDropOldest`] shared between the two tasks at either end of it,
/// with an await point for the consumer.
///
/// # Why a `Mutex` and not a channel
///
/// `tokio::sync::mpsc` cannot express drop-oldest: a bounded sender either
/// awaits or fails, and neither evicts the head. The critical section here is a
/// `VecDeque` push or pop — no allocation once warm, no `.await` held across
/// the guard, and a single producer against a single consumer per queue, so it
/// is uncontended in the ordinary case.
///
/// Poisoning is recovered rather than propagated (`into_inner`): a poisoned
/// media queue means some other task panicked while holding it, and refusing to
/// forward audio for the rest of the connection's life is a worse outcome than
/// continuing with a ring whose contents are structurally fine. There is no
/// `unwrap` on this path — ADR-0002 denies it at the lint level anyway.
#[derive(Debug)]
pub struct SharedQueue<T> {
    ring: Mutex<BoundedDropOldest<T>>,
    /// Wakes the consumer. `Notify` rather than a channel because the queue
    /// itself carries the items; this only carries "there might be something".
    ready: Notify,
    capacity: usize,
}

impl<T> SharedQueue<T> {
    /// A shared ring bounded at `capacity` items.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let ring = BoundedDropOldest::new(capacity);
        let capacity = ring.capacity();
        Self {
            ring: Mutex::new(ring),
            ready: Notify::new(),
            capacity,
        }
    }

    /// Push `item`, returning `(evicted, depth_after_push)`.
    ///
    /// The depth is returned with the eviction so the caller can publish the
    /// occupancy gauge without a second lock acquisition.
    pub fn push(&self, item: T) -> (Option<T>, usize) {
        let (evicted, depth) = {
            let mut guard = self
                .ring
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let evicted = guard.push(item);
            (evicted, guard.len())
        };
        self.ready.notify_one();
        (evicted, depth)
    }

    /// Pop the oldest item without waiting.
    pub fn try_pop(&self) -> Option<T> {
        let mut guard = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.pop()
    }

    /// Wait until an item is available, then pop it.
    ///
    /// The `notified()` future is created **before** the emptiness check, so a
    /// push landing between the two is not lost: `Notify` stores one permit.
    pub async fn pop(&self) -> T {
        loop {
            let waiter = self.ready.notified();
            if let Some(item) = self.try_pop() {
                return item;
            }
            waiter.await;
        }
    }

    /// Current occupancy.
    #[must_use]
    pub fn depth(&self) -> usize {
        let guard = self
            .ring
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.len()
    }

    /// The declared bound.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
}

/// A queue depth as a gauge value, saturating rather than losing precision
/// silently.
///
/// Depth is bounded by the ring's own capacity, so the saturation arm is
/// unreachable; it exists so the conversion is total. Lives here rather than
/// beside either caller because **both** the push side and the drain side
/// publish depth — a gauge written only on push sits at the last push-time
/// depth across an idle period, reporting a deep queue when the queue is empty.
#[must_use]
pub fn depth_as_gauge(depth: usize) -> f64 {
    f64::from(u32::try_from(depth).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_below_bound_evicts_nothing() {
        let mut ring = BoundedDropOldest::new(3);
        assert!(ring.push(1).is_none());
        assert!(ring.push(2).is_none());
        assert!(ring.push(3).is_none());
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn push_at_bound_evicts_the_oldest_and_keeps_the_freshest() {
        // The direction is the property, not merely the bound: a drop-newest
        // ring passes "len never exceeds capacity" and delivers staler audio.
        let mut ring = BoundedDropOldest::new(3);
        for i in 1..=3 {
            ring.push(i);
        }
        assert_eq!(ring.push(4), Some(1), "the OLDEST item must be evicted");
        assert_eq!(ring.len(), 3, "depth never exceeds the declared bound");
        assert_eq!(ring.pop(), Some(2));
        assert_eq!(ring.pop(), Some(3));
        assert_eq!(ring.pop(), Some(4), "the freshest item survived");
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn zero_capacity_is_clamped_rather_than_silently_dropping_everything() {
        let mut ring = BoundedDropOldest::new(0);
        assert_eq!(ring.capacity(), 1);
        assert!(ring.push(1).is_none());
        assert_eq!(ring.pop(), Some(1));
    }

    #[tokio::test]
    async fn shared_queue_pop_waits_then_yields_the_pushed_item() {
        let queue = std::sync::Arc::new(SharedQueue::new(4));
        let consumer = {
            let queue = std::sync::Arc::clone(&queue);
            tokio::spawn(async move { queue.pop().await })
        };
        // The push may land before or after the consumer parks; both orders
        // must deliver, which is why `pop` builds the waiter before checking.
        queue.push(7_u32);
        assert_eq!(consumer.await.unwrap(), 7);
    }

    #[test]
    fn shared_queue_reports_depth_and_eviction_together() {
        let queue = SharedQueue::new(2);
        assert_eq!(queue.push(1_u32), (None, 1));
        assert_eq!(queue.push(2), (None, 2));
        assert_eq!(
            queue.push(3),
            (Some(1), 2),
            "eviction and post-push depth come from one lock acquisition"
        );
    }
}
