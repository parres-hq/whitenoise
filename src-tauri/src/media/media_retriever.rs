use crate::accounts::Account;
use crate::media::sanitizer::SafeMediaMetadata;
use crate::media::MediaError;
use crate::utils::{execute_with_retry, GeneralRetryError};
use crate::whitenoise::Whitenoise;
use nostr_mls::prelude::group_types;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri::Emitter;
use tokio::time::Duration;

const MAX_DOWNLOAD_ATTEMPTS: u32 = 4;
const INITIAL_DOWNLOAD_DELAY_SECS: u64 = 1;
const DOWNLOAD_BACKOFF_FACTOR: u32 = 2;
const DOWNLOAD_TIMEOUT_SECS: std::time::Duration = std::time::Duration::from_secs(60);

pub async fn retrieve_and_cache_media_file(
    group: &group_types::Group,
    decryption_nonce_hex: &str,
    mime_type: &str,
    dimensions: Option<(u32, u32)>,
    file_hash_original: &str,
    blossom_url: &str,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: &AppHandle,
) -> Result<String, MediaError> {
    let nonce_bytes = validate_and_decode_params(
        decryption_nonce_hex,
        mime_type,
        file_hash_original,
        blossom_url,
    )?;

    let group_id_hex = hex::encode(group.mls_group_id.as_slice());

    let (active_account, exporter_secret) =
        get_account_and_secret(wn.clone(), &group.mls_group_id).await?;

    let encrypted_data = download_file_with_retries(app_handle, &group_id_hex, blossom_url).await?;

    let decrypted_data = decrypt_file_data(
        app_handle,
        &group_id_hex,
        &encrypted_data,
        &exporter_secret.secret,
        &nonce_bytes,
    )?;

    verify_file_hash(
        app_handle,
        &group_id_hex,
        &decrypted_data,
        file_hash_original,
    )?;

    let safe_metadata = construct_safe_media_metadata(mime_type, decrypted_data.len(), dimensions);

    add_file_to_cache_and_notify(
        wn.clone(),
        app_handle,
        decrypted_data,
        group,
        &active_account.pubkey.to_string(),
        blossom_url,
        safe_metadata,
    )
    .await
}

async fn get_account_and_secret(
    wn: tauri::State<'_, Whitenoise>,
    group_id: &nostr_mls::prelude::GroupId,
) -> Result<
    (
        Account,
        nostr_mls::prelude::group_types::GroupExporterSecret,
    ),
    MediaError,
> {
    let active_account = Account::get_active(wn.clone()).await.map_err(|e| match e {
        crate::accounts::AccountError::NoActiveAccount => MediaError::NoActiveAccount,
        _ => MediaError::NostrMLS(format!("FailedToGetActiveAccount: {}", e)),
    })?;

    let exporter_secret = {
        let nostr_mls_guard = wn.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            nostr_mls.exporter_secret(group_id).map_err(|e| {
                MediaError::NostrMLS(format!("NostrMLS error getting exporter secret: {}", e))
            })?
        } else {
            return Err(MediaError::NostrMLSNotInitialized);
        }
    };
    Ok((active_account, exporter_secret))
}

