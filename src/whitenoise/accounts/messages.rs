use crate::types::MessageWithTokens;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::*;

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
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `Ok(MessageWithTokens)` - The successfully sent message along with its parsed tokens
    /// - `Err(WhitenoiseError)` - An error if the operation fails at any step
    ///
    /// # Errors
    ///
    /// This method can return the following errors:
    /// - `WhitenoiseError::NostrMlsNotInitialized` - If the Nostr MLS instance is not
    ///   properly initialized for the sender's account
    /// - `WhitenoiseError::InvalidEvent` - If the message cannot be found after creation
    /// - Account-related errors from `fetch_account()` if the sender's pubkey is invalid
    /// - MLS-related errors from message creation or relay operations
    /// - Network errors from `publish_event_to()` if publishing to relays fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use whitenoise::Whitenoise;
    /// # use nostr_mls::prelude::*;
    /// # async fn example(whitenoise: &Whitenoise, sender_pubkey: PublicKey, group_id: GroupId) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_content = "Hello, group!".to_string();
    /// let kind = 1; // Text note
    /// let tags = Some(vec![Tag::hashtag("example")]);
    ///
    /// let message_with_tokens = whitenoise
    ///     .send_message(&sender_pubkey, &group_id, message_content, kind, tags)
    ///     .await?;
    ///
    /// println!("Sent message: {}", message_with_tokens.message.content);
    /// println!("Parsed tokens: {:?}", message_with_tokens.tokens);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_message_to_group(
        &self,
        sender_pubkey: &PublicKey,
        group_id: &GroupId,
        message: String,
        kind: u16,
        tags: Option<Vec<Tag>>,
    ) -> Result<MessageWithTokens> {
        let (inner_event, event_id) =
            self.create_unsigned_nostr_event(sender_pubkey, &message, kind, tags)?;

        let account = self.fetch_account(sender_pubkey).await?;

        let nostr_mls_guard = account.nostr_mls.lock().await;

        let (message_event, message, relays) = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            let msg_event = nostr_mls.create_message(group_id, inner_event)?;
            let msg = nostr_mls
                .get_message(&event_id)?
                .ok_or(WhitenoiseError::InvalidEvent(
                    "Message not found after creation".to_string(),
                ))?;
            let relays = nostr_mls.get_relays(group_id)?;
            (msg_event, msg, relays)
        } else {
            return Err(WhitenoiseError::NostrMlsNotInitialized);
        };

        self.nostr.publish_event_to(message_event, &relays).await?;

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
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing:
    /// - `Ok(Vec<MessageWithTokens>)` - A vector of messages with their parsed tokens
    /// - `Err(WhitenoiseError)` - An error if the operation fails
    ///
    /// # Errors
    ///
    /// This method can return the following errors:
    /// - `WhitenoiseError::NostrMlsNotInitialized` - If the Nostr MLS instance is not
    ///   properly initialized for the account
    /// - Account-related errors from `fetch_account()` if the pubkey is invalid or
    ///   the account cannot be retrieved
    /// - MLS-related errors from `nostr_mls.get_messages()` if there are issues
    ///   accessing the group messages
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use whitenoise::Whitenoise;
    /// # use nostr_mls::prelude::*;
    /// # async fn example(whitenoise: &Whitenoise, user_pubkey: PublicKey, group_id: GroupId) -> Result<(), Box<dyn std::error::Error>> {
    /// let messages = whitenoise
    ///     .fetch_messages_for_group(&user_pubkey, &group_id)
    ///     .await?;
    ///
    /// for message_with_tokens in messages {
    ///     println!("Message: {}", message_with_tokens.message.content);
    ///     println!("Tokens: {:?}", message_with_tokens.tokens);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch_messages_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
    ) -> Result<Vec<MessageWithTokens>> {
        let account = self.fetch_account(pubkey).await?;
        let nostr_mls_guard = account.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            let messages = nostr_mls.get_messages(group_id)?;
            let messages_with_tokens = messages
                .iter()
                .map(|message| MessageWithTokens {
                    message: message.clone(),
                    tokens: self.nostr.parse(&message.content),
                })
                .collect::<Vec<MessageWithTokens>>();
            Ok(messages_with_tokens)
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
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
