use super::types::ProcessingError;

/// Validates and normalizes reaction content
pub fn validate_and_normalize_reaction(
    content: &str,
    normalize_emoji: bool,
) -> Result<String, ProcessingError> {
    match content {
        "+" => Ok("👍".to_string()), // Normalize to thumbs up
        "-" => Ok("👎".to_string()), // Normalize to thumbs down
        emoji if is_valid_emoji(emoji) => {
            if normalize_emoji {
                Ok(normalize_emoji_string(emoji))
            } else {
                Ok(emoji.to_string())
            }
        }
        _ => {
            tracing::warn!("Invalid reaction content: {}", content);
            Err(ProcessingError::InvalidReaction)
        }
    }
}

/// Checks if a string is a valid emoji or emoji sequence
pub fn is_valid_emoji(s: &str) -> bool {
    // Simple validation - check if the string contains valid unicode emoji ranges
    // This is a basic implementation that could be enhanced with a proper emoji library
    if s.is_empty() || s.len() > 50 {
        return false;
    }

    // Check for common emoji patterns
    for ch in s.chars() {
        if is_emoji_char(ch) {
            return true;
        }
    }

    // Also allow common reaction strings
    matches!(
        s,
        "👍" | "👎" | "❤️" | "😀" | "😊" | "😂" | "🔥" | "✨" | "🎉" | "👏"
    )
}

/// Checks if a character is in emoji unicode ranges
fn is_emoji_char(ch: char) -> bool {
    let code = ch as u32;

    // Basic emoji ranges (simplified)
    matches!(code,
        0x1F600..=0x1F64F | // Emoticons
        0x1F300..=0x1F5FF | // Misc Symbols and Pictographs
        0x1F680..=0x1F6FF | // Transport and Map
        0x1F1E0..=0x1F1FF | // Regional indicators
        0x2600..=0x26FF |   // Misc symbols
        0x2700..=0x27BF |   // Dingbats
        0xFE00..=0xFE0F |   // Variation selectors
        0x200D |            // Zero width joiner
        0x20E3              // Combining enclosing keycap
    )
}

/// Normalizes emoji by removing skin tone modifiers and variations
pub fn normalize_emoji_string(emoji: &str) -> String {
    if !emoji.contains('\u{1F3FB}')
        && !emoji.contains('\u{1F3FC}')
        && !emoji.contains('\u{1F3FD}')
        && !emoji.contains('\u{1F3FE}')
        && !emoji.contains('\u{1F3FF}')
        && !emoji.contains('\u{FE0F}')
    {
        return emoji.to_string();
    }

    // Remove skin tone modifiers and variation selectors using replace_all approach
    let chars_to_remove = [
        '\u{1F3FB}',
        '\u{1F3FC}',
        '\u{1F3FD}',
        '\u{1F3FE}',
        '\u{1F3FF}',
        '\u{FE0F}',
    ];
    emoji
        .chars()
        .filter(|c| !chars_to_remove.contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_plus_minus() {
        assert_eq!(validate_and_normalize_reaction("+", true).unwrap(), "👍");
        assert_eq!(validate_and_normalize_reaction("-", true).unwrap(), "👎");
    }

    #[test]
    fn test_valid_emoji() {
        assert!(is_valid_emoji("👍"));
        assert!(is_valid_emoji("😀"));
        assert!(is_valid_emoji("❤️"));
        assert!(!is_valid_emoji(""));
        assert!(!is_valid_emoji("not an emoji"));
    }

    #[test]
    fn test_normalize_emoji() {
        // Should remove skin tone modifiers
        assert_eq!(normalize_emoji_string("👋🏽"), "👋");
        assert_eq!(normalize_emoji_string("👍🏿"), "👍");

        // Should handle no modifiers
        assert_eq!(normalize_emoji_string("😀"), "😀");

        // Should remove variation selector
        assert_eq!(normalize_emoji_string("❤️"), "❤");
    }

    #[test]
    fn test_invalid_reactions() {
        assert!(validate_and_normalize_reaction("invalid", true).is_err());
        assert!(validate_and_normalize_reaction("", true).is_err());
        assert!(validate_and_normalize_reaction(
            "way too long reaction string that exceeds limits",
            true
        )
        .is_err());
    }
}
