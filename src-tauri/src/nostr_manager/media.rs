use crate::nostr_manager::blossom::BlobDescriptor;
use crate::whitenoise::Whitenoise;
use ::image::GenericImageView;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use blurhash::encode;
use nostr_sdk::prelude::*;
use rand::RngCore;
use serde::Deserialize;

/// Represents a media file received from the frontend application.
/// This structure is used to handle file uploads in the messaging system.
#[derive(Debug, Deserialize)]
pub struct MediaFile {
    /// The original filename
    #[allow(dead_code)]
    pub name: String,
    /// The MIME type of the file (e.g., "image/jpeg", "video/mp4")
    #[serde(rename = "type")]
    pub mime_type: String,
    /// The raw binary data of the file
    pub data: Vec<u8>,
}

/// Represents a successfully uploaded and processed media file.
/// Contains both the upload result and the metadata tag for Nostr events.
#[derive(Debug)]
pub struct UploadedMedia {
    /// The descriptor returned by the Blossom server after upload
    pub blob_descriptor: BlobDescriptor,
    /// The IMETA tag containing metadata about the file for Nostr events
    pub imeta_tag: Tag,
}

/// Processes a media file for sending in a Nostr message.
///
/// This function handles the complete workflow for preparing a media file for sending:
/// 1. Encrypts the file data using AES-GCM
/// 2. Uploads the encrypted data to the Blossom server
/// 3. Generates appropriate metadata including image-specific data if applicable
///
/// # Arguments
/// * `file` - The media file to process
/// * `export_secret_hex` - The export secret in hex format for the current epoch
/// * `wn` - The Whitenoise state containing necessary clients
///
/// # Returns
/// * `Ok(UploadedMedia)` - The processed media result including upload info and metadata
/// * `Err(String)` - Error message if any step fails
///
/// # Example
/// ```ignore
/// let keys = Keys::generate();
/// let whitenoise = Whitenoise::new().await?;
///
/// let file = MediaFile {
///     name: "photo.jpg".to_string(),
///     mime_type: "image/jpeg".to_string(),
///     data: vec![/* ... */],
/// };
///
/// let result = process_media_file(file, &keys, &whitenoise).await?;
/// ```
pub async fn process_media_file(
    file: MediaFile,
    export_secret_hex: &str,
    wn: &Whitenoise,
) -> Result<UploadedMedia, String> {
    // Get the raw secret key bytes for AES-GCM
    let secret_key = hex::decode(export_secret_hex).map_err(|e| e.to_string())?;

    // Encrypt the file using AES-GCM
    let (encrypted_file_data, nonce) = encrypt_file(&file.data, &secret_key)?;

    // Upload encrypted file to Blossom
    let blob_descriptor = upload_encrypted_file(encrypted_file_data, wn).await?;

    // Generate metadata and IMETA tag
    let mut imeta_tag = generate_imeta_tag(&file, &blob_descriptor)?;

    // Add nonce to the IMETA tag
    let nonce_hex = hex::encode(nonce);
    let mut imeta_values = imeta_tag.to_vec();
    imeta_values.push(format!("decryption-nonce {}", nonce_hex));
    imeta_values.push(format!("encryption-algorithm {}", "aes-gcm"));
    imeta_tag = Tag::custom(TagKind::from("imeta"), imeta_values);

    Ok(UploadedMedia {
        blob_descriptor,
        imeta_tag,
    })
}