async fn download_file_with_retries(
    app_handle: &AppHandle,
    group_id_hex: &str,
    blossom_url: &str,
) -> Result<Vec<u8>, MediaError> {
    let operation_description = format!("download from {}", blossom_url);
    let max_download_attempts = MAX_DOWNLOAD_ATTEMPTS;
    let initial_download_delay = Duration::from_secs(INITIAL_DOWNLOAD_DELAY_SECS);
    let download_backoff_factor = DOWNLOAD_BACKOFF_FACTOR;
    let client = reqwest::Client::new();

    let download_result = execute_with_retry(
        operation_description.clone(),
        max_download_attempts,
        initial_download_delay,
        download_backoff_factor,
        || async { attempt_download_once(&client, blossom_url).await },
        |failed_attempt_num, _total_attempts, next_delay, error_ref| {
            let max_retries = max_download_attempts - 1;
            app_handle
                .emit(
                    "file_download_retry",
                    (
                        group_id_hex,
                        blossom_url,
                        failed_attempt_num,
                        max_retries,
                        error_ref,
                        next_delay.as_secs(),
                    ),
                )
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to emit file_download_retry event: {}", e);
                });
        },
    )
    .await;

    match download_result {
        Ok(data) => Ok(data),
        Err(GeneralRetryError::MaxRetriesExceeded { last_error, .. }) => {
            app_handle
                .emit(
                    "file_download_error",
                    (group_id_hex, blossom_url, &last_error),
                )
                .map_err(|e| {
                    MediaError::Cache(format!(
                        "Tauri event emit error for final download failure: {}",
                        e
                    ))
                })?;
            Err(MediaError::Download(last_error))
        }
    }
}

fn decrypt_file_data(
    app_handle: &AppHandle,
    group_id_hex: &str,
    encrypted_data: &[u8],
    exporter_secret_bytes: &[u8],
    nonce_bytes: &[u8],
) -> Result<Vec<u8>, MediaError> {
    crate::media::encryption::decrypt_file(encrypted_data, exporter_secret_bytes, nonce_bytes)
        .map_err(|e| {
            let error_msg = format!("Decryption failed: {}", e);
            app_handle
                .emit(
                    "file_download_error",
                    (group_id_hex, "decryption_error", &error_msg),
                )
                .unwrap_or_else(|log_e| {
                    tracing::warn!(
                        "Failed to emit file_download_error for decryption: {}",
                        log_e
                    );
                });
            MediaError::Decryption(error_msg)
        })
}

fn verify_file_hash(
    app_handle: &AppHandle,
    group_id_hex: &str,
    decrypted_data: &[u8],
    file_hash_original: &str,
) -> Result<(), MediaError> {
    let mut hasher = Sha256::new();
    hasher.update(decrypted_data);
    let calculated_hash = format!("{:x}", hasher.finalize());

    if !calculated_hash.eq_ignore_ascii_case(file_hash_original) {
        let error_msg = format!(
            "File integrity check failed: Hash mismatch. Expected: {}, Calculated: {}",
            file_hash_original, calculated_hash
        );
        app_handle
            .emit(
                "file_download_error",
                (group_id_hex, "hash_verification_error", &error_msg),
            )
            .unwrap_or_else(|log_e| {
                tracing::warn!(
                    "Failed to emit file_download_error for hash verification: {}",
                    log_e
                );
            });
        return Err(MediaError::Verification(error_msg));
    }
    Ok(())
}

async fn add_file_to_cache_and_notify(
    wn: tauri::State<'_, Whitenoise>,
    app_handle: &AppHandle,
    decrypted_data: Vec<u8>,
    group: &group_types::Group,
    active_account_pubkey: &str,
    blossom_url: &str,
    safe_metadata: SafeMediaMetadata,
) -> Result<String, MediaError> {
    let data_dir_path_str = wn.data_dir.to_str().ok_or_else(|| {
        MediaError::Cache("Invalid data directory path for media cache".to_string())
    })?;

    let media_file_record = crate::media::cache::add_to_cache(
        &decrypted_data,
        group,
        active_account_pubkey,
        Some(blossom_url.to_string()),
        None, // nostr_key - not applicable for downloaded files
        Some(safe_metadata),
        data_dir_path_str,
        &wn.database,
    )
    .await
    .map_err(|e| {
        let error_msg = format!("Failed to cache downloaded file: {}", e);
        tracing::error!("Error during caching: {}. Emitting event.", e);
        app_handle
            .emit(
                "file_download_error",
                (hex::encode(group.mls_group_id.as_slice()), blossom_url, &error_msg),
            )
            .unwrap_or_else(|emit_err| {
                tracing::warn!(
                    "Failed to emit file_download_error for caching failure: {}. Original cache error: {}",
                    emit_err, e
                );
            });
        e
    })?;

    app_handle
        .emit(
            "file_download_success",
            (
                &media_file_record.blossom_url,
                &media_file_record.file_path,
                &media_file_record
                    .file_metadata
                    .map(|m| m.mime_type)
                    .unwrap_or_default(),
            ),
        )
        .unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to emit file_download_success event for {}: {}",
                media_file_record.file_path,
                e
            );
        });

    Ok(media_file_record.file_path)
}

