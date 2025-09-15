# Event Timestamp Unification Implementation Plan

## Problem Statement

The current event timestamp tracking has critical bugs where timestamp comparisons can return incorrect results after data deletions:

1. **Relay List Issue**: `newest_relay_event_timestamp()` uses `MAX(event_created_at)` from `user_relays` table, but when relays are removed via `remove_relay()`, rows are deleted, causing the MAX to regress to older timestamps.

2. **Contact List Issue**: `get_latest_follow_list_timestamp()` has the same issue - uses `MAX(event_created_at)` from `account_follows`, but `update_follows_from_event()` deletes all follows before inserting new ones.

3. **Inconsistent Event Tracking**: Events processed via subscriptions create `processed_events` entries, but events processed via background sync (`sync_metadata()`, `update_relay_lists()`) bypass this tracking entirely.

## Proposed Solution

Migrate all event timestamp tracking to the `processed_events` table with enhanced schema, creating a unified approach that:
- Prevents timestamp regression bugs
- Ensures consistent event tracking across all processing paths
- Provides complete audit trail and deduplication capabilities

## Implementation Plan

### Phase 1: Database Schema Migration

#### 1.1 Replace Existing Migration
**Delete** `db_migrations/0014_add_event_created_at_columns.sql` and **replace** it with enhanced `db_migrations/0014_enhance_processed_events.sql`:

```sql
-- Enhance processed_events table with event timestamp and kind tracking
-- This replaces the previous approach of adding event_created_at to individual tables
-- Following the official SQLite 12-step ALTER TABLE procedure

PRAGMA foreign_keys=OFF;

-- Create new table with enhanced schema
CREATE TABLE processed_events_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL
        CHECK (length(event_id) = 64 AND event_id GLOB '[0-9a-fA-F]*'), -- 64-char hex
    account_id INTEGER,                   -- NULL for global events, account ID for account-specific
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP, -- When we processed it
    event_created_at INTEGER NOT NULL,   -- Original Nostr event timestamp (milliseconds)
    event_kind INTEGER,                  -- Nostr event kind (0, 3, 10002, etc.) - NULL for legacy data

    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    UNIQUE(event_id, account_id)          -- Each account can only process a specific event once
);

-- Transfer existing data (set placeholder values for new fields)
INSERT INTO processed_events_new (id, event_id, account_id, created_at, event_created_at, event_kind)
SELECT id, event_id, account_id, created_at,
       COALESCE(strftime('%s', created_at) * 1000, 0) as event_created_at, -- Use processing time as fallback
       NULL as event_kind  -- NULL for existing data where we don't know the kind
FROM processed_events;

-- Drop old table and rename
DROP TABLE processed_events;
ALTER TABLE processed_events_new RENAME TO processed_events;

-- Create indexes
CREATE INDEX idx_processed_events_lookup ON processed_events(event_id);
CREATE INDEX idx_processed_events_account_id ON processed_events(account_id);
CREATE INDEX idx_processed_events_event_kind ON processed_events(event_kind);
CREATE INDEX idx_processed_events_event_created_at ON processed_events(event_created_at);
CREATE INDEX idx_processed_events_kind_timestamp ON processed_events(event_kind, event_created_at);

-- Partial unique index for global events
CREATE UNIQUE INDEX idx_processed_events_global_unique
ON processed_events(event_id)
WHERE account_id IS NULL;

PRAGMA foreign_key_check;
PRAGMA foreign_keys=ON;
```

### Phase 2: Update ProcessedEvent Model

#### 2.1 Update ProcessedEvent Struct
In `src/whitenoise/database/processed_events.rs`:

```rust
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProcessedEvent {
    pub id: i64,
    pub event_id: EventId,
    pub account_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub event_created_at: DateTime<Utc>,  // NEW
    pub event_kind: Option<u16>,          // NEW - nullable for legacy data
}
```

#### 2.2 Update ProcessedEvent Methods
- Update `create()` method to accept `event_created_at` and `event_kind: Option<u16>`
- Update `FromRow` implementation to handle nullable `event_kind`
- Add new query methods for timestamp lookups

### Phase 3: Create Unified Timestamp Functions

#### 3.1 Add Unified Query Functions
In `src/whitenoise/database/processed_events.rs`:

```rust
impl ProcessedEvent {
    /// Gets the newest event timestamp for specific event kinds and account
    pub(crate) async fn newest_event_timestamp_for_kinds(
        account_id: Option<i64>,
        event_kinds: &[u16],
        database: &Database,
    ) -> Result<Option<DateTime<Utc>>, DatabaseError> {
        // Implementation for querying MAX(event_created_at) by kinds
        // WHERE event_kind IN (kinds) AND account_id = ? (or IS NULL for global)
    }

    /// Gets the newest relay event timestamp for a user
    pub(crate) async fn newest_relay_event_timestamp(
        user_pubkey: &PublicKey,
        relay_type: RelayType,
        database: &Database,
    ) -> Result<Option<DateTime<Utc>>, WhitenoiseError> {
        // Query processed_events for relay list event kinds
        let kinds = match relay_type {
            RelayType::Nip65 => vec![10002],
            RelayType::Inbox => vec![10050],
            RelayType::KeyPackage => vec![10051],
        };
        // Note: For global events (user data), account_id will be NULL
        // Implementation queries with account_id IS NULL and event_kind IN (kinds)
    }

    /// Gets the newest contact list event timestamp for an account
    pub(crate) async fn newest_contact_list_timestamp(
        account_id: i64,
        database: &Database,
    ) -> Result<Option<DateTime<Utc>>, WhitenoiseError> {
        // Query processed_events for kind 3 events with specific account_id
        // WHERE event_kind = 3 AND account_id = ?
    }
}
```

