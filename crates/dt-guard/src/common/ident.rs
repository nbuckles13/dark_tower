//! Identifier-boundary matching, POLICY-FREE: whether a word occurs in text
//! as a whole identifier (ASCII alphanumerics and `_`), not inside a longer
//! one. Shared by the proto guards and `common::kind_labels`, which before
//! this module each carried their own copy.

/// An identifier byte: ASCII alphanumeric or `_`.
#[must_use]
pub const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Whether `text` names `word` as a whole identifier: some occurrence is
/// neither preceded nor followed by an identifier byte.
#[must_use]
pub fn names_identifier(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(word).any(|(at, m)| {
        let before = at.checked_sub(1).and_then(|i| bytes.get(i)).copied();
        let after = bytes.get(at + m.len()).copied();
        !before.is_some_and(is_ident_byte) && !after.is_some_and(is_ident_byte)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_bytes_are_alnum_and_underscore() {
        assert!(is_ident_byte(b'a') && is_ident_byte(b'Z') && is_ident_byte(b'0'));
        assert!(is_ident_byte(b'_'));
        assert!(!is_ident_byte(b'.') && !is_ident_byte(b' ') && !is_ident_byte(b'{'));
    }

    #[test]
    fn whole_identifiers_only() {
        assert!(names_identifier(
            "google.protobuf.Any a = 1",
            "google.protobuf.Any"
        ));
        assert!(names_identifier("by (media_kind)", "media_kind"));
        assert!(names_identifier("media_kind", "media_kind"));
        assert!(!names_identifier(
            "google.protobuf.AnyThing",
            "google.protobuf.Any"
        ));
        assert!(!names_identifier("MyAny", "Any"));
        assert!(!names_identifier("media_kind_unsupported", "media_kind"));
        assert!(!names_identifier("x_media_kind", "media_kind"));
        // A later whole occurrence counts even after a non-whole one.
        assert!(names_identifier("media_kinds media_kind", "media_kind"));
        assert!(!names_identifier("", "media_kind"));
    }
}
