use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

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

/// Events that can be processed by the Whitenoise event processing system
#[derive(Debug)]
pub enum ProcessableEvent {
    /// A Nostr event with an optional subscription ID for account-aware processing
    NostrEvent(Event, Option<String>), // Event and optional subscription_id
    /// A relay message for logging/monitoring purposes
    RelayMessage(RelayUrl, String),
}
