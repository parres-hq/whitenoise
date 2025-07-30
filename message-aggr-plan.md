# Message Aggregation Module Plan

## Overview

This document outlines the implementation plan for a message aggregation module that processes raw Nostr MLS messages into structured `ChatMessage` objects suitable for frontend display. The module will handle various message types (regular messages, reactions, deletions, replies) and aggregate related data into cohesive chat objects.

## Goals

- Transform raw `Vec<Message>` from `nostr_mls.get_messages()` into aggregated `Vec<ChatMessage>`
- Handle multiple message types: regular chat (kind 9), reactions (kind 7), deletions (kind 5), replies (kind 9 with e-tags)
- Provide resilient processing that handles out-of-order message arrival
- Support both stateless (immediate) and stateful (incremental) processing approaches
- Achieve <1 second processing time for typical group message histories
- Design extensible architecture for future features (editing, read receipts, threading)

## Data Structures

### Core Types

```rust
/// Represents an aggregated chat message ready for frontend display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// Unique identifier of the message
    pub id: String,

    /// Public key of the message author
    pub author: PublicKey,

    /// Message content (empty if deleted)
    pub content: String,

    /// Timestamp when the message was created
    pub created_at: Timestamp,

    /// Tags from the original Nostr event
    pub tags: Tags,

    /// Whether this message is a reply to another message
    pub is_reply: bool,

    /// ID of the message this is replying to (if is_reply is true)
    pub reply_to_id: Option<String>,

    /// Whether this message has been deleted
    pub is_deleted: bool,

    /// Parsed tokens from the message content (mentions, hashtags, etc.)
    pub content_tokens: Vec<SerializableToken>,

    /// Aggregated reactions on this message
    pub reactions: ReactionSummary,
}

/// Summary of reactions on a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReactionSummary {
    /// Map of emoji to reaction details
    pub by_emoji: HashMap<String, EmojiReaction>,

    /// List of all users who have reacted and with what
    pub user_reactions: Vec<UserReaction>,
}

/// Details for a specific emoji reaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmojiReaction {
    /// The emoji or reaction symbol
    pub emoji: String,

    /// Count of users who used this reaction
    pub count: usize,

    /// List of users who used this reaction
    pub users: Vec<PublicKey>,
}

/// Individual user's reaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserReaction {
    /// User who made the reaction
    pub user: PublicKey,

    /// The emoji they reacted with
    pub emoji: String,

    /// Timestamp of the reaction
    pub created_at: Timestamp,
}

/// Internal type for tracking unresolved references
#[derive(Debug, Clone)]
struct UnresolvedMessage {
    message: Message,
    retry_count: u8,
    reason: UnresolvedReason,
}

#[derive(Debug, Clone)]
enum UnresolvedReason {
    ReplyToMissing(String),  // Missing parent message ID
    ReactionToMissing(String), // Missing target message ID
    DeleteTargetMissing(String), // Missing delete target ID
}
```

### Configuration

```rust
/// Configuration for the message aggregator
#[derive(Debug, Clone)]
pub struct AggregatorConfig {
    /// Maximum number of retry attempts for unresolved messages
    pub max_retry_attempts: u8,

    /// Whether to normalize emoji (treat skin tone variants as same base emoji)
    pub normalize_emoji: bool,

    /// Whether to enable detailed logging of processing steps
    pub enable_debug_logging: bool,
}

impl Default for AggregatorConfig {
    fn default() -> Self {
        Self {
            max_retry_attempts: 3,
            normalize_emoji: true,
            enable_debug_logging: false,
        }
    }
}
```

## Module Structure

```
src/whitenoise/message_aggregator/
├── mod.rs                 # Public API and main aggregator
├── types.rs              # ChatMessage and related types
├── processor.rs          # Core message processing logic
├── reaction_handler.rs   # Reaction-specific processing
├── emoji_utils.rs        # Emoji validation and normalization
├── state.rs              # State management (future stateful implementation)
└── tests.rs              # Comprehensive test suite
```

## Processing Algorithm

### Phase 1: Stateless Implementation

The initial implementation will process all messages each time, following this algorithm:

1. **Initialize State**
   - Create empty `HashMap<String, ChatMessage>` for processed messages
   - Create `Vec<UnresolvedMessage>` for messages that reference missing targets
   - Sort all input messages by `created_at` timestamp

