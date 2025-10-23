use crate::WhitenoiseError;
use crate::integration_tests::core::*;
use async_trait::async_trait;
use nostr_sdk::prelude::*;

/// Test case for sending messages with media attachments and verifying aggregation links them correctly
pub struct SendMessageWithMediaTestCase {
    sender_account_name: String,
    group_name: String,
    message_content: String,
}

impl SendMessageWithMediaTestCase {
    pub fn new(sender_account_name: &str, group_name: &str) -> Self {
        Self {
            sender_account_name: sender_account_name.to_string(),
            group_name: group_name.to_string(),
            message_content: "Check out this image! 📸".to_string(),
        }
    }

    /// Build imeta tag per MIP-04 spec
    /// Format: `["imeta", "url <blossom_url>", "x <hash>", "m <mime_type>", ...]`
    fn build_imeta_tag(
        &self,
        hash_hex: &str,
        blossom_url: &str,
        mime_type: &str,
    ) -> Result<Tag, WhitenoiseError> {
        Tag::parse(vec![
            "imeta",
            &format!("url {}", blossom_url),
            &format!("x {}", hash_hex),
            &format!("m {}", mime_type),
        ])
        .map_err(|e| WhitenoiseError::Other(anyhow::anyhow!("Failed to create imeta tag: {}", e)))
    }
}

#[async_trait]
impl TestCase for SendMessageWithMediaTestCase {
    async fn run(&self, context: &mut ScenarioContext) -> Result<(), WhitenoiseError> {
        tracing::info!(
            "Sending message with media reference to group {} from account: {}",
            self.group_name,
            self.sender_account_name
        );

        let sender_account = context.get_account(&self.sender_account_name)?;
        let group = context.get_group(&self.group_name)?;

        // Get the uploaded media file from context
        let media_file = context.get_media_file("uploaded_chat_media")?;
        let media_hash_hex = hex::encode(&media_file.file_hash);

        let blossom_url = media_file.blossom_url.as_ref().ok_or_else(|| {
            WhitenoiseError::Configuration("Uploaded media has no blossom URL".to_string())
        })?;

        let imeta_tag =
            self.build_imeta_tag(&media_hash_hex, blossom_url, &media_file.mime_type)?;

        // Send message with imeta tag
        let send_result = context
            .whitenoise
            .send_message_to_group(
                sender_account,
                &group.mls_group_id,
                self.message_content.clone(),
                9, // Regular message
                Some(vec![imeta_tag]),
            )
            .await?;

        tracing::info!(
            "✓ Message with media reference sent: {}",
            send_result.message.id
        );

        // Wait for message processing
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Fetch aggregated messages and verify media is linked
        let aggregated_messages = retry(
            15,
            std::time::Duration::from_millis(100),
            || async {
                let messages = context
                    .whitenoise
                    .fetch_aggregated_messages_for_group(
                        &sender_account.pubkey,
                        &group.mls_group_id,
                    )
                    .await?;

                if messages.is_empty() {
                    return Err(WhitenoiseError::Other(anyhow::anyhow!(
                        "No messages found yet"
                    )));
                }

                Ok(messages)
            },
            "fetch aggregated messages with media",
        )
        .await?;

        // Find the message we just sent
        let sent_message_id = send_result.message.id.to_string();
        let message_with_media = aggregated_messages
            .iter()
            .find(|msg| msg.id == sent_message_id)
            .ok_or_else(|| {
                WhitenoiseError::Other(anyhow::anyhow!(
                    "Sent message {} not found in aggregated messages",
                    sent_message_id
                ))
            })?;

        // Verify media is attached
        assert!(
            !message_with_media.media_attachments.is_empty(),
            "Message should have media attachments linked"
        );

        assert_eq!(
            message_with_media.media_attachments.len(),
            1,
            "Message should have exactly 1 media attachment"
        );

        let attached_media = &message_with_media.media_attachments[0];
        let attached_hash_hex = hex::encode(&attached_media.file_hash);
        assert_eq!(
            attached_hash_hex, media_hash_hex,
            "Attached media hash should match uploaded file hash"
        );

        assert_eq!(
            attached_media.mime_type, media_file.mime_type,
            "Attached media MIME type should match uploaded file"
        );

        tracing::info!(
            "✓ Message aggregation correctly linked media: hash={}",
            attached_hash_hex
        );

        // Verify the message content is correct
        assert_eq!(
            message_with_media.content, self.message_content,
            "Message content should match"
        );

        tracing::info!(
            "✓ Message with media verified: {} media attachment(s) linked",
            message_with_media.media_attachments.len()
        );

        Ok(())
    }
}
