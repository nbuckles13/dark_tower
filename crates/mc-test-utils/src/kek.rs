//! KEK rotation lifecycle fixture (ADR-0036 §4 Rotation).
//!
//! One home for the `KekLifecycle` every test that builds a controller or a
//! meeting actor must now supply, so its W has a single value in the suite
//! rather than one literal per test file.

use std::sync::Arc;
use std::time::Duration;

/// W for tests: a REPRESENTATIVE value, deliberately NOT tied to the ConfigMap's
/// `MC_KEK_ROTATION_DEBOUNCE_SECONDS`. It happens to match today; nothing keeps
/// it matching, and nothing needs to — this is not a claim about deployment.
///
/// That is safe because no test WAITS for this to elapse. Tests that exercise rotation drive it with
/// `MeetingSeams::kek_debounce_manual` + `MeetingMessage::ForceKekRotation`, so
/// coalescing holds structurally rather than by racing a real window (ADR-0028
/// zero-retry). A test that did wait for 60 s would be the defect.
///
/// Seconds, in the wire's own `u32`, because that is the unit `KekLifecycle`
/// holds. [`test_kek_rotation_window`] derives the `Duration`.
pub const TEST_KEK_ROTATION_WINDOW_SECONDS: u32 = 60;

/// [`TEST_KEK_ROTATION_WINDOW_SECONDS`] as a `Duration`, derived — not a
/// second literal.
#[must_use]
pub fn test_kek_rotation_window() -> Duration {
    Duration::from_secs(u64::from(TEST_KEK_ROTATION_WINDOW_SECONDS))
}

/// A fresh, empty rotation lifecycle at [`TEST_KEK_ROTATION_WINDOW_SECONDS`].
#[must_use]
pub fn kek_lifecycle() -> Arc<mc_service::media_admission::KekLifecycle> {
    Arc::new(mc_service::media_admission::KekLifecycle::new(
        TEST_KEK_ROTATION_WINDOW_SECONDS,
    ))
}
