use std::sync::Arc;

use nostr_mls::prelude::*;
use thiserror::Error;

use crate::accounts::{Account, AccountError};
use crate::nostr_manager;
use crate::relays::RelayType;
use crate::whitenoise::Whitenoise;

#[derive(Error, Debug)]
pub enum KeyPackageError {
    #[error("No valid key package found: {0}")]
    NoValidKeyPackage(String),
    #[error("Error fetching key package: {0}")]
    FetchingKeyPackage(String),
    #[error("Account Error: {0}")]
    AccountError(#[from] AccountError),
    #[error("Nostr Error: {0}")]
    NostrError(#[from] nostr_manager::NostrManagerError),
    #[error("Nostr Client Error: {0}")]
    NostrClientError(#[from] nostr_sdk::client::Error),
    #[error("Nostr Signer Error: {0}")]
    NostrSignerError(#[from] nostr_sdk::SignerError),
    #[error("Nostr MLS Error: {0}")]
    NostrMlsError(#[from] nostr_mls::error::Error),
    #[error("Nostr MLS Not Initialized")]
    NostrMlsNotInitialized,
    #[error("Lock error: {0}")]
    LockError(String),
    #[error("Join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

#[derive(Debug)]
pub struct KeyPackageResponse {
    pub pubkey: String,
    pub event_id: EventId,
    pub key_package: KeyPackage,
}

pub type Result<T> = std::result::Result<T, KeyPackageError>;

/// Fetches key packages for a list of pubkeys
pub async fn fetch_key_packages_for_members(
    member_pubkeys: &[String],
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Vec<KeyPackageResponse>> {
    let mut member_key_packages: Vec<KeyPackageResponse> = Vec::new();

    tracing::debug!(
        target: "whitenoise::key_packages::fetch_key_packages_for_members",
        "Member pubkeys: {:?}",
        member_pubkeys
    );

    // Check that members are valid pubkeys & fetch key packages
    for pubkey in member_pubkeys.iter() {
        // Fetch prekeys from the members
        match fetch_key_package_for_pubkey(pubkey.clone(), wn.clone()).await {
            Ok(event_and_key_package) => match event_and_key_package {
                Some((event_id, kp)) => member_key_packages.push(KeyPackageResponse {
                    pubkey: pubkey.clone(),
                    event_id,
                    key_package: kp,
                }),
                None => {
                    // TODO: Need to fix this when we get to adding more than one member to a group at once.
                    return Err(KeyPackageError::NoValidKeyPackage(format!(
                        "No valid key package event found for member: {}",
                        pubkey
                    )));
                }
            },
            Err(_) => {
                return Err(KeyPackageError::FetchingKeyPackage(format!(
                    "Error fetching valid key package event for member: {}",
                    pubkey
                )));
            }
        };
    }
    Ok(member_key_packages)
}

/// Fetches key packages for a single pubkey
pub async fn fetch_key_package_for_pubkey(
    pubkey: String,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<Option<(EventId, KeyPackage)>> {
    tracing::debug!(target: "whitenoise::key_packages::fetch_key_package_for_pubkey", "Fetching key package for pubkey: {:?}", pubkey);
    let public_key = PublicKey::from_hex(&pubkey).expect("Invalid pubkey");
    let key_package_filter = Filter::new().kind(Kind::MlsKeyPackage).author(public_key);
    let key_package_events = wn
        .nostr
        .client
        .fetch_events(key_package_filter, wn.nostr.timeout().await.unwrap())
        .await
        .expect("Error fetching key_package events");

    let nostr_mls_guard = wn.nostr_mls.lock().await;
    if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let mut valid_key_packages: Vec<(EventId, KeyPackage)> = Vec::new();
        for event in key_package_events.iter() {
            let key_package = nostr_mls
                .parse_key_package(&event)
                .map_err(KeyPackageError::NostrMlsError)?;
            if key_package.ciphersuite() == nostr_mls.ciphersuite
                && key_package.last_resort()
                && key_package.leaf_node().capabilities().extensions().len()
                    == nostr_mls.extensions.len()
                && nostr_mls.extensions.iter().all(|&ext_type| {
                    key_package
                        .leaf_node()
                        .capabilities()
                        .extensions()
                        .iter()
                        .any(|ext| ext == &ext_type)
                })
            {
                valid_key_packages.push((event.id, key_package));
            }
        }

        match valid_key_packages.first() {
            Some((event_id, kp)) => {
                tracing::debug!(
                    target: "whitenoise::key_packages::fetch_key_package_for_pubkey",
                    "Found valid key package for user {:?}",
                    pubkey.clone()
                );
                Ok(Some((*event_id, kp.clone())))
            }
            None => {
                tracing::debug!(
                    target: "whitenoise::key_packages::fetch_key_package_for_pubkey",
                    "No valid key package found for user {:?}",
                    pubkey
                );
                Ok(None)
            }
        }
    } else {
        return Err(KeyPackageError::NostrMlsError(
            nostr_mls::error::Error::KeyPackage("NostrMls instance is not initialized".to_string()),
        ));
    }
}

/// Publishes a new key package to relays
pub async fn publish_key_package(wn: tauri::State<'_, Whitenoise>) -> Result<()> {
    let active_account = Account::get_active(wn.clone()).await?;
    let signer = wn.nostr.client.signer().await?;

    let key_package_relays: Vec<RelayUrl> = active_account
        .relays(RelayType::KeyPackage, wn.clone())
        .await?
        .into_iter()
        .map(|r| RelayUrl::parse(&r).expect("Invalid relay URL"))
        .collect();

    // Clone the values we need to avoid borrowing across await points
    let signer_clone = signer.clone();
    let key_package_relays_clone = key_package_relays.clone();
    let nostr_mls_arc_clone = Arc::clone(&wn.nostr_mls);

    // Check if NostrMls is initialized before spawning the blocking task
    {
        let is_initialized = nostr_mls_arc_clone
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false);
        if !is_initialized {
            return Err(KeyPackageError::NostrMlsNotInitialized);
        }
    }

    // Now spawn the blocking task, knowing that nostr_mls exists
    let result = tokio::task::spawn_blocking(move || {
        // Take a focused lock only for the operation we need
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        rt.block_on(async {
            // Only acquire the lock when absolutely needed and release it immediately
            let result = {
                // Use a scoped block to limit the lifetime of the lock
                let guard = nostr_mls_arc_clone.blocking_lock();
                if let Some(nostr_mls) = guard.as_ref() {
                    nostr_mls
                        .create_key_package(&signer_clone, key_package_relays_clone)
                        .await
                } else {
                    return Err(KeyPackageError::NostrMlsNotInitialized);
                }
            };

            result.map_err(|e| KeyPackageError::NostrMlsError(e))
        })
    })
    .await
    .unwrap_or_else(|e| Err(KeyPackageError::JoinError(e)));

    // Process the result outside the blocking thread
    match result {
        Ok(key_package_result) => {
            wn.nostr
                .client
                .send_event_to(&key_package_relays, &key_package_result)
                .await
                .map_err(|e| KeyPackageError::NostrClientError(e))?;

            tracing::debug!(
                target: "whitenoise::key_packages::publish_new_key_package",
                "Key package published: {:?}",
                key_package_result
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}
