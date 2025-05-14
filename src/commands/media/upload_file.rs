use nostr_mls::prelude::*;

use crate::media::{add_media_file, FileUpload, UploadedMedia};


/// Maximum number of retry attempts for file upload operations
const MAX_RETRIES: u8 = 3;

// TODO: FIX https://github.com/parres-hq/whitenoise/issues/138

/// Uploads a file to the media storage system with retry logic and event emission.
/// This is used for uploading any type of file to groups.
///
/// This function handles the file upload process with the following steps:
/// 1. Exports the MLS secret key for the given group
/// 2. Stores the export secret in the secrets store
/// 3. Attempts to upload the file with retry logic
/// 4. Emits appropriate events for success, retry, or failure
///
/// # Arguments
///
/// * `group_id` - The ID of the group for which the file is being uploaded
/// * `file` - The file upload details containing the file data and metadata
/// * `wn` - The Whitenoise application state containing necessary configurations
/// * `app_handle` - The Tauri application handle for emitting events
///
/// # Returns
///
/// * `Ok(UploadedMedia)` - The uploaded media details if successful
/// * `Err(String)` - An error message if the upload fails after all retries
///
/// # Events
///
/// The function emits the following events:
/// * `file_upload_success` - When the file is successfully uploaded
/// * `file_upload_retry` - When a retry attempt is made
/// * `file_upload_error` - When all retry attempts fail
pub async fn upload_file(
    group: group_types::Group,
    file: FileUpload,
) -> Result<UploadedMedia, String> {
    let mut retries = 0;
    let mut last_error = None;

    while retries < MAX_RETRIES {
        match add_media_file(&group, file.clone()).await {
            Ok(media) => {
                // Emit success event
                return Ok(media);
            }
            Err(e) => {
                last_error = Some(e.to_string());
                retries += 1;
                if retries < MAX_RETRIES {
                    // Emit retry event
                }
            }
        }
    }

    // If we get here, all retries failed
    let error = last_error.unwrap_or_else(|| "Unknown error".to_string());

    // Emit error event

    Err(error)
}
