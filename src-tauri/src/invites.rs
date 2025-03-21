use crate::accounts::Account;
use crate::database::DatabaseError;
use crate::Whitenoise;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InviteError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Event error: {0}")]
    Event(#[from] nostr_sdk::event::Error),

    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Account error: {0}")]
    Account(#[from] crate::accounts::AccountError),
}

pub type Result<T> = std::result::Result<T, InviteError>;

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct InviteRow {
    pub event_id: String,
    pub account_pubkey: String,
    pub event: String, // JSON string for UnsignedEvent
    pub mls_group_id: Vec<u8>,
    pub nostr_group_id: String,
    pub group_name: String,
    pub group_description: String,
    pub group_admin_pubkeys: String, // JSON array of strings
    pub group_relays: String,        // JSON array of strings
    pub inviter: String,
    pub member_count: u32,
    pub outer_event_id: String,
    pub state: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Invite {
    /// The event id of the invite
    pub event_id: String,
    /// The account that owns this invite
    pub account_pubkey: String,
    /// The event that contains the welcome message
    pub event: UnsignedEvent,
    /// MLS group id
    pub mls_group_id: Vec<u8>,
    /// Nostr group id (from NostrGroupDataExtension)
    pub nostr_group_id: String,
    /// Group name (from NostrGroupDataExtension)
    pub group_name: String,
    /// Group description (from NostrGroupDataExtension)
    pub group_description: String,
    /// Group admin pubkeys (from NostrGroupDataExtension)
    pub group_admin_pubkeys: Vec<String>,
    /// Group relays (from NostrGroupDataExtension)
    pub group_relays: Vec<String>,
    /// Pubkey of the user that sent the invite
    pub inviter: String,
    /// Member count of the group
    pub member_count: u32,
    /// The state of the invite
    pub state: InviteState,
    /// The event id of the 1059 event that contained the invite
    pub outer_event_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, sqlx::FromRow)]
pub struct ProcessedInviteRow {
    pub event_id: String,
    pub invite_event_id: String,
    pub account_pubkey: String,
    pub processed_at: u64,
    pub state: String,
    pub failure_reason: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessedInvite {
    pub event_id: String,
    pub invite_event_id: String,
    pub account_pubkey: PublicKey,
    pub processed_at: u64,
    pub state: ProcessedInviteState,
    pub failure_reason: String,
}

impl From<ProcessedInviteRow> for ProcessedInvite {
    fn from(row: ProcessedInviteRow) -> Self {
        Self {
            event_id: row.event_id,
            invite_event_id: row.invite_event_id,
            account_pubkey: PublicKey::from_hex(&row.account_pubkey).unwrap(),
            processed_at: row.processed_at,
            state: ProcessedInviteState::from(row.state),
            failure_reason: row.failure_reason,
        }
    }
}

impl From<ProcessedInvite> for ProcessedInviteRow {
    fn from(invite: ProcessedInvite) -> Self {
        Self {
            event_id: invite.event_id,
            invite_event_id: invite.invite_event_id,
            account_pubkey: invite.account_pubkey.to_hex(),
            processed_at: invite.processed_at,
            state: String::from(invite.state),
            failure_reason: invite.failure_reason,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum ProcessedInviteState {
    Processed,
    Failed,
}

impl From<String> for ProcessedInviteState {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "processed" => Self::Processed,
            "failed" => Self::Failed,
            _ => panic!("Invalid processed invite state: {}", s),
        }
    }
}

impl From<ProcessedInviteState> for String {
    fn from(state: ProcessedInviteState) -> Self {
        match state {
            ProcessedInviteState::Processed => "processed".to_string(),
            ProcessedInviteState::Failed => "failed".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum InviteState {
    Pending,
    Accepted,
    Declined,
    Ignored,
}

impl From<String> for InviteState {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => Self::Pending,
            "accepted" => Self::Accepted,
            "declined" => Self::Declined,
            "ignored" => Self::Ignored,
            _ => panic!("Invalid invite state: {}", s),
        }
    }
}

impl From<InviteState> for String {
    fn from(state: InviteState) -> Self {
        match state {
            InviteState::Pending => "pending".to_string(),
            InviteState::Accepted => "accepted".to_string(),
            InviteState::Declined => "declined".to_string(),
            InviteState::Ignored => "ignored".to_string(),
        }
    }
}

impl From<InviteRow> for Invite {
    fn from(row: InviteRow) -> Self {
        Self {
            event_id: row.event_id,
            account_pubkey: row.account_pubkey,
            event: UnsignedEvent::from_json(&row.event).unwrap(),
            mls_group_id: row.mls_group_id,
            nostr_group_id: row.nostr_group_id,
            group_name: row.group_name,
            group_description: row.group_description,
            group_admin_pubkeys: serde_json::from_str(&row.group_admin_pubkeys).unwrap(),
            group_relays: serde_json::from_str(&row.group_relays).unwrap(),
            inviter: row.inviter,
            member_count: row.member_count,
            state: InviteState::from(row.state),
            outer_event_id: row.outer_event_id,
        }
    }
}

impl Invite {
    pub async fn find_by_id(
        account_pubkey: &str,
        invite_event_id: &str,
        wn: tauri::State<'_, Whitenoise>,
    ) -> Result<Self> {
        let invite_row = sqlx::query_as::<_, InviteRow>(
            "SELECT * FROM invites WHERE account_pubkey = ? AND event_id = ?",
        )
        .bind(account_pubkey)
        .bind(invite_event_id)
        .fetch_one(&wn.database.pool)
        .await?;

        Ok(Self {
            event_id: invite_row.event_id,
            account_pubkey: invite_row.account_pubkey,
            event: UnsignedEvent::from_json(&invite_row.event)?,
            mls_group_id: invite_row.mls_group_id,
            nostr_group_id: invite_row.nostr_group_id,
            group_name: invite_row.group_name,
            group_description: invite_row.group_description,
            group_admin_pubkeys: serde_json::from_str(&invite_row.group_admin_pubkeys)?,
            group_relays: serde_json::from_str(&invite_row.group_relays)?,
            inviter: invite_row.inviter,
            member_count: invite_row.member_count,
            state: InviteState::from(invite_row.state),
            outer_event_id: invite_row.outer_event_id,
        })
    }

    pub async fn pending(wn: tauri::State<'_, Whitenoise>) -> Result<Vec<Self>> {
        let active_account = Account::get_active(wn.clone()).await?;
        let invites = sqlx::query_as::<_, InviteRow>(
            "SELECT * FROM invites WHERE state = 'pending' AND account_pubkey = ?",
        )
        .bind(active_account.pubkey.to_hex())
        .fetch_all(&wn.database.pool)
        .await?;
        Ok(invites.into_iter().map(|row| row.into()).collect())
    }

    pub async fn save(&self, wn: tauri::State<'_, Whitenoise>) -> Result<Self> {
        let mut txn = wn.database.pool.begin().await?;
        sqlx::query("INSERT OR REPLACE INTO invites (event_id, account_pubkey, event, mls_group_id, nostr_group_id, group_name, group_description, group_admin_pubkeys, group_relays, inviter, member_count, outer_event_id, state) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&self.event_id)
            .bind(&self.account_pubkey)
            .bind(serde_json::to_string(&self.event)?)
            .bind(&self.mls_group_id)
            .bind(&self.nostr_group_id)
            .bind(&self.group_name)
            .bind(&self.group_description)
            .bind(serde_json::to_string(&self.group_admin_pubkeys)?)
            .bind(serde_json::to_string(&self.group_relays)?)
            .bind(&self.inviter)
            .bind(self.member_count)
            .bind(&self.outer_event_id)
            .bind(String::from(self.state.clone()))
            .execute(&mut *txn)
            .await?;
        txn.commit().await?;
        Ok(self.clone())
    }

    // pub fn new(event: UnsignedEvent, database: &Database) -> Result<Invite> {}
    // pub fn find_by_event_id(event_id: &str, database: &Database) -> Result<Option<Invite>> {}
    // pub fn fetch_invites_from_relays(database: &Database) -> Result<()> {}

    // pub fn accept(&self, database: &Database) -> Result<()> {}
    // pub fn decline(&self, database: &Database) -> Result<()> {}
    // pub fn remove(&self, database: &Database) -> Result<()> {}
}

impl ProcessedInvite {
    pub async fn find_by_invite_event_id(
        event_id: EventId,
        wn: tauri::State<'_, Whitenoise>,
    ) -> Result<Option<Self>> {
        let active_account = Account::get_active(wn.clone()).await?;

        let processed_invite_row = sqlx::query_as::<_, ProcessedInviteRow>(
            "SELECT * FROM processed_invites WHERE event_id = ? AND account_pubkey = ?",
        )
        .bind(event_id.to_string())
        .bind(active_account.pubkey.to_hex())
        .fetch_optional(&wn.database.pool)
        .await?;
        match processed_invite_row {
            Some(row) => Ok(Some(row.into())),
            None => Ok(None),
        }
    }

    pub async fn failed_with_reason(
        wn: tauri::State<'_, Whitenoise>,
    ) -> Result<Vec<(EventId, String)>> {
        let active_account = Account::get_active(wn.clone()).await?;

        let processed_invite_rows = sqlx::query_as::<_, ProcessedInviteRow>(
            "SELECT * FROM processed_invites WHERE state = 'failed' AND account_pubkey = ?",
        )
        .bind(active_account.pubkey.to_hex())
        .fetch_all(&wn.database.pool)
        .await?;
        Ok(processed_invite_rows
            .into_iter()
            .map(|row| (EventId::parse(&row.event_id).unwrap(), row.failure_reason))
            .collect())
    }

    pub async fn create_with_state_and_reason(
        event_id: EventId,
        invite_event_id: EventId,
        state: ProcessedInviteState,
        reason: String,
        wn: tauri::State<'_, Whitenoise>,
    ) -> Result<Self> {
        let active_account = Account::get_active(wn.clone()).await?;

        let mut txn = wn.database.pool.begin().await?;
        let processed_at = chrono::Utc::now().timestamp() as u64;
        sqlx::query("INSERT INTO processed_invites (event_id, invite_event_id, account_pubkey, processed_at, state, failure_reason) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(event_id.to_string())
            .bind(invite_event_id.to_string())
            .bind(active_account.pubkey.to_hex())
            .bind(processed_at as i64)
            .bind(String::from(state.clone()))
            .bind(reason.clone())
            .execute(&mut *txn)
            .await?;
        txn.commit().await?;

        Ok(Self {
            event_id: event_id.to_string(),
            invite_event_id: invite_event_id.to_string(),
            account_pubkey: active_account.pubkey,
            processed_at,
            state,
            failure_reason: reason,
        })
    }
}
