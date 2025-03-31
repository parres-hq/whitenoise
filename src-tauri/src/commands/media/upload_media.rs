use crate::accounts::Account;
use crate::media::FileUpload;
use crate::Whitenoise;
use tauri::State;

/// Uploads media content to the Blossom service.
///
/// 🚨 WARNING 🚨 This is NOT used for uploading media to groups. It's only used when you want to upload profile images.
///
/// This function handles the upload of media files (images, videos, etc.) to the Blossom service
/// using the active account's credentials. It requires an active account to be set up.
///
/// # Arguments
///
/// * `file` - A `FileUpload` struct containing the media data and MIME type
/// * `wn` - The Whitenoise application state
///
/// # Returns
///
/// Returns a `Result` containing:
/// * `Ok(String)` - The URL of the uploaded media on success
/// * `Err(String)` - An error message if:
///   - No active account is found
///   - Account keys cannot be retrieved
///   - The upload to Blossom fails
#[tauri::command]
pub async fn upload_media(file: FileUpload, wn: State<'_, Whitenoise>) -> Result<String, String> {
    // Get the active account
    let account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    let keys = account.keys(wn.clone()).map_err(|e| e.to_string())?;

    // Upload the file to Blossom
    let blob_descriptor = wn
        .nostr
        .blossom
        .upload_media(file.data, &file.mime_type, &keys)
        .await
        .map_err(|e| format!("Failed to upload file to Blossom: {}", e))?;

    Ok(blob_descriptor.url)
}
