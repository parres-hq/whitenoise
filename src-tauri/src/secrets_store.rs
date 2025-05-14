use base64::{engine::general_purpose, Engine as _};
// use keyring::Entry;
use nostr_sdk::Keys;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
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
/// * `Result<()>` - Ok(()) if the operation was successful or if the key didn't exist, or an error if the Entry creation fails.
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
