use tracing_subscriber::{filter::EnvFilter, fmt::Layer, prelude::*, registry::Registry};

// mod media;
mod nostr_manager;
mod types;
pub mod whitenoise;

// Re-export main types for library users
pub use crate::nostr_manager::parser::SerializableToken;
pub use crate::types::ImageType;
pub use crate::types::MessageWithTokens;
pub use crate::whitenoise::accounts::relays::RelayType;
pub use crate::whitenoise::accounts::{Account, AccountSettings, OnboardingState};
pub use crate::whitenoise::error::WhitenoiseError;
pub use crate::whitenoise::message_aggregator::{
    ChatMessage, EmojiReaction, ReactionSummary, UserReaction,
};
pub use nostr_mls::groups::NostrGroupConfigData;
pub use whitenoise::{Whitenoise, WhitenoiseConfig};

// Re-export nostr types with documentation
//
// Note: These types are re-exported from the `nostr` crate for convenience
// and to ensure version compatibility. Whitenoise is tested with nostr crate
// version as specified in Cargo.toml.
//
/// Nostr public key for user identification. Re-exported from [`nostr::PublicKey`](https://docs.rs/nostr/latest/nostr/key/public_key/struct.PublicKey.html).
#[doc(alias = "pubkey")]
#[doc(alias = "public_key")]
pub use nostr::PublicKey;

/// Nostr event containing signed data. Re-exported from [`nostr::Event`](https://docs.rs/nostr/latest/nostr/event/struct.Event.html).
pub use nostr::Event;

/// User profile metadata (name, bio, etc.). Re-exported from [`nostr::Metadata`](https://docs.rs/nostr/latest/nostr/nips/nip01/struct.Metadata.html).
#[doc(alias = "profile")]
pub use nostr::Metadata;

/// Nostr relay URL. Re-exported from [`nostr::RelayUrl`](https://docs.rs/nostr/latest/nostr/struct.RelayUrl.html).
pub use nostr::RelayUrl;
pub use nostr_sdk::RelayStatus;

/// Nostr event kind. Re-exported from [`nostr::Kind`](https://docs.rs/nostr/latest/nostr/struct.Kind.html).
pub use nostr::Kind;

/// Nostr event tag. Re-exported from [`nostr::Tag`](https://docs.rs/nostr/latest/nostr/event/tag/struct.Tag.html).
pub use nostr::Tag;

/// Nostr event tags. Re-exported from [`nostr::Tags`](https://docs.rs/nostr/latest/nostr/event/tag/list/struct.Tags.html).
pub use nostr::Tags;

// Nostr MLS Types
/// Nostr MLS Group. Re-exported from [`nostr_mls::group_types::Group`](https://docs.rs/nostr-mls/latest/nostr_mls/group_types/struct.Group.html)
pub use nostr_mls::prelude::group_types::{Group, GroupState, GroupType};

/// Nostr MLS Group ID. Re-exported from [`open_mls::group::GroupId`](https://latest.openmls.tech/doc/openmls/group/struct.GroupId.html)
pub use nostr_mls::prelude::GroupId;

/// Nostr MLS Message. Re-exported from [`nostr_mls::prelude::Message`](https://docs.rs/nostr-mls/latest/nostr_mls/prelude/struct.Message.html)
pub use nostr_mls::prelude::message_types::{Message, MessageState};

/// Nostr MLS Welcome. Re-exported from [`nostr_mls::prelude::Welcome`](https://docs.rs/nostr-mls/latest/nostr_mls/prelude/struct.Welcome.html)
pub use nostr_mls::prelude::welcome_types::{Welcome, WelcomeState};

fn init_tracing(logs_dir: &std::path::Path) {
    // Check if tracing has already been initialized
    if tracing::dispatcher::has_been_set() {
        // For tests or when tracing is already set up, just log to console
        tracing::debug!("Tracing already initialized, logging will use existing configuration");
        return;
    }

    // Create log file with a simple name
    let log_file_path = logs_dir.join("nrc.log");

    // Ensure the logs directory exists
    std::fs::create_dir_all(logs_dir).ok();

    // Open or create the log file
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .expect("Failed to open log file");

    let (non_blocking_file, _file_guard) = tracing_appender::non_blocking(file);
    let (non_blocking_stdout, _stdout_guard) = tracing_appender::non_blocking(std::io::stdout());

    // Keep guards alive for the lifetime of the program
    // We leak them intentionally since logging should last the entire program
    Box::leak(Box::new(_file_guard));
    Box::leak(Box::new(_stdout_guard));

    let stdout_layer = Layer::new()
        .with_writer(non_blocking_stdout)
        .with_ansi(true)
        .with_target(true);

    let file_layer = Layer::new()
        .with_writer(non_blocking_file)
        .with_ansi(false)
        .with_target(true);

    Registry::default()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(stdout_layer)
        .with(file_layer)
        .init();
}
