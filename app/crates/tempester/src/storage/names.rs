//! File names the app writes or reads: always a plain name inside one folder, never a path.

const MAX_STEM: usize = 80;

/// A safe file stem made of the user's text: letters, digits, space and `-_.()` are kept, anything else becomes
/// `_`; leading dots and surrounding spaces go; at most 80 characters. `None` when nothing is left.
pub fn sanitize_stem(text: &str) -> Option<String> {
    let kept: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " -_.()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(MAX_STEM)
        .collect();
    let trimmed = kept.trim().trim_start_matches('.').trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// True for a name that is a single path component: no separator, no `..`, not hidden, not empty.
pub fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0', ':'])
        && name.len() <= 255
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_stem_when_plain_name_returns_it_unchanged() {
        assert_eq!(
            sanitize_stem("Kick Tight (v2)").as_deref(),
            Some("Kick Tight (v2)")
        );
    }

    #[test]
    fn test_sanitize_stem_when_separators_and_specials_returns_underscores() {
        assert_eq!(sanitize_stem("../a/b:c*d").as_deref(), Some("_a_b_c_d"));
    }

    #[test]
    fn test_sanitize_stem_when_leading_dots_and_spaces_returns_trimmed() {
        assert_eq!(sanitize_stem("  ..hidden ").as_deref(), Some("hidden"));
    }

    #[test]
    fn test_sanitize_stem_when_only_dots_or_empty_returns_none() {
        assert_eq!(sanitize_stem("..."), None);
        assert_eq!(sanitize_stem("   "), None);
        assert_eq!(sanitize_stem(""), None);
    }

    #[test]
    fn test_sanitize_stem_when_very_long_returns_at_most_80_characters() {
        assert_eq!(sanitize_stem(&"a".repeat(300)).map(|s| s.len()), Some(80));
    }

    #[test]
    fn test_sanitize_stem_when_non_ascii_returns_underscore() {
        assert_eq!(
            sanitize_stem("Grosse caisse é").as_deref(),
            Some("Grosse caisse _")
        );
    }

    #[test]
    fn test_is_plain_file_name_when_traversal_or_hidden_returns_false() {
        for name in [
            "", "..", "../x.syx", "a/b.syx", "a\\b.syx", ".hidden", "c:x", "a\0b",
        ] {
            assert!(!is_plain_file_name(name), "{name:?}");
        }
    }

    #[test]
    fn test_is_plain_file_name_when_ordinary_returns_true() {
        assert!(is_plain_file_name("Kick Tight.syx"));
    }
}
