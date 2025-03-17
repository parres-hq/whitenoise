use crate::media::blossom::BlobDescriptor;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents a file upload received from the frontend application.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileUpload {
    /// The original filename
    pub filename: String,
    /// The MIME type of the file (e.g., "image/jpeg", "video/mp4")
    pub mime_type: String,
    /// The raw binary data of the file
    pub data: Vec<u8>,
}

/// Represents a media_file row in the database.
#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct MediaFile {
    /// The ID of the media_file row
    pub id: i64,
    /// The MLS group ID
    pub mls_group_id: Vec<u8>,
    /// The path to the file on the local filesystem
    pub file_path: String,
    /// The URL of the file on Blossom
    pub blossom_url: Option<String>,
    /// The SHA256 hash of the file
    pub file_hash: String,
    /// The nostr private key used to upload the file to Blossom
    pub nostr_key: Option<String>,
    /// Unix timestamp when the file was created
    pub created_at: u64,
    /// JSON string for file metadata
    pub file_metadata: Option<String>,
}

/// Represents a cached media file, including both the file data and the media file row from the database.
#[derive(Debug)]
pub struct CachedMediaFile {
    /// The media file row from the database
    pub media_file: MediaFile,
    /// The file data
    pub file_data: Vec<u8>,
}

/// Represents a successfully uploaded and processed media file.
/// Contains both the upload result and the imeta tag for Nostr events.
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadedMedia {
    /// The descriptor returned by the Blossom server after upload
    pub blob_descriptor: BlobDescriptor,
    /// The IMETA tag containing metadata about the file for Nostr events
    pub imeta_tag: Tag,
}
