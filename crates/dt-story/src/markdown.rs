//! Locate and rewrite the single fenced manifest block in a story file.
//!
//! Mutations replace ONLY the bytes between the opening ```` ```yaml ````
//! fence line and the closing ```` ``` ```` fence line; every other byte of
//! the markdown file is preserved exactly.

use crate::manifest::MARKER;
use anyhow::{bail, Result};

/// Byte span of a manifest block's content (marker line through the last
/// content byte, exclusive of both fence lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSpan {
    /// Byte offset of the first content byte (start of the marker line).
    pub content_start: usize,
    /// Byte offset of the start of the closing ``` fence line.
    pub content_end: usize,
}

/// State for the fence currently being scanned.
struct OpenFence {
    is_yaml: bool,
    content_start: usize,
    /// `None` until the first content line classifies the block.
    is_manifest: Option<bool>,
}

/// An orphaned task-entry line outside the manifest block is the signature
/// of a SILENT TRUNCATION: a prompt carrying a markdown fence closed the
/// block early, so the manifest's tail now sits in the document as prose.
///
/// THE REGEX FORM IS LOAD-BEARING. It tolerates LEADING WHITESPACE because
/// hand-authored manifests indent entries under `tasks:` — `serde_norway`
/// emits at column 0 only because it is the machine writer, and a
/// column-0-anchored pattern was MEASURED to find zero matches on a real
/// truncated fixture. It also requires the line to END after the id, so a
/// story file merely DISCUSSING `- id: 3 (see above)` in prose is not a
/// match; without that anchor this hard error would red Layer 3 on ordinary
/// documentation. Both halves were measured as free (identical match counts
/// tree-wide) and both are needed. The narrow form looks more correct and is
/// not: do not "tighten" this back to `^- id:`.
fn is_orphaned_task_entry(line: &str) -> bool {
    let rest = line.trim_start();
    let Some(rest) = rest.strip_prefix('-') else {
        return false;
    };
    // `-` must be followed by at least one space (a YAML sequence dash).
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return false;
    }
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix("id:") else {
        return false;
    };
    let rest = rest.trim_start();
    let digits_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digits_end == 0 {
        return false;
    }
    rest[digits_end..].trim().is_empty()
}

/// Find the single manifest block: a ```` ```yaml ```` fence whose first
/// content line is the v1 marker comment. Errors if none or more than one
/// exists, if any fence in the file is unterminated, or if manifest-shaped
/// content is found outside the located block.
///
/// THE TWO INTEGRITY CHECKS LIVE HERE, NOT IN THE CALLERS, because this is
/// the common ancestor of every verb: `main.rs::load` calls it, and
/// `engine::validate_story` calls it DIRECTLY — `validate` is the one verb
/// that does not go through `load`, and it is precisely the verb behind
/// `validate-story-manifest.sh` (Layer 3 / CI) and `preflight-story.sh`.
/// A check placed caller-side would therefore be invisible to both gates and
/// would fire only once the runner was already executing the story.
pub fn find_manifest_block(text: &str) -> Result<BlockSpan> {
    let mut blocks: Vec<BlockSpan> = Vec::new();
    let mut open: Option<OpenFence> = None;
    let mut offset = 0usize;

    for raw in text.split_inclusive('\n') {
        let trimmed = raw.trim();
        match open.as_mut() {
            None => {
                if let Some(info) = trimmed.strip_prefix("```") {
                    open = Some(OpenFence {
                        is_yaml: info.trim() == "yaml",
                        content_start: offset + raw.len(),
                        is_manifest: None,
                    });
                }
            }
            Some(fence) => {
                if trimmed == "```" {
                    if fence.is_yaml && fence.is_manifest == Some(true) {
                        blocks.push(BlockSpan {
                            content_start: fence.content_start,
                            content_end: offset,
                        });
                    }
                    open = None;
                } else if fence.is_manifest.is_none() {
                    fence.is_manifest = Some(fence.is_yaml && trimmed == MARKER);
                }
            }
        }
        offset += raw.len();
    }

    // ANY unterminated fence is an error, not only an unterminated MANIFEST
    // fence. When a prompt's fenced example closes the manifest early, the
    // manifest's own closing fence then OPENS a new one that never closes —
    // so this catches that shape. Scoped honestly: it catches it only when
    // the fence lines remaining after the truncation point leave one open,
    // which depends on whether the model tagged its fence and how many
    // fenced blocks follow. It is markdown well-formedness, not a detector
    // for the truncation class; the orphan scan below is that.
    //
    // Known, deliberate consequence: a 4-backtick fence is a hard failure of
    // the whole file (it satisfies the ``` prefix so it opens a fence, but
    // can never equal a bare ``` so it never closes). Zero occurrences in
    // story files; nested fenced examples are unsupported by this parser.
    if open.is_some() {
        bail!(
            "unterminated ``` fence: the file has an odd number of fence lines. If a manifest \
             prompt contains a fenced code example, it closed the manifest block early and \
             SILENTLY truncated it. Manifest prompts cannot contain fenced code examples \
             (nested/4-backtick fences are also unsupported)."
        );
    }

    let span = match blocks.len() {
        1 => blocks
            .pop()
            .ok_or_else(|| anyhow::anyhow!("manifest block vanished (internal error)"))?,
        0 => bail!(
            "no manifest block found (expected one ```yaml fence whose first line is '{MARKER}')"
        ),
        n => bail!("found {n} manifest blocks; exactly one is required"),
    };

    // ORPHAN SCAN — the read-side detector for silent truncation, and the
    // one that does not depend on fence parity. A truncation leaves the
    // manifest's tail in the document as prose, full of task-entry lines.
    // HARD ERROR, not a warning: `engine::validate_story` turns an Err from
    // this function into a violation with the right exit code, whereas it
    // collects violations and has no warning channel at all — a WARN here
    // would be silently swallowed by the very gate that needs it.
    let mut offset = 0usize;
    for raw in text.split_inclusive('\n') {
        let line_end = offset + raw.len();
        let outside = line_end <= span.content_start || offset >= span.content_end;
        if outside && is_orphaned_task_entry(raw) {
            bail!(
                "manifest-shaped line outside the manifest block: {:?}. This is the signature of \
                 a SILENTLY TRUNCATED manifest — a prompt containing a markdown fence closed the \
                 block early, leaving the remaining tasks in the document as prose. The surviving \
                 block still parses and still validates, so this check is the only thing that \
                 sees the loss.",
                raw.trim()
            );
        }
        offset = line_end;
    }

    Ok(span)
}

