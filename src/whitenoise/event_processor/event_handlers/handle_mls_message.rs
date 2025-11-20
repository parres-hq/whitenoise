use mdk_core::prelude::message_types::Message;
use mdk_core::prelude::{GroupId, MessageProcessingResult};
use nostr_sdk::prelude::*;

use crate::whitenoise::{
    Whitenoise,
    accounts::Account,
    database::AggregatedMessage,
    error::{Result, WhitenoiseError},
    media_files::MediaFile,
    message_aggregator::{emoji_utils, reaction_handler},
};

impl Whitenoise {
    pub async fn handle_mls_message(&self, account: &Account, event: Event) -> Result<()> {
        tracing::debug!(
          target: "whitenoise::event_handlers::handle_mls_message",
          "Handling MLS message for account: {}",
          account.pubkey.to_hex()
        );

        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        match mdk.process_message(&event) {
            Ok(result) => {
                tracing::debug!(
                  target: "whitenoise::event_handlers::handle_mls_message",
                  "Handled MLS message - Result: {:?}",
                  result
                );

                // Extract and store media references synchronously
                if let Some((group_id, inner_event)) = Self::extract_message_details(&result) {
                    let parsed_references = {
                        let media_manager = mdk.media_manager(group_id.clone());
                        self.media_files()
                            .parse_imeta_tags_from_event(&inner_event, &media_manager)?
                    };

                    self.media_files()
                        .store_parsed_media_references(
                            &group_id,
                            &account.pubkey,
                            parsed_references,
                        )
                        .await?;

                    // Update aggregated message cache synchronously
                    self.update_message_cache(&group_id, inner_event).await?;
                }

                // Background sync for group images (existing pattern)
                if let MessageProcessingResult::Commit { mls_group_id } = result {
                    Whitenoise::background_sync_group_image_cache_if_needed(account, &mls_group_id);
                }
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::event_handlers::handle_mls_message",
                    "MLS message handling failed for account {}: {}",
                    account.pubkey.to_hex(),
                    e
                );
                Err(WhitenoiseError::MdkCoreError(e))
            }
        }
    }

    /// Extracts group_id and inner_event from MessageProcessingResult
    ///
    /// Returns Some if the result contains an application message with inner event content,
    /// None otherwise (e.g., for commits, proposals, or other non-message results).
    fn extract_message_details(
        result: &MessageProcessingResult,
    ) -> Option<(mdk_core::prelude::GroupId, UnsignedEvent)> {
        match result {
            MessageProcessingResult::ApplicationMessage(message) => {
                // The message.event is the decrypted rumor (UnsignedEvent) from the MLS message
                Some((message.mls_group_id.clone(), message.event.clone()))
            }
            _ => None,
        }
    }

    async fn update_message_cache(
        &self,
        group_id: &GroupId,
        inner_event: UnsignedEvent,
    ) -> Result<()> {
        let event_id = inner_event.id.ok_or_else(|| {
            WhitenoiseError::Other(anyhow::anyhow!(
                "Inner event missing ID in group {}",
                hex::encode(group_id.as_slice())
            ))
        })?;

        // Construct a Message from UnsignedEvent
        let message = Message {
            id: event_id,
            pubkey: inner_event.pubkey,
            created_at: inner_event.created_at,
            kind: inner_event.kind,
            tags: inner_event.tags.clone(),
            content: inner_event.content.clone(),
            mls_group_id: group_id.clone(),
            event: inner_event,
            wrapper_event_id: event_id, // Reuse event_id as placeholder, we don't need it
            state: mdk_core::prelude::message_types::MessageState::Processed,
        };

        match message.kind {
            Kind::Custom(9) => {
                let media_files = MediaFile::find_by_group(&self.database, group_id).await?;

                let chat_message = self
                    .message_aggregator
                    .process_single_message(&message, &self.nostr, media_files)
                    .await
                    .map_err(|e| {
                        WhitenoiseError::Other(anyhow::anyhow!("Failed to process message: {}", e))
                    })?;

                AggregatedMessage::insert_message(&chat_message, group_id, &self.database).await?;

                self.apply_orphaned_reactions_and_deletions(&chat_message.id, group_id)
                    .await?;

                tracing::debug!(
                    target: "whitenoise::cache",
                    "Cached kind 9 message {} in group {}",
                    message.id,
                    hex::encode(group_id.as_slice())
                );
            }
            Kind::Reaction => {
                AggregatedMessage::insert_reaction(&message, group_id, &self.database).await?;

                if let Err(e) = self
                    .update_cached_message_with_reaction(&message, group_id)
                    .await
                {
                    tracing::debug!(
                        target: "whitenoise::cache",
                        "Could not apply reaction {} immediately (target not cached yet): {}",
                        message.id,
                        e
                    );
                }

                tracing::debug!(
                    target: "whitenoise::cache",
                    "Cached kind 7 reaction {} in group {}",
                    message.id,
                    hex::encode(group_id.as_slice())
                );
            }
            Kind::EventDeletion => {
                AggregatedMessage::insert_deletion(&message, group_id, &self.database).await?;

                if let Err(e) = self
                    .update_cached_messages_with_deletion(&message, group_id)
                    .await
                {
                    tracing::debug!(
                        target: "whitenoise::cache",
                        "Could not apply deletion {} immediately (targets not cached yet): {}",
                        message.id,
                        e
                    );
                }

                tracing::debug!(
                    target: "whitenoise::cache",
                    "Cached kind 5 deletion {} in group {}",
                    message.id,
                    hex::encode(group_id.as_slice())
                );
            }
            _ => {
                tracing::debug!("Ignoring message kind {:?} for cache", message.kind);
            }
        }

        Ok(())
    }

    async fn update_cached_message_with_reaction(
        &self,
        reaction_msg: &Message,
        group_id: &GroupId,
    ) -> Result<()> {
        let target_id = Self::extract_reaction_target_id(&reaction_msg.tags)?;

        let cached_msg = AggregatedMessage::find_by_id(&target_id, group_id, &self.database)
            .await?
            .ok_or_else(|| WhitenoiseError::Other(anyhow::anyhow!("Target message not cached")))?;

        let mut msg = cached_msg;

        let reaction_emoji = emoji_utils::validate_and_normalize_reaction(
            &reaction_msg.content,
            self.message_aggregator.config().normalize_emoji,
        )
        .map_err(|e| {
            WhitenoiseError::Other(anyhow::anyhow!("Failed to validate reaction: {}", e))
        })?;

        reaction_handler::add_reaction_to_message(
            &mut msg,
            &reaction_msg.pubkey,
            &reaction_emoji,
            reaction_msg.created_at,
        );

        AggregatedMessage::update_reactions(&msg.id, group_id, &msg.reactions, &self.database)
            .await?;

        Ok(())
    }

    async fn update_cached_messages_with_deletion(
        &self,
        deletion_msg: &Message,
        group_id: &GroupId,
    ) -> Result<()> {
        let target_ids = Self::extract_deletion_target_ids(&deletion_msg.tags);

        for target_id in target_ids {
            AggregatedMessage::mark_deleted(
                &target_id,
                group_id,
                &deletion_msg.id.to_string(),
                &self.database,
            )
            .await?;
        }

        Ok(())
    }

    fn extract_reaction_target_id(tags: &Tags) -> Result<String> {
        tags.iter()
            .find(|tag| tag.kind() == nostr_sdk::TagKind::e())
            .and_then(|tag| tag.content().map(|s| s.to_string()))
            .ok_or_else(|| WhitenoiseError::Other(anyhow::anyhow!("Reaction missing e-tag")))
    }

    fn extract_deletion_target_ids(tags: &Tags) -> Vec<String> {
        tags.iter()
            .filter(|tag| tag.kind() == nostr_sdk::TagKind::e())
            .filter_map(|tag| tag.content().map(|s| s.to_string()))
            .collect()
    }

    async fn apply_orphaned_reactions_and_deletions(
        &self,
        message_id: &str,
        group_id: &GroupId,
    ) -> Result<()> {
        let orphaned_reactions =
            AggregatedMessage::find_orphaned_reactions(message_id, group_id, &self.database)
                .await
                .map_err(|e| {
                    WhitenoiseError::from(anyhow::anyhow!(
                        "Failed to find orphaned reactions: {}",
                        e
                    ))
                })?;

        let orphaned_deletions =
            AggregatedMessage::find_orphaned_deletions(message_id, group_id, &self.database)
                .await
                .map_err(|e| {
                    WhitenoiseError::from(anyhow::anyhow!(
                        "Failed to find orphaned deletions: {}",
                        e
                    ))
                })?;

        if !orphaned_reactions.is_empty() || !orphaned_deletions.is_empty() {
            tracing::info!(
                target: "whitenoise::cache",
                "Found {} orphaned reactions and {} orphaned deletions for message {}, applying...",
                orphaned_reactions.len(),
                orphaned_deletions.len(),
                message_id
            );
        }

        for reaction_row in orphaned_reactions {
            let reaction_timestamp = Timestamp::from(reaction_row.created_at.timestamp() as u64);

            if let Some(mut msg) =
                AggregatedMessage::find_by_id(message_id, group_id, &self.database)
                    .await
                    .map_err(|e| {
                        WhitenoiseError::from(anyhow::anyhow!("Failed to find message: {}", e))
                    })?
            {
                let reaction_emoji = emoji_utils::validate_and_normalize_reaction(
                    &reaction_row.content,
                    self.message_aggregator.config().normalize_emoji,
                )
                .map_err(|e| {
                    WhitenoiseError::Other(anyhow::anyhow!("Failed to validate reaction: {}", e))
                })?;

                reaction_handler::add_reaction_to_message(
                    &mut msg,
                    &reaction_row.author,
                    &reaction_emoji,
                    reaction_timestamp,
                );

                AggregatedMessage::update_reactions(
                    &msg.id,
                    group_id,
                    &msg.reactions,
                    &self.database,
                )
                .await
                .map_err(|e| {
                    WhitenoiseError::from(anyhow::anyhow!(
                        "Failed to update reactions in cache: {}",
                        e
                    ))
                })?;
            }
        }

        for deletion_row in orphaned_deletions {
            AggregatedMessage::mark_deleted(
                message_id,
                group_id,
                &deletion_row.message_id.to_string(),
                &self.database,
            )
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!(
                    "Failed to mark message deleted in cache: {}",
                    e
                ))
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_handle_mls_message_success() {
        // Arrange: Whitenoise and accounts
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let creator_account = whitenoise.create_identity().await.unwrap();
        // Create one member account, set contact, publish key package
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_pubkey = members[0].0.pubkey;

        // Give time for key package publish to propagate in test relays
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create the group via high-level API
        let _group = whitenoise
            .create_group(
                &creator_account,
                vec![member_pubkey],
                create_nostr_group_config_data(vec![creator_account.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Build a valid MLS group message event for the new group
        let mdk = Account::create_mdk(creator_account.pubkey, &whitenoise.config.data_dir).unwrap();
        let groups = mdk.get_groups().unwrap();
        let group_id = groups
            .first()
            .expect("group must exist")
            .mls_group_id
            .clone();

        let mut inner = UnsignedEvent::new(
            creator_account.pubkey,
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "hello from test".to_string(),
        );
        inner.ensure_id();
        let message_event = mdk.create_message(&group_id, inner).unwrap();

        // Act
        let result = whitenoise
            .handle_mls_message(&creator_account, message_event)
            .await;

        // Assert
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mls_message_error_path() {
        // Arrange: Whitenoise and accounts
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let creator_account = whitenoise.create_identity().await.unwrap();
        let members = setup_multiple_test_accounts(&whitenoise, 1).await;
        let member_pubkey = members[0].0.pubkey;
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Create the group via high-level API
        let _group = whitenoise
            .create_group(
                &creator_account,
                vec![member_pubkey],
                create_nostr_group_config_data(vec![creator_account.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Create a valid MLS message event for that group
        let mdk = Account::create_mdk(creator_account.pubkey, &whitenoise.config.data_dir).unwrap();
        let groups = mdk.get_groups().unwrap();
        let group_id = groups
            .first()
            .expect("group must exist")
            .mls_group_id
            .clone();
        let mut inner = UnsignedEvent::new(
            creator_account.pubkey,
            Timestamp::now(),
            Kind::TextNote,
            vec![],
            "msg".to_string(),
        );
        inner.ensure_id();
        let valid_event = mdk.create_message(&group_id, inner).unwrap();

        // Corrupt it by changing the kind so MLS processing fails
        let mut bad_event = valid_event.clone();
        bad_event.kind = Kind::TextNote;

        // Act
        let result = whitenoise
            .handle_mls_message(&creator_account, bad_event)
            .await;

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            WhitenoiseError::MdkCoreError(_) => {}
            other => panic!("Expected MdkCoreError, got: {:?}", other),
        }
    }
}
