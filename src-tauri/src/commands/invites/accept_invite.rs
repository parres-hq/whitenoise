use crate::accounts::Account;
use crate::groups::{Group, GroupType};
use crate::invites::{Invite, InviteState};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use tauri::Emitter;

/// Accepts a group invite and joins the corresponding group.
///
/// # Arguments
/// * `invite` - The invite to accept
/// * `wn` - The Whitenoise state
/// * `app_handle` - The Tauri app handle
///
/// # Returns
/// * `Ok(())` if the invite was successfully accepted and the group was joined
/// * `Err(String)` if there was an error accepting the invite or joining the group
///
/// # Events Emitted
/// * `group_added` - Emitted with the newly joined group after successful join
/// * `invite_accepted` - Emitted with the updated invite after it is accepted
#[tauri::command]
pub async fn accept_invite(
    mut invite: Invite,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    tracing::debug!(target: "whitenoise::invites::accept_invite", "Accepting invite {:?}", invite.event.id.unwrap());

    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Scope the MutexGuard to drop it before the .await points
    let (mls_group, nostr_group_data) = {
        let nostr_mls = wn.nostr_mls.lock().await;
        let joined_group_result = nostr_mls
            .join_group_from_welcome(
                hex::decode(&invite.event.content)
                    .map_err(|e| format!("Error decoding welcome event: {}", e))?,
            )
            .map_err(|e| format!("Error joining group from welcome: {}", e))?;

        (
            joined_group_result.mls_group,
            joined_group_result.nostr_group_data,
        )
    };

    let group_type = match mls_group.members().count() {
        2 => GroupType::DirectMessage,
        _ => GroupType::Group,
    };

    let group = Group::new(
        mls_group.group_id().to_vec(),
        mls_group.epoch().as_u64(),
        group_type,
        nostr_group_data,
        wn.clone(),
        &app_handle,
    )
    .await
    .map_err(|e| format!("Failed to add group: {}", e))?;

    // Update the subscription for MLS group messages to include the new group
    let group_ids = active_account
        .groups(wn.clone())
        .await
        .map_err(|e| format!("Failed to get groups: {}", e))?
        .into_iter()
        .map(|group| group.nostr_group_id)
        .collect::<Vec<_>>();

    wn.nostr
        .subscribe_mls_group_messages(group_ids.clone())
        .await
        .map_err(|e| format!("Failed to update MLS group subscription: {}", e))?;

    // Manually fetch for MLS messages for the new group
    wn.nostr
        .fetch_group_messages(Timestamp::zero(), group_ids.clone())
        .await
        .map_err(|e| format!("Failed to fetch group messages: {}", e))?;

    app_handle
        .emit("group_added", group.clone())
        .map_err(|e| e.to_string())?;

    // Update the invite state to accepted
    invite.state = InviteState::Accepted;
    invite.save(wn.clone()).await.map_err(|e| e.to_string())?;

    app_handle
        .emit("invite_accepted", invite)
        .map_err(|e| e.to_string())?;

    tracing::debug!(target: "whitenoise::invites::accept_invite", "Accepted invite - Added group: {:?}", group);

    Ok(())
}