2. **First Pass: Process Base Messages**
   - Iterate through all messages in chronological order
   - For each message of kind 9 (regular chat):
     - Create `ChatMessage` with basic fields
     - Parse content to tokens using existing `nostr.parse()` functionality
     - Check if it's a reply (has e-tag): set `is_reply` and `reply_to_id`
     - Add to processed messages map

3. **Second Pass: Process Reactions**
   - For each reaction message (kind 7):
     - Validate reaction content (emoji, "+", or "-")
     - Normalize emoji if configured
     - Extract target message ID from e-tag
     - If target message exists in processed map:
       - Add reaction to target's `ReactionSummary`
       - Enforce one reaction per user (replace if duplicate)
     - If target missing: add to unresolved list

4. **Third Pass: Process Deletions**
   - For each deletion message (kind 5):
     - Extract target message IDs from e-tags
     - For each target ID:
       - If target exists in processed map: mark as deleted
       - If target missing: add to unresolved list

5. **Retry Pass: Handle Unresolved Messages**
   - For up to `max_retry_attempts`:
     - Process unresolved messages again
     - Remove successfully processed ones
     - Increment retry count on remaining
   - Log warnings for messages that remain unresolved after max attempts

6. **Return Results**
   - Extract `ChatMessage` values from map
   - Sort by `created_at` timestamp
   - Return as `Vec<ChatMessage>`

### Message Type Handlers

#### Regular Messages (Kind 9)
```rust
fn process_regular_message(message: &Message, content_tokens: Vec<SerializableToken>) -> ChatMessage {
    let is_reply = message.tags.iter().any(|tag| {
        tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E))
    });

    let reply_to_id = if is_reply {
        message.tags.iter()
            .filter(|tag| tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)))
            .last()  // Use last e-tag as per Nostr convention
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())
    } else {
        None
    };

    ChatMessage {
        id: message.id.clone(),
        author: message.author,
        content: message.content.clone(),
        created_at: message.created_at,
        tags: message.tags.clone(),
        is_reply,
        reply_to_id,
        is_deleted: false,
        content_tokens,
        reactions: ReactionSummary::default(),
    }
}
```

#### Reactions (Kind 7)
```rust
fn process_reaction(
    message: &Message,
    processed_messages: &mut HashMap<String, ChatMessage>,
    unresolved: &mut Vec<UnresolvedMessage>
) -> Result<(), ProcessingError> {
    // Validate reaction content
    let reaction_emoji = validate_and_normalize_reaction(&message.content)?;

    // Extract target message ID
    let target_id = extract_target_message_id(&message.tags)?;

    if let Some(target_message) = processed_messages.get_mut(&target_id) {
        add_reaction_to_message(target_message, &message.author, &reaction_emoji, message.created_at);
    } else {
        unresolved.push(UnresolvedMessage {
            message: message.clone(),
            retry_count: 0,
            reason: UnresolvedReason::ReactionToMissing(target_id),
        });
    }

    Ok(())
}

fn validate_and_normalize_reaction(content: &str) -> Result<String, ProcessingError> {
    match content {
        "+" => Ok("👍".to_string()), // Normalize to thumbs up
        "-" => Ok("👎".to_string()), // Normalize to thumbs down
        emoji if is_valid_emoji(emoji) => Ok(normalize_emoji(emoji)),
        _ => {
            log::warn!("Invalid reaction content: {}", content);
            Err(ProcessingError::InvalidReaction)
        }
    }
}
```

#### Deletions (Kind 5)
```rust
fn process_deletion(
    message: &Message,
    processed_messages: &mut HashMap<String, ChatMessage>,
    unresolved: &mut Vec<UnresolvedMessage>
) -> Result<(), ProcessingError> {
    let target_ids = extract_deletion_target_ids(&message.tags);

    for target_id in target_ids {
        if let Some(target_message) = processed_messages.get_mut(&target_id) {
            target_message.is_deleted = true;
            target_message.content = String::new(); // Clear content
        } else {
            unresolved.push(UnresolvedMessage {
                message: message.clone(),
                retry_count: 0,
                reason: UnresolvedReason::DeleteTargetMissing(target_id),
            });
        }
    }

    Ok(())
}
```

## Public API

The module will expose a clean public API through `mod.rs`:

