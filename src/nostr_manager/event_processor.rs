//! Event processor for the Nostr manager
//!
//! This module is responsible for processing events from the Nostr manager

use nostr_mls::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::accounts::{Account, AccountError};
use crate::nostr_manager::NostrManagerError;
use crate::secrets_store;

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
    NostrEvent(Event, Option<String>), // Event and optional subscription_id
    RelayMessage(RelayUrl, String),
}

impl ProcessableEvent {
    /// Extract the account pubkey from a subscription_id
    /// Subscription IDs follow the format: {pubkey}_{subscription_type}
    fn extract_pubkey_from_subscription_id(subscription_id: &str) -> Option<PublicKey> {
        if let Some(underscore_pos) = subscription_id.find('_') {
            let pubkey_str = &subscription_id[..underscore_pos];
            PublicKey::parse(pubkey_str).ok()
        } else {
            None
        }
    }
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
    pub fn new() -> Self {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Creating new event processor"
        );
        let (sender, receiver) = mpsc::channel(500);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        // Spawn the processing loop
        tokio::spawn(async move {
            tracing::debug!(
                target: "whitenoise::nostr_manager::event_processor",
                "Starting event processor loop"
            );
            Self::process_events(receiver, shutdown_rx).await;
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



    pub async fn queue_message(&self, relay_url: RelayUrl, message: RelayMessage<'_>) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Queuing message from {}: {:?}",
            relay_url,
            message
        );

        match message {
            RelayMessage::Event { subscription_id, event } => {
                // Extract events from messages and include subscription_id for account-aware processing
                tracing::debug!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "Queuing event from subscription: {}",
                    subscription_id
                );
                match self.sender.send(ProcessableEvent::NostrEvent(event.as_ref().clone(), Some(subscription_id.to_string()))).await {
                    Ok(_) => {
                        tracing::debug!(
                            target: "whitenoise::nostr_manager::event_processor",
                            "Event queued successfully with subscription_id"
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
            _ => {
                // Handle other relay messages as before
                let message_str = match message {
                    RelayMessage::Ok { .. } => "Ok".to_string(),
                    RelayMessage::Notice { .. } => "Notice".to_string(),
                    RelayMessage::Closed { .. } => "Closed".to_string(),
                    RelayMessage::EndOfStoredEvents(_) => "EndOfStoredEvents".to_string(),
                    RelayMessage::Auth { .. } => "Auth".to_string(),
                    RelayMessage::Count { .. } => "Count".to_string(),
                    RelayMessage::NegMsg { .. } => "NegMsg".to_string(),
                    RelayMessage::NegErr { .. } => "NegErr".to_string(),
                    _ => "Unknown".to_string(),
                };

                match self.sender.send(ProcessableEvent::RelayMessage(relay_url, message_str)).await {
                    Ok(_) => {
                        tracing::debug!(
                            target: "whitenoise::nostr_manager::event_processor",
                            "Message queued successfully"
                        );
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!(
                            target: "whitenoise::nostr_manager::event_processor",
                            "Failed to queue message: {}",
                            e
                        );
                        Err(e.into())
                    }
                }
            }
        }
    }

    async fn process_events(mut receiver: Receiver<ProcessableEvent>, mut shutdown: Receiver<()>) {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Entering process_events loop"
        );

        let mut shutting_down = false;

        loop {
            tokio::select! {
                Some(event) = receiver.recv() => {
                    tracing::debug!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Received event in processing loop"
                    );
                    match event {
                        ProcessableEvent::NostrEvent(event, subscription_id) => {
                            // Filter and route nostr events based on kind
                            match event.kind {
                                Kind::GiftWrap => {
                                    if let Err(e) = Self::process_giftwrap(event, subscription_id).await {
                                        tracing::error!(
                                            target: "whitenoise::nostr_manager::event_processor",
                                            "Error processing giftwrap: {}",
                                            e
                                        );
                                    }
                                }
                                Kind::MlsGroupMessage => {
                                    if let Err(e) = Self::process_mls_message(event, subscription_id).await {
                                        tracing::error!(
                                            target: "whitenoise::nostr_manager::event_processor",
                                            "Error processing MLS message: {}",
                                            e
                                        );
                                    }
                                }
                                _ => {
                                    // For now, just log other event types
                                    tracing::debug!(
                                        target: "whitenoise::nostr_manager::event_processor",
                                        "Received unhandled event of kind: {:?}",
                                        event.kind
                                    );
                                }
                            }
                        }

                        ProcessableEvent::RelayMessage(relay_url, message) => {
                            Self::process_relay_message(relay_url, message);
                        }
                    }
                }
                Some(_) = shutdown.recv(), if !shutting_down => {
                    tracing::info!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Received shutdown signal, finishing current queue..."
                    );
                    shutting_down = true;
                    // Continue processing remaining events in queue, but don't wait for new shutdown signals
                }
                else => {
                    if shutting_down {
                        tracing::debug!(
                            target: "whitenoise::nostr_manager::event_processor",
                            "Queue flushed, shutting down event processor"
                        );
                    } else {
                        tracing::debug!(
                            target: "whitenoise::nostr_manager::event_processor",
                            "All channels closed, exiting process_events loop"
                        );
                    }
                    break;
                }
            }
        }
    }

    /// Initiates a graceful shutdown that finishes processing the current queue.
    /// Returns immediately - doesn't wait for shutdown to complete.
    pub async fn shutdown(&self) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Initiating graceful shutdown"
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
                Ok(()) // Still return Ok since this is expected if processor already shut down
            }
        }
    }

    fn process_relay_message(relay_url: RelayUrl, message_type: String) {
        tracing::debug!(
            target: "whitenoise::nostr_client::event_processor::process_relay_message",
            "Processing message from {}: {}",
            relay_url,
            message_type
        );
    }

            async fn process_giftwrap(event: Event, subscription_id: Option<String>) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Processing giftwrap: {:?}",
            event
        );

        // For giftwrap events, the target account (who the giftwrap is encrypted for)
        // is specified in a 'p' tag, not in the event.pubkey field
        let target_pubkey = event
            .tags
            .iter()
            .find(|tag| tag.kind() == TagKind::p())
            .and_then(|tag| tag.content())
            .and_then(|pubkey_str| PublicKey::parse(pubkey_str).ok());

        let target_pubkey = match target_pubkey {
            Some(pk) => pk,
            None => {
                tracing::warn!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "No target pubkey found in 'p' tag for giftwrap event"
                );
                return Ok(());
            }
        };

        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Processing giftwrap for target account: {} (author: {})",
            target_pubkey.to_hex(),
            event.pubkey.to_hex()
        );

        // Validate that this matches the subscription_id if available
        if let Some(sub_id) = subscription_id {
            if let Some(sub_pubkey) = ProcessableEvent::extract_pubkey_from_subscription_id(&sub_id) {
                if target_pubkey != sub_pubkey {
                    tracing::warn!(
                        target: "whitenoise::nostr_manager::event_processor",
                        "Giftwrap target pubkey {} does not match subscription pubkey {} - possible routing error",
                        target_pubkey.to_hex(),
                        sub_pubkey.to_hex()
                    );
                    return Ok(());
                }
            }
            tracing::debug!(
                target: "whitenoise::nostr_manager::event_processor",
                "Processing giftwrap from subscription: {} for account: {}",
                sub_id,
                target_pubkey.to_hex()
            );
        } else {
            tracing::warn!(
                target: "whitenoise::nostr_manager::event_processor",
                "No subscription_id provided for giftwrap event - this should not happen with Message-based processing"
            );
        }

        // TODO: Re-enable once we have access to the Whitenoise instance for account loading
        // For now, we have the account pubkey but need a way to load the account and process the giftwrap
        // This requires refactoring to pass the Whitenoise instance or AccountManager to the event processor

        // let account = whitenoise.find_account_by_pubkey(&target_pubkey).await?;
        // let keys = whitenoise.get_nostr_keys_for_pubkey(&target_pubkey)?;
        // if let Ok(unwrapped) = extract_rumor(&keys, &event).await {
        //     match unwrapped.rumor.kind {
        //         Kind::MlsWelcome => {
        //             Self::process_welcome(account, event, unwrapped.rumor).await?;
        //         }
        //         Kind::PrivateDirectMessage => {
        //             tracing::debug!(
        //                 target: "whitenoise::nostr_manager::event_processor",
        //                 "Received private direct message: {:?}",
        //                 unwrapped.rumor
        //             );
        //         }
        //         _ => {
        //             tracing::debug!(
        //                 target: "whitenoise::nostr_manager::event_processor",
        //                 "Received unhandled giftwrap of kind {:?}",
        //                 unwrapped.rumor.kind
        //             );
        //         }
        //     }
        // }
        Ok(())
    }

    async fn process_welcome(
        _account: Account,
        _outer_event: Event,
        rumor_event: UnsignedEvent,
    ) -> Result<()> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Processing welcome: {:?}",
            rumor_event
        );
        // TODO: Update for multi-account support
        // let welcome: welcome_types::Welcome;
        // tracing::debug!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Attempting to acquire nostr_mls lock");
        // {
        //     let nostr_mls_guard = match tokio::time::timeout(
        //         std::time::Duration::from_secs(5),
        //         wn.nostr_mls.lock(),
        //     )
        //     .await
        //     {
        //         Ok(guard) => {
        //             tracing::debug!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "nostr_mls lock acquired");
        //             guard
        //         }
        //         Err(_) => {
        //             tracing::error!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Timeout waiting for nostr_mls lock");
        //             return Err(EventProcessorError::NostrMlsError(
        //                 nostr_mls::Error::KeyPackage(
        //                     "Timeout waiting for nostr_mls lock".to_string(),
        //                 ),
        //             ));
        //         }
        //     };
        //     let result = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        //         match nostr_mls.process_welcome(&outer_event.id, &rumor_event) {
        //             Ok(result) => {
        //                 tracing::debug!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Processed welcome event: {:?}", result);
        //                 Ok(result)
        //             }
        //             Err(e) => {
        //                 tracing::error!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Error processing welcome event: {}", e);
        //                 Err(EventProcessorError::NostrMlsError(e))
        //             }
        //         }
        //     } else {
        //         tracing::error!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Nostr MLS not initialized");
        //         Err(EventProcessorError::NostrMlsNotInitialized)
        //     };
        //     welcome = result?;
        // }
        // tracing::debug!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "nostr_mls lock released");

        // let key_package_event_id = rumor_event
        //     .tags
        //     .iter()
        //     .find(|tag| {
        //         tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
        //     })
        //     .and_then(|tag| tag.content());

        // let key_package_relays: Vec<String> = if cfg!(debug_assertions) {
        //     vec![
        //         "ws://localhost:8080".to_string(),
        //         "ws://localhost:7777".to_string(),
        //     ]
        // } else {
        //     account.relays(RelayType::KeyPackage).await?
        // };

        // if let Some(key_package_event_id) = key_package_event_id {
        //     key_packages::delete_key_package_from_relays(
        //         &EventId::parse(key_package_event_id).unwrap(),
        //         &key_package_relays,
        //         false, // For now we don't want to delete the key packages from MLS storage
        //     )
        //     .await?;
        //     tracing::debug!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Deleted used key package from relays");

        //     key_packages::publish_key_package().await?;
        //     tracing::debug!(target: "whitenoise::nostr_manager::event_processor::process_welcome", "Published new key package");
        // }

        // Ok(welcome)
        Ok(())
    }

    async fn process_mls_message(event: Event, subscription_id: Option<String>) -> Result<Option<message_types::Message>> {
        tracing::debug!(
            target: "whitenoise::nostr_manager::event_processor",
            "Processing MLS message: {:?}",
            event
        );

        // For MLS messages, we can extract the account pubkey from the subscription_id if available
        // or use other methods to determine the target account
        if let Some(sub_id) = subscription_id {
            if let Some(target_pubkey) = ProcessableEvent::extract_pubkey_from_subscription_id(&sub_id) {
                tracing::debug!(
                    target: "whitenoise::nostr_manager::event_processor",
                    "Processing MLS message for account: {}",
                    target_pubkey.to_hex()
                );
            }
        }

        // TODO: Update for multi-account support - need to determine which account this message is for
        // tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "Attempting to acquire nostr_mls lock");
        // let nostr_mls_guard = match tokio::time::timeout(
        //     std::time::Duration::from_secs(5),
        //     wn.nostr_mls.lock(),
        // )
        // .await
        // {
        //     Ok(guard) => {
        //         tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "nostr_mls lock acquired");
        //         guard
        //     }
        //     Err(_) => {
        //         tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Timeout waiting for nostr_mls lock");
        //         return Err(EventProcessorError::NostrMlsError(
        //             nostr_mls::Error::KeyPackage("Timeout waiting for nostr_mls lock".to_string()),
        //         ));
        //     }
        // };
        // let result = if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
        //     match nostr_mls.process_message(&event) {
        //         Ok(message) => {
        //             // TODO: Need to handle proposals and commits
        //             tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "Processed MLS message");
        //             Ok(message)
        //         }
        //         Err(e) => {
        //             // TODO: Need to figure out how to reprocess events that fail because a commit arrives out of order
        //             tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Error processing MLS message: {}", e);
        //             Err(EventProcessorError::NostrMlsError(e))
        //         }
        //     }
        // } else {
        //     tracing::error!(target: "whitenoise::nostr_manager::event_processor", "Nostr MLS not initialized");
        //     Err(EventProcessorError::NostrMlsNotInitialized)
        // };
        // tracing::debug!(target: "whitenoise::nostr_manager::event_processor", "nostr_mls lock released");
        // result
        Ok(None)
    }
}
