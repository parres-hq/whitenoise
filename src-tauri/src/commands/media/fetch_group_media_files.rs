use crate::media;
use crate::Whitenoise;
use ::hex;
use nostr_mls::prelude::*;
use tauri::State;

#[tauri::command]
pub async fn fetch_group_media_files(
    group_id: String,
    wn: State<'_, Whitenoise>,
) -> Result<Vec<media::CachedMediaFile>, String> {
    // Convert hex string to bytes
    let mls_group_id =
        hex::decode(&group_id).map_err(|e| format!("Invalid group ID format: {}", e))?;

    // Create a minimal group with just the ID
    let group = nostr_mls::prelude::group_types::Group {
        mls_group_id: nostr_mls::prelude::GroupId::from_slice(&mls_group_id),
        nostr_group_id: [0u8; 32],
        name: String::new(),
        description: String::new(),
        admin_pubkeys: std::collections::BTreeSet::new(),
        last_message_id: None,
        last_message_at: None,
        group_type: nostr_mls::prelude::group_types::GroupType::DirectMessage,
        epoch: 0,
        state: nostr_mls::prelude::group_types::GroupState::Active,
    };

    media::cache::fetch_group_media_files(&group, &wn.database)
        .await
        .map_err(|e| e.to_string())
}