```rust
use tokio::sync::OnceCell;
use std::sync::Arc;

/// Main message aggregator - designed to be a singleton per Whitenoise instance
/// Group-aware to ensure proper isolation between different group conversations
pub struct MessageAggregator {
    config: AggregatorConfig,
    // Future: state management for stateful mode, keyed by GroupId
    // state: Arc<tokio::sync::RwLock<HashMap<GroupId, AggregatorState>>>,
}

impl MessageAggregator {
    /// Create a new message aggregator with default configuration
    /// This should typically be called only during Whitenoise initialization
    pub fn new() -> Self {
        Self::with_config(AggregatorConfig::default())
    }
    
    /// Create a new message aggregator with custom configuration
    /// This should typically be called only during Whitenoise initialization
    pub fn with_config(config: AggregatorConfig) -> Self {
        Self { 
            config,
            // state: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
    
    /// Fetch and aggregate messages for a specific group
    /// This is the main entry point that handles the complete pipeline:
    /// 1. Fetch raw messages from nostr_mls
    /// 2. Parse content tokens for each message
    /// 3. Aggregate reactions, replies, and deletions
    /// 4. Return structured ChatMessage objects
    /// 
    /// # Arguments
    /// * `pubkey` - The public key of the user requesting messages (for account access)
    /// * `group_id` - The group to fetch and aggregate messages for
    /// * `nostr_mls` - Reference to the nostr_mls instance for fetching raw messages
    /// * `parser` - Reference to the nostr parser for tokenizing message content
    pub async fn fetch_and_aggregate_messages_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
        nostr_mls: &NostrMls, // We'll need to determine the exact type
        parser: &dyn Parser,  // We'll need to determine the exact type
    ) -> Result<Vec<ChatMessage>, ProcessingError> {
        if self.config.enable_debug_logging {
            log::debug!("Fetching and aggregating messages for group {}", hex::encode(group_id.as_slice()));
        }
        
        // Step 1: Fetch raw messages from nostr_mls
        let raw_messages = nostr_mls.get_messages(group_id)
            .map_err(|e| ProcessingError::FetchFailed(e.to_string()))?;
        
        if self.config.enable_debug_logging {
            log::debug!("Fetched {} raw messages for group {}", raw_messages.len(), hex::encode(group_id.as_slice()));
        }
        
        // Step 2: Process messages with token parsing
        self.process_messages_internal(group_id, raw_messages, parser).await
    }
    
    /// Internal method to process raw messages into aggregated chat messages
    /// This handles the core aggregation logic and can be used for both fresh fetches
    /// and incremental updates in the future
    async fn process_messages_internal(
        &self,
        group_id: &GroupId,
        messages: Vec<Message>,
        parser: &dyn Parser,
    ) -> Result<Vec<ChatMessage>, ProcessingError> {
        // Implementation as described in algorithm above
        // In stateless mode, group_id is used for logging/debugging
        // In future stateful mode, group_id will be used for state isolation
        
        if self.config.enable_debug_logging {
            log::debug!("Processing {} messages for group {}", messages.len(), hex::encode(group_id.as_slice()));
        }
        
        // Implementation continues with token parsing integrated...
        // Note: This method is async to support future stateful operations
        // that may need to acquire locks or perform I/O
    }
    
    /// Get the current configuration
    pub fn config(&self) -> &AggregatorConfig {
        &self.config
    }
    
    // Future APIs for stateful implementation - all async for lock management:
    
    /// Update the aggregated state with new messages for a specific group
    /// This will be used when we transition to stateful processing
    // pub async fn update_group_with_new_messages(
    //     &self, 
    //     group_id: &GroupId,
    //     messages: Vec<Message>
    // ) -> Result<(), ProcessingError>
    
    /// Get the current aggregated messages for a specific group
    // pub async fn get_aggregated_messages_for_group(&self, group_id: &GroupId) -> Option<Vec<ChatMessage>>
    
    /// Persist state for a specific group to disk
    // pub async fn persist_group_state(&self, group_id: &GroupId) -> Result<(), StateError>
    
    /// Load persisted state for a specific group from disk
    // pub async fn load_group_state(&self, group_id: &GroupId) -> Result<(), StateError>
    
    /// Clear all cached/persisted state for a specific group
    /// Useful when a user leaves a group or wants to reset message history
    // pub async fn clear_group_state(&self, group_id: &GroupId) -> Result<(), StateError>
    
    /// Clear all state for all groups (complete reset)
    // pub async fn clear_all_state(&self) -> Result<(), StateError>
    
    /// Get statistics about memory usage per group
    // pub async fn get_group_statistics(&self) -> HashMap<GroupId, GroupStatistics>
}

/// Statistics about a group's message processing
#[derive(Debug, Clone)]
pub struct GroupStatistics {
    pub message_count: usize,
    pub reaction_count: usize,
    pub deleted_message_count: usize,
    pub memory_usage_bytes: usize,
    pub last_processed_at: Option<Timestamp>,
}
```

