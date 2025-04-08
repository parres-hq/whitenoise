use crate::accounts::Account;
use crate::relays::RelayType;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;

#[tauri::command]
pub async fn publish_relay_list(
    relays: Vec<String>,
    kind: u64,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<(), String> {
    let signer = wn.nostr.client.signer().await.map_err(|e| e.to_string())?;

    let mut tags: Vec<Tag> = Vec::new();
    for relay in relays.clone() {
        tags.push(Tag::custom(TagKind::Relay, [relay]));
    }

    let event_kind = match kind {
        10050 => Kind::InboxRelays,
        10051 => Kind::MlsKeyPackageRelays,
        _ => return Err("Invalid relay list kind".to_string()),
    };

    let event = EventBuilder::new(event_kind, "")
        .tags(tags)
        .sign(&signer)
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!("Publishing relay list: {:?}", event);

    wn.nostr
        .client
        .send_event(&event)
        .await
        .map_err(|e| e.to_string())?;

    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    match kind {
        10050 => {
            active_account
                .update_relays(RelayType::Inbox, &relays, wn.clone())
                .await
                .map_err(|e| format!("Failed to update relays: {}", e))?;
        }
        10051 => {
            active_account
                .update_relays(RelayType::KeyPackage, &relays, wn.clone())
                .await
                .map_err(|e| format!("Failed to update relays: {}", e))?;
        }
        _ => return Err("Invalid relay list kind".to_string()),
    }

    tracing::debug!("Relay list published & relays updated");

    for relay in relays.clone() {
        wn.nostr
            .client
            .add_relay(&relay)
            .await
            .map_err(|e| e.to_string())?;
        wn.nostr
            .client
            .connect_relay(&relay)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
