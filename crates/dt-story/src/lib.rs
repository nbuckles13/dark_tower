//! dt-story library — pure logic re-exported for binary + integration tests.
//!
//! State engine for the story runner: owns all reads and writes of a user
//! story's embedded task manifest (fenced ```yaml block whose first line is
//! `# task-metadata (dt-story manifest v1)`) so bash never parses YAML.
//!
//! * [`manifest`] — serde types + YAML (de)serialization for manifest v1.
//! * [`markdown`] — locate/replace the single fenced manifest block while
//!   preserving every other byte of the story file.
//! * [`engine`] — `next`/`complete`/`escalate`/`validate` state transitions
//!   and the atomic same-directory write used by mutations.

pub mod engine;
pub mod manifest;
pub mod markdown;