Integration with existing codebase:

```rust
// In src/whitenoise/mod.rs, add:
pub mod message_aggregator;
pub use message_aggregator::{MessageAggregator, AggregatorConfig, GroupStatistics};

// Add to WhitenoiseConfig to allow customization
#[derive(Debug, Clone)]
pub struct WhitenoiseConfig {
    // ... existing fields ...
    
    /// Configuration for the message aggregator
    pub message_aggregator_config: Option<AggregatorConfig>,
}

impl Default for WhitenoiseConfig {
    fn default() -> Self {
        Self {
            // ... existing defaults ...
            message_aggregator_config: None, // Will use MessageAggregator::default()
        }
    }
}

// Option 1: Using Arc<OnceCell> for lazy async initialization
pub struct Whitenoise {
    // ... existing fields ...
    
    /// Message aggregator singleton - initialized lazily in async context
    message_aggregator: Arc<OnceCell<MessageAggregator>>,
    
    /// Store config for lazy initialization
    aggregator_config: Option<AggregatorConfig>,
}

impl Whitenoise {
    /// Create a new Whitenoise instance with the given configuration
    pub fn new(config: WhitenoiseConfig) -> Result<Self> {
        // ... existing initialization code ...
        
        Ok(Self {
            // ... existing fields ...
            message_aggregator: Arc::new(OnceCell::new()),
            aggregator_config: config.message_aggregator_config,
        })
    }
    
    /// Get or initialize the message aggregator singleton
    async fn get_message_aggregator(&self) -> &MessageAggregator {
        self.message_aggregator
            .get_or_init(|| async {
                // Initialize with config - this runs only once
                if let Some(config) = self.aggregator_config.clone() {
                    MessageAggregator::with_config(config)
                } else {
                    MessageAggregator::new()
                }
            })
            .await
    }
    
    /// Fetch and aggregate messages for a group - Main consumer API
    /// This is the primary method that consumers should use to get chat messages
    pub async fn fetch_aggregated_messages_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
    ) -> Result<Vec<ChatMessage>> {
        // Get account to access nostr_mls instance
        let account = self.fetch_account(pubkey).await?;
        let nostr_mls_guard = account.nostr_mls.lock().unwrap();
        
        if let Some(nostr_mls) = nostr_mls_guard.as_ref() {
            // Use the aggregator to handle the complete pipeline
            self.message_aggregator
                .fetch_and_aggregate_messages_for_group(
                    pubkey,
                    group_id,
                    nostr_mls,
                    &self.nostr, // For token parsing
                )
                .await
                .map_err(|e| WhitenoiseError::MessageAggregation(e.to_string()))
        } else {
            Err(WhitenoiseError::NostrMlsNotInitialized)
        }
    }
    
    /// Get a reference to the message aggregator for advanced usage
    pub async fn message_aggregator(&self) -> &MessageAggregator {
        self.get_message_aggregator().await
    }
}

// Option 2: Simpler approach - Initialize during Whitenoise::new()
// (This might be better since initialization is lightweight for stateless mode)
pub struct Whitenoise {
    // ... existing fields ...
    
    /// Message aggregator singleton - initialized at creation time
    message_aggregator: MessageAggregator,
}

impl Whitenoise {
    /// Create a new Whitenoise instance with the given configuration
    pub fn new(config: WhitenoiseConfig) -> Result<Self> {
        // ... existing initialization code ...
        
        let message_aggregator = if let Some(aggr_config) = config.message_aggregator_config {
            MessageAggregator::with_config(aggr_config)
        } else {
            MessageAggregator::new()
        };
        
        Ok(Self {
            // ... existing fields ...
            message_aggregator,
        })
    }
    
    /// Fetch and aggregate messages for a group
    pub async fn fetch_aggregated_messages_for_group(
        &self,
        pubkey: &PublicKey,
        group_id: &GroupId,
    ) -> Result<Vec<ChatMessage>> {
        let messages_with_tokens = self.fetch_messages_for_group(pubkey, group_id).await?;
        let messages: Vec<Message> = messages_with_tokens.into_iter().map(|mwt| mwt.message).collect();
        
        // Direct access to aggregator - simpler approach
        self.message_aggregator.process_messages_for_group(group_id, messages)
            .await
            .map_err(|e| WhitenoiseError::MessageAggregation(e.to_string()))
    }
    
    /// Get a reference to the message aggregator for advanced usage
    pub fn message_aggregator(&self) -> &MessageAggregator {
        &self.message_aggregator
    }
}

// Future convenience methods for stateful processing:

/// Get statistics about message processing for a specific group
// pub async fn get_group_message_statistics(&self, group_id: &GroupId) -> Option<GroupStatistics> {
//     let stats = self.message_aggregator().get_group_statistics().await;
//     stats.get(group_id).cloned()
// }

/// Clear message aggregation state when leaving a group
// pub async fn clear_group_message_state(&self, group_id: &GroupId) -> Result<()> {
//     self.message_aggregator()
//         .clear_group_state(group_id)
//         .await
//         .map_err(|e| WhitenoiseError::MessageAggregation(e.to_string()))
// }
```

