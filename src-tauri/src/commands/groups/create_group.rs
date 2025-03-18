use crate::accounts::Account;
use crate::fetch_enriched_contact;
use crate::groups::{Group, GroupType};
use crate::key_packages::fetch_key_packages_for_members;
use crate::whitenoise::Whitenoise;
use nostr_sdk::prelude::*;
use nostr_sdk::NostrSigner;
use std::ops::Add;
use tauri::Emitter;

/// Creates a new MLS group with the specified members and settings
///
/// # Arguments
/// * `creator_pubkey` - Public key of the group creator (must be the active account)
/// * `member_pubkeys` - List of public keys for group members
/// * `admin_pubkeys` - List of public keys for group admins
/// * `group_name` - Name of the group
/// * `description` - Description of the group
/// * `wn` - Whitenoise state
/// * `app_handle` - Tauri app handle
///
/// # Returns
/// * `Ok(Group)` - The newly created group
/// * `Err(String)` - Error message if group creation fails
///
/// # Flow
/// 1. Validates that active account is the creator and signer
/// 2. Validates member and admin lists
/// 3. Fetches key packages for all members
/// 4. Creates MLS group with NostrMls
/// 5. Sends welcome messages to all members via Nostr
/// 6. Adds group to GroupManager database
/// 7. Updates account with new group ID
/// 8. Emits group_added event
///
/// # Errors
/// Returns error if:
/// - Active account is not the creator
/// - Member/admin validation fails
/// - Key package fetching fails
/// - MLS group creation fails
/// - Welcome message sending fails
/// - Database operations fail
#[tauri::command]
pub async fn create_group(
    creator_pubkey: String,
    member_pubkeys: Vec<String>,
    admin_pubkeys: Vec<String>,
    group_name: String,
    description: String,
    wn: tauri::State<'_, Whitenoise>,
    app_handle: tauri::AppHandle,
) -> Result<Group, String> {
    let active_account = Account::get_active(wn.clone())
        .await
        .map_err(|e| e.to_string())?;
    let signer = wn.nostr.client.signer().await.map_err(|e| e.to_string())?;

    // Check that active account is the creator and signer
    if active_account.pubkey.to_hex() != creator_pubkey
        || active_account.pubkey.to_hex()
            != signer
                .get_public_key()
                .await
                .map_err(|e| e.to_string())?
                .to_hex()
    {
        return Err("You cannot create a group for another account".to_string());
    }

    // Run various checks on the group members
    Group::validate_group_members(&creator_pubkey, &member_pubkeys, &admin_pubkeys)
        .map_err(|e| e.to_string())?;

    // Fetch key packages for all members
    let member_key_packages = fetch_key_packages_for_members(&member_pubkeys, wn.clone())
        .await
        .map_err(|e| e.to_string())?;

    tracing::debug!(
        target: "whitenoise::groups::create_group",
        "Member key packages: {:?}",
        member_key_packages
    );

    // TODO: Add ability to specify relays for the group
    let group_relays = wn.nostr.relays().await.unwrap();

    let create_group_result;
    {
        let nostr_mls = wn.nostr_mls.lock().await;

        create_group_result = nostr_mls
            .create_group(
                group_name,
                description,
                member_key_packages
                    .iter()
                    .map(|kp| kp.key_package.clone())
                    .collect(),
                admin_pubkeys,
                creator_pubkey,
                group_relays,
            )
            .map_err(|e| e.to_string())?;
    }

    let mls_group = create_group_result.mls_group;
    let serialized_welcome_message = create_group_result.serialized_welcome_message;
    let group_data = create_group_result.nostr_group_data;

    // Fan out the welcome message to all members
    for member in member_key_packages {
        let member_pubkey = PublicKey::from_hex(&member.pubkey).map_err(|e| e.to_string())?;
        let contact =
            fetch_enriched_contact(member.pubkey.clone(), false, wn.clone(), app_handle.clone())
                .await?;

        // We only want to connect to user relays in release mode
        let relay_urls: Vec<String> = if cfg!(dev) {
            vec!["ws://localhost:8080".to_string()]
        } else if !contact.inbox_relays.is_empty() {
            contact.inbox_relays
        } else if !contact.nostr_relays.is_empty() {
            contact.nostr_relays
        } else {
            // Get default relays from the client
            wn.nostr
                .client
                .relays()
                .await
                .keys()
                .map(|url| url.to_string())
                .collect()
        };

        let welcome_rumor =
            EventBuilder::new(Kind::MlsWelcome, hex::encode(&serialized_welcome_message)).tags(
                vec![
                    Tag::from_standardized(TagStandard::Relays(
                        relay_urls
                            .iter()
                            .filter_map(|r| Url::parse(r).ok())
                            .collect(),
                    )),
                    Tag::event(member.event_id),
                ],
            );

        tracing::debug!(
            target: "whitenoise::groups::create_group",
            "Welcome rumor: {:?}",
            welcome_rumor
        );

        // Create a timestamp 1 month in the future
        let one_month_future = Timestamp::now().add(30 * 24 * 60 * 60);

        let wrapped_event = EventBuilder::gift_wrap(
            &signer,
            &member_pubkey,
            welcome_rumor,
            vec![Tag::expiration(one_month_future)],
        )
        .await
        .map_err(|e| e.to_string())?;

        let max_retries = 5;
        let mut retry_count = 0;
        let mut last_error = None;

        let mut relays_to_remove: Vec<String> = Vec::new();

        for url in relay_urls.clone() {
            let to_remove = wn
                .nostr
                .client
                .add_relay(url.clone())
                .await
                .map_err(|e| e.to_string())?;
            if to_remove {
                relays_to_remove.push(url);
            }
        }

        while retry_count < max_retries {
            match wn
                .nostr
                .client
                .send_event_to(relay_urls.clone(), wrapped_event.clone())
                .await
            {
                Ok(result) => {
                    // Successfully sent, break the loop
                    // TODO: Remove the identifying info from the log
                    tracing::info!(
                        target: "whitenoise::groups::create_group",
                        "Sent welcome message RESULT: {:?}",
                        result
                    );
                    tracing::info!(
                        target: "whitenoise::groups::create_group",
                        "Successfully sent welcome message {:?} to {:?} on {:?}",
                        wrapped_event,
                        &member_pubkey,
                        &relay_urls
                    );
                    break;
                }
                Err(e) => {
                    tracing::error!(
                        target: "whitenoise::groups::create_group",
                        "Failed to send welcome message to {:?} on {:?}: {:?}",
                        &member_pubkey,
                        &relay_urls,
                        e
                    );
                    last_error = Some(e);
                    retry_count += 1;
                    if retry_count < max_retries {
                        // Wait for a short time before retrying
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }

        if retry_count == max_retries {
            return Err(format!(
                "Failed to send welcome message to {:?} on {:?} after {} attempts. Last error: {:?}",
                &member_pubkey, &relay_urls, max_retries, last_error
            ));
        }

        tracing::debug!(
            target: "whitenoise::groups::create_group",
            "Published welcome message to {:?} on {:?}: ID: {:?}",
            &member_pubkey,
            &relay_urls,
            wrapped_event.id
        );

        for url in relays_to_remove {
            wn.nostr
                .client
                .remove_relay(url)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    let group_type = if mls_group.members().count() == 2 {
        GroupType::DirectMessage
    } else {
        GroupType::Group
    };

    let group_id = mls_group.group_id().to_vec();

    // Create the group and save it to the database
    let nostr_group = Group::new(
        group_id.clone(),
        mls_group.epoch().as_u64(),
        group_type,
        group_data,
        wn.clone(),
        &app_handle,
    )
    .await
    .map_err(|e| e.to_string())?;

    tracing::debug!(
        target: "whitenoise::groups::create_group",
        "Added group to database: {:?}",
        nostr_group
    );

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

    app_handle
        .emit("group_added", nostr_group.clone())
        .map_err(|e| e.to_string())?;

    Ok(nostr_group)
}
