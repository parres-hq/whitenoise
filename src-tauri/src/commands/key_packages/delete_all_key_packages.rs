use crate::accounts::Account;
use crate::relays::RelayType;
use crate::Whitenoise;
use nostr_sdk::event::EventBuilder;
use nostr_sdk::nips::nip09::EventDeletionRequest;
#[tauri::command]
pub async fn delete_all_key_packages(wn: tauri::State<'_, Whitenoise>) -> Result<(), String> {
    let pubkey = wn
        .nostr
        .client
        .signer()
        .await
        .map_err(|e| e.to_string())?
        .get_public_key()
        .await
        .map_err(|e| e.to_string())?;

    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    let key_package_relays: Vec<String> = if cfg!(dev) {
        vec!["ws://localhost:8080".to_string()]
    } else {
        active_account
            .relays(RelayType::KeyPackage, wn.clone())
            .await
            .map_err(|e| e.to_string())?
    };

    let key_package_events = wn
        .nostr
        .query_user_key_packages(pubkey)
        .await
        .map_err(|e| e.to_string())?;

    if !key_package_events.is_empty() {
        let delete_event = EventBuilder::delete(
            EventDeletionRequest::new()
                .ids(key_package_events.iter().map(|e| e.id))
                .reason("Delete own key package"),
        );
        tracing::debug!(target: "whitenoise::commands::key_packages::delete_all_key_packages", "Deleting key packages: {:?}", delete_event);
        wn.nostr
            .client
            .send_event_builder_to(key_package_relays, delete_event)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        tracing::debug!(target: "whitenoise::commands::key_packages::delete_all_key_packages", "No key packages to delete");
    }
    Ok(())
}
