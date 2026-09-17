//! Manifest v1 serde types + YAML (de)serialization.
//!
//! Per-status field requirements (pending tasks must carry specialist,
//! prompt) are deliberately NOT encoded in the types: completed
//! tasks may appear as minimal stubs (`id` + `status` only), so every
//! per-status field is `Option<>` here and enforced in
//! [`crate::engine::validate_manifest`] and on `next`'s output path.
//!
//! NOTE: `validate_manifest` is no longer purely per-status. It also carries
//! one MANIFEST-LEVEL check — a manifest with zero tasks is rejected — which
//! is not status-aware and has no per-task analogue. See that function's own
//! comment for why. Stated here because this doc previously framed the
//! function as enforcing per-status requirements only, and that framing
//! stopped being true in the same commit that made the check load-bearing.
//!
//! # Why the `v1` marker is not bumped by schema changes
//!
//! [`MARKER`] is stamped unconditionally on every rewrite and nothing parses
//! a version out of it, so a bump could never carry migration semantics — a
//! v1 file would silently become v2-marked on the first `complete`. The
//! reason a required-field removal (`branch`, 2026-08-17) needs no bump is
//! that there is no version skew to support: one producer, all consumers
//! in-tree, and `preflight-story.sh` requires a `target/release/dt-story`
//! built from the same tree. Read "v1" as a discriminator for the block, not
//! as a compatibility promise the code is able to keep.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// First line inside the fenced manifest block; preserved on every rewrite.
pub const MARKER: &str = "# task-metadata (dt-story manifest v1)";

/// CANONICAL character class for a devloop-output slug. This constant is the
/// single source of truth for the class, and
/// `scripts/guards/simple/validate-slug-class-sync.sh` asserts that every
/// other site carries the identical literal.
///
/// The guard covers the THREE MANIFEST-SLUG CLASSES: this one,
/// `/close-story`'s read-time regex, and `run-story.sh`'s
/// `SLUG_CLASS_CANONICAL` write-time floor. It deliberately does NOT cover
/// `run-story.sh`'s `^[0-9A-Za-z._-]+$` RESUME floors, which are a separate,
/// WIDER class guarding shell command-line interpolation of an ephemeral
/// resume value — a different job on a different value. Unifying those
/// "upward" would silently widen the manifest class, which is the opposite of
/// the point. See the guard's own SCOPE comment for the authoritative list.
pub const SLUG_PATTERN: &str = "^[a-z0-9]+(-[a-z0-9]+)*$";

/// A devloop-output directory name (`docs/devloop-outputs/<slug>/`).
///
/// A newtype rather than a bare `String` so the character class is enforced
/// in the DESERIALIZER. That placement is the whole point: `next`,
/// `validate`, `list-tasks`, `preflight-story.sh`, the Layer-3 guard and
/// `/close-story` all inherit the floor from one definition, and a
/// hand-edited manifest cannot smuggle a value past a check applied only at
/// the clap boundary.
///
/// The class is the INTERSECTION of its consumers, not the union: it
/// excludes `.` (so no path component can be `..`) and cannot begin with `-`
/// (so the value cannot present as a flag where it is interpolated).
///
/// Deliberate blast radius: an invalid slug fails the WHOLE FILE to parse
/// (`next` exits 2 "malformed") rather than yielding a per-task
/// `validate_manifest` violation. That matches how illegal `Status` values
/// and unknown fields already behave here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Enforce [`SLUG_PATTERN`] by hand (no regex dependency in this crate).
    /// Kept deliberately literal so it reads as the pattern it implements:
    /// one or more `[a-z0-9]` segments joined by single `-`.
    fn is_valid(s: &str) -> bool {
        !s.is_empty()
            && s.split('-').all(|seg| {
                !seg.is_empty()
                    && seg
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
    }
}

impl TryFrom<String> for Slug {
    type Error = String;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        if Slug::is_valid(&value) {
            Ok(Slug(value))
        } else {
            Err(format!(
                "slug '{value}' does not match {SLUG_PATTERN} (lowercase alphanumeric segments joined by single hyphens)"
            ))
        }
    }
}