fn construct_safe_media_metadata(
    mime_type: &str,
    data_len: usize,
    dimensions: Option<(u32, u32)>,
) -> SafeMediaMetadata {
    SafeMediaMetadata {
        mime_type: mime_type.to_string(),
        size_bytes: data_len as u64,
        format: Some(
            mime_type
                .split('/')
                .nth(1)
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string(),
        ),
        dimensions,
        color_space: None,
        has_alpha: None,
        bits_per_pixel: None,
        duration_seconds: None,
        frame_rate: None,
        video_codec: None,
        audio_codec: None,
        video_bitrate: None,
        audio_bitrate: None,
        video_dimensions: None,
        page_count: None,
        author: None,
        title: None,
        created_at: None,
        modified_at: None,
    }
}

async fn attempt_download_once(
    client: &reqwest::Client,
    blossom_url: &str,
) -> Result<Vec<u8>, String> {
    match client
        .get(blossom_url)
        .timeout(DOWNLOAD_TIMEOUT_SECS)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.bytes().await {
                    Ok(bytes) => Ok(bytes.to_vec()),
                    Err(e) => Err(format!("Failed to read downloaded bytes: {}", e)),
                }
            } else {
                Err(format!(
                    "Download failed with status: {}",
                    response.status()
                ))
            }
        }
        Err(e) => Err(format!("Failed to initiate download: {}", e)),
    }
}

