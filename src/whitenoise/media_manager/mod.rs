//! Media handling module for the Whitenoise application.
//!
//! This module provides functionality for handling media files in the application:
//! - File encryption and decryption using ChaCha20-Poly1305
//! - File upload to the Blossom server
//! - Local caching of media files
//! - Generation of IMETA tags for Nostr events
//! - Image processing and metadata extraction
//! - Media sanitization and security checks
//!
//! The module is designed to work with the following workflow:
//! 1. Files are sanitized to remove sensitive metadata
//! 2. Files are encrypted before upload
//! 3. Encrypted files are uploaded to Blossom
//! 4. Original files are cached locally
//! 5. IMETA tags are generated for Nostr events
//!
//! # Security
//!
//! All files are encrypted using ChaCha20-Poly1305 before upload to ensure
//! end-to-end encryption. The encryption key is derived from the exporter secret.
//! Files are also sanitized to remove potentially sensitive metadata before being
//! processed or stored.
//!
//! # Caching
//!
//! Files are cached locally to improve performance and reduce bandwidth usage.
//! The cache is organized by MLS group ID and uses SHA256 hashes for file identification.
//!
//! # IMETA Tags
//!
//! IMETA tags are generated for Nostr events containing:
//! - File URL
//! - MIME type
//! - Original filename
//! - For images: dimensions and blurhash
//! - SHA256 hash of the original file
//! - Decryption information (nonce and algorithm)

mod cache;
mod encryption;
mod errors;
mod sanitizer;
mod types;

use anyhow::anyhow;
pub use errors::MediaError;
use nostr_blossom::client::BlossomClient;
use nwc::nostr::hashes::Hash;
pub use sanitizer::sanitize_media;
pub use types::*;

use nostr_mls::prelude::*;
use nostr_sdk::hashes::sha256::Hash as Sha256Hash;
use sha2::{Digest, Sha256};

use crate::whitenoise::accounts::Account;
use crate::whitenoise::error::{BlossomError, Result};
use crate::whitenoise::media_manager::encryption::decrypt_file;
use crate::{MessageWithTokens, Whitenoise, WhitenoiseError};

pub struct MediaManager {
    blossom: BlossomClient,
}

impl MediaManager {
    pub fn new() -> Self {
        let blossom = if cfg!(debug_assertions) {
            BlossomClient::new(Url::parse("http://localhost:3000").unwrap())
        } else {
            BlossomClient::new(Url::parse("https://blossom.primal.net/").unwrap())
        };
        Self { blossom }
    }
    /// Adds a media file, ready to be used in a chat.
    ///
    /// This method handles the complete workflow for adding a media file:
    /// 1. Encrypts the file using ChaCha20-Poly1305
    /// 2. Uploads the encrypted file to Blossom
    /// 3. Caches the original file locally
    /// 4. Generates an IMETA tag with file metadata
    ///
    /// # Arguments
    ///
    /// * `account` - The account of the user
    /// * `group_id` - The MLS group_id that the media file belongs to
    /// * `file` - The file to be added, containing filename, MIME type, and data
    /// * `encrypted_data` - Encrypted data that will be stored in Blossom
    /// * `whitenoise` - The Whitenoise state
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Success indicator
    /// * `Err(MediaError)` - Error if any step of the process fails
    pub(crate) async fn upload_media_file(
        &self,
        account: &Account,
        group_id: &GroupId,
        file: FileDetails,
        encrypted_data: &[u8],
        whitenoise: &Whitenoise,
    ) -> Result<()> {
        // Calculate file hash
        let mut hasher = Sha256::new();
        hasher.update(encrypted_data);
        let encrypted_file_hash = hasher.finalize();

        if let Some(_cached) = whitenoise
            .fetch_cached_file(group_id, &encrypted_file_hash, whitenoise)
            .await?
        {
            tracing::info!("File was already uploaded to blossom");
            return Ok(());
        }

        // Upload file to Blossom
        let nostr_keys = whitenoise
            .secrets_store
            .get_nostr_keys_for_pubkey(&account.pubkey)?;
        let _blob_descriptor = self
            .blossom
            .upload_blob(encrypted_data.to_vec(), None, None, Some(&nostr_keys))
            .await
            .map_err(BlossomError::from)?;

        // Add the file to the local cache
        whitenoise
            .add_to_cache(&file.data, group_id, &account.pubkey, &encrypted_file_hash)
            .await?;

        Ok(())
    }
}

