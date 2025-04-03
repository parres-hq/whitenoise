use crate::invites::{Invite, ProcessedInvite};
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct InvitesWithFailures {
    invites: Vec<Invite>,
    failures: Vec<(EventId, String)>,
}

/// Fetches invites from the database for the active user
#[tauri::command]
pub async fn get_invites(wn: tauri::State<'_, Whitenoise>) -> Result<InvitesWithFailures, String> {
    let pending_invites = Invite::pending(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    let failed_invites: Vec<(EventId, String)> = ProcessedInvite::failed_with_reason(wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!(target: "whitenoise::commands::invites::get_invites", "Pending invites: {:?}", pending_invites);
    tracing::debug!(target: "whitenoise::commands::invites::get_invites", "Failed invites: {:?}", failed_invites);

    Ok(InvitesWithFailures {
        invites: pending_invites,
        failures: failed_invites,
    })
}
