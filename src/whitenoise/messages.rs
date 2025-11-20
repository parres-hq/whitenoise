use crate::{
    types::MessageWithTokens,
    whitenoise::{
        Whitenoise,
        accounts::Account,
        database::AggregatedMessage,
        error::{Result, WhitenoiseError},
        media_files::MediaFile,
        message_aggregator::ChatMessage,
    },
};
use mdk_core::prelude::{message_types::Message, *};
use nostr_sdk::prelude::*;

impl Whitenoise {
    /// Sends a message to a specific group and returns the message with parsed tokens.
    ///
    /// This method creates and sends a message to a group using the Nostr MLS protocol.
    /// It handles the complete message lifecycle including event creation, MLS message
    /// generation, publishing to relays, and token parsing. The message content is
    /// automatically parsed for tokens (e.g., mentions, hashtags) before returning.
    ///
    /// # Arguments
    ///
    /// * `sender_pubkey` - The public key of the user sending the message. This is used
    ///   to identify the sender and fetch their account for message creation.
    /// * `group_id` - The unique identifier of the target group where the message will be sent.
    /// * `message` - The content of the message to be sent as a string.
    /// * `kind` - The Nostr event kind as a u16. This determines the type of event being created
    ///   (e.g., text note, reaction, etc.).
    /// * `tags` - Optional vector of Nostr tags to include with the message. If None, an empty
    ///   tag list will be used.
    pub async fn send_message_to_group(
        &self,
        account: &Account,
        group_id: &GroupId,
        message: String,
        kind: u16,
        tags: Option<Vec<Tag>>,
    ) -> Result<MessageWithTokens> {
        let (inner_event, event_id) =
            self.create_unsigned_nostr_event(&account.pubkey, &message, kind, tags)?;

        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        let message_event = mdk.create_message(group_id, inner_event)?;
        let message = mdk
            .get_message(&event_id)?
            .ok_or(WhitenoiseError::MdkCoreError(
                mdk_core::error::Error::MessageNotFound,
            ))?;
        let group_relays = mdk.get_relays(group_id)?;

        // Publish message in background without blocking
        self.nostr.background_publish_event_to(
            message_event,
            account.pubkey,
            group_relays.into_iter().collect::<Vec<_>>(),
        );

        let tokens = self.nostr.parse(&message.content);

        Ok(MessageWithTokens::new(message, tokens))
    }

    /// Fetches all messages for a specific group with parsed tokens.
    ///
    /// This method retrieves all messages that have been sent to a particular group,
    /// parsing the content of each message to extract tokens (e.g., mentions, hashtags).
    /// The messages are returned with both the original message data and the parsed tokens.
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The public key of the user requesting the messages. This is used to
    ///   fetch the appropriate account and verify access permissions.
    /// * `group_id` - The unique identifier of the group whose messages should be retrieved.
    pub async fn fetch_messages_for_group(
        &self,
        account: &Account,
        group_id: &GroupId,
    ) -> Result<Vec<MessageWithTokens>> {
        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        let messages = mdk.get_messages(group_id)?;
        let messages_with_tokens = messages
            .iter()
            .map(|message| MessageWithTokens {
                message: message.clone(),
                tokens: self.nostr.parse(&message.content),
            })
            .collect::<Vec<MessageWithTokens>>();
        Ok(messages_with_tokens)
    }

