use crate::accounts::{Account, AccountError};
use crate::database::DatabaseError;
use crate::messages::{Message, MessageRow};
use crate::secrets_store;
use crate::utils::is_valid_hex_pubkey;
use crate::Whitenoise;
use nostr_openmls::groups::GroupError as NostrMlsError;
use nostr_openmls::nostr_group_data_extension::NostrGroupDataExtension;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use tauri_plugin_notification::NotificationExt;
use thiserror::Error;

/// This is an intermediate struct representing a group in the database
#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct GroupRow {
    pub mls_group_id: Vec<u8>,
    pub account_pubkey: String,
    pub nostr_group_id: String,
    pub name: String,
    pub description: String,
    pub admin_pubkeys: String, // JSON string
    pub last_message_id: Option<String>,
    pub last_message_at: Option<u64>,
    pub group_type: String,
    pub epoch: u64,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroupWithRelays {
    pub group: Group,
    pub relays: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Group {
    /// This is the MLS group ID, this will serve as the PK in the DB and doesn't change
    pub mls_group_id: Vec<u8>,
    /// The account that owns this group
    pub account_pubkey: PublicKey,
    /// Hex encoded (same value as the NostrGroupDataExtension) this is the group_id used in Nostr events
    pub nostr_group_id: String,
    /// UTF-8 encoded (same value as the NostrGroupDataExtension)
    pub name: String,
    /// UTF-8 encoded (same value as the NostrGroupDataExtension)
    pub description: String,
    /// Hex encoded (same value as the NostrGroupDataExtension)
    pub admin_pubkeys: Vec<String>,
    /// Hex encoded Nostr event ID of the last message in the group
    pub last_message_id: Option<String>,
    /// Timestamp of the last message in the group
    pub last_message_at: Option<Timestamp>,
    /// Type of Nostr MLS group
    pub group_type: GroupType,
    /// Epoch of the group
    pub epoch: u64,
    /// The state of the group
    pub state: GroupState,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum GroupType {
    /// A group with only two members
    DirectMessage,
    /// A group with more than two members
    Group,
}

impl From<String> for GroupType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "DirectMessage" => Self::DirectMessage,
            "Group" => Self::Group,
            _ => panic!("Invalid group type: {}", s),
        }
    }
}

impl From<GroupType> for String {
    fn from(group_type: GroupType) -> Self {
        match group_type {
            GroupType::DirectMessage => "DirectMessage".to_string(),
            GroupType::Group => "Group".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum GroupState {
    Active,
    Inactive,
}

impl From<String> for GroupState {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Active" => Self::Active,
            "Inactive" => Self::Inactive,
            _ => panic!("Invalid group state: {}", s),
        }
    }
}

impl From<GroupState> for String {
    fn from(state: GroupState) -> Self {
        match state {
            GroupState::Active => "Active".to_string(),
            GroupState::Inactive => "Inactive".to_string(),
        }
    }
}

#[derive(Error, Debug)]
pub enum GroupError {
    #[error("Group not found")]
    GroupNotFound,

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Account error: {0}")]
    AccountError(#[from] AccountError),

    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),

