use crate::types::NostrEncryptionMethod;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

#[tauri::command]
pub async fn invite_to_white_noise(
    pubkey: String,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<(), String> {
    let public_key = PublicKey::from_hex(&pubkey).map_err(|e| e.to_string())?;
    let content = "Hi, I'm using White Noise to chat securely on Nostr. Join me! https://github.com/parres-hq/whitenoise/releases".to_string();
    let encrypted_content = wn
        .nostr
        .encrypt_content(content, pubkey, NostrEncryptionMethod::Nip04)
        .await
        .map_err(|e| e.to_string())?;

    let event = EventBuilder::new(Kind::EncryptedDirectMessage, encrypted_content)
        .tag(Tag::public_key(public_key));

    tracing::debug!(
        target: "whitenoise::commands::nostr::invite_to_white_noise",
        "Sending event: {:?}",
        event
    );
    wn.nostr
        .client
        .send_event_builder(event)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
