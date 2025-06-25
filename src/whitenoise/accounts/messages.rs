use crate::types::MessageWithTokens;
use crate::whitenoise::error::{Result, WhitenoiseError};
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::*;

impl Whitenoise {
    pub async fn send_message(
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
