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

pub mod blossom;
mod cache;
mod encryption;
mod errors;
mod sanitizer;
mod types;

pub use errors::MediaError;
pub use sanitizer::sanitize_media;
pub use types::*;

use ::image::GenericImageView;
use nostr_mls::prelude::*;

use crate::accounts::Account;
use crate::database::Database;
use crate::Whitenoise;

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
/// * `group` - The MLS group that the media file belongs to
/// * `uploaded_file` - The file to be added, containing filename, MIME type, and data
/// * `wn` - The Whitenoise state
///
/// # Returns
///
/// * `Ok(UploadedMedia)` - The uploaded media descriptor and IMETA tag
/// * `Err(MediaError)` - Error if any step of the process fails
pub async fn add_media_file(
    group: &group_types::Group,
    uploaded_file: FileUpload,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<UploadedMedia, MediaError> {
    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|_| MediaError::NoActiveAccount)?;

    // Get the raw secret key bytes
    let exporter_secret: group_types::GroupExporterSecret;
    let nostr_mls_guard = wn.nostr_mls.lock().await;
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        exporter_secret = nostr_mls
            .exporter_secret(&group.mls_group_id)
            .map_err(|e| MediaError::NostrMLS(e.to_string()))?;
    } else {
        return Err(MediaError::NostrMLSNotInitialized);
    }

    // Sanitize the file
    let sanitized_file = sanitize_media(&uploaded_file)?;

    // Encrypt the file
    let (encrypted_file_data, nonce) =
        encryption::encrypt_file(&sanitized_file.data, &exporter_secret.secret)?;

    // Upload encrypted file to Blossom
    let (blob_descriptor, keys) = wn
        .nostr
        .blossom
        .upload(encrypted_file_data)
        .await
        .map_err(|e| MediaError::Upload(e.to_string()))?;

    // Add the file to the local cache
    let media_file = cache::add_to_cache(
        &uploaded_file.data,
        group,
        &active_account.pubkey.to_string(),
        Some(blob_descriptor.url.clone()),
        Some(keys.secret_key().to_secret_hex()),
        Some(sanitized_file.metadata),
        wn.data_dir.to_str().unwrap(),
        &wn.database,
    )
    .await?;

    // Generate IMETA values
    let mut imeta_values =
        generate_imeta_tag_values(&uploaded_file, &blob_descriptor, &media_file.file_hash)
            .map_err(|e| MediaError::Metadata(e.to_string()))?;

    // Add nonce to the IMETA tag
    let nonce_hex = hex::encode(nonce);
    imeta_values.push(format!("decryption-nonce {}", nonce_hex));
    imeta_values.push("encryption-algorithm chacha20-poly1305".to_string());
    let imeta_tag = Tag::custom(TagKind::from("imeta"), imeta_values);

    Ok(UploadedMedia {
        blob_descriptor,
        imeta_tag,
    })
}

/// Deletes a media file from both the Blossom server and local cache.
///
/// This method handles the complete deletion workflow:
/// 1. Retrieves the file from the local cache
/// 2. Deletes the file from the Blossom server using the stored Nostr key
/// 3. Removes the file from the local cache
///
/// If the file doesn't exist in the cache, the operation succeeds without error.
///
/// # Arguments
///
/// * `mls_group_id` - The MLS group ID that the media file belongs to
/// * `file_hash` - The SHA256 hash of the file to delete
/// * `db` - The database connection
/// * `blossom_client` - The client for interacting with the Blossom server
///
/// # Returns
///
/// * `Ok(())` - Success
/// * `Err(MediaError)` - Error if deletion fails or if no Nostr key is found
#[allow(dead_code)]
pub async fn delete_media_file(
    group: &group_types::Group,
    file_hash: &str,
    db: &Database,
    blossom_client: &blossom::BlossomClient,
) -> Result<(), MediaError> {
    // Get the file from the cache
    let cached_media_file = cache::fetch_cached_file(group, file_hash, db).await?;
    if let Some(cached_media_file) = cached_media_file {
        // Check that we have a nostr key for deletion
        if cached_media_file.media_file.nostr_key.is_none() {
            return Err(MediaError::Delete(
                "No Nostr key found for deletion".to_string(),
            ));
        }
        // Parse the nostr key
        let nostr_key = nostr_sdk::Keys::parse(&cached_media_file.media_file.nostr_key.unwrap())
            .map_err(|e| MediaError::Delete(e.to_string()))?;
        // Delete the file from Blossom first
        blossom_client
            .delete(file_hash, &nostr_key)
            .await
            .map_err(|e| MediaError::Delete(e.to_string()))?;
        // Delete the file from the cache
        cache::delete_cached_file(group, file_hash, db).await?;
    }
    Ok(())
}

