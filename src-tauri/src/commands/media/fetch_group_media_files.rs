use crate::media;
use crate::Whitenoise;
use nostr_mls::prelude::*;
use tauri::State;

#[tauri::command]
pub async fn fetch_group_media_files(
    group: group_types::Group,
    wn: State<'_, Whitenoise>,
) -> Result<Vec<media::CachedMediaFile>, String> {
    media::cache::fetch_group_media_files(&group, &wn.database)
        .await
        .map_err(|e| e.to_string())
}
