use nostr_mls::prelude::*;
use serde::Serialize;

use crate::nostr_manager::parser::SerializableToken;

/// Retry information for failed event processing
#[derive(Debug, Clone)]
pub struct RetryInfo {
    /// Number of times this event has been retried
    pub attempt: u32,
    /// Maximum number of retry attempts allowed
    pub max_attempts: u32,
    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,
}

impl RetryInfo {
    pub fn new() -> Self {
        Self {
            attempt: 0,
            max_attempts: 10,
            base_delay_ms: 1000,
        }
    }

    pub fn next_attempt(&self) -> Option<Self> {
        if self.attempt >= self.max_attempts {
            None
        } else {
            Some(Self {
                attempt: self.attempt + 1,
                max_attempts: self.max_attempts,
                base_delay_ms: self.base_delay_ms,
            })
        }
    }

    pub fn delay_ms(&self) -> u64 {
        self.base_delay_ms * (2_u64.pow(self.attempt))
    }

    pub fn should_retry(&self) -> bool {
        self.attempt < self.max_attempts
    }
}

impl Default for RetryInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Events that can be processed by the Whitenoise event processing system
#[derive(Debug)]
pub enum ProcessableEvent {
    /// A Nostr event with an optional subscription ID for account-aware processing
    NostrEvent {
        event: Event,
        subscription_id: Option<String>,
        retry_info: RetryInfo,
    },
    /// A relay message for logging/monitoring purposes
    RelayMessage(RelayUrl, String),
}

impl ProcessableEvent {
    /// Create a new NostrEvent with default retry settings
    pub fn new_nostr_event(event: Event, subscription_id: Option<String>) -> Self {
        Self::NostrEvent {
            event,
            subscription_id,
            retry_info: RetryInfo::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageWithTokens {
    pub message: message_types::Message,
    pub tokens: Vec<SerializableToken>,
}

impl MessageWithTokens {
    pub fn new(message: message_types::Message, tokens: Vec<SerializableToken>) -> Self {
        Self { message, tokens }
    }
}

pub enum ImageType {
    Jpg,
    Jpeg,
    Png,
    Gif,
    Webp,
}

impl ImageType {
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageType::Jpg => "image/jpg",
            ImageType::Jpeg => "image/jpeg",
            ImageType::Png => "image/png",
            ImageType::Gif => "image/gif",
            ImageType::Webp => "image/webp",
        }
    }
}

impl From<ImageType> for String {
    fn from(image_type: ImageType) -> Self {
        image_type.mime_type().to_string()
    }
}

impl TryFrom<String> for ImageType {
    type Error = anyhow::Error;

    fn try_from(mime_type: String) -> Result<Self, Self::Error> {
        match mime_type.as_str() {
            "image/jpg" => Ok(ImageType::Jpg),
            "image/jpeg" => Ok(ImageType::Jpeg),
            "image/png" => Ok(ImageType::Png),
            "image/gif" => Ok(ImageType::Gif),
            "image/webp" => Ok(ImageType::Webp),
            _ => Err(anyhow::anyhow!("Invalid image type: {}", mime_type)),
        }
    }
}
