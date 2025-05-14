

/// Publishes a new MLS key package for the active account to Nostr
pub async fn publish_new_key_package() -> Result<(), String> {
    crate::key_packages::publish_key_package()
        .await
        .map_err(|e| e.to_string())
}