    #[error("MLS error: {0}")]
    MlsError(#[from] NostrMlsError),

    #[error("Key error: {0}")]
    KeyError(#[from] nostr_sdk::key::Error),

    #[error("Nostr event error: {0}")]
    NostrEventError(#[from] nostr_sdk::event::builder::Error),

    #[error("NIP-44 encryption error: {0}")]
    NostrEncryptionError(#[from] nostr_sdk::nips::nip44::Error),

    #[error("Nostr error: {0}")]
    NostrError(#[from] nostr_sdk::client::Error),

    #[error("Secrets store error: {0}")]
    SecretsStoreError(#[from] secrets_store::SecretsStoreError),

    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Event ID error: {0}")]
    EventIdError(#[from] nostr_sdk::event::id::Error),

    #[error("Notification error: {0}")]
    NotificationError(#[from] tauri_plugin_notification::Error),
}

pub type Result<T> = std::result::Result<T, GroupError>;

impl Group {
    /// Validates the members and admins of a group during creation
    ///
    /// # Arguments
    /// * `creator_pubkey` - The public key of the group creator
    /// * `member_pubkeys` - List of public keys for group members
    /// * `admin_pubkeys` - List of public keys for group admins
    ///
    /// # Returns
    /// * `Ok(true)` if validation passes
    /// * `Err(GroupManagerError)` if validation fails
    ///
    /// # Validation Rules
    /// - Creator must be an admin but not included in member list
    /// - Creator must have a valid public key
    /// - All member public keys must be valid
    /// - All admin public keys must be valid
    /// - All admins must also be members (except creator)
    ///
    /// # Errors
    /// Returns `GroupManagerError::GroupCreationError` with descriptive message if:
    /// - Creator is not an admin
    /// - Creator is in member list
    /// - Creator has invalid public key
    /// - Any member has invalid public key
    /// - Any admin has invalid public key
    /// - Any admin is not a member
    pub fn validate_group_members(
        creator_pubkey: &String,
        member_pubkeys: &[String],
        admin_pubkeys: &[String],
    ) -> Result<bool> {
        // Creator must be an admin
        if !admin_pubkeys.contains(creator_pubkey) {
            return Err(GroupError::InvalidParameters(
                "Creator must be an admin".to_string(),
            ));
        }

        // Creator must not be included as a member
        if member_pubkeys.contains(creator_pubkey) {
            return Err(GroupError::InvalidParameters(
                "Creator must not be included as a member".to_string(),
            ));
        }

        // Creator must be valid pubkey
        if !is_valid_hex_pubkey(creator_pubkey) {
            return Err(GroupError::InvalidParameters(format!(
                "Invalid creator pubkey: {}",
                creator_pubkey
            )));
        }

        // Check that members are valid pubkeys
        for pubkey in member_pubkeys.iter() {
            if !is_valid_hex_pubkey(pubkey) {
                return Err(GroupError::InvalidParameters(format!(
                    "Invalid member pubkey: {}",
                    pubkey
                )));
            }
        }

        // Check that admins are valid pubkeys and are members
        for pubkey in admin_pubkeys.iter() {
            if !is_valid_hex_pubkey(pubkey) {
                return Err(GroupError::InvalidParameters(format!(
                    "Invalid admin pubkey: {}",
                    pubkey
                )));
            }
            if !member_pubkeys.contains(pubkey) && creator_pubkey != pubkey {
                return Err(GroupError::InvalidParameters(
                    "Admin must be a member".to_string(),
                ));
            }
        }
        Ok(true)
    }

    /// Create and save a new group to the database
    pub async fn new(
        mls_group_id: Vec<u8>,
        mls_group_epoch: u64,
        group_type: GroupType,
        group_data: NostrGroupDataExtension,
        wn: tauri::State<'_, Whitenoise>,
        _app_handle: &tauri::AppHandle,
    ) -> Result<Self> {
        tracing::debug!(
            target: "whitenoise::groups::new",
            "Creating group with mls_group_id: {:?}",
            &mls_group_id
        );

        let account = Account::get_active(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        let group = Self {
            mls_group_id,
            account_pubkey: account.pubkey,
            nostr_group_id: group_data.nostr_group_id(),
            name: group_data.name(),
            description: group_data.description(),
            admin_pubkeys: group_data.admin_pubkeys(),
            last_message_id: None,
            last_message_at: None,
            group_type,
            epoch: mls_group_epoch,
            state: GroupState::Active,
        };

        let mut txn = wn.database.pool.begin().await?;

        // Save the group - not using the save method because we want relay creation in the same transaction
        sqlx::query("INSERT INTO groups (mls_group_id, account_pubkey, nostr_group_id, name, description, admin_pubkeys, last_message_id, last_message_at, group_type, epoch, state) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(group.mls_group_id.clone())
            .bind(account.pubkey.to_hex().as_str())
            .bind(group.nostr_group_id.clone())
            .bind(group.name.clone())
            .bind(group.description.clone())
            .bind(serde_json::to_string(&group.admin_pubkeys)?)
            .bind(group.last_message_id.clone())
            .bind(group.last_message_at.map(|t| t.as_u64() as i64))
            .bind(String::from(group.group_type.clone()))
            .bind(group.epoch as i64)
            .bind(String::from(group.state.clone()))
            .execute(&mut *txn)
            .await?;

        // Add the relays for the group
        for relay in group_data.relays() {
            sqlx::query("INSERT OR REPLACE INTO group_relays (url, relay_type, account_pubkey, group_id) VALUES (?, ?, ?, ?)")
                .bind(relay)
                .bind("group")
                .bind(account.pubkey.to_hex())
                .bind(group.mls_group_id.clone())
                .execute(&mut *txn)
                .await?;
        }

        // Commit the transaction
        txn.commit().await?;

        Ok(group)
    }

    /// Find a group by their mls_group_id and the account it belongs to
    pub async fn find_by_mls_group_id(
        mls_group_id: &Vec<u8>,
        wn: tauri::State<'_, Whitenoise>,
    ) -> Result<Self> {
        let account = Account::get_active(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        let group_row = sqlx::query_as::<_, GroupRow>(
            "SELECT * FROM groups WHERE mls_group_id = ? AND account_pubkey = ?",
        )
        .bind(mls_group_id)
        .bind(account.pubkey.to_hex())
        .fetch_optional(&wn.database.pool)
        .await?
        .ok_or_else(|| GroupError::GroupNotFound)?;

        tracing::debug!(
            target: "whitenoise::groups::find_by_mls_group_id",
            "Found group: {:?}",
            group_row
        );

        Ok(Self {
            mls_group_id: group_row.mls_group_id,
            account_pubkey: account.pubkey,
            nostr_group_id: group_row.nostr_group_id,
            name: group_row.name,
            description: group_row.description,
            admin_pubkeys: serde_json::from_str(&group_row.admin_pubkeys)?,
            last_message_id: group_row.last_message_id,
            last_message_at: group_row.last_message_at.map(Timestamp::from),
            group_type: group_row.group_type.into(),
            epoch: group_row.epoch,
            state: group_row.state.into(),
        })
    }

    pub async fn get_by_nostr_group_id(
        nostr_group_id: &str,
        wn: tauri::State<'_, Whitenoise>,
    ) -> Result<Self> {
        let account = Account::get_active(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        let group_row = sqlx::query_as::<_, GroupRow>(
            "SELECT * FROM groups WHERE nostr_group_id = ? AND account_pubkey = ?",
        )
        .bind(nostr_group_id)
        .bind(account.pubkey.to_hex().as_str())
        .fetch_optional(&wn.database.pool)
        .await?
        .ok_or_else(|| GroupError::GroupNotFound)?;

        Ok(Self {
            mls_group_id: group_row.mls_group_id,
            account_pubkey: account.pubkey,
            nostr_group_id: group_row.nostr_group_id,
            name: group_row.name,
            description: group_row.description,
            admin_pubkeys: serde_json::from_str(&group_row.admin_pubkeys)?,
            last_message_id: group_row.last_message_id,
            last_message_at: group_row.last_message_at.map(Timestamp::from),
            group_type: group_row.group_type.into(),
            epoch: group_row.epoch,
            state: group_row.state.into(),
        })
    }

    /// Gets all groups for a given account
    pub async fn get_all_groups(wn: tauri::State<'_, Whitenoise>) -> Result<Vec<Self>> {
        // Test database connection
        sqlx::query("SELECT 1").execute(&wn.database.pool).await?;

        tracing::debug!(
            target: "whitenoise::groups::get_all_groups",
            "Database connection verified"
        );

        let account = Account::get_active(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        tracing::debug!(
            target: "whitenoise::groups::get_all_groups",
            "Fetching groups for active account: {}",
            account.pubkey.to_hex()
        );

        let group_rows =
            sqlx::query_as::<_, GroupRow>("SELECT * FROM groups WHERE account_pubkey = ?")
                .bind(account.pubkey.to_hex())
                .fetch_all(&wn.database.pool)
                .await?;

        tracing::debug!(
            target: "whitenoise::groups::get_all_groups",
            "Found {} groups: {:?}",
            group_rows.len(),
            group_rows
        );

        group_rows
            .into_iter()
            .map(|row| {
                Ok(Self {
                    mls_group_id: row.mls_group_id,
                    account_pubkey: account.pubkey,
                    nostr_group_id: row.nostr_group_id,
                    name: row.name,
                    description: row.description,
                    admin_pubkeys: serde_json::from_str(&row.admin_pubkeys)?,
                    last_message_id: row.last_message_id,
                    last_message_at: row.last_message_at.map(Timestamp::from),
                    group_type: row.group_type.into(),
                    epoch: row.epoch,
                    state: row.state.into(),
                })
            })
            .collect::<Result<Vec<_>>>()
    }

    // Save the group to the database
    #[allow(dead_code)]
    pub async fn save(&self, wn: tauri::State<'_, Whitenoise>) -> Result<Self> {
        let mut txn = wn.database.pool.begin().await?;

        sqlx::query("INSERT INTO groups (mls_group_id, account_pubkey, nostr_group_id, name, description, admin_pubkeys, last_message_id, last_message_at, group_type, epoch, state) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(self.mls_group_id.clone())
            .bind(self.account_pubkey.to_hex().as_str())
            .bind(self.nostr_group_id.clone())
            .bind(self.name.clone())
            .bind(self.description.clone())
            .bind(serde_json::to_string(&self.admin_pubkeys)?)
            .bind(self.last_message_id.clone())
            .bind(self.last_message_at.map(|t| t.as_u64() as i64))
            .bind(String::from(self.group_type.clone()))
            .bind(self.epoch as i64)
            .bind(String::from(self.state.clone()))
            .execute(&mut *txn)
            .await?;

        txn.commit().await?;
        Ok(self.clone())
    }

    pub async fn add_message(
        &self,
        outer_event_id: String,
        message: UnsignedEvent,
        wn: tauri::State<'_, Whitenoise>,
        app_handle: tauri::AppHandle,
    ) -> Result<Message> {
        let account = Account::get_active(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        let mut txn = wn.database.pool.begin().await?;

        let event_json = serde_json::to_string(&message)?;
        let tags_json = serde_json::to_string(&message.tags)?;

        tracing::debug!(
            target: "whitenoise::groups::add_message",
            "Inserting message into database; event_id: {:?}, account_pubkey: {:?}, author_pubkey: {:?}, mls_group_id: {:?}, created_at: {:?}, content: {:?}, tags: {:?}, event: {:?}, outer_event_id: {:?}",
            message.id.unwrap().to_string(),
            account.pubkey.to_hex(),
            message.pubkey.to_hex(),
            self.mls_group_id,
            message.created_at.to_string(),
            message.content,
            tags_json,
            event_json,
            outer_event_id.to_string(),
        );

        // First insert the message
        let query = sqlx::query(
            r#"
            INSERT INTO messages (
                event_id, account_pubkey, author_pubkey, mls_group_id,
                created_at, content, tags, event, outer_event_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(message.id.unwrap().to_hex()) // Convert EventId to hex string
        .bind(account.pubkey.to_hex())
        .bind(message.pubkey.to_hex())
        .bind(&self.mls_group_id as &[u8]) // Explicitly bind as bytes
        .bind(message.created_at.as_u64() as i64) // Convert timestamp to i64
        .bind(&message.content)
        .bind(&tags_json)
        .bind(&event_json)
        .bind(&outer_event_id)
        .execute(&mut *txn)
        .await?;

        tracing::debug!(
            target: "whitenoise::groups::add_message",
            "Insert message result: {:?}",
            query
        );

        // Update the last message id and last message at
        sqlx::query(
            "UPDATE groups SET last_message_id = ?, last_message_at = ? WHERE mls_group_id = ?",
        )
        .bind(message.id.unwrap().to_string())
        .bind(message.created_at.as_u64() as i64)
        .bind(&self.mls_group_id)
        .execute(&mut *txn)
        .await?;

        // Then fetch the inserted row if needed
        let message_row = sqlx::query_as::<_, MessageRow>(
            "SELECT * FROM messages WHERE event_id = ? AND account_pubkey = ?",
        )
        .bind(message.id.unwrap().to_string())
        .bind(account.pubkey.to_hex())
        .fetch_one(&mut *txn)
        .await?;

        txn.commit().await?;

        // Send notification
        if account.pubkey.to_hex() != message.pubkey.to_hex() {
            let message_author = wn
                .nostr
                .client
                .database()
                .metadata(message.pubkey)
                .await
                .map_err(|e| GroupError::NostrError(nostr_sdk::client::Error::Database(e)))?;

            if let Some(author) = message_author {
                app_handle
                    .notification()
                    .builder()
                    .title(
                        author
                            .display_name
                            .unwrap_or(author.name.unwrap_or("Unknown".to_string())),
                    )
                    .body(message.content.clone())
                    .show()
                    .map_err(GroupError::NotificationError)?;
            }
        }

        Ok(Message {
            event_id: EventId::from_hex(&message_row.event_id)?,
            account_pubkey: account.pubkey,
            author_pubkey: message.pubkey,
            mls_group_id: self.mls_group_id.clone(),
            created_at: message.created_at,
            content: message.content.clone(),
            tags: message.tags.clone(),
            event: message,
            outer_event_id: EventId::from_hex(&message_row.outer_event_id)?,
        })
    }

    pub async fn messages(&self, wn: tauri::State<'_, Whitenoise>) -> Result<Vec<UnsignedEvent>> {
        let pubkey = Account::get_active_pubkey(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        let message_rows = sqlx::query_as::<_, MessageRow>(
            "SELECT * FROM messages WHERE mls_group_id = ? AND account_pubkey = ?",
        )
        .bind(&self.mls_group_id)
        .bind(pubkey.to_hex())
        .fetch_all(&wn.database.pool)
        .await?;

        message_rows
            .into_iter()
            .map(|row| serde_json::from_str(&row.event).map_err(GroupError::SerializationError))
            .collect::<Result<Vec<_>>>()
    }

    pub async fn members(&self, wn: tauri::State<'_, Whitenoise>) -> Result<Vec<PublicKey>> {
        let nostr_mls = wn.nostr_mls.lock().await;
        let member_pubkeys = nostr_mls
            .member_pubkeys(self.mls_group_id.clone())
            .map_err(GroupError::MlsError)?;
        member_pubkeys
            .iter()
            .try_fold(Vec::with_capacity(member_pubkeys.len()), |mut acc, pk| {
                acc.push(PublicKey::parse(pk)?);
                Ok(acc)
            })
    }

    pub fn admins(&self) -> Result<Vec<PublicKey>> {
        self.admin_pubkeys.iter().try_fold(
            Vec::with_capacity(self.admin_pubkeys.len()),
            |mut acc, pk| {
                acc.push(PublicKey::parse(pk)?);
                Ok(acc)
            },
        )
    }

    pub async fn relays(&self, wn: tauri::State<'_, Whitenoise>) -> Result<Vec<String>> {
        let account = Account::get_active(wn.clone())
            .await
            .map_err(GroupError::AccountError)?;

        Ok(sqlx::query_scalar::<_, String>(
            "SELECT url FROM group_relays WHERE group_id = ? AND account_pubkey = ?",
        )
        .bind(&self.mls_group_id)
        .bind(account.pubkey.to_hex())
        .fetch_all(&wn.database.pool)
        .await?)
    }

    pub async fn self_update_keys(&self, wn: tauri::State<'_, Whitenoise>) -> Result<()> {
        let serialized_commit_message: Vec<u8>;
        let current_exporter_secret_hex: String;
        let new_exporter_secret_hex: String;
        let new_epoch: u64;
        {
            let nostr_mls = wn.nostr_mls.lock().await;
            let self_update_result = nostr_mls
                .self_update(self.mls_group_id.clone())
                .map_err(GroupError::MlsError)?;
            serialized_commit_message = self_update_result.serialized_message;
            current_exporter_secret_hex = self_update_result.current_exporter_secret_hex;
            new_exporter_secret_hex = self_update_result.new_exporter_secret_hex;
            new_epoch = self_update_result.new_epoch;
        }

        // Send 445 event with commit_message - needs to be encrypted to the last epoch's exporter secret key

        let last_epoch_export_nostr_keys =
            Keys::parse(current_exporter_secret_hex.as_str()).map_err(GroupError::KeyError)?;

        let encrypted_content = nip44::encrypt(
            last_epoch_export_nostr_keys.secret_key(),
            &last_epoch_export_nostr_keys.public_key(),
            &serialized_commit_message,
            nip44::Version::V2,
        )
        .map_err(GroupError::NostrEncryptionError)?;

        let ephemeral_nostr_keys = Keys::generate();
        let commit_message_event = EventBuilder::new(Kind::MlsGroupMessage, encrypted_content)
            .tags(vec![Tag::custom(
                TagKind::h(),
                vec![self.nostr_group_id.clone()],
            )])
            .sign(&ephemeral_nostr_keys)
            .await
            .map_err(GroupError::NostrEventError)?;

        tracing::debug!(
            target: "whitenoise::groups::self_update_keys",
            "Publishing MLS commit message event to group relays"
        );

        wn.nostr
            .client
            .send_event_to(self.relays(wn.clone()).await?, commit_message_event)
            .await
            .map_err(GroupError::NostrError)?;

        // TODO: This is assuming we don't have any welcome messages in this commit we probably need to handle that case in the future

        // Add the new epoch secret to the secret store
        secrets_store::store_mls_export_secret(
            self.mls_group_id.clone(),
            new_epoch,
            new_exporter_secret_hex.clone(),
            wn.data_dir.as_path(),
        )
        .map_err(GroupError::SecretsStoreError)?;

        Ok(())
    }

    // pub fn remove(&self, wn: &tauri::State<'_, Whitenoise>) -> Result<()> {}
}
