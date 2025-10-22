//! Core message processing logic
//!
//! This module implements the stateless message aggregation algorithm that transforms
//! raw Nostr MLS messages into structured ChatMessage objects.

use nostr_sdk::prelude::*;
use std::collections::HashMap;

use super::reaction_handler;
use super::types::{AggregatorConfig, ChatMessage, ProcessingError};
use crate::nostr_manager::parser::Parser;
use crate::whitenoise::media_files::MediaFile;
use mdk_core::prelude::message_types::Message;

/// Process raw messages into aggregated chat messages
pub async fn process_messages(
    messages: Vec<Message>,
    parser: &dyn Parser,
    config: &AggregatorConfig,
    media_files: Vec<MediaFile>,
) -> Result<Vec<ChatMessage>, ProcessingError> {
    if messages.is_empty() {
        return Ok(Vec::new());
    }

    // Build internal lookup map for O(1) access during processing
    let media_files_map: HashMap<String, MediaFile> = media_files
        .into_iter()
        .map(|mf| (hex::encode(&mf.file_hash), mf))
        .collect();

    let mut processed_messages = HashMap::new();
    let mut orphaned_messages = Vec::new();

    let mut sorted_messages = messages;
    sorted_messages.sort_unstable_by(|a, b| a.created_at.cmp(&b.created_at));

    if config.enable_debug_logging {
        tracing::debug!(
            "Processing {} messages chronologically",
            sorted_messages.len()
        );
    }

    // Pass 1: Process all messages in chronological order
    for message in &sorted_messages {
        match message.kind {
            Kind::Custom(9) => {
                if let Ok(chat_message) =
                    process_regular_message(message, parser, &media_files_map).await
                {
                    processed_messages.insert(message.id.to_string(), chat_message);
                } else if config.enable_debug_logging {
                    tracing::warn!("Failed to process regular message: {}", message.id);
                }
            }
            Kind::Reaction => {
                if reaction_handler::process_reaction(message, &mut processed_messages, config)
                    .is_err()
                {
                    orphaned_messages.push(message);
                }
            }
            Kind::EventDeletion => {
                if !try_process_deletion(message, &mut processed_messages) {
                    orphaned_messages.push(message);
                }
            }
            _ => continue,
        }
    }

    if config.enable_debug_logging {
        tracing::debug!(
            "Pass 1 complete: {} messages processed, {} orphaned",
            processed_messages.len(),
            orphaned_messages.len()
        );
    }

    // Pass 2: Process orphaned messages (their targets should exist now)
    for message in orphaned_messages {
        match message.kind {
            Kind::Reaction => {
                if reaction_handler::process_reaction(message, &mut processed_messages, config)
                    .is_err()
                    && config.enable_debug_logging
                {
                    tracing::warn!(
                        "Reaction {} references non-existent message, ignoring",
                        message.id
                    );
                }
            }
            Kind::EventDeletion => {
                if !try_process_deletion(message, &mut processed_messages)
                    && config.enable_debug_logging
                {
                    tracing::warn!(
                        "Deletion {} references non-existent message, ignoring",
                        message.id
                    );
                }
            }
            _ => {}
        }
    }

    let mut result: Vec<ChatMessage> = processed_messages.into_values().collect();
    result.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    if config.enable_debug_logging {
        tracing::debug!("Returning {} aggregated messages", result.len());
    }

    Ok(result)
}

/// Process a regular chat message (kind 9)
async fn process_regular_message(
    message: &Message,
    parser: &dyn Parser,
    media_files_map: &HashMap<String, MediaFile>,
) -> Result<ChatMessage, ProcessingError> {
    // Parse content tokens
    let content_tokens = match parser.parse(&message.content) {
        Ok(tokens) => tokens,
        Err(e) => {
            tracing::warn!("Failed to parse message content: {}", e);
            Vec::new() // Use empty tokens if parsing fails
        }
    };

    // Check if this is a reply (has e-tag)
    let reply_to_id = extract_reply_info(&message.tags);
    let is_reply = reply_to_id.is_some();

    // Extract media attachments
    let media_attachments = extract_media_attachments(&message.tags, media_files_map);

    Ok(ChatMessage {
        id: message.id.to_string(),
        author: message.pubkey,
        content: message.content.clone(),
        created_at: message.created_at,
        tags: message.tags.clone(),
        is_reply,
        reply_to_id,
        is_deleted: false,
        content_tokens,
        reactions: Default::default(),
        kind: u16::from(message.kind),
        media_attachments,
    })
}

/// Extract reply information from message tags
fn extract_reply_info(tags: &Tags) -> Option<String> {
    // Look for e-tags indicating this is a reply
    let e_tags: Vec<_> = tags
        .iter()
        .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
        .collect();

    if e_tags.is_empty() {
        return None;
    }

    // Use the last e-tag as per Nostr convention (NIP-10)
    if let Some(last_e_tag) = e_tags.last()
        && let Some(event_id) = last_e_tag.content()
    {
        return Some(event_id.to_string());
    }

    None
}

/// Try to process deletion message (kind 5)
/// Returns true if at least one target was found and deleted, false otherwise
fn try_process_deletion(
    message: &Message,
    processed_messages: &mut HashMap<String, ChatMessage>,
) -> bool {
    let target_ids = extract_deletion_target_ids(&message.tags);
    let mut any_processed = false;

    for target_id in target_ids {
        if let Some(target_message) = processed_messages.get_mut(&target_id) {
            target_message.is_deleted = true;
            target_message.content = String::new();
            any_processed = true;
        }
    }

    any_processed
}

