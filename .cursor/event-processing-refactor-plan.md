# Event Processing Architecture Refactor Plan

## Overview
Refactor the event processing architecture to eliminate the EventProcessor layer and move event queuing/processing directly to the Whitenoise struct. This will provide direct access to accounts, database, and other application state for account-aware event processing.

## Current Architecture Issues
- EventProcessor is nested under NostrManager but needs access to Whitenoise state
- Complex routing between layers
- No clean way to access accounts for account-aware processing
- State synchronization challenges between EventProcessor and Whitenoise

## Target Architecture
```
Whitenoise
├── NostrManager (client + notification handler)
├── Event Queue + Processing Loop (moved here from EventProcessor)
├── Accounts HashMap
├── Database
└── Processing Methods (process_giftwrap, process_mls_message, etc.)
```

## Tasks

### 1. Update Whitenoise Struct
**File:** `src/lib.rs`

- [ ] Add event queue fields to Whitenoise struct:
  ```rust
  pub struct Whitenoise {
      // ... existing fields ...
      event_sender: Sender<ProcessableEvent>,
      shutdown_sender: Sender<()>,
  }
  ```

### 2. Move ProcessableEvent Enum
**File:** `src/lib.rs` or `src/types.rs`

- [ ] Move `ProcessableEvent` enum from `event_processor.rs` to a shared location
- [ ] Keep the current definition:
  ```rust
  #[derive(Debug)]
  pub enum ProcessableEvent {
      NostrEvent(Event, Option<String>), // Event and optional subscription_id
      RelayMessage(RelayUrl, String),
  }
  ```

### 3. Implement Event Queue in Whitenoise
**File:** `src/lib.rs`

- [ ] Add event processing methods to `impl Whitenoise`:
  ```rust
  pub async fn queue_event(&self, event: Event, subscription_id: Option<String>) -> Result<()>
  pub async fn queue_message(&self, relay_url: RelayUrl, message: RelayMessage<'_>) -> Result<()>
  async fn start_event_processing_loop(&self, receiver: Receiver<ProcessableEvent>)
  async fn process_event(&mut self, event: ProcessableEvent) -> Result<()>
  async fn shutdown_event_processing(&self) -> Result<()>
  ```

- [ ] Update `initialize_whitenoise()` to:
  - Create the event queue channels
  - Start the event processing loop
  - Pass event sender to NostrManager

### 4. Implement Account-Aware Processing Methods
**File:** `src/lib.rs`

- [ ] Move and update processing methods from EventProcessor:
  ```rust
  async fn process_giftwrap(&mut self, event: Event, subscription_id: Option<String>) -> Result<()>
  async fn process_mls_message(&mut self, event: Event, subscription_id: Option<String>) -> Result<()>
  async fn process_relay_message(&self, relay_url: RelayUrl, message_type: String)
  ```

- [ ] Implement helper function for extracting pubkey from subscription_id:
  ```rust
  fn extract_pubkey_from_subscription_id(subscription_id: &str) -> Option<PublicKey>
  ```

- [ ] Make `process_giftwrap` fully account-aware:
  - Extract target pubkey from 'p' tag in giftwrap event
  - Validate against subscription_id pubkey
  - Look up account from `self.accounts`
  - Get nostr keys using `self.get_nostr_keys_for_pubkey()`
  - Process the giftwrap with full context

### 5. Update NostrManager
**File:** `src/nostr_manager/mod.rs`

- [ ] Remove `event_processor` field from NostrManager struct
- [ ] Update `NostrManager::new()` signature:
  ```rust
  pub async fn new(
      db_path: PathBuf,
      event_sender: Sender<ProcessableEvent>
  ) -> Result<Self>
  ```

- [ ] Update notification handler to send directly to Whitenoise:
  ```rust
  RelayPoolNotification::Message { relay_url, message } => {
      // Extract events and send to Whitenoise queue
      match message {
          RelayMessage::Event { subscription_id, event } => {
              event_sender.send(ProcessableEvent::NostrEvent(
                  event.as_ref().clone(),
                  Some(subscription_id.to_string())
              )).await?;
          }
          _ => {
              event_sender.send(ProcessableEvent::RelayMessage(
                  relay_url,
                  message_type_string
              )).await?;
          }
      }
  }
  ```

### 6. Remove EventProcessor
**File:** `src/nostr_manager/event_processor.rs`

- [ ] Delete the entire file (functionality moved to Whitenoise)

### 7. Update Module Declarations
**File:** `src/nostr_manager/mod.rs`

- [ ] Remove `pub mod event_processor;` line
- [ ] Remove EventProcessor-related imports

### 8. Update Error Handling
**Files:** `src/error.rs`, `src/nostr_manager/mod.rs`

- [ ] Remove EventProcessor-related error types if no longer needed
- [ ] Update error handling for new queue-based architecture

### 9. Update Integration Points
**Files:** Various

- [ ] Update any code that was calling EventProcessor methods
- [ ] Update initialization code that was setting up EventProcessor
- [ ] Update shutdown code to use new Whitenoise shutdown method

### 10. Testing & Validation
- [ ] Test that events are properly queued and processed
- [ ] Test account-aware processing with multiple accounts
- [ ] Test subscription_id extraction and validation
- [ ] Test graceful shutdown of event processing
- [ ] Test giftwrap processing with correct account lookup

## Key Benefits After Refactor

1. **Direct State Access**: Processing methods have direct access to `self.accounts`, `self.database`, etc.
2. **Simplified Architecture**: Eliminates entire EventProcessor layer
3. **Account-Aware Processing**: Can easily lookup accounts and process events for the correct account
4. **No Circular Dependencies**: Clean ownership hierarchy
5. **Better Testing**: Can unit test processing methods directly on Whitenoise instances
6. **Clearer Ownership**: All application logic lives in Whitenoise

## Implementation Notes

- **Subscription ID Format**: `{pubkey}_{subscription_type}` (e.g., `{pubkey}_giftwrap`)
- **Giftwrap Target**: Extract from 'p' tag, not from `event.pubkey` (which is the author)
- **Message Processing**: Use `RelayPoolNotification::Message` to get subscription_id information
- **Graceful Shutdown**: Ensure event processing loop can be cleanly shut down

## Files to Modify

- `src/lib.rs` (main changes)
- `src/nostr_manager/mod.rs` (remove EventProcessor, update notification handler)
- `src/nostr_manager/event_processor.rs` (delete)
- `src/error.rs` (cleanup)
- `src/types.rs` (possibly move ProcessableEvent here)

## Success Criteria

- [ ] Events are processed account-aware based on subscription_id
- [ ] Giftwrap events are correctly routed to the target account
- [ ] No EventProcessor layer exists
- [ ] Event queue and processing live directly in Whitenoise
- [ ] Clean architecture with direct state access
- [ ] All tests pass
- [ ] Code compiles without warnings
