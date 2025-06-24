use nostr_mls::prelude::*;
use serde::{Deserialize, Serialize};

use crate::nostr_manager::parser::SerializableToken;

/// A contact enriched with Nostr metadata and relay information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnrichedContact {
    /// The Nostr metadata of the contact.
    pub metadata: Metadata,
    /// Whether the contact supports NIP-17.
    pub nip17: bool,
    /// Whether the contact supports NIP-104.
    pub nip104: bool,
    /// The relays for the user. NIP-65
    pub nostr_relays: Vec<String>,
    /// The relays for the contact's inbox. NIP-17
    pub inbox_relays: Vec<String>,
    /// The relays for the contact's key package. NIP-104
    pub key_package_relays: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NostrEncryptionMethod {
    Nip04,
    Nip44,
}

/// Retry information for failed event processing
#[derive(Debug, Clone)]
pub struct RetryInfo {
    /// Number of times this event has been retried
    pub attempt: u32,
    /// Maximum number of retry attempts allowed
    pub max_attempts: u32,
    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,
}

impl RetryInfo {
    pub fn new() -> Self {
        Self {
            attempt: 0,
            max_attempts: 10,
            base_delay_ms: 1000,
        }
    }

    pub fn next_attempt(&self) -> Option<Self> {
        if self.attempt >= self.max_attempts {
            None
        } else {
            Some(Self {
                attempt: self.attempt + 1,
                max_attempts: self.max_attempts,
                base_delay_ms: self.base_delay_ms,
            })
        }
    }

    pub fn delay_ms(&self) -> u64 {
        self.base_delay_ms * (2_u64.pow(self.attempt))
    }

    pub fn should_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }
}

impl Default for RetryInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Events that can be processed by the Whitenoise event processing system
#[derive(Debug)]
pub enum ProcessableEvent {
    /// A Nostr event with an optional subscription ID for account-aware processing
    NostrEvent {
        event: Event,
        subscription_id: Option<String>,
        retry_info: RetryInfo,
    },
    /// A relay message for logging/monitoring purposes
    RelayMessage(RelayUrl, String),
}

impl ProcessableEvent {
    /// Create a new NostrEvent with default retry settings
    pub fn new_nostr_event(event: Event, subscription_id: Option<String>) -> Self {
        Self::NostrEvent {
            event,
            subscription_id,
            retry_info: RetryInfo::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageWithTokens {
    pub message: message_types::Message,
    pub tokens: Vec<SerializableToken>,
}

impl MessageWithTokens {
    pub fn new(message: message_types::Message, tokens: Vec<SerializableToken>) -> Self {
        Self { message, tokens }
    }
}
