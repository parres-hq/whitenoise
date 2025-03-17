use crate::media::{add_media_file, FileUpload, UploadedMedia};
use crate::secrets_store;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use tauri::Emitter;

const MAX_RETRIES: u32 = 3;

#[tauri::command]
pub async fn upload_file(
    group_id: Vec<u8>,
    file: FileUpload,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<UploadedMedia, String> {
    let export_secret_hex;
    let epoch;

    {
        let nostr_mls = wn.nostr_mls.lock().await;
        (export_secret_hex, epoch) = nostr_mls
            .export_secret_as_hex_secret_key_and_epoch(group_id.clone())
            .map_err(|e| e.to_string())?;
    }

    // Store the export secret key in the secrets store
    secrets_store::store_mls_export_secret(
        group_id.clone(),
        epoch,
        export_secret_hex.clone(),
        wn.data_dir.as_path(),
    )
    .map_err(|e| e.to_string())?;

    let mut retries = 0;
    let mut last_error = None;

    while retries < MAX_RETRIES {
        match add_media_file(
            &group_id,
            file.clone(),
            &export_secret_hex,
            wn.data_dir.to_str().unwrap(),
            &wn.database,
            &wn.nostr.blossom,
        )
        .await
        {
            Ok(media) => {
                // Emit success event
                app_handle
                    .emit(
                        "file_upload_success",
                        (group_id.clone(), media.blob_descriptor.url.clone()),
                    )
                    .expect("Couldn't emit event");
                return Ok(media);
            }
            Err(e) => {
                last_error = Some(e.to_string());
                retries += 1;
                if retries < MAX_RETRIES {
                    // Emit retry event
                    app_handle
                        .emit(
                            "file_upload_retry",
                            (group_id.clone(), retries, MAX_RETRIES),
                        )
                        .expect("Couldn't emit event");
                }
            }
        }
    }

    // If we get here, all retries failed
    let error = last_error.unwrap_or_else(|| "Unknown error".to_string());

    // Emit error event
    app_handle
        .emit("file_upload_error", (group_id.clone(), error.clone()))
        .expect("Couldn't emit event");

    Err(error)
}