## Error Handling

```rust
/// Errors that can occur during message processing
#[derive(Debug, thiserror::Error)]
pub enum ProcessingError {
    #[error("Invalid reaction content: {0}")]
    InvalidReaction,

    #[error("Missing required e-tag in message")]
    MissingETag,

    #[error("Invalid tag format")]
    InvalidTag,

    #[error("Failed to parse message timestamp")]
    InvalidTimestamp,
    
    #[error("Failed to fetch messages from nostr_mls: {0}")]
    FetchFailed(String),

    #[error("Internal processing error: {0}")]
    Internal(String),
}

// Add to WhitenoiseError enum in src/whitenoise/error.rs:
#[derive(Debug, thiserror::Error)]
pub enum WhitenoiseError {
    // ... existing error variants ...

    #[error("Message aggregation failed: {0}")]
    MessageAggregation(String),
}
```

## Testing Strategy

### Unit Tests
- Test each message type processor individually
- Test emoji validation and normalization
- Test edge cases (duplicate reactions, multiple deletions, etc.)
- Test retry logic for unresolved messages

### Integration Tests
- Test full processing pipeline with realistic message sequences
- Test out-of-order message delivery scenarios
- Test performance with large message sets
- Test error handling and resilience

### Performance Tests
- Benchmark processing time for 1K, 10K, 100K messages
- Memory usage profiling
- Identify performance bottlenecks for future optimization

### Test Data
Create comprehensive test scenarios including:
- Simple conversations with reactions
- Complex reply chains
- Multiple users reacting to same message
- Deletions of messages with existing reactions/replies
- Out-of-order delivery patterns
- Edge cases and error conditions

## Phase 2: Stateful Implementation (Future)

When performance requirements demand it, implement stateful processing with group isolation:

### State Storage Architecture
- Maintain per-group in-memory `HashMap<String, ChatMessage>` for fast access
- Use `HashMap<GroupId, GroupState>` to isolate state between groups
- Persist state to disk using bincode or similar efficient serialization
- Implement per-group incremental updates for new messages
- Add state recovery mechanisms with group-level granularity

### Group State Structure
```rust
/// Internal state for a specific group's message aggregation
struct GroupState {
    /// Processed messages by message ID
    messages: HashMap<String, ChatMessage>,

    /// Last processed timestamp to support incremental updates
    last_processed_at: Option<Timestamp>,

    /// Statistics for monitoring and debugging
    stats: GroupStatistics,

    /// Dirty flag to track when state needs persistence
    needs_persistence: bool,
}

/// Per-group aggregator state with isolation guarantees
struct AggregatorState {
    /// State per group, ensuring no cross-group contamination
    groups: HashMap<GroupId, GroupState>,

    /// Global configuration
    config: AggregatorConfig,

    /// Last cleanup timestamp
    last_cleanup: Option<SystemTime>,
}
```