fn validate_and_decode_params(
    decryption_nonce_hex: &str,
    mime_type: &str,
    file_hash_original: &str,
    blossom_url: &str,
) -> Result<Vec<u8>, MediaError> {
    if decryption_nonce_hex.is_empty()
        || mime_type.is_empty()
        || file_hash_original.is_empty()
        || blossom_url.is_empty()
    {
        return Err(MediaError::Metadata(
            "Missing required imeta fields (url, nonce, mime_type, or original file hash 'x')"
                .to_string(),
        ));
    }
    hex::decode(decryption_nonce_hex)
        .map_err(|e| MediaError::Metadata(format!("Failed to decode nonce: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaError;

    #[test]
    fn test_validate_and_decode_params_valid() {
        let nonce_hex = "0123456789abcdef";
        let mime_type = "image/png";
        let hash = "somehash";
        let url = "https://example.com/image.png";
        let result = validate_and_decode_params(nonce_hex, mime_type, hash, url);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), hex::decode(nonce_hex).unwrap());
    }
    #[test]
    fn test_validate_and_decode_params_empty_nonce() {
        let result = validate_and_decode_params("", "image/png", "hash", "url");
        assert!(matches!(result, Err(MediaError::Metadata(_))));
        if let Err(MediaError::Metadata(msg)) = result {
            assert!(msg.contains("Missing required imeta fields"));
        }
    }
    #[test]
    fn test_validate_and_decode_params_empty_mime_type() {
        let result = validate_and_decode_params("nonce", "", "hash", "url");
        assert!(matches!(result, Err(MediaError::Metadata(_))));
        if let Err(MediaError::Metadata(msg)) = result {
            assert!(msg.contains("Missing required imeta fields"));
        }
    }
    #[test]
    fn test_validate_and_decode_params_empty_hash() {
        let result = validate_and_decode_params("nonce", "mime", "", "url");
        assert!(matches!(result, Err(MediaError::Metadata(_))));
        if let Err(MediaError::Metadata(msg)) = result {
            assert!(msg.contains("Missing required imeta fields"));
        }
    }
    #[test]
    fn test_validate_and_decode_params_empty_url() {
        let result = validate_and_decode_params("nonce", "mime", "hash", "");
        assert!(matches!(result, Err(MediaError::Metadata(_))));
        if let Err(MediaError::Metadata(msg)) = result {
            assert!(msg.contains("Missing required imeta fields"));
        }
    }
    #[test]
    fn test_validate_and_decode_params_invalid_nonce_hex() {
        let result = validate_and_decode_params("not-hex", "mime", "hash", "url");
        assert!(matches!(result, Err(MediaError::Metadata(_))));
        if let Err(MediaError::Metadata(msg)) = result {
            assert!(msg.contains("Failed to decode nonce"));
        }
    }

    #[tokio::test]
    async fn test_attempt_download_once_success() {
        let client = reqwest::Client::new();
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/file.enc")
            .with_status(200)
            .with_body("some_data")
            .create_async()
            .await;
        let result = attempt_download_once(&client, &format!("{}/file.enc", server.url())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"some_data");
        mock.assert_async().await;
    }
    #[tokio::test]
    async fn test_attempt_download_once_server_error() {
        let client = reqwest::Client::new();
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/file.enc")
            .with_status(500)
            .create_async()
            .await;
        let result = attempt_download_once(&client, &format!("{}/file.enc", server.url())).await;
        assert!(result.is_err());
        if let Err(msg) = result {
            assert!(msg.contains("Download failed with status: 500"));
        }
        mock.assert_async().await;
    }
    #[tokio::test]
    async fn test_attempt_download_once_network_error() {
        let client = reqwest::Client::new();
        let result = attempt_download_once(&client, "http://127.0.0.1:1/file.enc").await;
        assert!(result.is_err());
        if let Err(msg) = result {
            assert!(
                msg.contains("Failed to initiate download")
                    || msg.contains("Connection refused")
                    || msg.contains("Network is unreachable")
            );
        }
    }

    #[test]
    fn test_construct_safe_media_metadata_basic() {
        let mime = "image/png";
        let len = 1024;
        let dims = Some((100, 200));
        let metadata = construct_safe_media_metadata(mime, len, dims);

        assert_eq!(metadata.mime_type, mime);
        assert_eq!(metadata.size_bytes, len as u64);
        assert_eq!(metadata.format, Some("png".to_string()));
        assert_eq!(metadata.dimensions, dims);
        assert!(metadata.color_space.is_none());
        assert!(metadata.video_dimensions.is_none());
    }

    #[test]
    fn test_construct_safe_media_metadata_no_dimensions() {
        let mime = "application/pdf";
        let len = 512;
        let metadata = construct_safe_media_metadata(mime, len, None);

        assert_eq!(metadata.mime_type, mime);
        assert_eq!(metadata.size_bytes, len as u64);
        assert_eq!(metadata.format, Some("pdf".to_string()));
        assert!(metadata.dimensions.is_none());
        assert!(metadata.video_dimensions.is_none());
    }

    #[test]
    fn test_construct_safe_media_metadata_mime_format_extraction() {
        let metadata_video = construct_safe_media_metadata("video/mp4", 0, None);
        assert_eq!(metadata_video.format, Some("mp4".to_string()));

        let metadata_audio = construct_safe_media_metadata("audio/mpeg", 0, None);
        assert_eq!(metadata_audio.format, Some("mpeg".to_string()));

        // Test with a mime type that doesn't have a clear subtype after '/'
        let metadata_simple = construct_safe_media_metadata("text", 0, None);
        assert_eq!(metadata_simple.format, Some("unknown".to_string()));

        // Test with an empty subtype
        let metadata_empty_subtype = construct_safe_media_metadata("image/", 0, None);
        assert_eq!(metadata_empty_subtype.format, Some("unknown".to_string()));

        // Test with a complex mime type
        let metadata_complex =
            construct_safe_media_metadata("application/vnd.oasis.opendocument.text", 0, None);
        assert_eq!(
            metadata_complex.format,
            Some("vnd.oasis.opendocument.text".to_string())
        );
    }
}
