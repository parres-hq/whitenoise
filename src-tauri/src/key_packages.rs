//! Key package management
//!
//! This module provides shared functionality for managing key packages, which are used to authenticate and
//! establish secure communication channels between peers.
//!
//! It includes functions for fetching key packages from Nostr relays, publishing new key packages,
//! and deleting key packages from relays.

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

    tracing::debug!(target: "whitenoise::key_packages::fetch_key_package_for_pubkey", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        wn.nostr_mls.lock(),
    )
    .await
    {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::key_packages::fetch_key_package_for_pubkey", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::key_packages::fetch_key_package_for_pubkey", "Timeout waiting for nostr_mls lock");
            return Err(KeyPackageError::NostrMlsError(
                nostr_mls::error::Error::KeyPackage(
                    "Timeout waiting for nostr_mls lock".to_string(),
                ),
            ));
        }
    };
    let result = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let mut valid_key_packages: Vec<(EventId, KeyPackage)> = Vec::new();
        for event in key_package_events.iter() {
            let key_package = nostr_mls
                .parse_key_package(event)
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
        Err(KeyPackageError::NostrMlsError(
            nostr_mls::error::Error::KeyPackage("NostrMls instance is not initialized".to_string()),
        ))
    };
    tracing::debug!(target: "whitenoise::key_packages::fetch_key_package_for_pubkey", "nostr_mls lock released");
    result
}

/// Publishes a new key package to relays
pub async fn publish_key_package(wn: tauri::State<'_, Whitenoise>) -> Result<()> {
    let active_account = Account::get_active(wn.clone()).await?;

    let key_package_relays: Vec<RelayUrl> = active_account
        .relays(RelayType::KeyPackage, wn.clone())
        .await?
        .into_iter()
        .map(|r| RelayUrl::parse(&r).expect("Invalid relay URL"))
        .collect();

    let mut encoded_key_package: Option<String> = None;
    let mut tags: Option<[Tag; 4]> = None;
    tracing::debug!(target: "whitenoise::key_packages::publish_key_package", "Attempting to acquire nostr_mls lock");
    let nostr_mls_guard = match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        wn.nostr_mls.lock(),
    )
    .await
    {
        Ok(guard) => {
            tracing::debug!(target: "whitenoise::key_packages::publish_key_package", "nostr_mls lock acquired");
            guard
        }
        Err(_) => {
            tracing::error!(target: "whitenoise::key_packages::publish_key_package", "Timeout waiting for nostr_mls lock");
            return Err(KeyPackageError::NostrMlsError(
                nostr_mls::error::Error::KeyPackage(
                    "Timeout waiting for nostr_mls lock".to_string(),
                ),
            ));
        }
    };
    let _result = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        let (encoded_key_package_value, tags_value) = nostr_mls
            .create_key_package_for_event(&active_account.pubkey, key_package_relays.clone())
            .map_err(KeyPackageError::NostrMlsError)?;
        encoded_key_package = Some(encoded_key_package_value);
        tags = Some(tags_value);
        Ok(())
    } else {
        Err(KeyPackageError::NostrMlsNotInitialized)
    };
    tracing::debug!(target: "whitenoise::key_packages::publish_key_package", "nostr_mls lock released");

    if encoded_key_package.is_some() && tags.is_some() {
        let key_package_event_builder =
            EventBuilder::new(Kind::MlsKeyPackage, encoded_key_package.unwrap())
                .tags(tags.unwrap());

        wn.nostr
            .client
            .send_event_builder_to(key_package_relays, key_package_event_builder)
            .await?;
    }

    Ok(())
}

/// Deletes a specific key package event from Nostr relays.
///
/// This function performs the following steps:
/// 1. Retrieves the relays associated with key packages for the current identity.
/// 2. Fetches the specific key package event from the Nostr network.
/// 3. Verifies that the event is a valid key package event and is authored by the current user.
/// 4. Creates and sends a delete event for the specified key package event.
///
/// # Arguments
///
/// * `event_id` - The `EventId` of the key package event to be deleted.
/// * `wn` - A Tauri State containing a Whitenoise instance, which provides access to Nostr functionality.
///
/// # Returns
///
/// * `Result<()>` - A Result that is Ok(()) if the key package was successfully deleted,
///   or an Err with a descriptive error message if any step of the process failed.
///
/// # Errors
///
/// This function may return an error if:
/// - There's an error retrieving the key package relays for the current identity.
/// - There's an error fetching the specified event from the Nostr network.
/// - The specified event is not a key package event (Kind::KeyPackage).
/// - The specified event is not authored by the current user.
/// - There's an error creating or sending the delete event.
#[allow(unused)]
pub async fn delete_key_package_from_relays(
    event_id: &EventId,
    key_package_relays: &[String],
    delete_mls_stored_keys: bool,
    wn: tauri::State<'_, Whitenoise>,
) -> Result<()> {
    let active_account = Account::get_active(wn.clone()).await?;
    let current_pubkey = active_account.pubkey;

    let key_package_filter = Filter::new()
        .id(*event_id)
        .kind(Kind::MlsKeyPackage)
        .author(current_pubkey);

    let key_package_events = wn
        .nostr
        .client
        .fetch_events(key_package_filter, wn.nostr.timeout().await.unwrap())
        .await?;

    if let Some(event) = key_package_events.first() {
        tracing::debug!(target: "whitenoise::key_packages::delete_key_package_from_relays", "Attempting to acquire nostr_mls lock");
        let nostr_mls_guard = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            wn.nostr_mls.lock(),
        )
        .await
        {
            Ok(guard) => {
                tracing::debug!(target: "whitenoise::key_packages::delete_key_package_from_relays", "nostr_mls lock acquired");
                guard
            }
            Err(_) => {
                tracing::error!(target: "whitenoise::key_packages::delete_key_package_from_relays", "Timeout waiting for nostr_mls lock");
                return Err(KeyPackageError::NostrMlsError(
                    nostr_mls::error::Error::KeyPackage(
                        "Timeout waiting for nostr_mls lock".to_string(),
                    ),
                ));
            }
        };
        let result = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Make sure we delete the private key material from MLS storage if requested
            if delete_mls_stored_keys {
                let key_package = nostr_mls
                    .parse_key_package(event)
                    .map_err(KeyPackageError::NostrMlsError)?;

                nostr_mls
                    .delete_key_package_from_storage(&key_package)
                    .map_err(KeyPackageError::NostrMlsError)?;
            }

            let builder = EventBuilder::delete(EventDeletionRequest::new().id(event.id));
            wn.nostr
                .client
                .send_event_builder_to(key_package_relays, builder)
                .await?;
            Ok(())
        } else {
            Err(KeyPackageError::NostrMlsNotInitialized)
        };
        tracing::debug!(target: "whitenoise::key_packages::delete_key_package_from_relays", "nostr_mls lock released");
        result
    } else {
        Ok(())
    }
}