    /// Fetch and aggregate messages for a group - Main consumer API
    ///
    /// Returns pre-aggregated messages from the cache. The cache is kept up-to-date by:
    /// - Event processor: Caches messages as they arrive (real-time updates)
    /// - Startup sync: Populates cache with existing messages on initialization
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the user requesting messages
    /// * `group_id` - The group to fetch messages for
    pub async fn fetch_aggregated_messages_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
    ) -> Result<Vec<ChatMessage>> {
        Account::find_by_pubkey(pubkey, &self.database).await?;  // Verify account exists (security check)

        AggregatedMessage::find_messages_by_group(group_id, &self.database)
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Failed to read cached messages: {}", e))
            })
    }

    /// Creates an unsigned nostr event with the given parameters
    fn create_unsigned_nostr_event(
        &self,
        pubkey: &PublicKey,
        message: &String,
        kind: u16,
        tags: Option<Vec<Tag>>,
    ) -> Result<(UnsignedEvent, EventId)> {
        let final_tags = tags.unwrap_or_default();

        let mut inner_event =
            UnsignedEvent::new(*pubkey, Timestamp::now(), kind.into(), final_tags, message);

        inner_event.ensure_id();

        let event_id = inner_event.id.unwrap(); // This is guaranteed to be Some by ensure_id

        Ok((inner_event, event_id))
    }

    /// Synchronize message cache with MDK on startup
    ///
    /// MUST be called BEFORE event processor starts to avoid race conditions.
    /// Uses simple count comparison to detect sync needs, then incrementally syncs missing events.
    /// Not yet called during initialization - will be wired in a future commit.
    #[allow(dead_code)] // Will be called from Whitenoise::new() in upcoming commit
    pub(crate) async fn sync_message_cache_on_startup(&self) -> Result<()> {
        tracing::info!(
            target: "whitenoise::cache",
            "Starting message cache synchronization..."
        );

        let mut total_synced = 0;
        let mut total_groups_checked = 0;

        let accounts = Account::all(&self.database).await?;

        for account in accounts {
            let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
            let groups = mdk.get_groups()?;

            for group_info in groups {
                total_groups_checked += 1;

                let mdk_messages = mdk.get_messages(&group_info.mls_group_id)?;

                if self
                    .cache_needs_sync(&group_info.mls_group_id, &mdk_messages)
                    .await?
                {
                    tracing::info!(
                        target: "whitenoise::cache",
                        "Syncing cache for group {} (account {}): {} events",
                        hex::encode(group_info.mls_group_id.as_slice()),
                        account.pubkey.to_hex(),
                        mdk_messages.len()
                    );

                    self.sync_cache_for_group(
                        &account.pubkey,
                        &group_info.mls_group_id,
                        mdk_messages,
                    )
                    .await?;

                    total_synced += 1;
                }
            }
        }

        tracing::info!(
            target: "whitenoise::cache",
            "Message cache synchronization complete: synced {}/{} groups",
            total_synced,
            total_groups_checked
        );

        Ok(())
    }

    async fn cache_needs_sync(&self, group_id: &GroupId, mdk_messages: &[Message]) -> Result<bool> {
        if mdk_messages.is_empty() {
            return Ok(false);
        }

        let cached_count = AggregatedMessage::count_by_group(group_id, &self.database)
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Failed to count cached events: {}", e))
            })?;

        if mdk_messages.len() != cached_count {
            tracing::debug!(
                target: "whitenoise::cache",
                "Cache count mismatch for group {}: MDK={}, Cache={}",
                hex::encode(group_id.as_slice()),
                mdk_messages.len(),
                cached_count
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// Synchronize cache for a specific group
    ///
    /// Filters out events already in cache, then processes and saves only new events.
    async fn sync_cache_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
        mdk_messages: Vec<Message>,
    ) -> Result<()> {
        if mdk_messages.is_empty() {
            return Ok(());
        }

        let cached_ids = AggregatedMessage::get_all_event_ids_by_group(group_id, &self.database)
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Failed to get cached event IDs: {}", e))
            })?;

        let new_events: Vec<Message> = mdk_messages
            .into_iter()
            .filter(|msg| !cached_ids.contains(&msg.id.to_string()))
            .collect();

        if new_events.is_empty() {
            tracing::debug!(
                target: "whitenoise::cache",
                "No new events to sync for group {}",
                hex::encode(group_id.as_slice())
            );
            return Ok(());
        }

        let num_new_events = new_events.len();

        tracing::info!(
            target: "whitenoise::cache",
            "Found {} new events to cache for group {}",
            num_new_events,
            hex::encode(group_id.as_slice())
        );

        let media_files = MediaFile::find_by_group(&self.database, group_id).await?;

        let processed_messages = self
            .message_aggregator
            .aggregate_messages_for_group(
                pubkey,
                group_id,
                new_events.clone(),
                &self.nostr,
                media_files,
            )
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Message aggregation failed: {}", e))
            })?;

        AggregatedMessage::save_events(new_events, processed_messages, group_id, &self.database)
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Failed to save events to cache: {}", e))
            })?;

        tracing::debug!(
            target: "whitenoise::cache",
            "Successfully synced {} new events for group {}",
            num_new_events,
            hex::encode(group_id.as_slice())
        );

        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn rebuild_message_cache_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
    ) -> Result<()> {
        tracing::info!(
            target: "whitenoise::cache",
            "Rebuilding message cache for group {}",
            hex::encode(group_id.as_slice())
        );

        AggregatedMessage::delete_by_group(group_id, &self.database)
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Failed to delete cached events: {}", e))
            })?;

        let account = Account::find_by_pubkey(pubkey, &self.database).await?;
        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        let mdk_messages = mdk.get_messages(group_id)?;

        self.sync_cache_for_group(pubkey, group_id, mdk_messages)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::whitenoise::test_utils::create_mock_whitenoise;

    #[tokio::test]
    async fn test_cache_needs_sync_empty_mdk() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[1; 32]);

        // Empty MDK messages should not need sync
        let needs_sync = whitenoise.cache_needs_sync(&group_id, &[]).await.unwrap();
        assert!(!needs_sync);
    }

    #[tokio::test]
    async fn test_sync_cache_for_group_empty() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;
        let group_id = GroupId::from_slice(&[2; 32]);
        let pubkey = nostr_sdk::Keys::generate().public_key();

        // Syncing empty messages should succeed without error
        let result = whitenoise
            .sync_cache_for_group(&pubkey, &group_id, vec![])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_cache_with_actual_messages() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create accounts
        let creator = whitenoise.create_identity().await.unwrap();
        let member = whitenoise.create_identity().await.unwrap();

        // Create a group
        let group = whitenoise
            .create_group(
                &creator,
                vec![member.pubkey],
                crate::whitenoise::test_utils::create_nostr_group_config_data(vec![creator.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Send a few messages
        whitenoise
            .send_message_to_group(
                &creator,
                &group.mls_group_id,
                "Message 1".to_string(),
                9,
                None,
            )
            .await
            .unwrap();

        whitenoise
            .send_message_to_group(
                &creator,
                &group.mls_group_id,
                "Message 2".to_string(),
                9,
                None,
            )
            .await
            .unwrap();

        whitenoise
            .send_message_to_group(
                &creator,
                &group.mls_group_id,
                "Message 3".to_string(),
                9,
                None,
            )
            .await
            .unwrap();

        // Get messages from MDK
        let mdk = Account::create_mdk(creator.pubkey, &whitenoise.config.data_dir).unwrap();
        let mdk_messages = mdk.get_messages(&group.mls_group_id).unwrap();

        // Verify we have 3 messages in MDK
        assert_eq!(mdk_messages.len(), 3);

        // Cache should need sync since it's empty
        let needs_sync = whitenoise
            .cache_needs_sync(&group.mls_group_id, &mdk_messages)
            .await
            .unwrap();
        assert!(needs_sync, "Cache should need sync when empty");

        // Verify cache is empty
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 0);

        // Sync the cache
        whitenoise
            .sync_cache_for_group(&creator.pubkey, &group.mls_group_id, mdk_messages.clone())
            .await
            .unwrap();

        // Verify cache now has 3 messages
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 3);

        // Cache should not need sync anymore
        let needs_sync = whitenoise
            .cache_needs_sync(&group.mls_group_id, &mdk_messages)
            .await
            .unwrap();
        assert!(!needs_sync, "Cache should not need sync after syncing");

        // Verify we can fetch the messages from cache
        let messages =
            AggregatedMessage::find_messages_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].content.contains("Message"));
        assert!(messages[1].content.contains("Message"));
        assert!(messages[2].content.contains("Message"));
    }

    #[tokio::test]
    async fn test_incremental_sync() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create accounts and group
        let creator = whitenoise.create_identity().await.unwrap();
        let member = whitenoise.create_identity().await.unwrap();

        let group = whitenoise
            .create_group(
                &creator,
                vec![member.pubkey],
                crate::whitenoise::test_utils::create_nostr_group_config_data(vec![creator.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Send 2 messages
        whitenoise
            .send_message_to_group(&creator, &group.mls_group_id, "First".to_string(), 9, None)
            .await
            .unwrap();

        whitenoise
            .send_message_to_group(&creator, &group.mls_group_id, "Second".to_string(), 9, None)
            .await
            .unwrap();

        // Sync the cache
        let mdk = Account::create_mdk(creator.pubkey, &whitenoise.config.data_dir).unwrap();
        let mdk_messages = mdk.get_messages(&group.mls_group_id).unwrap();
        whitenoise
            .sync_cache_for_group(&creator.pubkey, &group.mls_group_id, mdk_messages)
            .await
            .unwrap();

        // Verify 2 messages in cache
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 2);

        // Send a 3rd message
        whitenoise
            .send_message_to_group(&creator, &group.mls_group_id, "Third".to_string(), 9, None)
            .await
            .unwrap();

        // Get updated messages from MDK
        let mdk_messages = mdk.get_messages(&group.mls_group_id).unwrap();
        assert_eq!(mdk_messages.len(), 3);

        // Cache should need sync now
        let needs_sync = whitenoise
            .cache_needs_sync(&group.mls_group_id, &mdk_messages)
            .await
            .unwrap();
        assert!(needs_sync, "Cache should need sync after new message");

        // Incremental sync should only process the new message
        whitenoise
            .sync_cache_for_group(&creator.pubkey, &group.mls_group_id, mdk_messages)
            .await
            .unwrap();

        // Verify 3 messages in cache now
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 3);

        // Verify all messages are retrievable
        let messages =
            AggregatedMessage::find_messages_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(messages.len(), 3);

        let contents: Vec<String> = messages.iter().map(|m| m.content.clone()).collect();
        assert!(contents.contains(&"First".to_string()));
        assert!(contents.contains(&"Second".to_string()));
        assert!(contents.contains(&"Third".to_string()));
    }

    #[tokio::test]
    async fn test_rebuild_message_cache() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create accounts and group
        let creator = whitenoise.create_identity().await.unwrap();
        let member = whitenoise.create_identity().await.unwrap();

        let group = whitenoise
            .create_group(
                &creator,
                vec![member.pubkey],
                crate::whitenoise::test_utils::create_nostr_group_config_data(vec![creator.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Send messages
        whitenoise
            .send_message_to_group(&creator, &group.mls_group_id, "Test 1".to_string(), 9, None)
            .await
            .unwrap();

        whitenoise
            .send_message_to_group(&creator, &group.mls_group_id, "Test 2".to_string(), 9, None)
            .await
            .unwrap();

        // Sync cache
        let mdk = Account::create_mdk(creator.pubkey, &whitenoise.config.data_dir).unwrap();
        let mdk_messages = mdk.get_messages(&group.mls_group_id).unwrap();
        whitenoise
            .sync_cache_for_group(&creator.pubkey, &group.mls_group_id, mdk_messages)
            .await
            .unwrap();

        // Verify cache has 2 messages
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 2);

        // Rebuild the cache
        whitenoise
            .rebuild_message_cache_for_group(&creator.pubkey, &group.mls_group_id)
            .await
            .unwrap();

        // Cache should still have 2 messages
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 2);

        // Messages should be the same
        let messages =
            AggregatedMessage::find_messages_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Test 1");
        assert_eq!(messages[1].content, "Test 2");
    }

    #[tokio::test]
    async fn test_sync_message_cache_on_startup() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create accounts and group
        let creator = whitenoise.create_identity().await.unwrap();
        let member = whitenoise.create_identity().await.unwrap();

        let group = whitenoise
            .create_group(
                &creator,
                vec![member.pubkey],
                crate::whitenoise::test_utils::create_nostr_group_config_data(vec![creator.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Send messages
        for i in 1..=5 {
            whitenoise
                .send_message_to_group(
                    &creator,
                    &group.mls_group_id,
                    format!("Startup test {}", i),
                    9,
                    None,
                )
                .await
                .unwrap();
        }

        // Cache should be empty
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 0);

        // Run startup sync
        whitenoise.sync_message_cache_on_startup().await.unwrap();

        // Cache should now have all 5 messages
        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 5);

        // Verify messages are correct
        let messages =
            AggregatedMessage::find_messages_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(messages.len(), 5);
        for (i, msg) in messages.iter().enumerate() {
            assert!(msg.content.contains(&format!("Startup test {}", i + 1)));
        }

        // Running startup sync again should be idempotent
        whitenoise.sync_message_cache_on_startup().await.unwrap();

        let cached_count =
            AggregatedMessage::count_by_group(&group.mls_group_id, &whitenoise.database)
                .await
                .unwrap();
        assert_eq!(cached_count, 5);
    }

    #[tokio::test]
    async fn test_fetch_aggregated_messages_reads_from_cache() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create accounts and group
        let creator = whitenoise.create_identity().await.unwrap();
        let member = whitenoise.create_identity().await.unwrap();

        let group = whitenoise
            .create_group(
                &creator,
                vec![member.pubkey],
                crate::whitenoise::test_utils::create_nostr_group_config_data(vec![creator.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Send messages
        for i in 1..=3 {
            whitenoise
                .send_message_to_group(
                    &creator,
                    &group.mls_group_id,
                    format!("Cache test {}", i),
                    9,
                    None,
                )
                .await
                .unwrap();
        }

        // Populate cache
        let mdk = Account::create_mdk(creator.pubkey, &whitenoise.config.data_dir).unwrap();
        let mdk_messages = mdk.get_messages(&group.mls_group_id).unwrap();
        whitenoise
            .sync_cache_for_group(&creator.pubkey, &group.mls_group_id, mdk_messages)
            .await
            .unwrap();

        // Fetch messages via the main API - should read from cache
        let fetched_messages = whitenoise
            .fetch_aggregated_messages_for_group(&creator.pubkey, &group.mls_group_id)
            .await
            .unwrap();

        // Verify we got all 3 messages
        assert_eq!(fetched_messages.len(), 3);

        // Verify content
        for (i, msg) in fetched_messages.iter().enumerate() {
            assert!(
                msg.content.contains(&format!("Cache test {}", i + 1)),
                "Message {} should contain 'Cache test {}'",
                i,
                i + 1
            );
        }

        // Verify messages are ordered by created_at
        for i in 0..fetched_messages.len() - 1 {
            assert!(
                fetched_messages[i].created_at.as_u64()
                    <= fetched_messages[i + 1].created_at.as_u64(),
                "Messages should be ordered by timestamp"
            );
        }
    }

    #[tokio::test]
    async fn test_fetch_with_reactions_and_media() {
        let (whitenoise, _data_temp, _logs_temp) = create_mock_whitenoise().await;

        // Create accounts and group
        let creator = whitenoise.create_identity().await.unwrap();
        let member = whitenoise.create_identity().await.unwrap();

        let group = whitenoise
            .create_group(
                &creator,
                vec![member.pubkey],
                crate::whitenoise::test_utils::create_nostr_group_config_data(vec![creator.pubkey]),
                None,
            )
            .await
            .unwrap();

        // Send a message
        whitenoise
            .send_message_to_group(
                &creator,
                &group.mls_group_id,
                "Message with reactions".to_string(),
                9,
                None,
            )
            .await
            .unwrap();

        // Populate cache
        let mdk = Account::create_mdk(creator.pubkey, &whitenoise.config.data_dir).unwrap();
        let mdk_messages = mdk.get_messages(&group.mls_group_id).unwrap();
        whitenoise
            .sync_cache_for_group(&creator.pubkey, &group.mls_group_id, mdk_messages)
            .await
            .unwrap();

        // Fetch messages - should include empty reactions and media
        let messages = whitenoise
            .fetch_aggregated_messages_for_group(&creator.pubkey, &group.mls_group_id)
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Message with reactions");

        // Verify reactions summary exists (even if empty)
        assert_eq!(messages[0].reactions.by_emoji.len(), 0);
        assert_eq!(messages[0].reactions.user_reactions.len(), 0);

        // Verify media attachments exists (even if empty)
        assert_eq!(messages[0].media_attachments.len(), 0);
    }
}
