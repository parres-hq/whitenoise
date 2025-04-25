use crate::accounts::Account;
use crate::whitenoise::Whitenoise;
use nostr_mls::NostrMls;
use nostr_mls_sqlite_storage::NostrMlsSqliteStorage;
use nostr_sdk::prelude::*;

#[tauri::command]
pub async fn init_nostr_for_current_user(
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let current_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Then update Nostr MLS instance
    {
        let mut nostr_mls = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            wn.nostr_mls.lock(),
        )
        .await
        {
            Ok(guard) => guard,
            Err(_) => {
                tracing::error!(
                    target: "whitenoise::commands::nostr::init_nostr_for_current_user",
                    "Timeout waiting for nostr_mls lock"
                );
                return Err("Timeout waiting for nostr_mls lock".to_string());
            }
        };
        // Create the nostr-mls instance at the data_dir/mls/pubkey directory to partition data between users
        let storage_dir = wn
            .data_dir
            .join("mls")
            .join(current_account.pubkey.to_hex());
        *nostr_mls = Some(NostrMls::new(
            NostrMlsSqliteStorage::new(storage_dir).map_err(|e| e.to_string())?,
        ));
    }

    // Update Nostr identity and connect relays
    wn.nostr
        .set_nostr_identity(&current_account, wn.clone(), &app_handle)
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!(
        target: "whitenoise::commands::nostr::init_nostr_for_current_user",
        "Nostr initialized for current user"
    );
    Ok(())
}
