use base64::{engine::general_purpose, Engine as _};
// use keyring::Entry;
use nostr_sdk::{util::hex, Keys};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::is_dev;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum SecretsStoreError {
    #[error("Failed to parse JSON: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("UUID error: {0}")]
    UuidError(#[from] uuid::Error),

    #[error("File error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Base64 error: {0}")]
    Base64Error(#[from] base64::DecodeError),

    #[error("UTF-8 error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error("Keyring error: {0}")]
    KeyringError(#[from] keyring::Error),

    #[error("Key error: {0}")]
    KeyError(#[from] nostr_sdk::key::Error),

    #[error("Key not found")]
    KeyNotFound,
}

pub type Result<T> = std::result::Result<T, SecretsStoreError>;

#[allow(dead_code)]
fn get_service_name() -> String {
    match is_dev() {
        true => "White Noise Dev".to_string(),
        false => "White Noise".to_string(),
    }
}

fn get_device_key(data_dir: &Path) -> Vec<u8> {
    let uuid_file = data_dir.join("whitenoise_uuid");

    let uuid = if uuid_file.exists() {
        // Read existing UUID
        std::fs::read_to_string(&uuid_file)
            .map_err(SecretsStoreError::FileError)
            .and_then(|s| s.parse::<Uuid>().map_err(SecretsStoreError::UuidError))
    } else {
        // Generate new UUID
        let new_uuid = Uuid::new_v4();
        let _ = std::fs::create_dir_all(data_dir).map_err(SecretsStoreError::FileError);
        let _ =
            std::fs::write(uuid_file, new_uuid.to_string()).map_err(SecretsStoreError::FileError);
        Ok(new_uuid)
    };

    uuid.expect("Couldn't unwrap UUID").as_bytes().to_vec()
}

fn get_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("whitenoise.json")
}

fn obfuscate(data: &str, data_dir: &Path) -> String {
    let device_key = get_device_key(data_dir);
    let xored: Vec<u8> = data
        .as_bytes()
        .iter()
        .zip(device_key.iter().cycle())
        .map(|(&x1, &x2)| x1 ^ x2)
        .collect();
    general_purpose::STANDARD_NO_PAD.encode(xored)
}

fn deobfuscate(data: &str, data_dir: &Path) -> Result<String> {
    let device_key = get_device_key(data_dir);
    let decoded = general_purpose::STANDARD_NO_PAD
        .decode(data)
        .map_err(SecretsStoreError::Base64Error)?;
    let xored: Vec<u8> = decoded
        .iter()
        .zip(device_key.iter().cycle())
        .map(|(&x1, &x2)| x1 ^ x2)
        .collect();
    String::from_utf8(xored).map_err(SecretsStoreError::Utf8Error)
}

fn read_secrets_file(data_dir: &Path) -> Result<Value> {
    let content = match fs::read_to_string(get_file_path(data_dir)) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::from("{}"),
        Err(e) => return Err(e.into()),
    };
    Ok(serde_json::from_str(&content)?)
}

fn write_secrets_file(data_dir: &Path, secrets: &Value) -> Result<()> {
    let content = serde_json::to_string_pretty(secrets)?;
    fs::write(get_file_path(data_dir), content)?;
    Ok(())
}

/// Stores the private key associated with the given Keys in the system's keyring.
///
/// This function takes a reference to a `Keys` object and stores the private key
/// in the system's keyring, using the public key as an identifier.
///
/// # Arguments
///
/// * `keys` - A reference to a `Keys` object containing the keypair to store.
/// * `file_path` - The path to the secrets file.
///
/// # Returns
///
/// * `Result<()>` - Ok(()) if the operation was successful, or an error if it failed.
///
/// # Errors
///
/// This function will return an error if:
/// * The Entry creation fails
/// * Setting the password in the keyring fails
/// * The secret key cannot be retrieved from the keypair
pub fn store_private_key(keys: &Keys, data_dir: &Path) -> Result<()> {
    let mut secrets = read_secrets_file(data_dir).unwrap_or(json!({}));
    let obfuscated_key = obfuscate(keys.secret_key().to_secret_hex().as_str(), data_dir);
    secrets[keys.public_key().to_hex()] = json!(obfuscated_key);
    write_secrets_file(data_dir, &secrets)?;

    // if cfg!(target_os = "android") {
    //     let mut secrets = read_secrets_file(data_dir).unwrap_or(json!({}));
    //     let obfuscated_key = obfuscate(keys.secret_key().to_secret_hex().as_str(), data_dir);
    //     secrets[keys.public_key().to_hex()] = json!(obfuscated_key);
    //     write_secrets_file(data_dir, &secrets)?;
    // } else {
    //     let service = get_service_name();
    //     let entry = Entry::new(service.as_str(), keys.public_key().to_hex().as_str())
    //         .map_err(SecretsStoreError::KeyringError)?;
    //     entry
    //         .set_password(keys.secret_key().to_secret_hex().as_str())
    //         .map_err(SecretsStoreError::KeyringError)?;
    // }

    Ok(())
}

/// Retrieves the Nostr keys associated with a given public key from the system's keyring.
///
/// This function looks up the private key stored in the system's keyring using the provided
/// public key as an identifier, and then constructs a `Keys` object from the retrieved private key.
///
/// # Arguments
///
/// * `pubkey` - A string slice containing the public key to look up.
/// * `file_path` - The path to the secrets file.
///
/// # Returns
///
/// * `Result<Keys>` - A Result containing the `Keys` object if successful, or an error if the operation fails.
///
/// # Errors
///
/// This function will return an error if:
/// * The Entry creation fails
/// * Retrieving the password from the keyring fails
/// * Parsing the private key into a `Keys` object fails
pub fn get_nostr_keys_for_pubkey(pubkey: &str, data_dir: &Path) -> Result<Keys> {
    let secrets = read_secrets_file(data_dir)?;
    let obfuscated_key = secrets[pubkey]
        .as_str()
        .ok_or(SecretsStoreError::KeyNotFound)?;
    let private_key = deobfuscate(obfuscated_key, data_dir)?;
    Keys::parse(&private_key).map_err(SecretsStoreError::KeyError)

    // if cfg!(target_os = "android") {
    //     let secrets = read_secrets_file(data_dir)?;
    //     let obfuscated_key = secrets[pubkey]
    //         .as_str()
    //         .ok_or(SecretsStoreError::KeyNotFound)?;
    //     let private_key = deobfuscate(obfuscated_key, data_dir)?;
    //     Keys::parse(private_key).map_err(SecretsStoreError::KeyError)
    // } else {
    //     let service = get_service_name();
    //     let entry =
    //         Entry::new(service.as_str(), pubkey).map_err(SecretsStoreError::KeyringError)?;
    //     let private_key = entry
    //         .get_password()
    //         .map_err(SecretsStoreError::KeyringError)?;
    //     Keys::parse(private_key).map_err(SecretsStoreError::KeyError)
    // }
}

/// Removes the private key associated with a given public key from the system's keyring.
///
/// This function attempts to delete the credential entry for the specified public key
/// from the system's keyring. If the entry doesn't exist or the deletion fails, the
/// function will still return Ok(()) to maintain idempotency.
///
/// # Arguments
///
/// * `pubkey` - A string slice containing the public key for which to remove the associated private key.
/// * `file_path` - The path to the secrets file.
///
/// # Returns
///
/// * `Result<()>` - Ok(()) if the operation was successful or if the key didn't exist,
///                  or an error if the Entry creation fails.
///
/// # Errors
///
/// This function will return an error if:
/// * The Entry creation fails
pub fn remove_private_key_for_pubkey(pubkey: &str, data_dir: &Path) -> Result<()> {
    let mut secrets = read_secrets_file(data_dir)?;
    secrets.as_object_mut().map(|obj| obj.remove(pubkey));
    write_secrets_file(data_dir, &secrets)?;

    // if cfg!(target_os = "android") {
    //     let mut secrets = read_secrets_file(data_dir)?;
    //     secrets.as_object_mut().map(|obj| obj.remove(pubkey));
    //     write_secrets_file(data_dir, &secrets)?;
    // } else {
    //     let service = get_service_name();
    //     let entry = Entry::new(service.as_str(), pubkey);
    //     if let Ok(entry) = entry {
    //         let _ = entry.delete_credential();
    //     }
    // }
    Ok(())
}

/// Stores the MLS export secret for a specific group and epoch in the system's keyring.
///
/// This function creates a unique key by combining the group ID and epoch, then stores
/// the provided secret in the system's keyring using this key.
///
/// # Arguments
///
/// * `mls_group_id` - A vector of bytes containing the ID of the MLS group.
/// * `epoch` - The epoch number as a u64.
/// * `secret` - A string slice containing the export secret to be stored.
/// * `file_path` - The path to the secrets file.
///
/// # Returns
///
/// * `Result<()>` - Ok(()) if the operation was successful, or an error if it fails.
///
/// # Errors
///
/// This function will return an error if:
/// * The Entry creation fails
/// * Setting the password in the keyring fails
pub fn store_mls_export_secret(
    mls_group_id: Vec<u8>,
    epoch: u64,
    secret: String,
    data_dir: &Path,
) -> Result<()> {
    let mls_group_id_hex = hex::encode(&mls_group_id);
    let key = format!("{mls_group_id_hex}:{epoch}");

    let mut secrets = read_secrets_file(data_dir).unwrap_or(json!({}));
    let obfuscated_secret = obfuscate(&secret, data_dir);
    secrets[key] = json!(obfuscated_secret);
    write_secrets_file(data_dir, &secrets)?;

    // if cfg!(target_os = "android") {
    //     let mut secrets = read_secrets_file(data_dir).unwrap_or(json!({}));
    //     let obfuscated_secret = obfuscate(&secret, data_dir);
    //     secrets[key] = json!(obfuscated_secret);
    //     write_secrets_file(data_dir, &secrets)?;
    // } else {
    // let service = get_service_name();
    //     let entry = Entry::new(service.as_str(), key.as_str())?;
    //     entry.set_password(&secret)?;
    // }
    Ok(())
}

/// Retrieves the export secret keys for a specific MLS group and epoch from the system's keyring.
///
/// This function constructs a unique key by combining the group ID and epoch, then retrieves
/// the corresponding secret from the system's keyring. It then parses this secret into Keys.
///
/// # Arguments
///
/// * `mls_group_id` - A vector of bytes containing the ID of the MLS group.
/// * `epoch` - The epoch number as a u64.
/// * `file_path` - The path to the secrets file.
///
/// # Returns
///
/// * `Result<Keys>` - Ok(Keys) if the operation was successful, or an error if it fails.
///
/// # Errors
///
/// This function will return an error if:
/// * The Entry creation fails
/// * Retrieving the password from the keyring fails
/// * Parsing the secret into Keys fails
pub fn get_export_secret_keys_for_group(
    mls_group_id: Vec<u8>,
    epoch: u64,
    data_dir: &Path,
) -> Result<Keys> {
    let mls_group_id_hex = hex::encode(&mls_group_id);
    let key = format!("{mls_group_id_hex}:{epoch}");

    let secrets = read_secrets_file(data_dir)?;
    let obfuscated_secret = secrets[key]
        .as_str()
        .ok_or(SecretsStoreError::KeyNotFound)?;
    let secret = deobfuscate(obfuscated_secret, data_dir)?;
    let keys = Keys::parse(&secret).map_err(SecretsStoreError::KeyError)?;
    Ok(keys)

    // if cfg!(target_os = "android") {
    //     let secrets = read_secrets_file(data_dir)?;
    //     let obfuscated_secret = secrets[key]
    //         .as_str()
    //         .ok_or(SecretsStoreError::KeyNotFound)?;
    //     let secret = deobfuscate(obfuscated_secret, data_dir)?;
    //     let keys = Keys::parse(secret).map_err(SecretsStoreError::KeyError)?;
    //     Ok(keys)
    // } else {
    // let service = get_service_name();
    //     let entry = Entry::new(service.as_str(), key.as_str())?;
    //     let secret = entry
    //         .get_password()
    //         .map_err(SecretsStoreError::KeyringError)?;
    //     let keys = Keys::parse(secret).map_err(SecretsStoreError::KeyError)?;
    //     Ok(keys)
    // }
}

/// Stores the NWC (Nostr Wallet Connect) URI for a specific public key in the secrets store.
///
/// # Arguments
///
/// * `pubkey` - The public key to associate the NWC URI with
/// * `nostr_wallet_connect_uri` - The NWC URI to store
/// * `data_dir` - Path to the data directory
///
/// # Returns
///
/// * `Result<()>` - Ok(()) if successful, or an error if the operation fails
pub fn store_nostr_wallet_connect_uri(
    pubkey: &str,
    nostr_wallet_connect_uri: &str,
    data_dir: &Path,
) -> Result<()> {
    let mut secrets = read_secrets_file(data_dir).unwrap_or(json!({}));
    let key = format!("nwc:{}", pubkey);
    let obfuscated_uri = obfuscate(nostr_wallet_connect_uri, data_dir);
    secrets[key] = json!(obfuscated_uri);
    write_secrets_file(data_dir, &secrets)?;
    Ok(())
}

/// Retrieves the NWC URI for a specific public key from the secrets store.
///
/// # Arguments
///
/// * `pubkey` - The public key to get the NWC URI for
/// * `data_dir` - Path to the data directory
///
/// # Returns
///
/// * `Result<Option<String>>` - Some(uri) if found, None if not found, or an error if operation fails
pub fn get_nostr_wallet_connect_uri(pubkey: &str, data_dir: &Path) -> Result<Option<String>> {
    let secrets = read_secrets_file(data_dir)?;
    let key = format!("nwc:{}", pubkey);

    match secrets[key].as_str() {
        Some(obfuscated_uri) => Ok(Some(deobfuscate(obfuscated_uri, data_dir)?)),
        None => Ok(None),
    }
}

/// Removes the NWC URI for a specific public key from the secrets store.
///
/// # Arguments
///
/// * `pubkey` - The public key to remove the NWC URI for
/// * `data_dir` - Path to the data directory
///
/// # Returns
///
/// * `Result<()>` - Ok(()) if successful, or an error if the operation fails
pub fn remove_nostr_wallet_connect_uri(pubkey: &str, data_dir: &Path) -> Result<()> {
    let mut secrets = read_secrets_file(data_dir)?;
    let key = format!("nwc:{}", pubkey);
    secrets.as_object_mut().map(|obj| obj.remove(&key));
    write_secrets_file(data_dir, &secrets)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_temp_dir() -> TempDir {
        TempDir::new().expect("Failed to create temp directory")
    }

    #[test]
    fn test_store_and_retrieve_private_key() -> Result<()> {
        let temp_dir = setup_temp_dir();
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();

        // Store the private key
        store_private_key(&keys, temp_dir.path())?;

        // Retrieve the keys
        let retrieved_keys = get_nostr_keys_for_pubkey(&pubkey, temp_dir.path())?;

        assert_eq!(keys.public_key(), retrieved_keys.public_key());
        assert_eq!(keys.secret_key(), retrieved_keys.secret_key());

        // Clean up
        remove_private_key_for_pubkey(&pubkey, temp_dir.path())?;

        Ok(())
    }

    #[test]
    fn test_remove_private_key() -> Result<()> {
        let temp_dir = setup_temp_dir();
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();

        // Store the private key
        store_private_key(&keys, temp_dir.path())?;

        // Remove the private key
        remove_private_key_for_pubkey(&pubkey, temp_dir.path())?;

        // Attempt to retrieve the removed key
        let result = get_nostr_keys_for_pubkey(&pubkey, temp_dir.path());

        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_get_nonexistent_key() {
        let temp_dir = setup_temp_dir();
        let nonexistent_pubkey = "nonexistent_pubkey";
        let result = get_nostr_keys_for_pubkey(nonexistent_pubkey, temp_dir.path());

        assert!(result.is_err());
    }

    #[test]
    fn test_store_and_retrieve_mls_export_secret() -> Result<()> {
        let temp_dir = setup_temp_dir();
        let group_id = vec![0u8; 32];
        let epoch = 42;
        let secret =
            String::from("9b9da9c6ee9a62016ab2db1a3397d267a575c02266c6ca9b5ec8e015db67c30e");

        // Store the MLS export secret
        store_mls_export_secret(group_id.clone(), epoch, secret.clone(), temp_dir.path())?;

        // Retrieve the keys
        let retrieved_keys =
            get_export_secret_keys_for_group(group_id.clone(), epoch, temp_dir.path())?;

        // Verify that the retrieved keys match the original secret
        assert_eq!(retrieved_keys.secret_key().to_secret_hex(), secret);

        Ok(())
    }

    #[test]
    fn test_get_nonexistent_mls_export_secret() {
        let temp_dir = setup_temp_dir();
        let nonexistent_group_id = vec![0u8; 32];
        let nonexistent_epoch = 999;

        let result = get_export_secret_keys_for_group(
            nonexistent_group_id,
            nonexistent_epoch,
            temp_dir.path(),
        );

        assert!(result.is_err());
    }

    #[test]
    #[cfg(target_os = "android")]
    fn test_android_store_and_retrieve_private_key() -> Result<()> {
        let temp_dir = setup_temp_dir();
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();

        // Store the private key
        store_private_key(&keys, temp_dir.path())?;

        // Retrieve the keys
        let retrieved_keys = get_nostr_keys_for_pubkey(&pubkey, temp_dir.path())?;

        assert_eq!(keys.public_key(), retrieved_keys.public_key());
        assert_eq!(keys.secret_key(), retrieved_keys.secret_key());

        // Verify that the key is stored in the file
        let secrets = read_secrets_file(temp_dir.path())?;
        assert!(secrets.get(&pubkey).is_some());

        // Clean up
        remove_private_key_for_pubkey(&pubkey, temp_dir.path())?;

        // Verify that the key is removed from the file
        let secrets = read_secrets_file(temp_dir.path())?;
        assert!(secrets.get(&pubkey).is_none());

        Ok(())
    }

    #[test]
    #[cfg(target_os = "android")]
    fn test_android_store_and_retrieve_mls_export_secret() -> Result<()> {
        let temp_dir = setup_temp_dir();
        let group_id = "test_group";
        let epoch = 42;
        let secret = "9b9da9c6ee9a62016ab2db1a3397d267a575c02266c6ca9b5ec8e015db67c30e";

        // Store the MLS export secret
        store_mls_export_secret(group_id, epoch, secret, temp_dir.path())?;

        // Retrieve the keys
        let retrieved_keys = get_export_secret_keys_for_group(group_id, epoch, temp_dir.path())?;

        // Verify that the retrieved keys match the original secret
        assert_eq!(retrieved_keys.secret_key().to_secret_hex(), secret);

        // Verify that the secret is stored in the file
        let secrets = read_secrets_file(temp_dir.path())?;
        let key = format!("{group_id}:{epoch}");
        assert!(secrets.get(&key).is_some());

        Ok(())
    }

    #[test]
    fn test_store_and_retrieve_nostr_wallet_connect_uri() -> Result<()> {
        let temp_dir = setup_temp_dir();
        let pubkey = "test_pubkey";
        let nostr_wallet_connect_uri = "nostr+walletconnect://abcdef1234567890?secret=mysecret";

        // Test non-existent URI returns None
        let result = get_nostr_wallet_connect_uri(pubkey, temp_dir.path())?;
        assert!(result.is_none());

        // Store the NWC URI
        store_nostr_wallet_connect_uri(pubkey, nostr_wallet_connect_uri, temp_dir.path())?;

        // Retrieve the NWC URI
        let retrieved_uri =
            get_nostr_wallet_connect_uri(pubkey, temp_dir.path())?.expect("URI should exist");
        assert_eq!(nostr_wallet_connect_uri, retrieved_uri);

        // Clean up
        remove_nostr_wallet_connect_uri(pubkey, temp_dir.path())?;

        // Verify removal returns None
        let result = get_nostr_wallet_connect_uri(pubkey, temp_dir.path())?;
        assert!(result.is_none());

        Ok(())
    }
}
