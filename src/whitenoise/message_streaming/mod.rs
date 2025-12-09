//! Message Streaming Module
//!
//! This module provides real-time message streaming capabilities for group chats.
//! It enables subscribers to receive live updates as messages, reactions, and
//! deletions are processed, without requiring polling.

mod types;

pub use types::{GroupMessageSubscription, MessageUpdate, UpdateTrigger};
