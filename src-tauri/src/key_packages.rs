use crate::accounts::{Account, AccountError};
use crate::nostr_manager;
use crate::relays::RelayType;
use crate::whitenoise::Whitenoise;
use nostr_openmls::key_packages::{create_key_package_for_event, KeyPackage};
use nostr_sdk::prelude::*;
use thiserror::Error;

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
    NostrMlsError(#[from] nostr_openmls::key_packages::KeyPackageError),
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
        .fetch_events(vec![key_package_filter], wn.nostr.timeout().await.unwrap())
        .await
        .expect("Error fetching key_package events");

    let nostr_mls = wn.nostr_mls.lock().await;
    let ciphersuite = nostr_mls.ciphersuite;
    let extensions = nostr_mls.extensions.clone();

    let mut valid_key_packages: Vec<(EventId, KeyPackage)> = Vec::new();
    for event in key_package_events.iter() {
        let key_package =
            nostr_openmls::key_packages::parse_key_package(event.content.to_string(), &nostr_mls)
                .map_err(KeyPackageError::NostrMlsError)?;
        if key_package.ciphersuite() == ciphersuite
            && key_package.last_resort()
            && key_package.leaf_node().capabilities().extensions().len() == extensions.len()
            && extensions.iter().all(|&ext_type| {
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
    let current_pubkey = wn
        .nostr
        .client
        .signer()
        .await
        .unwrap()
        .get_public_key()
        .await
        .unwrap();
    let key_package_events = wn
        .nostr
        .client
        .fetch_events(
            vec![Filter::new()
                .id(*event_id)
                .kind(Kind::MlsKeyPackage)
                .author(current_pubkey)],
            wn.nostr.timeout().await.unwrap(),
        )
        .await?;

    if let Some(event) = key_package_events.first() {
        // Make sure we delete the private key material from MLS storage if requested
        if delete_mls_stored_keys {
            let nostr_mls = wn.nostr_mls.lock().await;
            let key_package = nostr_openmls::key_packages::parse_key_package(
                event.content.to_string(),
                &nostr_mls,
            )
            .map_err(KeyPackageError::NostrMlsError)?;

            nostr_openmls::key_packages::delete_key_package_from_storage(key_package, &nostr_mls)
                .map_err(KeyPackageError::NostrMlsError)?;
        }
        let builder = EventBuilder::delete(vec![event.id]);
        wn.nostr
            .client
            .send_event_builder_to(key_package_relays, builder)
            .await?;
    }
    Ok(())
}

pub async fn publish_key_package(wn: tauri::State<'_, Whitenoise>) -> Result<()> {
    let active_account = Account::get_active(wn.clone()).await?;
    let pubkey = active_account.pubkey;

    let event: EventBuilder;
    let key_package_relays = if cfg!(dev) {
        vec![
            "ws://localhost:8080".to_string(),
            "ws://localhost:7777".to_string(),
        ]
    } else {
        active_account
            .relays(RelayType::KeyPackage, wn.clone())
            .await?
    };

    {
        let nostr_mls = wn.nostr_mls.lock().await;
        let ciphersuite = nostr_mls.ciphersuite_value().to_string();
        let extensions = nostr_mls.extensions_value();

        let serialized_key_package = create_key_package_for_event(pubkey.to_hex(), &nostr_mls)?;

        event = EventBuilder::new(Kind::MlsKeyPackage, serialized_key_package).tags([
            Tag::custom(TagKind::MlsProtocolVersion, ["1.0"]),
            Tag::custom(TagKind::MlsCiphersuite, [ciphersuite]),
            Tag::custom(TagKind::MlsExtensions, [extensions]),
            Tag::custom(TagKind::Client, ["whitenoise"]),
            Tag::custom(TagKind::Relays, key_package_relays.clone()),
        ]);
    }
    wn.nostr
        .client
        .send_event_builder_to(key_package_relays.clone(), event)
        .await?;

    Ok(())
}
