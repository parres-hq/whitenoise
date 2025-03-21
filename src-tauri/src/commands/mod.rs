use crate::whitenoise::Whitenoise;

pub mod accounts;
pub mod groups;
pub mod invites;
pub mod key_packages;
pub mod media;
pub mod messages;
pub mod nostr;
pub mod payments;

#[tauri::command]
pub async fn delete_all_data(wn: tauri::State<'_, Whitenoise>) -> Result<(), String> {
    wn.delete_all_data().await.map_err(|e| e.to_string())?;
    Ok(())
}