/// Generates an IMETA tag containing file metadata for Nostr events.
///
/// Creates a tag containing:
/// - URL of the uploaded file
/// - MIME type
/// - Original filename
/// - For images: dimensions and blurhash
/// - SHA256 hash of the original file
///
/// The tag is used in Nostr events to provide metadata about attached media files.
/// For images, additional metadata like dimensions and blurhash are included to
/// help clients optimize display and loading.
///
/// # Arguments
///
/// * `file` - The original uploaded media file
/// * `blob` - The upload descriptor from Blossom containing URL and other metadata
/// * `original_sha256` - The SHA256 hash of the original file
///
/// # Returns
///
/// * `Ok(Vec<String>)` - The IMETA tag values
/// * `Err(String)` - Error message if metadata generation fails
fn generate_imeta_tag_values(
    file: &FileUpload,
    blob: &blossom::BlobDescriptor,
    original_sha256: &str,
) -> Result<Vec<String>, String> {
    let mut imeta_values = vec![
        format!("url {}", blob.url),
        format!("m {}", file.mime_type),
        format!("filename {}", file.filename),
    ];

    // Add dimensions and blurhash for images
    if file.mime_type.starts_with("image/") {
        println!("Image file detected");
        match ::image::load_from_memory(&file.data) {
            Ok(img) => {
                let (width, height) = img.dimensions();
                imeta_values.push(format!("dim {}x{}", width, height));

                // Calculate blurhash
                let rgb_img = img.to_rgba8().into_vec();
                let blurhash = blurhash::encode(4, 3, width, height, &rgb_img);
                imeta_values.push(format!("blurhash {}", blurhash));
            }
            Err(e) => {
                return Err(format!("Failed to load image: {}", e));
            }
        }
    }

    // TODO: This is where we'd do any video or other file type processing

    // Use SHA256 hash of original file
    imeta_values.push(format!("x {}", original_sha256));

    Ok(imeta_values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn create_test_file(filename: &str, mime_type: &str, data: &[u8]) -> FileUpload {
        FileUpload {
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn test_generate_imeta_tag_values() {
        let file = create_test_file("test.txt", "text/plain", b"test data");
        let blob = blossom::BlobDescriptor {
            url: "https://example.com/test.txt".to_string(),
            sha256: "test_sha256".to_string(),
            size: 1000,
            r#type: Some("text/plain".to_string()),
            uploaded: 1234567890,
            compressed: None,
        };

        let result = generate_imeta_tag_values(&file, &blob, "original_sha256");
        assert!(result.is_ok());

        let values = result.unwrap();
        assert!(values.contains(&"url https://example.com/test.txt".to_string()));
        assert!(values.contains(&"m text/plain".to_string()));
        assert!(values.contains(&"filename test.txt".to_string()));
        assert!(values.contains(&"x original_sha256".to_string()));
    }

    #[test]
    fn test_generate_imeta_tag_values_image() {
        // A valid 1x1 black pixel PNG file (base64 decoded)
        let image_data = base64::engine::general_purpose::STANDARD.decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg=="
        ).unwrap();

        let file = create_test_file("test.png", "image/png", &image_data);
        let blob = blossom::BlobDescriptor {
            url: "https://example.com/test.png".to_string(),
            sha256: "test_sha256".to_string(),
            size: 1000,
            r#type: Some("image/png".to_string()),
            uploaded: 1234567890,
            compressed: None,
        };

        let result = generate_imeta_tag_values(&file, &blob, "original_sha256");
        assert!(result.is_ok());

        let values = result.unwrap();
        assert!(values.contains(&"url https://example.com/test.png".to_string()));
        assert!(values.contains(&"m image/png".to_string()));
        assert!(values.contains(&"filename test.png".to_string()));
        assert!(values.contains(&"x original_sha256".to_string()));
        assert!(values.iter().any(|v| v.starts_with("dim ")));
        assert!(values.iter().any(|v| v.starts_with("blurhash ")));
    }

    #[test]
    fn test_generate_imeta_tag_values_invalid_image() {
        let file = create_test_file("test.png", "image/png", b"not a real image");
        let blob = blossom::BlobDescriptor {
            url: "https://example.com/test.png".to_string(),
            sha256: "test_sha256".to_string(),
            size: 1000,
            r#type: Some("image/png".to_string()),
            uploaded: 1234567890,
            compressed: None,
        };

        let result = generate_imeta_tag_values(&file, &blob, "original_sha256");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to load image"));
    }
}
