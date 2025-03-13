use base64::{engine::general_purpose::STANDARD, Engine};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Parameters for compressing blobs
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CompressionParams {
    /// Quality level of compression (0-100)
    pub quality: u32,
    /// Compression mode/algorithm used
    pub mode: String,
}

/// Information about a compressed blob
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CompressedInfo {
    /// SHA-256 hash of the compressed data
    pub sha256: String,
    /// Size of the compressed data in bytes
    pub size: u64,
    /// Library used for compression
    pub library: String,
    /// Version of the compression library
    pub version: String,
    /// Parameters used for compression
    pub parameters: CompressionParams,
}

/// Descriptor for a blob stored on the Blossom server
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobDescriptor {
    /// URL where the blob can be accessed
    pub url: String,
    /// SHA-256 hash of the blob data
    pub sha256: String,
    /// Size of the blob in bytes
    pub size: u64,
    /// Optional MIME type of the blob
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Unix timestamp when the blob was uploaded
    pub uploaded: u64,
    /// Optional information about compression if the blob is compressed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed: Option<CompressedInfo>,
}

/// Client for interacting with a Blossom server
#[derive(Clone, Debug)]
pub struct BlossomClient {
    /// Base URL of the Blossom server
    pub url: String,
}

impl BlossomClient {
    /// Creates a new BlossomClient instance
    ///
    /// # Arguments
    /// * `url` - The base URL of the Blossom server
    pub fn new(url: &str) -> Self {
        BlossomClient {
            url: url.to_string(),
        }
    }

    /// Creates a Nostr event for upload authorization
    ///
    /// # Arguments
    /// * `sha256` - The SHA-256 hash of the file being uploaded
    ///
    /// # Returns
    /// A Result containing the authorization header value or an error
    async fn create_upload_auth_event(
        &self,
        sha256: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Generate a new key pair for this upload
        let keys = Keys::generate();

        let tags = vec![
            Tag::custom(TagKind::Custom("t".into()), vec!["upload".to_string()]),
            Tag::expiration(Timestamp::now() + 24 * 60 * 60),
            Tag::custom(TagKind::Custom("x".into()), vec![sha256.to_string()]),
        ];

        let event = EventBuilder::new(Kind::Custom(24242), "")
            .tags(tags)
            .sign(&keys)
            .await?;

        // Convert event to JSON string
        let event_json = serde_json::to_string(&event)?;

        // Base64 encode the event
        let encoded = STANDARD.encode(event_json);

        // Create the Authorization header value
        Ok(format!("Nostr {}", encoded))
    }

    /// Uploads a file to the Blossom server
    ///
    /// # Arguments
    /// * `file` - The file contents as a byte vector
    ///
    /// # Returns
    /// A Result containing the BlobDescriptor or an error
    pub async fn upload(
        &self,
        file: Vec<u8>,
    ) -> Result<BlobDescriptor, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        tracing::info!(
            target: "whitenoise::nostr_manager::blossom",
            "Uploading file to Blossom server: {}",
            self.url
        );

        // Calculate SHA-256 hash of the file
        let mut hasher = Sha256::new();
        hasher.update(&file);
        let sha256 = format!("{:x}", hasher.finalize());

        // Create the authorization header
        let auth_header = self.create_upload_auth_event(&sha256).await?;

