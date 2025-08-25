use nostr_sdk::prelude::*;

use crate::nostr_manager::NostrManager;

impl NostrManager {
    pub(crate) fn pubkeys_from_event(event: Event) -> Vec<PublicKey> {
        event
            .tags
            .iter()
            .filter(|tag| {
                tag.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P))
            })
            .map(|tag| tag.content().unwrap().parse::<PublicKey>().unwrap())
            .collect()
    }
}
