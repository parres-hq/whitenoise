use crate::whitenoise::error::Result;
use crate::whitenoise::Whitenoise;
use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Hash)]
pub struct Relay {
    pub id: Option<i64>,
    pub url: RelayUrl,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum RelayType {
    Nip65,
    Inbox,
    KeyPackage,
}

impl From<String> for RelayType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "nip65" => Self::Nip65,
            "inbox" => Self::Inbox,
            "key_package" => Self::KeyPackage,
            _ => panic!("Invalid relay type: {}", s),
        }
    }
}

impl From<RelayType> for String {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nip65 => "nip65".to_string(),
            RelayType::Inbox => "inbox".to_string(),
            RelayType::KeyPackage => "key_package".to_string(),
        }
    }
}

impl From<RelayType> for Kind {
    fn from(relay_type: RelayType) -> Self {
        match relay_type {
            RelayType::Nip65 => Kind::RelayList,
            RelayType::Inbox => Kind::InboxRelays,
            RelayType::KeyPackage => Kind::MlsKeyPackageRelays,
        }
    }
}

impl From<Kind> for RelayType {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::RelayList => RelayType::Nip65,
            Kind::InboxRelays => RelayType::Inbox,
            Kind::MlsKeyPackageRelays => RelayType::KeyPackage,
            _ => RelayType::Nip65, // Default fallback
        }
    }
}

impl Relay {
    pub(crate) fn new(url: &RelayUrl) -> Self {
        Relay {
            id: None,
            url: url.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl Whitenoise {
    pub(crate) async fn find_or_create_relay(&self, url: &RelayUrl) -> Result<Relay> {
        Relay::find_or_create_by_url(url, &self.database).await
    }
}
