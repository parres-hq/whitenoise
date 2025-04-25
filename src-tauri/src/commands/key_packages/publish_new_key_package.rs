use crate::Whitenoise;

/// Publishes a new MLS key package for the active account to Nostr
#[tauri::command]
pub async fn publish_new_key_package(wn: tauri::State<'_, Whitenoise>) -> Result<(), String> {
    crate::key_packages::publish_key_package(wn)
        .await
        .map_err(|e| e.to_string())
}