### Phase 4: Update Event Processing Paths

#### 4.1 Update Event Handlers
**Files to modify:**
- `src/whitenoise/event_processor/event_handlers/handle_metadata.rs`
- `src/whitenoise/event_processor/event_handlers/handle_relay_list.rs`
- `src/whitenoise/event_processor/event_handlers/handle_contact_list.rs`

**Changes:**
- All handlers should call `ProcessedEvent::create()` with event timestamp and `Some(event_kind)`
- Remove direct timestamp comparisons, use `ProcessedEvent::newest_event_timestamp_for_kinds()`

#### 4.2 Update Background Sync Paths
**Files to modify:**
- `src/whitenoise/users.rs` (`sync_metadata()`, `update_relay_lists()`)
- `src/whitenoise/accounts.rs` (`process_user_event_streams()`)

**Changes:**
- Ensure all background event processing creates `processed_events` entries
- Use unified timestamp functions for staleness checks

#### 4.3 Update EventTracker
In `src/whitenoise/event_tracker.rs`:
- Update `track_processed_account_event()` and `track_processed_global_event()` to pass event timestamp and `Some(event_kind)`
- All callers must provide this information
- For legacy events where kind is unknown, pass `None` for event_kind

### Phase 5: Replace Old Timestamp Functions

#### 5.1 Remove Old Functions
- `User::newest_relay_event_timestamp()` in `src/whitenoise/database/users.rs`
- `Account::get_latest_follow_list_timestamp()` in `src/whitenoise/database/accounts.rs`

#### 5.2 Update All Callers
- `User::sync_relay_urls()` - use new `ProcessedEvent::newest_relay_event_timestamp()`
- Contact list processing - use new `ProcessedEvent::newest_contact_list_timestamp()`

### Phase 6: Clean Up Legacy Code

#### 6.1 Remove Code References
Since the `event_created_at` columns only exist in this branch and we're replacing the migration:
- Remove any code that references `users.event_created_at`
- Remove any code that references `user_relays.event_created_at`
- Remove any code that references `account_follows.event_created_at`
- These columns will never exist since we're replacing migration 0014

#### 6.2 Migration Strategy
- **No additional migration needed** since we're replacing the original 0014 migration
- The new migration 0014 will create the enhanced `processed_events` table directly
- No cleanup migration required

## Testing Strategy

### 6.1 Unit Tests
- Test `ProcessedEvent` new methods with nullable `event_kind`
- Test timestamp queries with various scenarios (including legacy NULL kinds)
- Test event deduplication across processing paths

### 6.2 Integration Tests
- Test relay list processing doesn't regress timestamps after removals
- Test contact list processing doesn't regress timestamps after deletions
- Test background sync creates proper processed_events
- Test subscription and background sync produce identical results

### 6.3 Migration Tests
- Test migration runs successfully on existing data
- Test rollback scenarios
- Test performance of new indexes

## Storage Impact

| Rows | Current Size | Enhanced Size | Additional Storage |
|------|-------------|---------------|-------------------|
| 10,000 | ~1.0 MB | ~1.8 MB | **+0.8 MB** |
| 50,000 | ~5.0 MB | ~9.0 MB | **+4.0 MB** |

*Includes data and index storage. Impact is minimal for local SQLite database.*

## Benefits

1. **Bug Fixes**: Eliminates timestamp regression bugs in relay lists and contact lists
2. **Consistency**: Unified event tracking across all processing paths
3. **Deduplication**: Proper event deduplication between subscription and background sync
4. **Audit Trail**: Complete history of processed events with original timestamps
5. **Performance**: Efficient queries with proper indexing
6. **Future-Proof**: Foundation for additional event filtering and analysis

## Rollout Plan

1. **Phase 1**: Replace migration 0014 with enhanced processed_events schema
2. **Phase 2**: Update ProcessedEvent model to handle nullable event_kind
3. **Phase 3-4**: Add new functions and update event processing (gradual rollout)
4. **Phase 5**: Switch over to new functions (can be feature-flagged)
5. **Phase 6**: Remove legacy code references (no migration needed)

Each phase can be implemented and tested independently, allowing for safe incremental deployment.

**Simplified Migration**: Since we're replacing the existing migration rather than adding new ones, the rollout is cleaner with no additional database migrations required.

## Risk Mitigation

- **Database Migration**: Use SQLite transaction-safe migration procedures
- **Backward Compatibility**: Keep old functions during transition period
- **Testing**: Comprehensive test coverage for all timestamp scenarios
- **Monitoring**: Add logging to verify timestamp calculations are correct
- **Rollback Plan**: Each migration can be reversed if issues are discovered

## Timeline Estimate

- **Phase 1**: 1 day (replace migration 0014)
- **Phase 2**: 1-2 days (update ProcessedEvent model with nullable kind)
- **Phase 3**: 2-3 days (unified functions)
- **Phase 4**: 3-4 days (update all processing paths)
- **Phase 5**: 2-3 days (replace old functions)
- **Phase 6**: 1-2 days (remove legacy code references)

**Total**: ~2-3 weeks with proper testing and validation

**Simplified Timeline**: The migration replacement approach reduces complexity and eliminates the need for additional cleanup migrations.
