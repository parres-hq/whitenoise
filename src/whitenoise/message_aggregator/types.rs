use nostr_mls::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::nostr_manager::parser::SerializableToken;
pub type MlsMessage = nostr_mls::prelude::message_types::Message;

/// Represents an aggregated chat message ready for frontend display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// Unique identifier of the message
    pub id: String,

    /// Public key of the message author
    pub author: PublicKey,

    /// Message content (empty if deleted)
    pub content: String,

    /// Timestamp when the message was created
    pub created_at: Timestamp,

    /// Tags from the original Nostr event
    pub tags: Tags,

    /// Whether this message is a reply to another message
    pub is_reply: bool,

    /// ID of the message this is replying to (if is_reply is true)
    pub reply_to_id: Option<String>,

    /// Whether this message has been deleted
    pub is_deleted: bool,

    /// Parsed tokens from the message content (mentions, hashtags, etc.)
    pub content_tokens: Vec<SerializableToken>,

    /// Aggregated reactions on this message
    pub reactions: ReactionSummary,

    /// The kind of the original Nostr event
    pub kind: u16,
}

/// Summary of reactions on a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReactionSummary {
    /// Map of emoji to reaction details
    pub by_emoji: HashMap<String, EmojiReaction>,

    /// List of all users who have reacted and with what
    pub user_reactions: Vec<UserReaction>,
}

/// Details for a specific emoji reaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmojiReaction {
    /// The emoji or reaction symbol
    pub emoji: String,

    /// Count of users who used this reaction
    pub count: usize,

    /// List of users who used this reaction
    pub users: Vec<PublicKey>,
}

/// Individual user's reaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserReaction {
    /// User who made the reaction
    pub user: PublicKey,

    /// The emoji they reacted with
    pub emoji: String,

    /// Timestamp of the reaction
    pub created_at: Timestamp,
}

/// Internal type for tracking unresolved references
#[derive(Debug, Clone)]
pub(crate) struct UnresolvedMessage {
    pub message: MlsMessage,
    pub retry_count: u8,
    pub reason: UnresolvedReason,
}

/// Reasons why messages might remain unresolved during processing
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub(crate) enum UnresolvedReason {
    #[allow(dead_code)] // Future: For reply processing
    ReplyToMissing(String), // Missing parent message ID
    ReactionToMissing(String),   // Missing target message ID
    DeleteTargetMissing(String), // Missing delete target ID
}

/// Configuration for the message aggregator
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AggregatorConfig {
    /// Maximum number of retry attempts for unresolved messages
    pub max_retry_attempts: u8,

    /// Whether to normalize emoji (treat skin tone variants as same base emoji)
    pub normalize_emoji: bool,

    /// Whether to enable detailed logging of processing steps
    pub enable_debug_logging: bool,
}

impl Default for AggregatorConfig {
    fn default() -> Self {
        Self {
            max_retry_attempts: 3,
            normalize_emoji: true,
            enable_debug_logging: false,
        }
    }
}

/// Statistics about a group's message processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupStatistics {
    pub message_count: usize,
    pub reaction_count: usize,
    pub deleted_message_count: usize,
    pub memory_usage_bytes: usize,
    pub last_processed_at: Option<Timestamp>,
}

/// Errors that can occur during message processing
#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("Invalid reaction content")]
    InvalidReaction,

    #[error("Missing required e-tag in message")]
    MissingETag,

    #[error("Invalid tag format")]
    InvalidTag,

    #[error("Invalid timestamp")]
    InvalidTimestamp,

    #[error("Failed to fetch messages from nostr_mls: {0}")]
    FetchFailed(String),

    #[error("Internal processing error: {0}")]
    Internal(String),
}
