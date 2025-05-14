use crate::key_packages::fetch_key_package_for_pubkey;


/// Checks if a valid MLS key package exists for a given user
///
/// # Arguments
/// * `pubkey` - Hex encoded Nostr public key of the user to check
/// * `wn` - Whitenoise state containing Nostr client
///
/// # Returns
/// * `Ok(bool)` - True if valid key package exists, false otherwise
/// * `Err(String)` - Error message if check fails
///
/// # Errors
/// Returns error if:
/// - Public key is invalid
/// - Network error occurs fetching key package
/// - Key package parsing fails
pub async fn valid_key_package_exists_for_user(pubkey: String) -> Result<bool, String> {
    let key_package = fetch_key_package_for_pubkey(pubkey)
        .await
        .map_err(|e| e.to_string())?;
    Ok(key_package.is_some())
}
