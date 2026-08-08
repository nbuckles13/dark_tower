//! Manifest v1 serde types + YAML (de)serialization.
//!
//! Per-status field requirements (pending tasks must carry specialist,
//! env_tests, prompt) are deliberately NOT encoded in the types: completed
//! tasks may appear as minimal stubs (`id` + `status` only), so every
//! per-status field is `Option<>` here and enforced in
//! [`crate::engine::validate_manifest`] and on `next`'s output path.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// First line inside the fenced manifest block; preserved on every rewrite.
pub const MARKER: &str = "# task-metadata (dt-story manifest v1)";

/// Task lifecycle status. Illegal statuses fail YAML deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Completed,
    Escalated,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Status::Pending => "pending",
            Status::Completed => "completed",
            Status::Escalated => "escalated",
        };
        f.write_str(s)
    }
}

/// One task entry. Field declaration order is the stable serialization
/// order for rewritten manifest blocks (matches the schema contract).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: u32,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_tests: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation: Option<String>,
    /// Idempotency key for programmatically-added tasks (e.g. audit
    /// remediation): `add-task` refuses to append a second task with the
    /// same tag. Absent on hand-authored tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// The embedded task manifest (v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub story: String,
    pub branch: String,
    pub tasks: Vec<Task>,
}

impl Manifest {
    /// Parse the YAML body of a manifest block (comments are ignored by
    /// the YAML parser, so the marker line may be included).
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_norway::from_str(yaml).context("manifest YAML failed to parse")
    }

    /// Serialize back to a fenced-block body: the `# task-metadata` marker
    /// comment first, then YAML in schema field order. Multiline prompts
    /// round-trip as YAML block scalars via the emitter's literal style.
    pub fn to_block_body(&self) -> Result<String> {
        let yaml = serde_norway::to_string(self).context("manifest YAML serialization failed")?;
        Ok(format!("{MARKER}\n{yaml}"))
    }

    pub fn task(&self, id: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn task_mut(&mut self, id: u32) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }
}
