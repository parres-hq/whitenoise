use crate::{
    types::MessageWithTokens,
    whitenoise::{
        Whitenoise,
        accounts::Account,
        error::{Result, WhitenoiseError},
        media_files::MediaFile,
        message_aggregator::ChatMessage,
    },
};
use mdk_core::prelude::*;
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
    /// This is the primary method that consumers should use to get chat messages
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the user requesting messages
    /// * `group_id` - The group to fetch and aggregate messages for
    pub async fn fetch_aggregated_messages_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
    ) -> Result<Vec<ChatMessage>> {
        // Get account to access mdk instance
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;

        let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
        let raw_messages = mdk.get_messages(group_id)?;

        // Fetch all media files for this group upfront
        let media_files = MediaFile::find_by_group(&self.database, group_id).await?;

        // Use the aggregator to process the messages
        self.message_aggregator
            .aggregate_messages_for_group(
                pubkey,
                group_id,
                raw_messages,
                &self.nostr, // For token parsing
                media_files,
            )
            .await
            .map_err(|e| {
                WhitenoiseError::from(anyhow::anyhow!("Message aggregation failed: {}", e))
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
}