/// Borrow the YAML content (marker line included) of a located block.
pub fn manifest_yaml<'a>(text: &'a str, span: &BlockSpan) -> Result<&'a str> {
    text.get(span.content_start..span.content_end)
        .ok_or_else(|| anyhow::anyhow!("manifest block span out of bounds (internal error)"))
}

/// Rebuild the story file with `new_body` substituted for the block's
/// content. Everything outside `span` is preserved byte-for-byte.
pub fn replace_manifest_block(text: &str, span: &BlockSpan, new_body: &str) -> Result<String> {
    let prefix = text
        .get(..span.content_start)
        .ok_or_else(|| anyhow::anyhow!("manifest block span out of bounds (internal error)"))?;
    let suffix = text
        .get(span.content_end..)
        .ok_or_else(|| anyhow::anyhow!("manifest block span out of bounds (internal error)"))?;
    Ok(format!("{prefix}{new_body}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(body: &str) -> String {
        format!("# Title\n\nprose\n\n```yaml\n{MARKER}\n{body}```\n\nafter\n")
    }

    #[test]
    fn finds_single_block_and_round_trips_content() {
        let text = story("story: s\ntasks: []\n");
        let span = find_manifest_block(&text).expect("span");
        let yaml = manifest_yaml(&text, &span).expect("yaml");
        assert!(yaml.starts_with(MARKER));
        assert!(yaml.ends_with("tasks: []\n"));
        let rebuilt =
            replace_manifest_block(&text, &span, yaml).expect("replace with identical body");
        assert_eq!(rebuilt, text, "identity replacement must be byte-exact");
    }

    #[test]
    fn missing_block_is_an_error() {
        let err = find_manifest_block("# Title\n\nno manifest here\n")
            .expect_err("must fail without a manifest block");
        assert!(err.to_string().contains("no manifest block"));
    }

    #[test]
    fn plain_yaml_fence_without_marker_is_not_a_manifest() {
        let text = "```yaml\nkey: value\n```\n";
        assert!(find_manifest_block(text).is_err());
    }

    #[test]
    fn two_blocks_is_an_error() {
        let one = story("story: s\ntasks: []\n");
        let text = format!("{one}\n{one}");
        let err = find_manifest_block(&text).expect_err("must fail with two blocks");
        assert!(err.to_string().contains("2 manifest blocks"));
    }
}