/// Encrypts file data using AES-GCM encryption.
///
/// This function encrypts the raw file data using AES-GCM,
/// which provides authenticated encryption.
///
/// # Arguments
/// * `data` - The raw file data to encrypt
/// * `key` - The 32-byte key to use for encryption
///
/// # Returns
/// * `Ok((Vec<u8>, Vec<u8>))` - The encrypted data and nonce
/// * `Err(String)` - Error message if encryption fails
fn encrypt_file(data: &[u8], key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    if key.len() != 32 {
        return Err("Key must be 32 bytes".to_string());
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .encrypt(nonce, data)
        .map(|encrypted| (encrypted, nonce_bytes.to_vec()))
        .map_err(|e| e.to_string())
}

/// Uploads encrypted file data to the Blossom server.
///
/// # Arguments
/// * `encrypted_data` - The encrypted file data to upload
/// * `wn` - The Whitenoise state containing the Blossom client
///
/// # Returns
/// * `Ok(BlobDescriptor)` - The upload result containing URL and metadata
/// * `Err(String)` - Error message if upload fails
async fn upload_encrypted_file(
    encrypted_data: Vec<u8>,
    wn: &Whitenoise,
) -> Result<BlobDescriptor, String> {
    wn.nostr
        .blossom
        .upload(encrypted_data)
        .await
        .map_err(|e| e.to_string())
}

/// Generates an IMETA tag containing file metadata for Nostr events.
///
/// Creates a tag containing:
/// - URL of the uploaded file
/// - MIME type
/// - For images: dimensions and blurhash
/// - SHA256 hash of the encrypted file
///
/// # Arguments
/// * `file` - The original media file
/// * `blob` - The upload descriptor from Blossom
///
/// # Returns
/// * `Ok(Tag)` - The generated IMETA tag
/// * `Err(String)` - Error message if metadata generation fails
fn generate_imeta_tag(file: &MediaFile, blob: &BlobDescriptor) -> Result<Tag, String> {
    let mut imeta = vec![format!("url {}", blob.url), format!("m {}", file.mime_type)];

    // Add dimensions and blurhash for images
    if file.mime_type.starts_with("image/") {
        println!("Image file detected");
        match ::image::load_from_memory(&file.data) {
            Ok(img) => {
                let (width, height) = img.dimensions();
                imeta.push(format!("dim {}x{}", width, height));

                // Calculate blurhash
                let rgb_img = img.to_rgba8().into_vec();
                let blurhash = encode(4, 3, width, height, &rgb_img);
                imeta.push(format!("blurhash {}", blurhash));
            }
            Err(e) => {
                return Err(format!("Failed to load image: {}", e));
            }
        }
    }

    // Use SHA256 hash from blob descriptor
    imeta.push(format!("x {}", blob.sha256));

    Ok(Tag::custom(TagKind::from("imeta"), imeta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn create_test_file(name: &str, mime_type: &str, data: &[u8]) -> MediaFile {
        MediaFile {
            name: name.to_string(),
            mime_type: mime_type.to_string(),
            data: data.to_vec(),
        }
    }

    fn create_test_blob(url: &str, sha256: &str) -> BlobDescriptor {
        BlobDescriptor {
            url: url.to_string(),
            sha256: sha256.to_string(),
            size: 123,
            r#type: Some("text/plain".to_string()),
            uploaded: 12345,
            compressed: None,
        }
    }

    #[test]
    fn test_generate_imeta_tag_text_file() {
        let file = create_test_file("test.txt", "text/plain", b"test data");
        let blob = create_test_blob("https://example.com/test.txt", "abcdef");

        let tag = generate_imeta_tag(&file, &blob).unwrap();
        let tag_values = tag.to_vec();

        assert_eq!(tag_values[0], "imeta");
        assert!(tag_values.contains(&"url https://example.com/test.txt".to_string()));
        assert!(tag_values.contains(&"m text/plain".to_string()));
        assert!(tag_values.contains(&"x abcdef".to_string()));
    }

    #[test]
    fn test_generate_imeta_tag_image_file() {
        // A valid 1x1 black pixel PNG file (base64 decoded)
        let image_data = base64::engine::general_purpose::STANDARD.decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg=="
        ).unwrap();

        let file = create_test_file("test.png", "image/png", &image_data);
        let blob = create_test_blob("https://example.com/test.png", "abcdef");

        let tag = generate_imeta_tag(&file, &blob).unwrap();
        let tag_values = tag.clone().to_vec();

        assert_eq!(tag_values[0], "imeta");
        assert!(tag_values.contains(&"url https://example.com/test.png".to_string()));
        assert!(tag_values.contains(&"m image/png".to_string()));
        assert!(tag_values.iter().any(|v| v.starts_with("dim ")));
        assert!(tag_values.contains(&"dim 1x1".to_string()));
        assert!(tag_values.iter().any(|v| v.starts_with("blurhash ")));
        assert!(tag_values.contains(&"x abcdef".to_string()));
    }

    #[test]
    fn test_generate_imeta_tag_invalid_image() {
        let file = create_test_file("test.png", "image/png", b"not a real image");
        let blob = create_test_blob("https://example.com/test.png", "abcdef");

        let result = generate_imeta_tag(&file, &blob);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to load image"));
    }

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
}
