//! Redis client with fencing token support (ADR-0023 Section 3 & 6).
//!
//! This module provides:
//! - `FencedRedisClient` - Redis client with fencing token validation
//! - Lua scripts for atomic fenced operations
//!
//! # Fencing Token (ADR-0023 Section 3)
//!
//! Fencing tokens prevent split-brain during MC failover:
//! - Each meeting has a monotonically increasing generation number
//! - All writes include the generation as a fencing token
//! - Writes with stale generations are rejected
//!
//! # State Storage (ADR-0023 Section 6)
//!
//! Meeting state in Redis:
//! - `meeting:{id}:generation` - Current fencing generation
//! - `meeting:{id}:mh` - MH assignment data (JSON)
//! - `meeting:{id}:participants` - Participant list (ZSET by join time)
//! - `meeting:{id}:state` - Meeting metadata (HASH)

pub mod client;
pub mod lua_scripts;

pub use client::FencedRedisClient;
