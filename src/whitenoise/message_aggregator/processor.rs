//! Core message processing logic
//!
//! This module implements the stateless message aggregation algorithm that transforms
//! raw Nostr MLS messages into structured ChatMessage objects.

use nostr_sdk::prelude::*;
use std::collections::HashMap;

use super::reaction_handler;
use super::types::{
    AggregatorConfig, ChatMessage, ProcessingError, UnresolvedMessage, UnresolvedReason,
};
use crate::nostr_manager::parser::Parser;
use nostr_mls::prelude::message_types::Message;

/// Process raw messages into aggregated chat messages
/// This implements the Phase 1 stateless algorithm from the plan
pub async fn process_messages(
    messages: Vec<Message>,
    parser: &dyn Parser,
    config: &AggregatorConfig,
) -> Result<Vec<ChatMessage>, ProcessingError> {
    if messages.is_empty() {
        return Ok(Vec::new());
    }

    // Step 1: Initialize state
    let mut processed_messages: HashMap<String, ChatMessage> = HashMap::new();
    let mut unresolved_messages: Vec<UnresolvedMessage> = Vec::new();

    // Step 2: Sort messages by timestamp for chronological processing
    let mut sorted_messages = messages;
    sorted_messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    if config.enable_debug_logging {
        tracing::debug!("Sorted {} messages chronologically", sorted_messages.len());
    }

    // Step 3: First Pass - Process base messages (kind 9)
    for message in &sorted_messages {
        match message.kind {
            Kind::Custom(9) => {
                if let Ok(chat_message) = process_regular_message(message, parser).await {
                    processed_messages.insert(message.id.to_string(), chat_message);
                } else if config.enable_debug_logging {
                    tracing::warn!("Failed to process regular message: {}", message.id);
                }
            }
            _ => {
                // Non-chat messages will be processed in later passes
                continue;
            }
        }
    }

    if config.enable_debug_logging {
        tracing::debug!("Processed {} base messages", processed_messages.len());
    }

    // Step 4: Second Pass - Process reactions (kind 7)
    for message in &sorted_messages {
        if message.kind == Kind::Reaction
            && reaction_handler::process_reaction(
                message,
                &mut processed_messages,
                &mut unresolved_messages,
                config,
            )
            .is_err()
            && config.enable_debug_logging
        {
            tracing::warn!("Failed to process reaction: {}", message.id);
        }
    }

    if config.enable_debug_logging {
        tracing::debug!(
            "Processed reactions, {} unresolved messages",
            unresolved_messages.len()
        );
    }

    // Step 5: Third Pass - Process deletions (kind 5)
    for message in &sorted_messages {
        if message.kind == Kind::EventDeletion {
            process_deletion(message, &mut processed_messages, &mut unresolved_messages);
        }
    }

    if config.enable_debug_logging {
        tracing::debug!(
            "Processed deletions, {} unresolved messages",
            unresolved_messages.len()
        );
    }

    // Step 6: Retry Pass - Handle unresolved messages
    for retry_attempt in 1..=config.max_retry_attempts {
        if unresolved_messages.is_empty() {
            break;
        }

        if config.enable_debug_logging {
            tracing::debug!(
                "Retry attempt {} for {} unresolved messages",
                retry_attempt,
                unresolved_messages.len()
            );
        }

        let mut remaining_unresolved = Vec::new();

        for mut unresolved in unresolved_messages {
            unresolved.retry_count += 1;

            let resolved = match &unresolved.reason {
                UnresolvedReason::ReplyToMissing(_) => {
                    // For replies that failed processing, we just skip retries
                    // since the parent message structure doesn't change
                    false
                }
                UnresolvedReason::ReactionToMissing(_) => reaction_handler::retry_reaction(
                    &unresolved.message,
                    &mut processed_messages,
                    config,
                )
                .is_ok(),
                UnresolvedReason::DeleteTargetMissing(_) => {
                    retry_deletion(&unresolved.message, &mut processed_messages).is_ok()
                }
            };

            if !resolved && unresolved.retry_count < config.max_retry_attempts {
                remaining_unresolved.push(unresolved);
            } else if !resolved && config.enable_debug_logging {
                let reason_detail = match &unresolved.reason {
                    UnresolvedReason::ReplyToMissing(parent_id) => {
                        format!("ReplyToMissing({})", parent_id)
                    }
                    UnresolvedReason::ReactionToMissing(target_id) => {
                        format!("ReactionToMissing({})", target_id)
                    }
                    UnresolvedReason::DeleteTargetMissing(target_id) => {
                        format!("DeleteTargetMissing({})", target_id)
                    }
                };
                tracing::warn!(
                    "Message {} unresolved after {} attempts: {}",
                    unresolved.message.id,
                    unresolved.retry_count,
                    reason_detail
                );
            }
        }

        unresolved_messages = remaining_unresolved;
    }

    // Step 7: Return sorted results
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
    if let Some(last_e_tag) = e_tags.last() {
        if let Some(event_id) = last_e_tag.content() {
            return Some(event_id.to_string());
        }
    }

    None
}

/// Process deletion message (kind 5)
fn process_deletion(
    message: &Message,
    processed_messages: &mut HashMap<String, ChatMessage>,
    unresolved_messages: &mut Vec<UnresolvedMessage>,
) {
    let target_ids = extract_deletion_target_ids(&message.tags);

    for target_id in target_ids {
        if let Some(target_message) = processed_messages.get_mut(&target_id) {
            target_message.is_deleted = true;
            target_message.content = String::new(); // Clear content
        } else {
            unresolved_messages.push(UnresolvedMessage {
                message: message.clone(),
                retry_count: 0,
                reason: UnresolvedReason::DeleteTargetMissing(target_id),
            });
        }
    }
}

/// Extract target message IDs from deletion event e-tags
pub(crate) fn extract_deletion_target_ids(tags: &Tags) -> Vec<String> {
    tags.iter()
        .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
        .filter_map(|tag| tag.content().map(|s| s.to_string()))
        .collect()
}

/// Retry processing a deletion message
fn retry_deletion(
    message: &Message,
    processed_messages: &mut HashMap<String, ChatMessage>,
) -> Result<(), ProcessingError> {
    let target_ids = extract_deletion_target_ids(&message.tags);
    let mut any_resolved = false;

    for target_id in target_ids {
        if let Some(target_message) = processed_messages.get_mut(&target_id) {
            target_message.is_deleted = true;
            target_message.content = String::new();
            any_resolved = true;
        }
    }

    if any_resolved {
        Ok(())
    } else {
        Err(ProcessingError::Internal(
            "No deletion targets resolved".to_string(),
        ))
    }
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

        let result = process_messages(vec![], &parser, &config).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_config_defaults() {
        let config = AggregatorConfig::default();

        assert_eq!(config.max_retry_attempts, 3);
        assert!(config.normalize_emoji);
        assert!(!config.enable_debug_logging);
    }

    #[test]
    fn test_config_custom() {
        let config = AggregatorConfig {
            max_retry_attempts: 5,
            normalize_emoji: false,
            enable_debug_logging: true,
        };

        assert_eq!(config.max_retry_attempts, 5);
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
