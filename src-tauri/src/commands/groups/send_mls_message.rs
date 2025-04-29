use std::collections::BTreeSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use lightning_invoice::SignedRawBolt11Invoice;
use nostr_mls::prelude::*;
use nostr_sdk::prelude::*;
use tauri::Emitter;

use super::MessageWithTokens;
use crate::media::{add_media_file, FileUpload};
use crate::nostr_manager::parser::parse;
use crate::whitenoise::Whitenoise;

#[tauri::command]
pub async fn send_mls_message(
    group: group_types::Group,
    message: String,
    kind: u16,
    tags: Option<Vec<Tag>>,
    uploaded_files: Option<Vec<FileUpload>>,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<MessageWithTokens, String> {
    let nostr_keys = wn.nostr.client.signer().await.map_err(|e| e.to_string())?;
    let mut final_tags = tags.unwrap_or_default();
    let mut final_content = message;

    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Sending MLSMessage event to group: {:?}", group);

    // Process media files if present
    if let Some(uploaded_files) = uploaded_files {
        let mut uploaded_media = Vec::new();
        let files_count = uploaded_files.len();

        // Process files sequentially
        for file in uploaded_files {
            match add_media_file(&group, file, wn.clone()).await {
                Ok(media) => uploaded_media.push(media),
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::commands::groups::send_mls_message",
                        "Media processing error: {}",
                        e
                    );
                    // Continue processing other files instead of failing completely
                }
            }
        }

        // If no files were processed successfully, return an error
        if uploaded_media.is_empty() && files_count > 0 {
            return Err("Failed to process any media files".to_string());
        }

        // Add media content and tags
        let mut media_urls = Vec::new();
        for media in uploaded_media {
            media_urls.push(media.blob_descriptor.url.clone());
            final_tags.push(media.imeta_tag);
        }

        // Add all URLs to content with consistent formatting
        if !media_urls.is_empty() {
            if !final_content.is_empty() {
                final_content.push('\n');
            }
            final_content.push_str(&media_urls.join("\n"));
        }
    }

    let inner_event =
        create_unsigned_nostr_event(&nostr_keys, &final_content, kind, Some(final_tags))
            .await
            .map_err(|e| e.to_string())?;

    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Attempting to acquire nostr_mls lock");
    let mut event_to_publish: Option<Event> = None;
    let mut relays: Option<BTreeSet<RelayUrl>> = None;
    #[allow(unused_assignments)]
    let mut message: Option<message_types::Message> = None;
    {
        let nostr_mls_guard = match timeout(Duration::from_secs(5), wn.nostr_mls.lock()).await {
            Ok(guard) => {
                tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "nostr_mls lock acquired");
                guard
            }
            Err(_) => {
                tracing::error!(target: "whitenoise::commands::groups::send_mls_message", "Timeout waiting for nostr_mls lock");
                return Err("Timeout waiting for nostr_mls lock".to_string());
            }
        };

        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            match nostr_mls
                .create_message(&group.mls_group_id, inner_event.clone())
                .map_err(|e| e.to_string())
            {
                Ok(event) => {
                    // Get group relays
                    relays = Some(
                        nostr_mls
                            .get_relays(&group.mls_group_id)
                            .map_err(|e| e.to_string())?,
                    );
                    event_to_publish = Some(event);
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::commands::groups::send_mls_message",
                        "Error creating message: {}",
                        e
                    );
                }
            }

            if let Some(message_id) = inner_event.id {
                message = nostr_mls
                    .get_message(&message_id)
                    .map_err(|e| e.to_string())?;
            } else {
                return Err("Message ID not found".to_string());
            }
        } else {
            return Err("Nostr MLS not initialized".to_string());
        }
    }
    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "nostr_mls lock released");

    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Sending event to relays");
    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Relays: {:?}", relays);
    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Event: {:?}", event_to_publish);
    tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Message: {:?}", message);

    if let Some(relays) = relays {
        if let Some(event_to_publish) = event_to_publish {
            let result = wn
                .nostr
                .client
                .send_event_to(relays, &event_to_publish)
                .await
                .map_err(|e| e.to_string())?;

            tracing::debug!(target: "whitenoise::commands::groups::send_mls_message", "Event sent to relays: {:?}", result);
        }
    }

    if let Some(message) = message {
        let tokens = parse(&message.content);
        app_handle
            .emit("mls_message_sent", (&group, &message))
            .expect("Couldn't emit event");
        Ok(MessageWithTokens { message, tokens })
    } else {
        Err("Message not found".to_string())
    }
}

/// Creates an unsigned nostr event with the given parameters
async fn create_unsigned_nostr_event(
    nostr_keys: &Arc<dyn NostrSigner>,
    message: &String,
    kind: u16,
    tags: Option<Vec<Tag>>,
) -> Result<UnsignedEvent, Error> {
    let mut final_tags = tags.unwrap_or_default();
    final_tags.extend(bolt11_invoice_tags(message));

    let mut inner_event = UnsignedEvent::new(
        nostr_keys.get_public_key().await?,
        Timestamp::now(),
        kind.into(),
        final_tags,
        message,
    );
    inner_event.ensure_id();
    Ok(inner_event)
}

