use crate::accounts::Account;
use crate::groups::Group;
use crate::media::{add_media_file, FileUpload};
use crate::secrets_store;
use crate::whitenoise::Whitenoise;
use lightning_invoice::SignedRawBolt11Invoice;
use nostr_sdk::prelude::*;
use nostr_sdk::NostrSigner;
use std::str::FromStr;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub async fn send_mls_message(
    group: Group,
    message: String,
    kind: u16,
    tags: Option<Vec<Tag>>,
    uploaded_files: Option<Vec<FileUpload>>,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<UnsignedEvent, String> {
    let nostr_keys = wn.nostr.client.signer().await.map_err(|e| e.to_string())?;
    let mut final_tags = tags.unwrap_or_default();
    let mut final_content = message;

    // Get export secret early as we need it for file encryption
    let export_secret_hex;
    let epoch;
    {
        let nostr_mls = wn.nostr_mls.lock().await;
        (export_secret_hex, epoch) = nostr_mls
            .export_secret_as_hex_secret_key_and_epoch(group.mls_group_id.clone())
            .map_err(|e| e.to_string())?;
    }

    // Store the export secret key in the secrets store
    secrets_store::store_mls_export_secret(
        group.mls_group_id.clone(),
        epoch,
        export_secret_hex.clone(),
        wn.data_dir.as_path(),
    )
    .map_err(|e| e.to_string())?;

    let export_nostr_keys = Keys::parse(&export_secret_hex).map_err(|e| e.to_string())?;

    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Process media files if present
    if let Some(uploaded_files) = uploaded_files {
        let mut uploaded_media = Vec::new();
        let files_count = uploaded_files.len();

        // Process files sequentially
        for file in uploaded_files {
            match add_media_file(
                &group.mls_group_id,
                &active_account.pubkey.to_string(),
                file,
                &export_secret_hex,
                wn.data_dir.to_str().unwrap(),
                &wn.database,
                &wn.nostr.blossom,
            )
            .await
            {
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
        create_unsigned_nostr_event(&nostr_keys, final_content, kind, Some(final_tags))
            .await
            .map_err(|e| e.to_string())?;

    tracing::debug!(
        target: "whitenoise::commands::groups::send_mls_message",
        "Sending MLSMessage event to group relays: {:?}",
        inner_event.clone()
    );

    let json_event_string = serde_json::to_string(&inner_event).map_err(|e| e.to_string())?;

    let serialized_message;
    {
        let nostr_mls = wn.nostr_mls.lock().await;
        serialized_message = nostr_mls
            .create_message_for_group(group.mls_group_id.clone(), json_event_string)
            .map_err(|e| e.to_string())?;
    }

    let encrypted_content = nip44::encrypt(
        export_nostr_keys.secret_key(),
        &export_nostr_keys.public_key(),
        &serialized_message,
        nip44::Version::V2,
    )
    .map_err(|e| e.to_string())?;

    let ephemeral_nostr_keys = Keys::generate();

    let published_message_event = EventBuilder::new(Kind::MlsGroupMessage, encrypted_content)
        .tags(vec![Tag::custom(
            TagKind::h(),
            vec![group.nostr_group_id.clone()],
        )])
        .sign(&ephemeral_nostr_keys)
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!(
        target: "whitenoise::commands::groups::send_mls_message",
        "Publishing MLSMessage event to group relays"
    );

    let relays = group.relays(wn.clone()).await.map_err(|e| e.to_string())?;
    let outer_event_id = wn
        .nostr
        .client
        .send_event_to(relays, published_message_event)
        .await
        .map_err(|e| e.to_string())?;

    group
        .add_message(
            outer_event_id.id().to_string(),
            inner_event.clone(),
            wn.clone(),
            app_handle.clone(),
        )
        .await
        .map_err(|e| e.to_string())?;

    app_handle
        .emit("mls_message_sent", (group.clone(), inner_event.clone()))
        .expect("Couldn't emit event");

    Ok(inner_event)
}

/// Creates an unsigned nostr event with the given parameters
async fn create_unsigned_nostr_event(
    nostr_keys: &Arc<dyn NostrSigner>,
    message: String,
    kind: u16,
    tags: Option<Vec<Tag>>,
) -> Result<UnsignedEvent, Error> {
    let mut final_tags = tags.unwrap_or_default();
    final_tags.extend(bolt11_invoice_tags(&message));

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

        let result = create_unsigned_nostr_event(&signer, message.clone(), kind, tags).await;

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

        let result =
            create_unsigned_nostr_event(&signer, message.clone(), kind, Some(tags.clone())).await;

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
            create_unsigned_nostr_event(&signer, message, 1, Some(vec![existing_tag.clone()]))
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
            "Just a regular message".to_string(),
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
            "lnbc1invalid".to_string(),
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
                create_unsigned_nostr_event(&signer, message, 1, Some(vec![existing_tag.clone()]))
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
