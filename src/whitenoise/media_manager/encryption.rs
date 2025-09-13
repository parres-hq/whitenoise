//! This module contains functions for encrypting and decrypting file data.
//! It uses ChaCha20-Poly1305 encryption, which provides authenticated encryption
//! with better cross-platform performance characteristics compared to AES-GCM.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;

use super::errors::MediaError;
/// Encrypts file data using ChaCha20-Poly1305 encryption.
///
/// # Arguments
/// * `data` - The raw file data to encrypt
/// * `key` - The 32-byte key to use for encryption
///
/// # Returns
/// * `Ok((Vec<u8>, Vec<u8>))` - The encrypted data and nonce
/// * `Err(MediaError)` - Error if encryption fails
pub fn encrypt_file(data: &[u8], key: &[u8; 32]) -> Result<(Vec<u8>, [u8; 12]), MediaError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .encrypt(nonce, data)
        .map(|encrypted| (encrypted, nonce_bytes))
        .map_err(|e| MediaError::Encryption(e.to_string()))
}

/// Decrypts file data using ChaCha20-Poly1305 encryption.
///
/// # Arguments
/// * `data` - The encrypted file data to decrypt
/// * `key` - The 32-byte key to use for decryption
/// * `nonce` - The nonce used for encryption
///
/// # Returns
/// * `Ok(Vec<u8>)` - The decrypted data
/// * `Err(MediaError)` - Error if decryption fails
#[allow(dead_code)]
pub fn decrypt_file(data: &[u8], key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, MediaError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(nonce.into(), data)
        .map_err(|e| MediaError::Decryption(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    #[tokio::test]
    async fn test_encrypt_file() {
        let keys = Keys::generate();
        let data = b"test data";

        let encrypted = encrypt_file(data, &keys.secret_key().to_secret_bytes()).unwrap();

        // Encrypted data should be different from original
        assert_ne!(encrypted.0, data);

        // Encrypted data should be longer due to encryption overhead
        assert!(encrypted.0.len() > data.len());
    }

    #[tokio::test]
    async fn test_decrypt_file() {
        let keys = Keys::generate();
        let data = b"test data";

        let encrypted = encrypt_file(data, &keys.secret_key().to_secret_bytes()).unwrap();

        let decrypted = decrypt_file(
            &encrypted.0,
            &keys.secret_key().to_secret_bytes(),
            &encrypted.1,
        )
        .unwrap();

        assert_eq!(decrypted, data);
    }
}
