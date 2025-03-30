use base64::{engine::general_purpose::STANDARD, Engine};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Error response from a HEAD request
#[derive(Debug)]
#[allow(dead_code)]
pub struct UploadError {
    /// HTTP status code
    pub status: u16,
    /// Human readable error message from X-Reason header
    pub reason: String,
}

/// Result type for upload requirements check
pub type UploadRequirementsResult = Result<(), UploadError>;

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

    /// Creates a Nostr event for authorization
    ///
    /// # Arguments
    /// * `sha256` - The SHA-256 hash of the file
    /// * `action` - The action being authorized (e.g., "upload", "delete")
    /// * `keys` - The Nostr keys to use for signing the event
    ///
    /// # Returns
    /// A Result containing the authorization header value or an error
    async fn create_auth_event(
        &self,
        sha256: &str,
        action: &str,
        keys: &Keys,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let tags = vec![
            Tag::custom(TagKind::Custom("t".into()), vec![action.to_string()]),
            Tag::expiration(Timestamp::now() + 24 * 60 * 60),
            Tag::custom(TagKind::Custom("x".into()), vec![sha256.to_string()]),
        ];

        let event = EventBuilder::new(Kind::Custom(24242), "")
            .tags(tags)
            .sign(keys)
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
    /// A Result containing the BlobDescriptor and Nostr keys used to upload the file or an error
    pub async fn upload(
        &self,
        file: Vec<u8>,
    ) -> Result<(BlobDescriptor, Keys), Box<dyn std::error::Error + Send + Sync>> {
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

        // Generate keys for this upload
        let keys = Keys::generate();

        // Create the authorization header
        let auth_header = self.create_auth_event(&sha256, "upload", &keys).await?;

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
        Ok((blob_descriptor, keys))
    }

    /// Deletes a file from the Blossom server
    ///
    /// # Arguments
    /// * `sha256` - The SHA-256 hash of the file to delete
    /// * `keys` - The Nostr keys to use for authentication
    ///
    /// # Returns
    /// A Result containing the deleted BlobDescriptor or an error
    pub async fn delete(
        &self,
        sha256: &str,
        keys: &Keys,
    ) -> Result<BlobDescriptor, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        tracing::info!(
            target: "whitenoise::nostr_manager::blossom",
            "Deleting file from Blossom server: {}",
            self.url
        );

        // Create the authorization header
        let auth_header = self.create_auth_event(sha256, "delete", keys).await?;

        // Delete the file with the auth header
        let response = client
            .delete(format!("{}/{}", self.url, sha256))
            .header("Authorization", auth_header)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!(
                target: "whitenoise::nostr_manager::blossom",
                "Delete failed: {:?}",
                response
            );
            return Err(format!("Delete failed with status: {}", response.status()).into());
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

    /// Uploads and optimizes media to the Blossom server
    ///
    /// # Arguments
    /// * `file` - The file contents as a byte vector
    /// * `content_type` - The MIME type of the media
    /// * `keys` - The Nostr keys to use for authentication
    ///
    /// # Returns
    /// A Result containing the BlobDescriptor or an error
    pub async fn upload_media(
        &self,
        file: Vec<u8>,
        content_type: &str,
        keys: &Keys,
    ) -> Result<BlobDescriptor, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        tracing::info!(
            target: "whitenoise::nostr_manager::blossom",
            "Uploading media to Blossom server: {}",
            self.url
        );

        // Calculate SHA-256 hash of the file
        let mut hasher = Sha256::new();
        hasher.update(&file);
        let sha256 = format!("{:x}", hasher.finalize());

        // Create the authorization header
        let auth_header = self.create_auth_event(&sha256, "media", keys).await?;

        // Upload the file with the auth header
        let response = client
            .put(format!("{}/media", self.url))
            .header("Content-Length", file.len())
            .header("Content-Type", content_type)
            .header("Authorization", auth_header)
            .body(file)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!(
                target: "whitenoise::nostr_manager::blossom",
                "Media upload failed: {:?}",
                response
            );
            return Err(format!("Media upload failed with status: {}", response.status()).into());
        }

        let blob_descriptor: BlobDescriptor = response.json().await?;
        Ok(blob_descriptor)
    }

    /// Checks if a media file can be uploaded to Blossom
    ///
    /// # Arguments
    /// * `sha256` - The SHA-256 hash of the file to check
    /// * `keys` - The Nostr keys to use for authentication
    ///
    /// # Returns
    /// A Result containing whether the file exists and its metadata if it does, or an error
    #[allow(dead_code)]
    pub async fn head_media(
        &self,
        sha256: &str,
        keys: &Keys,
    ) -> Result<Option<BlobDescriptor>, Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        tracing::info!(
            target: "whitenoise::nostr_manager::blossom",
            "Checking media existence on Blossom server: {}",
            self.url
        );

        // Create the authorization header
        let auth_header = self.create_auth_event(sha256, "media", keys).await?;

        // Check if the file exists with the auth header
        let response = client
            .head(format!("{}/media/{}", self.url, sha256))
            .header("Authorization", auth_header.clone())
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !response.status().is_success() {
            tracing::error!(
                target: "whitenoise::nostr_manager::blossom",
                "Media head request failed: {:?}",
                response
            );
            return Err(format!(
                "Media head request failed with status: {}",
                response.status()
            )
            .into());
        }

        // If the file exists, get its metadata
        let response = client
            .get(format!("{}/media/{}", self.url, sha256))
            .header("Authorization", auth_header)
            .send()
            .await?;

        let blob_descriptor: BlobDescriptor = response.json().await?;
        Ok(Some(blob_descriptor))
    }

    /// Checks if a file can be uploaded to the Blossom server
    ///
    /// # Arguments
    /// * `sha256` - The SHA-256 hash of the file
    /// * `content_type` - The MIME type of the file
    /// * `content_length` - The size of the file in bytes
    /// * `keys` - The Nostr keys to use for authentication
    ///
    /// # Returns
    /// A Result indicating whether the upload can proceed or an error with details
    #[allow(dead_code)]
    pub async fn check_upload_requirements(
        &self,
        sha256: &str,
        content_type: &str,
        content_length: u64,
        keys: &Keys,
    ) -> UploadRequirementsResult {
        let client = reqwest::Client::new();
        tracing::info!(
            target: "whitenoise::nostr_manager::blossom",
            "Checking upload requirements on Blossom server: {}",
            self.url
        );

        // Create the authorization header
        let auth_header = self
            .create_auth_event(sha256, "upload", keys)
            .await
            .map_err(|e| UploadError {
                status: 401,
                reason: format!("Failed to create authorization: {}", e),
            })?;

        // Check upload requirements
        let response = client
            .head(format!("{}/upload", self.url))
            .header("X-SHA-256", sha256)
            .header("X-Content-Type", content_type)
            .header("X-Content-Length", content_length.to_string().as_str())
            .header("Authorization", auth_header)
            .send()
            .await
            .map_err(|e| UploadError {
                status: 500,
                reason: format!("Failed to check upload requirements: {}", e),
            })?;

        if response.status().is_success() {
            return Ok(());
        }

        // Get the error reason from the X-Reason header
        let reason = response
            .headers()
            .get("X-Reason")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown error")
            .to_string();

        Err(UploadError {
            status: response.status().as_u16(),
            reason,
        })
    }

    /// Checks if a media file can be uploaded to the Blossom server
    ///
    /// # Arguments
    /// * `sha256` - The SHA-256 hash of the file
    /// * `content_type` - The MIME type of the file
    /// * `content_length` - The size of the file in bytes
    /// * `keys` - The Nostr keys to use for authentication
    ///
    /// # Returns
    /// A Result indicating whether the upload can proceed or an error with details
    #[allow(dead_code)]
    pub async fn check_media_requirements(
        &self,
        sha256: &str,
        content_type: &str,
        content_length: u64,
        keys: &Keys,
    ) -> UploadRequirementsResult {
        let client = reqwest::Client::new();
        tracing::info!(
            target: "whitenoise::nostr_manager::blossom",
            "Checking media upload requirements on Blossom server: {}",
            self.url
        );

        // Create the authorization header
        let auth_header = self
            .create_auth_event(sha256, "media", keys)
            .await
            .map_err(|e| UploadError {
                status: 401,
                reason: format!("Failed to create authorization: {}", e),
            })?;

        // Check upload requirements
        let response = client
            .head(format!("{}/media", self.url))
            .header("X-SHA-256", sha256)
            .header("X-Content-Type", content_type)
            .header("X-Content-Length", content_length.to_string().as_str())
            .header("Authorization", auth_header)
            .send()
            .await
            .map_err(|e| UploadError {
                status: 500,
                reason: format!("Failed to check media upload requirements: {}", e),
            })?;

        if response.status().is_success() {
            return Ok(());
        }

        // Get the error reason from the X-Reason header
        let reason = response
            .headers()
            .get("X-Reason")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("Unknown error")
            .to_string();

        Err(UploadError {
            status: response.status().as_u16(),
            reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Server, ServerGuard};

    async fn setup_mock_server() -> (ServerGuard, BlossomClient) {
        let server = Server::new_async().await;
        let client = BlossomClient::new(&server.url());
        (server, client)
    }

    #[tokio::test]
    async fn test_upload() {
        let (mut server, client) = setup_mock_server().await;

        // Generate random bytes for testing
        let random_bytes: Vec<u8> = uuid::Uuid::new_v4().as_bytes().to_vec();

        // Create mock response
        let mock_response = BlobDescriptor {
            url: format!("{}/blob/123", server.url()),
            sha256: "test_sha256".to_string(),
            size: random_bytes.len() as u64,
            r#type: Some("application/octet-stream".to_string()),
            uploaded: chrono::Utc::now().timestamp() as u64,
            compressed: None,
        };

        // Setup mock
        let _m = server
            .mock("PUT", "/upload")
            .match_header("content-type", "application/octet-stream")
            .match_header("content-length", random_bytes.len().to_string().as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create();

        // First upload the file
        let (blob_descriptor, _keys) = client
            .upload(random_bytes.clone())
            .await
            .expect("Failed to upload file");

        // Verify response matches expectations
        assert_eq!(blob_descriptor.url, mock_response.url);
        assert_eq!(blob_descriptor.size, random_bytes.len() as u64);
    }

    #[tokio::test]
    async fn test_upload_empty_file() {
        let (mut server, client) = setup_mock_server().await;
        let empty_bytes: Vec<u8> = Vec::new();

        // Create mock response
        let mock_response = BlobDescriptor {
            url: format!("{}/blob/empty", server.url()),
            sha256: "empty_sha256".to_string(),
            size: 0,
            r#type: Some("application/octet-stream".to_string()),
            uploaded: chrono::Utc::now().timestamp() as u64,
            compressed: None,
        };

        // Setup mock
        let _m = server
            .mock("PUT", "/upload")
            .match_header("content-type", "application/octet-stream")
            .match_header("content-length", "0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create();

        let (blob_descriptor, _keys) = client
            .upload(empty_bytes)
            .await
            .expect("Failed to upload empty file");

        assert_eq!(blob_descriptor.size, 0);
        assert!(!blob_descriptor.url.is_empty());
    }

    #[tokio::test]
    async fn test_upload_large_file() {
        let (mut server, client) = setup_mock_server().await;
        let large_bytes: Vec<u8> = vec![0; 1024 * 1024];

        // Create mock response
        let mock_response = BlobDescriptor {
            url: format!("{}/blob/large", server.url()),
            sha256: "large_sha256".to_string(),
            size: large_bytes.len() as u64,
            r#type: Some("application/octet-stream".to_string()),
            uploaded: chrono::Utc::now().timestamp() as u64,
            compressed: None,
        };

        // Setup mock
        let _m = server
            .mock("PUT", "/upload")
            .match_header("content-type", "application/octet-stream")
            .match_header("content-length", large_bytes.len().to_string().as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create();

        let (blob_descriptor, _keys) = client
            .upload(large_bytes)
            .await
            .expect("Failed to upload large file");

        assert_eq!(blob_descriptor.size, 1024 * 1024);
    }

    #[tokio::test]
    async fn test_delete() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "test_sha256";
        let keys = Keys::generate();

        // Create mock response
        let mock_response = BlobDescriptor {
            url: format!("{}/{}", server.url(), sha256),
            sha256: sha256.to_string(),
            size: 1000,
            r#type: Some("application/octet-stream".to_string()),
            uploaded: chrono::Utc::now().timestamp() as u64,
            compressed: None,
        };

        // Setup mock
        let _m = server
            .mock("DELETE", format!("/{}", sha256).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create();

        let deleted_descriptor = client
            .delete(sha256, &keys)
            .await
            .expect("Failed to delete file");

        assert_eq!(deleted_descriptor.sha256, sha256);
        assert_eq!(deleted_descriptor.size, 1000);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_file() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "nonexistent_sha256";
        let keys = Keys::generate();

        // Setup mock for 404 response
        let _m = server
            .mock("DELETE", format!("/{}", sha256).as_str())
            .with_status(404)
            .create();

        let result = client.delete(sha256, &keys).await;
        assert!(result.is_err(), "Deleting nonexistent file should fail");
    }

    #[tokio::test]
    async fn test_download_nonexistent_file() {
        let (mut server, client) = setup_mock_server().await;

        // Setup mock for 404 response
        let _m = server.mock("GET", "/nonexistent").with_status(404).create();

        let result = client
            .download(&format!("{}/nonexistent", server.url()))
            .await;
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

    #[tokio::test]
    async fn test_upload_media() {
        let (mut server, client) = setup_mock_server().await;

        // Generate random bytes for testing
        let random_bytes: Vec<u8> = uuid::Uuid::new_v4().as_bytes().to_vec();
        let content_type = "image/jpeg";
        let keys = Keys::generate();

        // Create mock response
        let mock_response = BlobDescriptor {
            url: format!("{}/media/123", server.url()),
            sha256: "test_sha256".to_string(),
            size: random_bytes.len() as u64,
            r#type: Some(content_type.to_string()),
            uploaded: chrono::Utc::now().timestamp() as u64,
            compressed: Some(CompressedInfo {
                sha256: "compressed_sha256".to_string(),
                size: random_bytes.len() as u64 / 2,
                library: "mozjpeg".to_string(),
                version: "4.0.0".to_string(),
                parameters: CompressionParams {
                    quality: 85,
                    mode: "baseline".to_string(),
                },
            }),
        };

        // Setup mock
        let _m = server
            .mock("PUT", "/media")
            .match_header("content-type", content_type)
            .match_header("content-length", random_bytes.len().to_string().as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create();

        // Upload the media file
        let blob_descriptor = client
            .upload_media(random_bytes.clone(), content_type, &keys)
            .await
            .expect("Failed to upload media file");

        // Verify response matches expectations
        assert_eq!(blob_descriptor.url, mock_response.url);
        assert_eq!(blob_descriptor.size, random_bytes.len() as u64);
        assert_eq!(blob_descriptor.r#type, Some(content_type.to_string()));
        assert!(blob_descriptor.compressed.is_some());
    }

    #[tokio::test]
    async fn test_head_media() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "test_sha256";
        let keys = Keys::generate();

        // Create mock response
        let mock_response = BlobDescriptor {
            url: format!("{}/media/{}", server.url(), sha256),
            sha256: sha256.to_string(),
            size: 1000,
            r#type: Some("image/jpeg".to_string()),
            uploaded: chrono::Utc::now().timestamp() as u64,
            compressed: Some(CompressedInfo {
                sha256: "compressed_sha256".to_string(),
                size: 500,
                library: "mozjpeg".to_string(),
                version: "4.0.0".to_string(),
                parameters: CompressionParams {
                    quality: 85,
                    mode: "baseline".to_string(),
                },
            }),
        };

        // Setup mock for HEAD request
        let _m = server
            .mock("HEAD", format!("/media/{}", sha256).as_str())
            .with_status(200)
            .create();

        // Setup mock for GET request
        let _m = server
            .mock("GET", format!("/media/{}", sha256).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&mock_response).unwrap())
            .create();

        let result = client
            .head_media(sha256, &keys)
            .await
            .expect("Failed to check media existence");

        assert!(result.is_some());
        let descriptor = result.unwrap();
        assert_eq!(descriptor.sha256, sha256);
        assert_eq!(descriptor.size, 1000);
        assert!(descriptor.compressed.is_some());
    }

    #[tokio::test]
    async fn test_head_media_nonexistent() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "nonexistent_sha256";
        let keys = Keys::generate();

        // Setup mock for 404 response
        let _m = server
            .mock("HEAD", format!("/media/{}", sha256).as_str())
            .with_status(404)
            .create();

        let result = client
            .head_media(sha256, &keys)
            .await
            .expect("Failed to check media existence");

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_check_upload_requirements() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "test_sha256";
        let content_type = "image/jpeg";
        let content_length = 1000;
        let keys = Keys::generate();

        // Setup mock for successful check
        let _m = server
            .mock("HEAD", "/upload")
            .match_header("X-SHA-256", sha256)
            .match_header("X-Content-Type", content_type)
            .match_header("X-Content-Length", content_length.to_string().as_str())
            .with_status(200)
            .create();

        let result = client
            .check_upload_requirements(sha256, content_type, content_length, &keys)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_check_upload_requirements_error() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "test_sha256";
        let content_type = "image/jpeg";
        let content_length = 1000;
        let keys = Keys::generate();

        // Setup mock for error response
        let _m = server
            .mock("HEAD", "/upload")
            .match_header("X-SHA-256", sha256)
            .match_header("X-Content-Type", content_type)
            .match_header("X-Content-Length", content_length.to_string().as_str())
            .with_status(413)
            .with_header("X-Reason", "File too large. Max allowed size is 100MB.")
            .create();

        let result = client
            .check_upload_requirements(sha256, content_type, content_length, &keys)
            .await
            .expect_err("Should have failed with file too large error");

        assert_eq!(result.status, 413);
        assert_eq!(result.reason, "File too large. Max allowed size is 100MB.");
    }

    #[tokio::test]
    async fn test_check_media_requirements() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "test_sha256";
        let content_type = "image/jpeg";
        let content_length = 1000;
        let keys = Keys::generate();

        // Setup mock for successful check
        let _m = server
            .mock("HEAD", "/media")
            .match_header("X-SHA-256", sha256)
            .match_header("X-Content-Type", content_type)
            .match_header("X-Content-Length", content_length.to_string().as_str())
            .with_status(200)
            .create();

        let result = client
            .check_media_requirements(sha256, content_type, content_length, &keys)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_check_media_requirements_error() {
        let (mut server, client) = setup_mock_server().await;
        let sha256 = "test_sha256";
        let content_type = "image/jpeg";
        let content_length = 1000;
        let keys = Keys::generate();

        // Setup mock for error response
        let _m = server
            .mock("HEAD", "/media")
            .match_header("X-SHA-256", sha256)
            .match_header("X-Content-Type", content_type)
            .match_header("X-Content-Length", content_length.to_string().as_str())
            .with_status(415)
            .with_header("X-Reason", "Unsupported file type.")
            .create();

        let result = client
            .check_media_requirements(sha256, content_type, content_length, &keys)
            .await
            .expect_err("Should have failed with unsupported file type error");

        assert_eq!(result.status, 415);
        assert_eq!(result.reason, "Unsupported file type.");
    }
}
