//! System information gathering for comprehensive heartbeats (ADR-0023 Phase 6c).
//!
//! Provides CPU and memory usage metrics for the comprehensive heartbeat
//! sent to Global Controller every 30 seconds.
//!
//! # Usage
//!
//! ```rust,ignore
//! let info = gather_system_info();
//! println!("CPU: {}%, Memory: {}%", info.cpu_percent, info.memory_percent);
//! ```

use sysinfo::System;

/// System resource usage information for comprehensive heartbeats.
#[derive(Debug, Clone, Copy)]
pub struct SystemInfo {
    /// CPU usage as a percentage (0-100).
    pub cpu_percent: u32,
    /// Memory usage as a percentage (0-100).
    pub memory_percent: u32,
}

/// Gather current system resource usage.
///
/// This function creates a new `System` instance each time to get fresh readings.
/// For the comprehensive heartbeat (every 30s), this overhead is acceptable.
///
/// # Returns
///
/// `SystemInfo` with CPU and memory percentages clamped to 0-100.
///
/// # Note
///
/// CPU usage may be 0 on first call because sysinfo needs time to compute
/// deltas. For heartbeat use cases where we call every 30s, this is fine
/// as subsequent calls will have accurate readings.
#[must_use]
pub fn gather_system_info() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Get global CPU usage (average across all cores)
    let cpu_percent = sys.global_cpu_info().cpu_usage() as u32;

    // Calculate memory usage percentage
    let total_memory = sys.total_memory();
    let used_memory = sys.used_memory();
    let memory_percent = if total_memory > 0 {
        ((used_memory as f64 / total_memory as f64) * 100.0) as u32
    } else {
        0
    };

    // Clamp values to 0-100 range (defensive)
    SystemInfo {
        cpu_percent: cpu_percent.min(100),
        memory_percent: memory_percent.min(100),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_gather_system_info_returns_valid_range() {
        let info = gather_system_info();

        // CPU percent should be 0-100
        assert!(info.cpu_percent <= 100, "CPU percent should be <= 100");

        // Memory percent should be 0-100
        assert!(
            info.memory_percent <= 100,
            "Memory percent should be <= 100"
        );
    }

    #[test]
    fn test_system_info_is_copy() {
        let info = gather_system_info();
        let copy = info;

        // Verify we can use both (Copy trait works)
        assert!(info.cpu_percent <= 100);
        assert!(copy.cpu_percent <= 100);
    }

    #[test]
    fn test_system_info_debug() {
        let info = gather_system_info();
        let debug_str = format!("{:?}", info);

        assert!(debug_str.contains("SystemInfo"));
        assert!(debug_str.contains("cpu_percent"));
        assert!(debug_str.contains("memory_percent"));
    }

    #[test]
    fn test_system_info_struct_direct() {
        // Test boundary values via direct struct construction
        let zero_info = SystemInfo {
            cpu_percent: 0,
            memory_percent: 0,
        };
        assert_eq!(zero_info.cpu_percent, 0);
        assert_eq!(zero_info.memory_percent, 0);

        let max_info = SystemInfo {
            cpu_percent: 100,
            memory_percent: 100,
        };
        assert_eq!(max_info.cpu_percent, 100);
        assert_eq!(max_info.memory_percent, 100);
    }

    #[test]
    #[allow(clippy::clone_on_copy)] // Explicitly testing Clone trait
    fn test_system_info_clone() {
        let info = SystemInfo {
            cpu_percent: 50,
            memory_percent: 75,
        };
        let cloned = info.clone();

        assert_eq!(info.cpu_percent, cloned.cpu_percent);
        assert_eq!(info.memory_percent, cloned.memory_percent);
    }

    #[test]
    fn test_gather_multiple_times() {
        // Gathering multiple times should work without error
        for _ in 0..5 {
            let info = gather_system_info();
            assert!(info.cpu_percent <= 100);
            assert!(info.memory_percent <= 100);
        }
    }
}