impl Whitenoise {
    pub async fn decode_media_message(
        &self,
        account: &Account,
        group_id: &GroupId,
        message: message_types::Message,
    ) -> Result<CachedMediaFile> {
        if message.kind != Kind::FileMetadata {
            return Err(WhitenoiseError::Other(anyhow!("Not a media message")));
        }
        let mut mime: Option<String> = None;
        let mut sha256: Option<String> = None;
        let mut nonce: Option<Vec<u8>> = None;

        for tag in message.tags {
            match tag.kind() {
                TagKind::SingleLetter(letter) => {
                    if letter == SingleLetterTag::lowercase(Alphabet::M) {
                        mime = tag.content().map(|s| s.to_string());
                    }

                    if letter == SingleLetterTag::lowercase(Alphabet::X) {
                        sha256 = tag.content().map(|s| s.to_string());
                    }

                    if letter == SingleLetterTag::lowercase(Alphabet::N) {
                        nonce = tag
                            .content()
                            .map(|s| hex::decode(s).map_err(MediaError::from))
                            .transpose()?;
                    }
                }
                _ => {}
            }
        }
        match (mime, sha256, nonce) {
            (Some(_mime), Some(sha256), Some(nonce)) => {
                let encrypted_file_hash = hex::decode(&sha256).map_err(|_| BlossomError::InvalidSha256)?;
                match self.fetch_cached_file(group_id, &encrypted_file_hash, self).await? {
                    Some(cached_file) => Ok(cached_file),
                    None => {
                        // Download media from blossom
                        let hash_bytes: [u8; 32] = encrypted_file_hash.clone()
                            .try_into()
                            .map_err(|_| BlossomError::InvalidSha256)?;
                        let sha256 = Sha256Hash::from_byte_array(hash_bytes);
                        let encrypted_bytes = self
                            .media
                            .blossom
                            .get_blob(sha256, None, None, Option::<&Keys>::None)
                            .await
                            .map_err(BlossomError::from)?;
                        // Get the raw secret key bytes
                        let nostr_mls =
                            Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
                        let exporter_secret = nostr_mls.exporter_secret(group_id)?;
                        let decrypted_bytes = decrypt_file(&encrypted_bytes, &exporter_secret.secret, &nonce)?;
                        let media_file = self.add_to_cache(&decrypted_bytes, group_id, &account.pubkey, &encrypted_file_hash).await?;
                        Ok(CachedMediaFile { media_file, file_data: decrypted_bytes })
                    }
                }
            }
            _ => Err(WhitenoiseError::Media(MediaError::Metadata(
                "Missing fields".to_owned(),
            ))),
        }
    }

    pub async fn send_media_message(
        &self,
        account: &Account,
        group_id: &GroupId,
        file: FileDetails,
        caption: String,
    ) -> Result<MessageWithTokens> {
        // Encrypt the file data
        // Get the raw secret key bytes
        let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
        let exporter_secret = nostr_mls.exporter_secret(group_id)?;
        // Sanitize the file
        let sanitized_file = sanitize_media(&file)?;

        // Encrypt the file
        let (encrypted_file_data, nonce) =
            encryption::encrypt_file(&sanitized_file.data, &exporter_secret.secret)?;

        // Upload media file to blossom if necessary
        let mime = file.mime_type.clone();
        self.media
            .upload_media_file(
                account,
                group_id,
                file, &encrypted_file_data, self,
            )
            .await?;
        // Calculate file hash of the encrypted data uploaded to blossom
        let mut hasher = Sha256::new();
        hasher.update(encrypted_file_data);
        let encrypted_file_hash = hex::encode(&hasher.finalize());

        let tags = vec![
            Tag::parse(["url", "default"])?,
            Tag::parse(["m", &mime])?,
            Tag::parse(["x", &encrypted_file_hash])?,
            Tag::parse(["n", &hex::encode(&nonce)])?,
        ];
        self.send_message_to_group(
            account,
            group_id,
            caption,
            Kind::FileMetadata.as_u16(),
            Some(tags),
        )
        .await
    }
}