/// Extract target message IDs from deletion event e-tags
pub(crate) fn extract_deletion_target_ids(tags: &Tags) -> Vec<String> {
    tags.iter()
        .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
        .filter_map(|tag| tag.content().map(|s| s.to_string()))
        .collect()
}

/// Extract media file hashes from message imeta tags (MIP-04)
///
/// Returns a vector of unique file hashes found in the message tags.
/// Per MIP-04, imeta tags have format: ["imeta", "url <blossom_url>", "x <hash>", "m <mime_type>", ...]
fn extract_media_hashes(tags: &Tags) -> Vec<String> {
    let mut hashes = Vec::new();

    for tag in tags.iter() {
        if tag.kind() == TagKind::Custom("imeta".into()) {
            // Tag format: ["imeta", "url ...", "x <hash>", "m <mime>", ...]
            // Iterate through tag parameters looking for "x" parameter
            // Skip first element (tag name "imeta") by using tag.content() for second element,
            // then check remaining elements by converting tag to_vec and iterating
            let tag_vec = tag.clone().to_vec();
            for value in tag_vec.iter().skip(1) {
                // Look for "x" parameter which contains the hex-encoded hash
                if let Some(hash_str) = value.strip_prefix("x ") {
                    // Validate it's a 64-character hex string (32 bytes)
                    if hash_str.len() == 64 && hash_str.chars().all(|c| c.is_ascii_hexdigit()) {
                        hashes.push(hash_str.to_string());
                    }
                }
            }
        }
    }

    hashes
}

/// Extract media attachments from a message by matching hashes from imeta tags
///
/// Extracts media hashes from the message tags and looks them up in the provided map.
/// Returns a Vec of MediaFile records that were found.
fn extract_media_attachments(
    tags: &Tags,
    media_files_map: &HashMap<String, MediaFile>,
) -> Vec<MediaFile> {
    let media_hashes = extract_media_hashes(tags);
    let mut media_attachments = Vec::new();

    for hash in media_hashes {
        if let Some(media_file) = media_files_map.get(&hash) {
            media_attachments.push(media_file.clone());
        }
    }

    media_attachments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nostr_manager::parser::MockParser;

    // Test the pure logic functions that don't require complex Message structs

    #[test]
    fn test_extract_reply_info() {
        // Test with e-tag
        let mut tags = Tags::new();
        tags.push(Tag::parse(vec!["e", "original_message_id"]).unwrap());

        let reply_to_id = extract_reply_info(&tags);
        assert_eq!(reply_to_id, Some("original_message_id".to_string()));

        // Test with no e-tags
        let empty_tags = Tags::new();
        let reply_to_id = extract_reply_info(&empty_tags);
        assert!(reply_to_id.is_none());

        // Test with multiple e-tags (should use last one per NIP-10)
        let mut multi_tags = Tags::new();
        multi_tags.push(Tag::parse(vec!["e", "first_id"]).unwrap());
        multi_tags.push(Tag::parse(vec!["e", "second_id", "relay", "mention"]).unwrap());

        let reply_to_id = extract_reply_info(&multi_tags);
        assert_eq!(reply_to_id, Some("second_id".to_string()));
    }

    #[test]
    fn test_extract_deletion_target_ids() {
        let mut tags = Tags::new();
        tags.push(Tag::parse(vec!["e", "msg1"]).unwrap());
        tags.push(Tag::parse(vec!["e", "msg2"]).unwrap());
        tags.push(Tag::parse(vec!["p", "user1"]).unwrap()); // Should be ignored

        let target_ids = extract_deletion_target_ids(&tags);
        assert_eq!(target_ids.len(), 2);
        assert!(target_ids.contains(&"msg1".to_string()));
        assert!(target_ids.contains(&"msg2".to_string()));

        // Test with no e-tags
        let mut no_e_tags = Tags::new();
        no_e_tags.push(Tag::parse(vec!["p", "user1"]).unwrap());

        let target_ids = extract_deletion_target_ids(&no_e_tags);
        assert!(target_ids.is_empty());
    }

    #[tokio::test]
    async fn test_empty_messages() {
        let parser = MockParser::new();
        let config = AggregatorConfig::default();

        let result = process_messages(vec![], &parser, &config, vec![])
            .await
            .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_config_defaults() {
        let config = AggregatorConfig::default();

        assert!(config.normalize_emoji);
        assert!(!config.enable_debug_logging);
    }

    #[test]
    fn test_config_custom() {
        let config = AggregatorConfig {
            normalize_emoji: false,
            enable_debug_logging: true,
        };

        assert!(!config.normalize_emoji);
        assert!(config.enable_debug_logging);
    }

    #[test]
    fn test_extract_reply_info_edge_cases() {
        // Test with malformed e-tag (no content)
        let mut malformed_tags = Tags::new();
        // This will create a tag with just "e" but no content
        if let Ok(tag) = Tag::parse(vec!["e"]) {
            malformed_tags.push(tag);
        }

        let reply_to_id = extract_reply_info(&malformed_tags);
        // Should handle gracefully - return None for malformed tags
        assert!(reply_to_id.is_none());
    }
}