/// Parses a message for BOLT11 invoices and returns corresponding tags
fn bolt11_invoice_tags(message: &str) -> Vec<Tag> {
    let mut tags = Vec::new();

    // Bitcoin network prefixes according to BOLT-11 spec
    const NETWORK_PREFIXES: [&str; 4] = ["lnbc", "lntb", "lntbs", "lnbcrt"];

    // Check if message contains what looks like a bolt11 invoice
    if let Some(word) = message.split_whitespace().find(|w| {
        let w_lower = w.to_lowercase();
        NETWORK_PREFIXES
            .iter()
            .any(|prefix| w_lower.starts_with(prefix))
    }) {
        // Try to parse as BOLT11 invoice
        if let Ok(invoice) = SignedRawBolt11Invoice::from_str(word) {
            let raw_invoice = invoice.raw_invoice();
            let amount_msats = raw_invoice
                .amount_pico_btc()
                .map(|pico_btc| (pico_btc as f64 * 0.1) as u64);

            // Add the invoice, amount, and description tag
            if let Some(msats) = amount_msats {
                let mut tag_values = vec![word.to_string(), msats.to_string()];

                // Add description if present
                if let Some(description) = raw_invoice.description() {
                    tag_values.push(description.to_string());
                }

                tags.push(Tag::custom(TagKind::from("bolt11"), tag_values));
            }
        }
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_unsigned_nostr_event_basic() {
        let keys =
            Keys::from_str("nsec1d4ed5x49d7p24xn63flj4985dc4gpfngdhtqcxpth0ywhm6czxcs5l2exj")
                .unwrap();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());
        let message = "Stay humble & stack sats!".to_string();
        let kind = 1;
        let tags = None;

        let result = create_unsigned_nostr_event(&signer, &message, kind, tags).await;

        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.content, message);
        assert!(event.tags.is_empty());
        assert_eq!(event.kind, kind.into());
        assert_eq!(event.pubkey, keys.public_key());
    }

    #[tokio::test]
    async fn test_create_unsigned_nostr_event_with_tags() {
        let keys =
            Keys::from_str("nsec1d4ed5x49d7p24xn63flj4985dc4gpfngdhtqcxpth0ywhm6czxcs5l2exj")
                .unwrap();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());
        let message = "Stay humble & stack sats!".to_string();
        let kind = 1;
        let tags = vec![Tag::reference("test_id")];

        let result = create_unsigned_nostr_event(&signer, &message, kind, Some(tags.clone())).await;

        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.content, message);
        assert_eq!(event.tags.to_vec(), tags);
        assert_eq!(event.kind, kind.into());
        assert_eq!(event.pubkey, keys.public_key());
    }

    #[tokio::test]
    async fn test_create_unsigned_nostr_event_with_bolt11() {
        let keys =
            Keys::from_str("nsec1d4ed5x49d7p24xn63flj4985dc4gpfngdhtqcxpth0ywhm6czxcs5l2exj")
                .unwrap();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());

        // Test case 1: Message with invoice and existing tags
        let invoice = "lnbc15u1p3xnhl2pp5jptserfk3zk4qy42tlucycrfwxhydvlemu9pqr93tuzlv9cc7g3sdqsvfhkcap3xyhx7un8cqzpgxqzjcsp5f8c52y2stc300gl6s4xswtjpc37hrnnr3c9wvtgjfuvqmpm35evq9qyyssqy4lgd8tj637qcjp05rdpxxykjenthxftej7a2zzmwrmrl70fyj9hvj0rewhzj7jfyuwkwcg9g2jpwtk3wkjtwnkdks84hsnu8xps5vsq4gj5hs";
        let message: String = format!("Please pay me here: {}", invoice);
        let existing_tag = Tag::reference("test_id");
        let result =
            create_unsigned_nostr_event(&signer, &message, 1, Some(vec![existing_tag.clone()]))
                .await;

        assert!(result.is_ok());
        let event = result.unwrap();
        let tags_vec = event.tags.to_vec();

        // Check that original tag is preserved
        assert!(tags_vec.contains(&existing_tag));

        // Check bolt11 tag content
        let bolt11_tags: Vec<_> = tags_vec
            .iter()
            .filter(|tag| *tag != &existing_tag)
            .collect();
        assert_eq!(bolt11_tags.len(), 1);

        let tag = &bolt11_tags[0];
        let content = (*tag).clone().to_vec();
        assert_eq!(content[0], "bolt11");
        assert_eq!(content[1], invoice);
        assert!(!content[2].is_empty());
        assert_eq!(content[3], "bolt11.org");

        // Test case 2: Regular message with tags
        let result = create_unsigned_nostr_event(
            &signer,
            &"Just a regular message".to_string(),
            1,
            Some(vec![existing_tag.clone()]),
        )
        .await;

        assert!(result.is_ok());
        let event = result.unwrap();
        let tags_vec = event.tags.to_vec();
        assert!(tags_vec.contains(&existing_tag));
        assert_eq!(tags_vec.len(), 1); // Only the existing tag, no bolt11 tag

        // Test case 3: Invalid invoice
        let result = create_unsigned_nostr_event(
            &signer,
            &"lnbc1invalid".to_string(),
            1,
            Some(vec![existing_tag.clone()]),
        )
        .await;

        assert!(result.is_ok());
        let event = result.unwrap();
        let tags_vec = event.tags.to_vec();
        assert!(tags_vec.contains(&existing_tag));
        assert_eq!(tags_vec.len(), 1); // Only the existing tag, no bolt11 tag
    }

    #[tokio::test]
    async fn test_create_unsigned_nostr_event_with_bolt11_networks() {
        let keys =
            Keys::from_str("nsec1d4ed5x49d7p24xn63flj4985dc4gpfngdhtqcxpth0ywhm6czxcs5l2exj")
                .unwrap();
        let signer: Arc<dyn NostrSigner> = Arc::new(keys.clone());
        let existing_tag = Tag::reference("test_id");

        // Test cases for different network prefixes
        let test_cases = vec![
            // Mainnet invoice (lnbc)
            "lnbc15u1p3xnhl2pp5jptserfk3zk4qy42tlucycrfwxhydvlemu9pqr93tuzlv9cc7g3sdqsvfhkcap3xyhx7un8cqzpgxqzjcsp5f8c52y2stc300gl6s4xswtjpc37hrnnr3c9wvtgjfuvqmpm35evq9qyyssqy4lgd8tj637qcjp05rdpxxykjenthxftej7a2zzmwrmrl70fyj9hvj0rewhzj7jfyuwkwcg9g2jpwtk3wkjtwnkdks84hsnu8xps5vsq4gj5hs",
            // Testnet invoice (lntb)
            "lntb20m1pvjluezsp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygshp58yjmdan79s6qqdhdzgynm4zwqd5d7xmw5fk98klysy043l2ahrqspp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqfpp3x9et2e20v6pu37c5d9vax37wxq72un989qrsgqdj545axuxtnfemtpwkc45hx9d2ft7x04mt8q7y6t0k2dge9e7h8kpy9p34ytyslj3yu569aalz2xdk8xkd7ltxqld94u8h2esmsmacgpghe9k8",
            // Signet invoice (lntbs)
            "lntbs4320n1pnm35s8dqqnp4qg62h96f9rsq0fwq0wff6q2444j8ylp7984srtvxtdth8mmw008qgpp5uad7pp9cjtvde5l67dtakznj9x3fd4qggmeg4z6j5za6zxz0areqsp5dgdv4ugpfsgqmp7vuxpq5s06jxaesg9e7hu32ffjdc2va6cwpt4s9qyysgqcqpcxqyz5vqn94eujdlwdtjxqzu9tycyujzgwsq6xnjw3ycpqfvzk6dl3pk2wrjyja4645xftw7x4m4h9jl3wugczsdn9jeyhv75g63nk83y2848zqpsdqdx7",
            // Regtest invoice (lnbcrt)
            "lnbcrt12340n1pnm35h8pp5dz8c9ytfv0s6h97vp0mwdhmxm4c9jn5wjnyeez9th06t5lag6q4qdqqcqzzsxqyz5vqsp5v6jg8wrl37s6ggf0sc2jd0g6a2axnemyet227ckfwlxgrykclw8s9qxpqysgqy6966qlpgc2frw5307wy2a9f966ksv2f8zx6tatcmdcqpwxn9vp3m9s6eg4cewuprn0wljs3vkfs5cny5nq3n8slme2lvfxf70pzdlsqztw8hc",
        ];

        for invoice in test_cases {
            let message = format!("Please pay me here: {}", invoice);
            let result =
                create_unsigned_nostr_event(&signer, &message, 1, Some(vec![existing_tag.clone()]))
                    .await;

            assert!(result.is_ok());
            let event = result.unwrap();
            let tags_vec = event.tags.to_vec();

            // Check that original tag is preserved
            assert!(tags_vec.contains(&existing_tag));

            // Check bolt11 tag content
            let bolt11_tags: Vec<_> = tags_vec
                .iter()
                .filter(|tag| *tag != &existing_tag)
                .collect();
            assert_eq!(bolt11_tags.len(), 1);

            let tag = &bolt11_tags[0];
            let content = (*tag).clone().to_vec();
            assert_eq!(content[0], "bolt11");
            assert_eq!(content[1], invoice);
            assert!(!content[2].is_empty());
        }
    }
}
