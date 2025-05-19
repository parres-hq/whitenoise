use crate::media;
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::group_types;

#[tauri::command]
pub async fn download_file(
    group: group_types::Group,
    decryption_nonce_hex: String,
    mime_type: String,
    dimensions: Option<(u32, u32)>,
    file_hash_original: String,
    blossom_url: String,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    match media::retrieve_and_cache_media_file(
        &group,
        &decryption_nonce_hex,
        &mime_type,
        dimensions,
        &file_hash_original,
        &blossom_url,
        wn,
        &app_handle,
    )
    .await
    {
        Ok(file_path) => Ok(file_path),
        Err(media_error) => Err(media_error.to_string()),
    }
}
