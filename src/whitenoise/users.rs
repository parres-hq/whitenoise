use crate::whitenoise::error::Result;
use crate::whitenoise::relays::{Relay, RelayType};
use crate::whitenoise::Whitenoise;
use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
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

impl Whitenoise {
    /// Retrieves a user by their public key.
    ///
    /// This method looks up a user in the database using their Nostr public key.
    /// The user may have been discovered through various means such as:
    /// - Following lists from accounts
    /// - Message interactions
    /// - Direct user lookups
    /// - Metadata events
    ///
    /// # Arguments
    ///
    /// * `pubkey` - The Nostr public key of the user to retrieve
    ///
    /// # Returns
    ///
    /// Returns a `Result<User>` containing:
    /// - `Ok(User)` - The user with the specified public key, including their metadata
    /// - `Err(WhitenoiseError)` - If the user is not found or there's a database error
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use nostr_sdk::PublicKey;
    /// use whitenoise::Whitenoise;
    ///
    /// # async fn example(whitenoise: &Whitenoise) -> Result<(), Box<dyn std::error::Error>> {
    /// let pubkey = PublicKey::parse("npub1...")?;
    /// let user = whitenoise.user(&pubkey).await?;
    /// println!("Found user: {:?}", user.metadata.name);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The user with the specified public key doesn't exist in the database
    /// - There's a database connection or query error
    /// - The public key format is invalid (though this is typically caught at the type level)
    pub async fn user(&self, pubkey: &PublicKey) -> Result<User> {
        User::find_by_pubkey(pubkey, self).await
    }

    /// Retrieves the relay list for a specific user and relay type.
    ///
    /// This method fetches the configured relays for a user based on the specified relay type.
    /// Different relay types serve different purposes in the Nostr ecosystem:
    /// - **Nostr relays** (NIP-65): General purpose relays for reading/writing events
    /// - **Inbox relays** (NIP-65): Specialized relays for receiving private messages
    /// - **Key package relays**: Relays that store MLS key packages for encrypted group messaging
    ///
    /// The relay lists are typically published by users as relay list events (kinds 10002, 10050, 10051)
    /// and cached locally in the database for efficient access.
    ///
    /// # Arguments
    ///
    /// * `user` - The user whose relay list to retrieve
    /// * `relay_type` - The type of relays to fetch (Nostr, Inbox, or KeyPackage)
    ///
    /// # Returns
    ///
    /// Returns a `Result<Vec<Relay>>` containing:
    /// - `Ok(Vec<Relay>)` - A vector of relays configured for the user and relay type
    /// - `Err(WhitenoiseError)` - If there's a database error or user lookup fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use nostr_sdk::PublicKey;
    /// use whitenoise::{RelayType, Whitenoise};
    ///
    /// # async fn example(whitenoise: &Whitenoise) -> Result<(), Box<dyn std::error::Error>> {
    /// let pubkey = PublicKey::parse("npub1...")?;
    /// let user = whitenoise.user(&pubkey).await?;
    ///
    /// // Get user's inbox relays for private messaging
    /// let inbox_relays = whitenoise.user_relays(&user, RelayType::Inbox).await?;
    /// println!("User has {} inbox relays", inbox_relays.len());
    ///
    /// // Get user's key package relays for MLS group messaging
    /// let kp_relays = whitenoise.user_relays(&user, RelayType::KeyPackage).await?;
    /// for relay in kp_relays {
    ///     println!("Key package relay: {}", relay.url);
    /// }
    ///
    /// // Get user's general Nostr relays
    /// let nostr_relays = whitenoise.user_relays(&user, RelayType::Nostr).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Relay Types
    ///
    /// - `RelayType::Nostr` - General purpose relays from NIP-65 relay lists (kind 10002)
    /// - `RelayType::Inbox` - Inbox relays for private messages (kind 10050)
    /// - `RelayType::KeyPackage` - Relays storing MLS key packages (kind 10051)
    ///
    /// # Notes
    ///
    /// - If a user hasn't published relay lists, the returned vector may be empty
    /// - Relay lists are cached locally and updated when new relay list events are received
    /// - The method returns database records that include additional metadata like read/write permissions
    /// - For users that haven't configured specific relay types, consider falling back to default relays
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - There's a database connection or query error
    /// - The user object contains invalid data (shouldn't happen with valid User instances)
    pub async fn user_relays(&self, user: &User, relay_type: RelayType) -> Result<Vec<Relay>> {
        user.relays(relay_type, self).await
    }

    pub async fn user_metadata(&self, pubkey: &PublicKey) -> Result<Metadata> {
        let user = self.user(pubkey).await?;
        Ok(user.metadata.clone())
    }
}
