use nostr_mls::prelude::*;
use std::time::Duration;
use tokio::time::timeout;

use super::MessageWithTokens;
use crate::accounts::Account;
use crate::send_mls_message;
use crate::whitenoise::Whitenoise;

/// Deletes a message from an MLS group by creating and sending a deletion event
///
/// Creates a kind 5 (deletion) event with an "e" tag referencing the message
/// to be deleted, as specified in NIP-09.
///
/// # Arguments
/// * `group` - The MLS group containing the message
/// * `message_id` - ID of the message to delete (hex-encoded string)
/// * `wn` - Whitenoise state handle
/// * `app_handle` - Tauri app handle
///
/// # Returns
/// * `Ok(Message)` - The deletion event if successful
/// * `Err(String)` - Error message if deletion fails
///
/// # Errors
/// Returns error if:
/// * Message ID cannot be parsed as a valid EventId
/// * No active account is found
/// * Message cannot be found in the group
/// * User is not the owner of the message
/// * Sending the deletion event fails
#[tauri::command]
pub async fn delete_message(
    group: group_types::Group,
    message_id: String,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<MessageWithTokens, String> {
    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| format!("Failed to get active account: {}", e))?;

    tracing::debug!(
        target: "whitenoise::commands::groups::validate_deletion_request",
        "Active account: {}, attempting to delete message: {}",
        active_account.pubkey.to_hex(),
        message_id
    );

    tracing::debug!(target: "whitenoise::commands::groups::delete_message", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::commands::groups::delete_message", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::commands::groups::delete_message", "Timeout waiting for nostr_mls lock");
            return Err("Timeout waiting for nostr_mls lock".to_string());
        }
    };

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let group_messages = nostr_mls
            .get_messages(&group.mls_group_id)
            .map_err(|e| format!("Failed to fetch messages: {}", e))?;

        // Validate inputs and permissions
        let message_event_id =
            validate_deletion_request(&message_id, &group_messages, &active_account).await?;

        // Create deletion event with "e" tag (NIP-09)
        let deletion_tags = vec![Tag::event(message_event_id)];
        let deletion_reason = "Message deleted by user";

        tracing::debug!(
            target: "whitenoise::commands::groups::delete_message",
            "Creating deletion event for message ID: {}, from user: {}",
            message_id,
            active_account.pubkey.to_hex()
        );

        // Send the deletion event
        let result = send_mls_message(
            group,
            deletion_reason.to_string(),
            5, // Kind 5 for deletion events as per NIP-09
            Some(deletion_tags),
            None,
            wn.clone(),
            app_handle,
        )
        .await;
        tracing::debug!(target: "whitenoise::commands::groups::delete_message", "nostr_mls lock released");
        result
    } else {
        tracing::debug!(target: "whitenoise::commands::groups::delete_message", "nostr_mls lock released");
        Err("Failed to fetch messages: No Nostr MLS instance".to_string())
    }
}

/// Validates a message deletion request
///
/// # Arguments
/// * `message_id` - Hex-encoded message ID
/// * `group` - Group containing the message
/// * `wn` - Whitenoise state
///
/// # Returns
/// * `Ok((EventId, Account))` - Validated message ID and active account
/// * `Err(String)` - Error message if validation fails
async fn validate_deletion_request(
    message_id: &str,
    group_messages: &[message_types::Message],
    active_account: &Account,
) -> Result<EventId, String> {
    // Parse and validate message ID
    let message_event_id =
        EventId::from_hex(message_id).map_err(|e| format!("Invalid message ID format: {}", e))?;

    // Find the target message
    let message = group_messages
        .iter()
        .find(|m| m.id == message_event_id)
        .ok_or_else(|| format!("Message with ID {} not found in this group", message_id))?;

    // Verify ownership
    if message.pubkey != active_account.pubkey {
        tracing::warn!(
            target: "whitenoise::commands::groups::validate_deletion_request",
            "Permission denied: User {} attempted to delete message {} created by {}",
            active_account.pubkey.to_hex(),
            message_id,
            message.pubkey.to_hex()
        );
        return Err(format!(
            "Permission denied: Cannot delete message {}. Only the message creator can delete it.",
            message_id
        ));
    }

    tracing::debug!(
        target: "whitenoise::commands::groups::validate_deletion_request",
        "Validation successful for message: {}",
        message_id
    );

    Ok(message_event_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_account(pubkey: PublicKey) -> Account {
        Account {
            pubkey,
            metadata: Metadata::default(),
            settings: crate::accounts::AccountSettings::default(),
            onboarding: crate::accounts::AccountOnboarding::default(),
            last_used: Timestamp::now(),
            last_synced: Timestamp::zero(),
            active: true,
        }
    }

    fn create_test_message(event_id_str: &str, author_pubkey: PublicKey) -> message_types::Message {
        let message_id = EventId::from_hex(event_id_str).unwrap();
        let event = UnsignedEvent {
            id: Some(message_id),
            pubkey: author_pubkey,
            created_at: Timestamp::now(),
            kind: Kind::TextNote,
            tags: Tags::new(),
            content: "Test message".to_string(),
        };
        message_types::Message {
            id: message_id,
            pubkey: author_pubkey,
            kind: event.kind,
            mls_group_id: GroupId::from_slice(&[0; 32]),
            created_at: Timestamp::now(),
            content: "Test message".to_string(),
            tags: Tags::new(),
            event,
            wrapper_event_id: EventId::from_hex(
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            )
            .unwrap(),
            state: message_types::MessageState::Created,
        }
    }

    #[tokio::test]
    async fn test_validate_deletion_request_success() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let active_account = create_test_account(pubkey);

        let event_id_str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let message = create_test_message(event_id_str, pubkey);
        let group_messages = vec![message];

        let result =
            validate_deletion_request(event_id_str, &group_messages, &active_account).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), EventId::from_hex(event_id_str).unwrap());
    }

    #[tokio::test]
    async fn test_validate_deletion_request_invalid_id_format() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let active_account = create_test_account(pubkey);
        let group_messages = vec![];

        let result =
            validate_deletion_request("invalid-hex-id", &group_messages, &active_account).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid message ID format"));
    }

    #[tokio::test]
    async fn test_validate_deletion_request_message_not_found() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let active_account = create_test_account(pubkey);
        let group_messages = vec![];

        let result = validate_deletion_request(
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            &group_messages,
            &active_account,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found in this group"));
    }

    #[tokio::test]
    async fn test_validate_deletion_request_not_owner() {
        let active_keys = Keys::generate();
        let active_pubkey = active_keys.public_key();
        let active_account = create_test_account(active_pubkey);

        let owner_keys = Keys::generate();
        let owner_pubkey = owner_keys.public_key();

        let event_id_str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let message = create_test_message(event_id_str, owner_pubkey);
        let group_messages = vec![message];

        let result =
            validate_deletion_request(event_id_str, &group_messages, &active_account).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Permission denied"));
    }
}
