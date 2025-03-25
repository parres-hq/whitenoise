use crate::accounts::{Account, AccountError};
use crate::groups::{Group, GroupError};
use crate::invites::{Invite, InviteError, InviteState, ProcessedInvite, ProcessedInviteState};
use crate::key_packages;
use crate::messages::{MessageError, ProcessedMessage, ProcessedMessageState};
use crate::nostr_manager::NostrManagerError;
use crate::relays::RelayType;
use crate::secrets_store;
use crate::Whitenoise;
use nostr_openmls::groups::GroupError as NostrOpenmlsGroupError;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, Receiver, Sender};

#[derive(Error, Debug)]
pub enum EventProcessorError {
    #[error("Failed to send event to channel")]
    UnqueueableEvent(#[from] SendError<ProcessableEvent>),
    #[error("Failed to process event")]
    UnprocessableEvent(#[from] NostrManagerError),
    #[error("Error getting account")]
    NoAccount(#[from] AccountError),
    #[error("Error decoding hex")]
    UndecodableHex(#[from] nostr_sdk::util::hex::Error),
    #[error("Error saving invite: {0}")]
    BadInvite(#[from] InviteError),
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Key package error: {0}")]
    KeyPackageError(#[from] key_packages::KeyPackageError),
    #[error("Group error: {0}")]
    GroupError(#[from] GroupError),
    #[error("NIP44 encryption error: {0}")]
    EncryptionError(#[from] nostr_sdk::nips::nip44::Error),
    #[error("OpenMLS group error: {0}")]
    OpenMlsGroupNotFound(#[from] nostr_openmls::groups::GroupError),
    #[error("Secrets store error: {0}")]
    SecretsStoreError(#[from] secrets_store::SecretsStoreError),
    #[error("Key parsing error: {0}")]
    UnparseableKey(#[from] nostr_sdk::key::Error),
    #[error("Message error: {0}")]
    MessageError(#[from] MessageError),
}

pub type Result<T> = std::result::Result<T, EventProcessorError>;

#[derive(Debug)]
pub enum ProcessableEvent {
    GiftWrap(Event),
    MlsMessage(Event),
}

#[derive(Debug)]
pub struct EventProcessor {
    sender: Sender<ProcessableEvent>,
    shutdown: Sender<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlsMessageReceivedEvent {
    pub group_id: Vec<u8>,
    pub event: UnsignedEvent,
}

impl EventProcessor {
    pub fn new(app_handle: AppHandle) -> Self {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Creating new event processor"
        );
        let (sender, receiver) = mpsc::channel(500);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let app_handle_clone = app_handle;

        // Spawn the processing loop
        tokio::spawn(async move {
            tracing::debug!(
                target: "whitenoise::nostr_manager::event_processor",
                "Starting event processor loop"
            );
            Self::process_events(receiver, shutdown_rx, app_handle_clone).await;
            tracing::debug!(
                target: "whitenoise::nostr_manager::event_processor",
                "Event processor loop ended"
            );
        });

        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Event processor created successfully"
        );
        Self {
            sender,
            shutdown: shutdown_tx,
        }
    }

    pub async fn queue_event(&self, event: ProcessableEvent) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Queuing event: {:?}",
            event
        );
        match self.sender.send(event).await {
            Ok(_) => {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "Event queued successfully"
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "Failed to queue event: {}",
                    e
                );
                Err(e.into())
            }
        }
    }

    async fn process_events(
        mut receiver: Receiver<ProcessableEvent>,
        mut shutdown: Receiver<()>,
        app_handle: AppHandle,
    ) {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Entering process_events loop"
        );
        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Received event in processing loop"
                    );
                    match event {
                        ProcessableEvent::GiftWrap(event) => {
                            if let Err(e) = Self::process_giftwrap(&app_handle, event).await {
                                tracing::error!(
                                    target: "whitenoise::nostr_manager::event_processor",
                                    "Error processing giftwrap: {}",
                                    e
                                );
                            }
                        }
                        ProcessableEvent::MlsMessage(event) => {
                            if let Err(e) = Self::process_mls_message(&app_handle, event).await {
                                tracing::error!(
                                    target: "whitenoise::nostr_manager::event_processor",
                                    "Error processing MLS message: {}",
                                    e
                                );
                            }
                        }
                    }
                }
                Some(_) = shutdown.recv() => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Received shutdown signal"
                    );
                    break;
                }
                else => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "All channels closed, exiting process_events loop"
                    );
                    break;
                }
            }
        }
    }

    pub async fn clear_queue(&self) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Attempting to clear queue"
        );
        match self.shutdown.send(()).await {
            Ok(_) => {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "Shutdown signal sent successfully"
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "Failed to send shutdown signal: {}",
                    e
                );
                Ok(()) // Still return Ok since this is expected in some cases
            }
        }
    }

    async fn process_giftwrap(app_handle: &AppHandle, event: Event) -> Result<()> {
        let wn = app_handle.state::<Whitenoise>();
        let active_account = Account::get_active(wn.clone()).await?;
        let keys = active_account.keys(wn.clone())?;
        if let Ok(unwrapped) = extract_rumor(&keys, &event).await {
            match unwrapped.rumor.kind {
                Kind::MlsWelcome => {
                    Self::process_invite(app_handle, active_account, event, unwrapped.rumor)
                        .await?;
                }
                Kind::PrivateDirectMessage => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Received private direct message: {:?}",
                        unwrapped.rumor
                    );
                }
                _ => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Received unhandled giftwrap of kind {:?}",
                        unwrapped.rumor.kind
                    );
                }
            }
        }
        Ok(())
    }

    async fn process_invite(
        app_handle: &AppHandle,
        account: Account,
        outer_event: Event,
        rumor_event: UnsignedEvent,
    ) -> Result<()> {
        let wn = app_handle.state::<Whitenoise>();

        // Check to see if the invite has already been processed
        let processed_invite =
            ProcessedInvite::find_by_invite_event_id(outer_event.id, wn.clone()).await?;
        if processed_invite.is_some() {
            return Ok(());
        }

        let welcome_preview;
        {
            let hex_content = hex::decode(&rumor_event.content);
            if hex_content.is_err() {
                let error_string = format!(
                    "Error hex decoding welcome event: {:?}",
                    hex_content.err().unwrap()
                );
                let processed_invite = ProcessedInvite::create_with_state_and_reason(
                    outer_event.id,
                    rumor_event.id.unwrap(),
                    ProcessedInviteState::Failed,
                    error_string.clone(),
                    wn.clone(),
                )
                .await?;
                tracing::error!(target: "whitenoise::nostr_manager::event_processor", "{}", error_string);
                app_handle
                    .emit("invite_failed_to_process", processed_invite)
                    .map_err(NostrManagerError::TauriError)?;
                return Ok(());
            }

            {
                let nostr_mls = wn.nostr_mls.lock().await;
                welcome_preview = nostr_mls.preview_welcome_event(hex_content.unwrap());
            }

            if welcome_preview.is_err() {
                let error_string = format!(
                    "Error decrypting welcome event: {:?}",
                    welcome_preview.err().unwrap()
                );
                let processed_invite = ProcessedInvite::create_with_state_and_reason(
                    outer_event.id,
                    rumor_event.id.unwrap(),
                    ProcessedInviteState::Failed,
                    error_string.clone(),
                    wn.clone(),
                )
                .await?;
                tracing::error!(target: "whitenoise::nostr_manager::event_processor", "{}", error_string);
                app_handle
                    .emit("invite_failed_to_process", processed_invite)
                    .map_err(NostrManagerError::TauriError)?;
                return Ok(());
            }
        }

        let unwrapped_welcome_preview = welcome_preview.unwrap();

        // Create and save invite
        let invite = Invite {
            event_id: rumor_event.id.unwrap().to_string(),
            account_pubkey: account.pubkey.to_hex(),
            event: rumor_event.clone(),
            mls_group_id: unwrapped_welcome_preview
                .staged_welcome
                .group_context()
                .group_id()
                .to_vec(),
            nostr_group_id: unwrapped_welcome_preview.nostr_group_data.nostr_group_id(),
            group_name: unwrapped_welcome_preview.nostr_group_data.name(),
            group_description: unwrapped_welcome_preview.nostr_group_data.description(),
            group_admin_pubkeys: unwrapped_welcome_preview.nostr_group_data.admin_pubkeys(),
            group_relays: unwrapped_welcome_preview.nostr_group_data.relays(),
            inviter: rumor_event.pubkey.to_hex(),
            member_count: unwrapped_welcome_preview.staged_welcome.members().count() as u32,
            state: InviteState::Pending,
            outer_event_id: outer_event.id.to_string(),
        };

        invite.save(wn.clone()).await?;

        ProcessedInvite::create_with_state_and_reason(
            outer_event.id,
            rumor_event.id.unwrap(),
            ProcessedInviteState::Processed,
            "".to_string(),
            wn.clone(),
        )
        .await?;

        let key_package_event_id = rumor_event
            .tags
            .iter()
            .find(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .and_then(|tag| tag.content());

        app_handle
            .emit("invite_processed", invite)
            .map_err(NostrManagerError::TauriError)?;

        let key_package_relays: Vec<String> = if cfg!(dev) {
            vec![
                "ws://localhost:8080".to_string(),
                "ws://localhost:7777".to_string(),
            ]
        } else {
            account.relays(RelayType::KeyPackage, wn.clone()).await?
        };

        if let Some(key_package_event_id) = key_package_event_id {
            key_packages::delete_key_package_from_relays(
                &EventId::parse(key_package_event_id).unwrap(),
                &key_package_relays,
                false, // For now we don't want to delete the key packages from MLS storage
                wn.clone(),
            )
            .await?;
            tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "Deleted used key package from relays");

            key_packages::publish_key_package(wn.clone()).await?;
            tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "Published new key package");
        }

        Ok(())
    }

    // TODO: Implement private direct message processing, maybe...
    #[allow(dead_code)]
    async fn process_private_direct_message(
        _app_handle: &AppHandle,
        _outer_event: Event,
        inner_event: UnsignedEvent,
    ) -> Result<()> {
        // TODO: Implement private direct message processing
        tracing::debug!(
            target: "whitenoise::event_processor",
            "Received private direct message: {:?}",
            inner_event
        );
        Ok(())
    }

    async fn process_mls_message(app_handle: &AppHandle, event: Event) -> Result<()> {
        let wn = app_handle.state::<Whitenoise>();

        // Check to see if the event has already been processed
        let processed_event = ProcessedMessage::find_by_event_id(event.id, wn.clone()).await?;
        if processed_event.is_some() {
            return Ok(());
        }

        let group_id = event
            .tags
            .iter()
            .find(|tag| tag.kind() == TagKind::h())
            .and_then(|tag| tag.content())
            .unwrap();

        let group = Group::get_by_nostr_group_id(group_id, wn.clone()).await?;

        // TODO: Need to figure out how to reprocess events that fail because a commit arrives out of order

        let nostr_keys = match secrets_store::get_export_secret_keys_for_group(
            group.mls_group_id.clone(),
            group.epoch,
            wn.data_dir.as_path(),
        ) {
            Ok(keys) => keys,
            Err(_) => {
                tracing::debug!(
                    target: "whitenoise::commands::groups::fetch_mls_messages",
                    "No export secret keys found, fetching from nostr_openmls",
                );
                // We need to get the export secret for the group from nostr_openmls
                let nostr_mls = wn.nostr_mls.lock().await;
                let (export_secret_hex, epoch) = nostr_mls
                    .export_secret_as_hex_secret_key_and_epoch(group.mls_group_id.clone())?;

                // Store the export secret key in the secrets store
                secrets_store::store_mls_export_secret(
                    group.mls_group_id.clone(),
                    epoch,
                    export_secret_hex.clone(),
                    wn.data_dir.as_path(),
                )?;

                Keys::parse(&export_secret_hex)?
            }
        };

        // Decrypt events using export secret key
        let decrypted_content = nip44::decrypt_to_bytes(
            nostr_keys.secret_key(),
            &nostr_keys.public_key(),
            &event.content,
        )?;

        let message_vec;
        {
            let nostr_mls = wn.nostr_mls.lock().await;

            // TODO: This only handles application messages for now. We need to handle commits and proposals
            match nostr_mls
                .process_message_for_group(group.mls_group_id.clone(), decrypted_content.clone())
            {
                Ok(message) => message_vec = message,
                Err(e) => {
                    match e {
                        NostrOpenmlsGroupError::ProcessMessageError(e) => {
                            if !e.to_string().contains("Cannot decrypt own messages") {
                                tracing::error!(
                                    target: "whitenoise::commands::groups::fetch_mls_messages",
                                    "Error processing message for group: {}",
                                    e
                                );
                                ProcessedMessage::create_with_state_and_reason(
                                    event.id,
                                    None,
                                    ProcessedMessageState::Failed,
                                    "Cannot decrypt own messages".to_string(),
                                    wn.clone(),
                                )
                                .await?;
                            }
                        }
                        _ => {
                            let error_string =
                                format!("UNRECOGNIZED ERROR processing message for group: {}", e);
                            tracing::error!(
                                target: "whitenoise::commands::groups::fetch_mls_messages",
                                "{}",
                                error_string
                            );
                            ProcessedMessage::create_with_state_and_reason(
                                event.id,
                                None,
                                ProcessedMessageState::Failed,
                                error_string,
                                wn.clone(),
                            )
                            .await?;
                        }
                    }
                    // TODO: Need to figure out how to reprocess events that fail because a commit arrives out of order
                    return Ok(());
                }
            }
        }

        // This processes an application message into JSON.
        let json_event;
        match serde_json::from_slice::<serde_json::Value>(&message_vec) {
            Ok(json_value) => {
                tracing::debug!(
                    target: "whitenoise::commands::groups::fetch_mls_messages",
                    "Deserialized JSON message: {}",
                    json_value
                );
                let json_str = json_value.to_string();
                json_event = UnsignedEvent::from_json(&json_str).unwrap();

                if !group
                    .members(wn.clone())
                    .await?
                    .contains(&json_event.pubkey)
                {
                    tracing::error!(
                        target: "whitenoise::commands::groups::fetch_mls_messages",
                        "Message from non-member: {:?}",
                        json_event.pubkey
                    );
                    ProcessedMessage::create_with_state_and_reason(
                        event.id,
                        Some(json_event.id.unwrap()),
                        ProcessedMessageState::Failed,
                        "Message from non-member".to_string(),
                        wn.clone(),
                    )
                    .await?;
                    return Ok(());
                }

                let message = group
                    .add_message(
                        event.id.to_string(),
                        json_event.clone(),
                        wn.clone(),
                        app_handle.clone(),
                    )
                    .await?;

                app_handle
                    .emit("mls_message_processed", (group.clone(), message.clone()))
                    .expect("Couldn't emit event");
            }
            Err(e) => {
                tracing::error!(
                    target: "whitenoise::commands::groups::fetch_mls_messages",
                    "Failed to deserialize message into JSON: {}",
                    e
                );
                let error_string = format!("Failed to deserialize message into JSON: {}", e);
                ProcessedMessage::create_with_state_and_reason(
                    event.id,
                    None,
                    ProcessedMessageState::Failed,
                    error_string.clone(),
                    wn.clone(),
                )
                .await?;
                return Ok(());
            }
        }

        app_handle
            .emit(
                "mls_message_received",
                MlsMessageReceivedEvent {
                    group_id: group.mls_group_id.clone(),
                    event: json_event.clone(),
                },
            )
            .map_err(NostrManagerError::TauriError)?;
        Ok(())
    }

    // async fn schedule_retry(app_handle: &AppHandle, event: Event, retry_count: u32) -> Result<()> {
    //     // Give up after 5 retries
    //     if retry_count >= 5 {
    //         tracing::error!(
    //             target: "whitenoise::nostr_manager::event_processor",
    //             "Failed to process commit after 5 retries: {:?}",
    //             event
    //         );
    //         return Ok(());
    //     }

    //     let delay = 2u64.pow(retry_count) * 1000; // Exponential backoff in milliseconds
    //     let event_processor = app_handle.state::<EventProcessor>();

    //     tokio::spawn(async move {
    //         tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
    //         if let Err(e) = event_processor
    //             .queue_event(ProcessableEvent::RetryMlsMessage(event, retry_count + 1))
    //             .await
    //         {
    //             tracing::error!(
    //                 target: "whitenoise::nostr_manager::event_processor",
    //                 "Failed to schedule retry: {}",
    //                 e
    //             );
    //         }
    //     });

    //     Ok(())
    // }
}
