//! Event processor for the Nostr manager
//!
//! This module is responsible for processing events from the Nostr manager

use nostr_mls::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::accounts::{Account, AccountError};
use crate::key_packages;
use crate::nostr_manager::NostrManagerError;
use crate::relays::RelayType;
use crate::secrets_store;
use crate::Whitenoise;

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
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Key package error: {0}")]
    KeyPackageError(#[from] key_packages::KeyPackageError),
    #[error("NIP44 encryption error: {0}")]
    EncryptionError(#[from] nostr_sdk::nips::nip44::Error),
    #[error("Secrets store error: {0}")]
    SecretsStoreError(#[from] secrets_store::SecretsStoreError),
    #[error("Key parsing error: {0}")]
    UnparseableKey(#[from] nostr_sdk::key::Error),
    #[error("Nostr MLS error: {0}")]
    NostrMlsError(#[from] nostr_mls::Error),
    #[error("Nostr MLS not initialized")]
    NostrMlsNotInitialized,
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
                    Self::process_welcome(app_handle, active_account, event, unwrapped.rumor)
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

    async fn process_welcome(
        app_handle: &AppHandle,
        account: Account,
        outer_event: Event,
        rumor_event: UnsignedEvent,
    ) -> Result<welcome_types::Welcome> {
        let wn = app_handle.state::<Whitenoise>();

        let nostr_mls_guard = wn.nostr_mls.lock().await;
        let welcome: welcome_types::Welcome;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            match nostr_mls.process_welcome(&outer_event.id, &rumor_event) {
                Ok(result) => {
                    tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "Processed welcome event");
                    welcome = result;
                }
                Err(e) => {
                    tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Error processing welcome event: {}", e);
                    return Err(EventProcessorError::NostrMlsError(e));
                }
            }
        } else {
            tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Nostr MLS not initialized");
            return Err(EventProcessorError::NostrMlsNotInitialized);
        }

        let key_package_event_id = rumor_event
            .tags
            .iter()
            .find(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
            })
            .and_then(|tag| tag.content());

        app_handle
            .emit("mls_welcome_received", &welcome)
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

        Ok(welcome)
    }

    // TODO: Implement private direct message processing, maybe...
    #[allow(dead_code)]
    async fn process_private_direct_message(
        _app_handle: &AppHandle,
        _outer_event: Event,
        inner_event: UnsignedEvent,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::event_processor",
            "Received private direct message: {:?}",
            inner_event
        );
        Ok(())
    }

    async fn process_mls_message(
        app_handle: &AppHandle,
        event: Event,
    ) -> Result<Option<message_types::Message>> {
        let wn = app_handle.state::<Whitenoise>();

        let nostr_mls_guard = wn.nostr_mls.lock().await;
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            match nostr_mls.process_message(&event) {
                Ok(message) => {
                    // TODO: Need to handle proposals and commits
                    tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "Processed MLS message");
                    app_handle
                        .emit("mls_message_received", &message)
                        .map_err(NostrManagerError::TauriError)?;
                    Ok(message)
                }
                Err(e) => {
                    // TODO: Need to figure out how to reprocess events that fail because a commit arrives out of order
                    tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Error processing MLS message: {}", e);
                    Err(EventProcessorError::NostrMlsError(e))
                }
            }
        } else {
            tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Nostr MLS not initialized");
            Err(EventProcessorError::NostrMlsNotInitialized)
        }
    }
}
