//! This module contains the logic for parsing Nostr events into tokens.

use nostr::parser::{NostrParser, Token};
use serde::{Deserialize, Serialize};

/// Serializable Token
/// This is a parallel of the `Token` enum from the `nostr` crate, modified so that we can serialize it for use in commands and the DB
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SerializableToken {
    /// Nostr URI converted to a string
    Nostr(String),
    /// Url converted to a string
    Url(String),
    /// Hashtag
    Hashtag(String),
    /// Other text
    ///
    /// Spaces at the beginning or end of a text are parsed as [`Token::Whitespace`].
    Text(String),
    /// Line break
    LineBreak,
    /// A whitespace
    Whitespace,
}

// We use From instead of TryFrom because we want to show an error if the underlying token enum changes.
impl<'a> From<Token<'a>> for SerializableToken {
    fn from(value: Token<'a>) -> Self {
        match value {
            Token::Nostr(n) => SerializableToken::Nostr(n.to_nostr_uri().unwrap()),
            Token::Url(u) => SerializableToken::Url(u.to_string()),
            Token::Hashtag(h) => SerializableToken::Hashtag(h.to_string()),
            Token::Text(t) => SerializableToken::Text(t.to_string()),
            Token::LineBreak => SerializableToken::LineBreak,
            Token::Whitespace => SerializableToken::Whitespace,
        }
    }
}

/// Parses a string into a vector of serializable tokens.
///
/// This function takes a string content and returns a vector of `SerializableToken`s,
/// which can be used for database storage or frontend communication.
///
/// # Arguments
/// * `content` - The string content to parse
///
/// # Returns
/// A vector of `SerializableToken`s representing the parsed content
pub fn parse(content: &str) -> Vec<SerializableToken> {
    let parser = NostrParser::new();
    parser.parse(content).map(SerializableToken::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_text() {
        let content = "Hello, world!";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![SerializableToken::Text("Hello, world!".to_string())]
        );
    }

    #[test]
    fn test_parse_with_whitespace() {
        let content = "  Hello  world  ";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![
                SerializableToken::Whitespace,
                SerializableToken::Whitespace,
                SerializableToken::Text("Hello  world ".to_string()),
                SerializableToken::Whitespace,
            ]
        );
    }

    #[test]
    fn test_parse_with_line_breaks() {
        let content = "Hello\nworld";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![
                SerializableToken::Text("Hello".to_string()),
                SerializableToken::LineBreak,
                SerializableToken::Text("world".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_with_hashtags() {
        let content = "Hello #nostr world";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![
                SerializableToken::Text("Hello".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Hashtag("nostr".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Text("world".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_with_nostr_uri() {
        let content =
            "Check out nostr:npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![
                SerializableToken::Text("Check out".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Nostr(
                    "nostr:npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft"
                        .to_string()
                ),
            ]
        );
    }

    #[test]
    fn test_parse_with_url() {
        let content = "Visit https://example.com";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![
                SerializableToken::Text("Visit".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Url("https://example.com/".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_empty_string() {
        let content = "";
        let tokens = parse(content);
        assert_eq!(tokens, vec![]);
    }

    #[test]
    fn test_parse_complex_content() {
        let content = "Hello #nostr! Check out https://example.com and nostr:npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft\n\nBye!";
        let tokens = parse(content);
        assert_eq!(
            tokens,
            vec![
                SerializableToken::Text("Hello".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Hashtag("nostr".to_string()),
                SerializableToken::Text("! Check out".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Url("https://example.com/".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Text("and".to_string()),
                SerializableToken::Whitespace,
                SerializableToken::Nostr(
                    "nostr:npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft"
                        .to_string()
                ),
                SerializableToken::LineBreak,
                SerializableToken::LineBreak,
                SerializableToken::Text("Bye!".to_string()),
            ]
        );
    }
}
