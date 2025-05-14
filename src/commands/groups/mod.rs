use nostr_mls::prelude::*;
use serde::Serialize;

use crate::nostr_manager::parser::SerializableToken;

mod create_group;
mod delete_message;
mod get_active_groups;
mod get_group;
mod get_group_admins;
mod get_group_and_messages;
mod get_group_members;
mod get_group_relays;
mod rotate_key_in_group;
mod send_mls_message;

pub use create_group::create_group;
pub use delete_message::delete_message;
pub use get_active_groups::get_active_groups;
pub use get_group::get_group;
pub use get_group_admins::get_group_admins;
pub use get_group_and_messages::get_group_and_messages;
pub use get_group_members::get_group_members;
pub use get_group_relays::get_group_relays;
pub use rotate_key_in_group::rotate_key_in_group;
pub use send_mls_message::send_mls_message;

#[derive(Debug, Clone, Serialize)]
pub struct GroupAndMessages {
    group: group_types::Group,
    messages: Vec<MessageWithTokens>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageWithTokens {
    message: message_types::Message,
    tokens: Vec<SerializableToken>,
}
