use nostr_blossom::bud02::BlobDescriptor;
use nostr_mls::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row};

use super::sanitizer::SafeMediaMetadata;

/// Represents a file upload received from the frontend application.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileDetails {
    /// The original filename
    pub filename: String,
    /// The MIME type of the file (e.g., "image/jpeg", "video/mp4")
    pub mime_type: String,
    /// The raw binary data of the file
    pub data: Vec<u8>,
}

/// Represents a media_file row in the database.
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaFile {
    /// The ID of the media_file row
    pub id: i64,
    /// The MLS group ID
    pub mls_group_id: GroupId,
    /// The SHA256 hash of the file
    pub file_hash: String,
    /// Unix timestamp when the file was created
    pub created_at: i64,
    /// JSONB metadata for the file
    pub file_metadata: Option<SafeMediaMetadata>,
}

impl<'r> FromRow<'r, sqlx::sqlite::SqliteRow> for MediaFile {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let file_metadata: Option<SafeMediaMetadata> = if let Some(json_value) =
            row.try_get::<Option<serde_json::Value>, _>("file_metadata")?
        {
            serde_json::from_value(json_value).map_err(|e| sqlx::Error::Decode(Box::new(e)))?
        } else {
            None
        };

        Ok(MediaFile {
            id: row.try_get("id")?,
            mls_group_id: GroupId::from_slice(&row.try_get::<Vec<u8>, _>("mls_group_id")?),
            file_hash: row.try_get("file_hash")?,
            created_at: row.try_get("created_at")?,
            file_metadata,
        })
    }
}

/// Represents a cached media file, including both the file data and the media file row from the database.
#[derive(Debug, Serialize, Deserialize)]
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