impl From<Slug> for String {
    fn from(slug: Slug) -> Self {
        slug.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Slug {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Slug::try_from(s.to_string())
    }
}

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

/// Gate-1 planning-round risk tier for a task (ADR-0037 §D2). `full` runs the
/// devloop's Gate-1 plan-panel round; `light` skips it (implementer plans
/// inline; Gate-3 and the full reviewer panel are unchanged — see the /devloop
/// SKILL, which is careful to keep this DISTINCT from `--light`'s panel cut).
///
/// This enum is the SINGLE SOURCE OF TRUTH for the tier value set. `rename_all
/// = "lowercase"` under the struct's `deny_unknown_fields` makes an illegal
/// tier fail YAML deserialization (the `Slug` placement principle: floor in the
/// deserializer, every consumer inherits it).
///
/// ADDING A THIRD TIER requires widening FOUR downstream copies of `{full,
/// light}` — deliberate copies, not a guarded mirror, because the fail-loud
/// property holds at every one (a forgotten site rejects the new value rather
/// than silently accepting it), so a comment is the right instrument, not a
/// sync guard:
///   1. `scripts/workflow/run-story.sh` — the `^(full|light)$` interpolation
///      floor (its own comment carries the full rationale);
///   2. `.claude/skills/devloop/SKILL.md` — §Arguments, the Step 1 parse (a
///      present-but-unrecognized value is a hard error), and the Step 5 gate;
///   3. `.claude/skills/user-story/SKILL.md` — Step 10.4's `--tier {full|light}`
///      and the full/light decision rule;
///   4. `docs/devloop-outputs/_template/main.md` — the `Tier` row + `Mode` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Full,
    Light,
}

impl Default for Tier {
    /// Absent `tier` ⇒ `Full`. This is the backward-compat contract: an
    /// existing manifest with no `tier` field parses as a full-tier task, and
    /// `skip_serializing_if = "Tier::is_full"` keeps it byte-identical on
    /// rewrite (a Full task never writes `tier: full`).
    fn default() -> Self {
        Tier::Full
    }
}

impl Tier {
    /// Skip-serialization predicate for [`Task::tier`] — a Full tier is the
    /// default and is never written, so existing (tier-less) manifests
    /// round-trip byte-identical.
    fn is_full(&self) -> bool {
        matches!(self, Tier::Full)
    }
}

// NB: no `Display` for `Tier` — the lowercase wire tokens have exactly ONE
// producer, serde `rename_all = "lowercase"` (read by run-story via `jq -r
// .tier`). A hand-written `Display` would be a second, unguarded copy of those
// tokens with no consumer today, free to drift from the serde token on a
// variant rename. Add one only WITH a test pinning it to the serde token, and
// only when a caller actually needs it (YAGNI).

