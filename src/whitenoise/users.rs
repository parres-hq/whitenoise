use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: Option<i64>,
    pub pubkey: PublicKey,
    pub metadata: Metadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(pubkey: PublicKey) -> Self {
        User {
            id: None,
            pubkey,
            metadata: Metadata::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
