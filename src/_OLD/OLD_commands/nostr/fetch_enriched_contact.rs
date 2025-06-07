use crate::accounts::Account;
use crate::relays::RelayType;
use crate::types::EnrichedContact;
use nostr_sdk::prelude::*;


pub async fn fetch_enriched_contact(
    pubkey: String,
    update_account: bool,
) -> Result<EnrichedContact, String> {
    let pubkey = PublicKey::from_hex(&pubkey).map_err(|_| "Invalid pubkey".to_string())?;

    let metadata = wn
        .nostr
        .fetch_user_metadata(pubkey)
        .await
        .map_err(|_| "Failed to get metadata".to_string())?;
    let nostr_relays = wn
        .nostr
        .fetch_user_relays(pubkey)
        .await
        .map_err(|_| "Failed to get user relays".to_string())?;
    let inbox_relays = wn
        .nostr
        .fetch_user_inbox_relays(pubkey)
        .await
        .map_err(|_| "Failed to get inbox relays".to_string())?;
    let key_package_relays = wn
        .nostr
        .fetch_user_key_package_relays(pubkey)
        .await
        .map_err(|_| "Failed to get key package relays".to_string())?;
    let key_packages = wn
        .nostr
        .fetch_user_key_packages(pubkey)
        .await
        .map_err(|_| "Failed to get key packages".to_string())?;

    let enriched_contact = EnrichedContact {
        metadata: metadata.unwrap_or_default(),
        nip17: !inbox_relays.is_empty(),
        nip104: !key_packages.is_empty(),
        nostr_relays,
        inbox_relays,
        key_package_relays,
    };

    if update_account {
        let mut account = Account::find_by_pubkey(&pubkey)
            .await
            .map_err(|e| format!("Failed to find account: {}", e))?;

        account.metadata = enriched_contact.metadata.clone();
        account
            .update_relays(RelayType::Nostr, &enriched_contact.nostr_relays)
            .await
            .map_err(|e| format!("Failed to update relays: {}", e))?;
        account
            .update_relays(RelayType::Inbox, &enriched_contact.inbox_relays)
            .await
            .map_err(|e| format!("Failed to update relays: {}", e))?;
        account
            .update_relays(RelayType::KeyPackage, &enriched_contact.key_package_relays)
            .await
            .map_err(|e| format!("Failed to update relays: {}", e))?;
        account
            .save()
            .await
            .map_err(|e| format!("Failed to save account: {}", e))?;
    }

    Ok(enriched_contact)
}