        // Upload the file with the auth header
        let response = client
            .put(format!("{}/upload", self.url))
            .header("Content-Length", file.len())
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", auth_header)
            .body(file)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!(
                target: "whitenoise::nostr_manager::blossom",
                "Upload failed: {:?}",
                response
            );
            return Err(format!("Upload failed with status: {}", response.status()).into());
        }

        let blob_descriptor: BlobDescriptor = response.json().await?;
        Ok(blob_descriptor)
    }

    /// Downloads a file from a given URL
    ///
    /// # Arguments
    /// * `url` - The URL to download the file from
    ///
    /// # Returns
    /// A Result containing the file contents as a byte vector or an error
    #[allow(dead_code)]
    pub async fn download(
        &self,
        url: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(format!("Download failed with status: {}", response.status()).into());
        }

        Ok(response.bytes().await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_upload() {
        let client = BlossomClient::new("http://localhost:3000");

        // Generate random bytes for testing
        let random_bytes: Vec<u8> = uuid::Uuid::new_v4().as_bytes().to_vec();

        // First upload the file
        let blob_descriptor = client
            .upload(random_bytes.clone())
            .await
            .expect("Failed to upload file");

        // Verify that we got a valid URL back
        assert!(!blob_descriptor.url.is_empty(), "URL should not be empty");
        assert!(
            blob_descriptor.url.starts_with("http"),
            "URL should start with http"
        );

        // Now download the file and verify contents
        let downloaded_bytes = client
            .download(&blob_descriptor.url)
            .await
            .expect("Failed to download file");

        // Assert that we got the same bytes back
        assert_eq!(
            downloaded_bytes, random_bytes,
            "Downloaded file contents don't match original"
        );
    }

    #[tokio::test]
    async fn test_upload_empty_file() {
        let client = BlossomClient::new("http://localhost:3000");
        let empty_bytes: Vec<u8> = Vec::new();

        let blob_descriptor = client
            .upload(empty_bytes)
            .await
            .expect("Failed to upload empty file");

        assert_eq!(blob_descriptor.size, 0, "Size should be 0 for empty file");
        assert!(!blob_descriptor.url.is_empty(), "URL should not be empty");
    }

    #[tokio::test]
    async fn test_upload_large_file() {
        let client = BlossomClient::new("http://localhost:3000");

        // Create a 1MB file
        let large_bytes: Vec<u8> = vec![0; 1024 * 1024];

        let blob_descriptor = client
            .upload(large_bytes.clone())
            .await
            .expect("Failed to upload large file");

        assert_eq!(
            blob_descriptor.size,
            1024 * 1024,
            "Size should match large file size"
        );

        // Download and verify
        let downloaded_bytes = client
            .download(&blob_descriptor.url)
            .await
            .expect("Failed to download large file");

        assert_eq!(
            downloaded_bytes.len(),
            1024 * 1024,
            "Downloaded file size should match original"
        );
    }

    #[tokio::test]
    async fn test_download_nonexistent_file() {
        let client = BlossomClient::new("http://localhost:3000");
        let result = client.download("http://localhost:3000/nonexistent").await;

        assert!(result.is_err(), "Downloading nonexistent file should fail");
    }

    #[tokio::test]
    async fn test_blob_descriptor_serialization() {
        let descriptor = BlobDescriptor {
            url: "http://example.com/blob".to_string(),
            sha256: "abc123".to_string(),
            size: 1000,
            r#type: Some("image/jpeg".to_string()),
            uploaded: 1234567890,
            compressed: Some(CompressedInfo {
                sha256: "def456".to_string(),
                size: 500,
                library: "mozjpeg".to_string(),
                version: "4.0.0".to_string(),
                parameters: CompressionParams {
                    quality: 85,
                    mode: "baseline".to_string(),
                },
            }),
        };

        let serialized = serde_json::to_string(&descriptor).expect("Failed to serialize");
        let deserialized: BlobDescriptor =
            serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(descriptor.url, deserialized.url);
        assert_eq!(descriptor.sha256, deserialized.sha256);
        assert_eq!(descriptor.size, deserialized.size);
        assert_eq!(descriptor.r#type, deserialized.r#type);
        assert_eq!(descriptor.uploaded, deserialized.uploaded);
        assert!(deserialized.compressed.is_some());

        let compressed = deserialized.compressed.unwrap();
        assert_eq!(compressed.sha256, "def456");
        assert_eq!(compressed.size, 500);
        assert_eq!(compressed.library, "mozjpeg");
        assert_eq!(compressed.version, "4.0.0");
        assert_eq!(compressed.parameters.quality, 85);
        assert_eq!(compressed.parameters.mode, "baseline");
    }
}