/// One task entry. Field declaration order is the stable serialization
/// order for rewritten manifest blocks (matches the schema contract).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: u32,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specialist: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Devloop-output slug of the attempt that actually COMPLETED this task.
    ///
    /// LAST-WRITER-WINS, deliberately opposite to `commit` directly above,
    /// and the two are adjacent so the divergence cannot be read in
    /// isolation:
    ///
    /// * `commit` is FIRST-writer-wins (a differing value warns and is not
    ///   overwritten) because the runner declines to supply it at all — the
    ///   post-completion `git commit --amend` changes the sha, so any sha
    ///   recordable there is stale by construction. A `commit` value present
    ///   is therefore a human annotation, and overwriting it with a value
    ///   the runner itself calls unreliable would be a regression.
    /// * `slug` is LAST-writer-wins because the runner IS its authoritative
    ///   producer, computing it at the one moment the task has provably
    ///   committed AND passed its gates. There is exactly one correct answer
    ///   per completion, and refusing to overwrite would freeze a superseded
    ///   attempt's slug.
    ///
    /// Different provenance, different rule. Not an inconsistency to tidy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<Slug>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation: Option<String>,
    /// Idempotency key for programmatically-added tasks (e.g. audit
    /// remediation, and every task `/user-story` emits): `add-task` refuses
    /// to append a second task with the same tag. Absent only on manifests
    /// hand-authored before this was the emission route.
    ///
    /// IDEMPOTENT IN THE WEAK SENSE ONLY — "will not duplicate", NOT
    /// "converges to the supplied values". `engine::add_task` returns
    /// `Exists` BEFORE writing any field, so a re-run with corrected
    /// `specialist` / `prompt` / `deps` discards them. The
    /// gesture for a changed plan is to reset the manifest to a skeleton and
    /// re-emit, not to re-run `add-task` over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Gate-1 planning-round tier (ADR-0037 §D2). Appended at the END of the
    /// field list so existing fields' serialization order is undisturbed, and
    /// `skip_serializing_if = "Tier::is_full"` so a tier-less manifest
    /// round-trips byte-identical and a Full task never writes `tier: full`.
    /// Absent ⇒ Full (see [`Tier::default`]).
    #[serde(default, skip_serializing_if = "Tier::is_full")]
    pub tier: Tier,
    /// Stated reason a task was tiered `light` (ADR-0037 §D2). Required and
    /// non-empty (after trim) whenever `tier == Light` — enforced in
    /// [`crate::engine::validate_manifest`], the guardable form of "any
    /// planning skip carries a stated reason". `skip_serializing_if =
    /// "Option::is_none"` skips it only when ABSENT — it is keyed on presence,
    /// NOT on tier, so a task hand-edited from `light` back to `full` without
    /// clearing this field still serializes a stale `tier_reason:` (harmless:
    /// no gate or runner reads a Full task's reason; validation only requires a
    /// reason for Light). This is deliberately NOT normalized on write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_reason: Option<String>,
}

/// The embedded task manifest (v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub story: String,
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
    ///
    /// REFUSES to emit a body containing a markdown fence line. A prompt
    /// carrying a fenced code example would otherwise close the manifest
    /// block early, and the resulting truncation is SILENT: the surviving
    /// prefix is a well-formed block, deps point backwards so nothing
    /// dangles, and `validate_manifest` returns zero violations on a
    /// manifest that has lost its tail.
    ///
    /// This closes the WRITE path completely — `complete`, `escalate`,
    /// `add-task` and `next`'s reopen can no longer corrupt a story file
    /// they merely round-trip. It is the primary control for that class;
    /// `markdown::find_manifest_block`'s checks are the read-side net for
    /// files this function never saw.
    ///
    /// Matches ANY line whose trim starts with three backticks, not only a
    /// bare fence: a language-tagged fence is equally fatal (it is swallowed
    /// as content while the example's closing fence ends the block), and
    /// CommonMark cannot nest a 3-backtick fence inside a 3-backtick block
    /// regardless, so such a prompt also breaks how the file renders.
    ///
    /// EXPRESSIVENESS LIMIT, stated rather than left to an error message:
    /// manifest prompts can never contain fenced code examples. Inline
    /// single-backtick spans are fine. An escape hatch exists and was
    /// considered — opening the manifest with a 4-backtick fence would let
    /// CommonMark contain 3-backtick fences — but the fence lines sit
    /// outside the span `replace_manifest_block` rewrites, so it is a hand
    /// edit of every story file plus a fence-length rule in the parser.
    pub fn to_block_body(&self) -> Result<String> {
        let yaml = serde_norway::to_string(self).context("manifest YAML serialization failed")?;
        for (idx, line) in yaml.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                bail!(
                    "refusing to write manifest: serialized line {} is a markdown fence ({:?}). \
                     A fence inside a manifest block closes it early and SILENTLY truncates the \
                     manifest — the surviving prefix still validates. Manifest prompts cannot \
                     contain fenced code examples; use inline `backtick spans` instead.",
                    idx + 1,
                    line.trim()
                );
            }
        }
        Ok(format!("{MARKER}\n{yaml}"))
    }

    pub fn task(&self, id: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn task_mut(&mut self, id: u32) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }
}
