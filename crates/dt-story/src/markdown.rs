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

/// Find the single manifest block: a ```` ```yaml ```` fence whose first
/// content line is the v1 marker comment. Errors if none or more than one
/// exists, or if the manifest fence is unterminated.
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

    if let Some(fence) = open {
        if fence.is_yaml && fence.is_manifest == Some(true) {
            bail!("manifest block is not terminated by a closing ``` fence");
        }
    }

    match blocks.len() {
        1 => blocks
            .pop()
            .ok_or_else(|| anyhow::anyhow!("manifest block vanished (internal error)")),
        0 => bail!(
            "no manifest block found (expected one ```yaml fence whose first line is '{MARKER}')"
        ),
        n => bail!("found {n} manifest blocks; exactly one is required"),
    }
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
        let text = story("story: s\nbranch: b\ntasks: []\n");
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
        let one = story("story: s\nbranch: b\ntasks: []\n");
        let text = format!("{one}\n{one}");
        let err = find_manifest_block(&text).expect_err("must fail with two blocks");
        assert!(err.to_string().contains("2 manifest blocks"));
    }
}