### Incremental Updates with Group Isolation
- Process only new messages since last state update **per group**
- Update existing messages when reactions/deletions arrive **within the same group**
- Maintain indexes for fast lookups **scoped to each group**
- Handle state corruption gracefully **with group-level recovery**
- Validate message-group consistency before processing

### Group Isolation Guarantees
1. **Data Separation**: Messages from Group A cannot affect aggregated state of Group B
2. **State Independence**: Corruption in one group's state doesn't affect others
3. **Selective Persistence**: Can persist/load state for individual groups
4. **Memory Management**: Can clear state for specific groups (e.g., when leaving)
5. **Processing Isolation**: Reactions/deletions only affect messages within the same group

### Security & Privacy Benefits
- **Cross-group analysis prevention**: No ability to correlate messages across groups
- **Selective data removal**: Complete state cleanup when leaving a group
- **Leak prevention**: Programming errors can't mix data between groups
- **Audit capability**: Clear per-group processing logs and statistics

### Migration Strategy
- Design state format to be backward compatible **with group versioning**
- Implement state version management **per group**
- Provide fallback to stateless mode if group state is corrupted
- Add migration tools for upgrading state format **group by group**
- Support graceful degradation when only some groups have corrupted state

## Performance Considerations

### Optimization Targets
- **Goal**: Process typical group history (<1000 messages) in <1 second
- **Memory**: Minimize allocations during processing
- **CPU**: Efficient algorithms for reaction/reply lookups

### Potential Optimizations
- Use `FxHashMap` instead of `HashMap` for better performance
- Pre-allocate vectors with estimated capacity
- Implement custom emoji normalization with lookup tables
- Use string interning for common emoji strings
- Consider SIMD for emoji detection/normalization

### Monitoring
- Add metrics for processing time per message count
- Track unresolved message rates
- Monitor memory usage patterns
- Log performance warnings for slow operations

## Future Extensions

This architecture is designed to accommodate future features:

### Message Editing
- Add `edit_history` field to `ChatMessage`
- Process kind 16 (replaceable) events
- Update aggregated state when edits arrive

### Read Receipts
- Add `read_status` field to track which users have read each message
- Process read receipt events
- Update status in real-time

### Enhanced Threading
- While not implementing full threading initially, the reply structure supports it
- `reply_to_id` can be used to build thread trees in the frontend
- Consider adding `thread_root_id` field for complex threading

### Real-time Updates
- WebSocket integration for live message updates
- Incremental state updates for new messages
- Event-driven architecture for reactive UIs

## Implementation Timeline

### Phase 1: Core Implementation (Week 1-2)
1. Create module structure and basic types
2. Implement stateless processing algorithm
3. Add emoji validation and reaction handling
4. Write comprehensive unit tests

### Phase 2: Integration & Polish (Week 3)
1. Integrate with existing Whitenoise API
2. Add performance benchmarks
3. Write integration tests
4. Documentation and examples

### Phase 3: Optimization (Week 4+)
1. Performance profiling and optimization
2. Consider stateful implementation if needed
3. Add advanced error handling and resilience
4. Prepare for future feature extensions

## Risks & Mitigations

### Performance Risk
- **Risk**: Processing time grows quadratically with message count
- **Mitigation**: Implement efficient algorithms, add benchmarking, plan stateful migration

### Data Consistency Risk
- **Risk**: Out-of-order messages cause incorrect aggregation
- **Mitigation**: Robust retry logic, chronological processing, comprehensive testing

### Memory Usage Risk
- **Risk**: Large groups consume excessive memory
- **Mitigation**: Lazy loading, pagination support, memory profiling

### Compatibility Risk
- **Risk**: Future Nostr protocol changes break processing
- **Mitigation**: Flexible tag parsing, version detection, graceful degradation

## Success Metrics

- ✅ Process 1000 messages in <1 second
- ✅ Handle out-of-order delivery correctly in >99% of cases
- ✅ Support groups with 10K+ messages without performance degradation
- ✅ Zero data loss for properly formatted messages
- ✅ Graceful handling of malformed or invalid messages
- ✅ Memory usage scales linearly with message count
- ✅ Easy integration with existing codebase
- ✅ Comprehensive test coverage (>90%)
