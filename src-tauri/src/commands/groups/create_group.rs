use std::ops::Add;

use nostr_mls::prelude::*;
use nostr_sdk::NostrSigner;
use tauri::Emitter;

use crate::accounts::Account;
use crate::fetch_enriched_contact;
use crate::key_packages::fetch_key_packages_for_members;
use crate::whitenoise::Whitenoise;

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
) -> Result<group_types::Group, String> {
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

    // Fetch key packages for all members
    let member_key_packages = fetch_key_packages_for_members(&member_pubkeys, wn.clone())
        .await
        .map_err(|e| e.to_string())?;
    let member_pubkeys = member_pubkeys
        .iter()
        .map(|pk| PublicKey::from_hex(pk).unwrap())
        .collect::<Vec<_>>();
    let admin_pubkeys = admin_pubkeys
        .iter()
        .map(|pk| PublicKey::from_hex(pk).unwrap())
        .collect::<Vec<_>>();
    let creator_pubkey = PublicKey::from_hex(&creator_pubkey).unwrap();

    tracing::debug!(
        target: "whitenoise::groups::create_group",
        "Member key packages: {:?}",
        member_key_packages
    );

    // TODO: Add ability to specify relays for the group
    let group_relays = wn
        .nostr
        .relays()
        .await
        .unwrap()
        .into_iter()
        .map(|r| RelayUrl::parse(&r).unwrap())
        .collect::<Vec<_>>();

    let group: group_types::Group;
    let serialized_welcome_message: Vec<u8>;
    let group_ids: Vec<String>;

    let nostr_mls_guard = wn.nostr_mls.lock().await;

    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let create_group_result = nostr_mls
            .create_group(
                group_name,
                description,
                &creator_pubkey,
                member_pubkeys,
                member_key_packages
                    .iter()
                    .map(|kp| kp.key_package.clone())
                    .collect(),
                admin_pubkeys,
                group_relays,
            )
            .map_err(|e| e.to_string())?;

        group = create_group_result.group;
        serialized_welcome_message = create_group_result.serialized_welcome_message;
        group_ids = nostr_mls
            .get_groups()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|g| hex::encode(g.mls_group_id.as_slice()))
            .collect::<Vec<_>>();
    } else {
        return Err("Nostr MLS not initialized".to_string());
    }

    // Fan out the welcome message to all members
    for member in member_key_packages {
        let member_pubkey = PublicKey::from_hex(&member.pubkey).map_err(|e| e.to_string())?;
        let contact =
            fetch_enriched_contact(member.pubkey.clone(), false, wn.clone(), app_handle.clone())
                .await?;

        // We only want to connect to user relays in release mode
        let relay_urls: Vec<String> = if cfg!(dev) {
            vec![
                "ws://localhost:8080".to_string(),
                "ws://localhost:7777".to_string(),
            ]
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
            EventBuilder::new(Kind::MlsWelcome, hex::encode(&serialized_welcome_message))
                .tags(vec![
                    Tag::from_standardized(TagStandard::Relays(
                        relay_urls
                            .iter()
                            .filter_map(|r| RelayUrl::parse(r).ok())
                            .collect(),
                    )),
                    Tag::event(member.event_id),
                ])
                .build(active_account.pubkey);

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
                .send_event_to(relay_urls.clone(), &wrapped_event)
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

    wn.nostr
        .subscribe_mls_group_messages(group_ids.clone())
        .await
        .map_err(|e| format!("Failed to update MLS group subscription: {}", e))?;

    app_handle
        .emit("group_added", group.clone())
        .map_err(|e| e.to_string())?;

    Ok(group)
}
