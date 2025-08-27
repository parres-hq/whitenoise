use nostr_mls::prelude::*;

use crate::{
    types::MessageWithTokens,
    whitenoise::{
        accounts::Account,
        error::{Result, WhitenoiseError},
        relays::Relay,
        Whitenoise,
    },
};

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

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let message_event = nostr_mls.create_message(group_id, inner_event)?;
        let message = nostr_mls
            .get_message(&event_id)?
            .ok_or(WhitenoiseError::NostrMlsError(
                nostr_mls::error::Error::MessageNotFound,
            ))?;
        let group_relays = nostr_mls.get_relays(group_id)?;

        let mut db_relays = Vec::with_capacity(group_relays.len());
        for relay_url in group_relays {
            let db_relay = Relay::find_or_create_by_url(&relay_url, &self.database).await?;
            db_relays.push(db_relay);
        }

        self.nostr
            .publish_mls_message_to(message_event, account, &db_relays)
            .await?;

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
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let messages = nostr_mls.get_messages(group_id)?;
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
    ) -> Result<Vec<crate::whitenoise::message_aggregator::ChatMessage>> {
        // Get account to access nostr_mls instance
        let account = Account::find_by_pubkey(pubkey, &self.database).await?;

        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let raw_messages = nostr_mls.get_messages(group_id)?;
        // Use the aggregator to process the messages
        self.message_aggregator
            .aggregate_messages_for_group(
                pubkey,
                group_id,
                raw_messages,
                &self.nostr, // For token parsing
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
