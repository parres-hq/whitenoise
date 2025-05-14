use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// A row in the relays table
#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct RelayRow {
    pub url: String,
    pub relay_type: String,
    pub account_pubkey: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Relay {
    pub url: String,
    pub relay_type: RelayType,
    pub account_pubkey: PublicKey,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum RelayType {
    Nostr,
    Inbox,
    KeyPackage,
}

impl From<String> for RelayType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "nostr" => Self::Nostr,
            "inbox" => Self::Inbox,
            "key_package" => Self::KeyPackage,
            _ => panic!("Invalid relay type: {}", s),
        }
    }
}

impl From<RelayType> for String {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nostr => "nostr".to_string(),
            RelayType::Inbox => "inbox".to_string(),
            RelayType::KeyPackage => "key_package".to_string(),
        }
    }
}
