use crate::WhitenoiseError;
use nostr_sdk::prelude::*;
use std::time::Duration;

pub async fn create_test_client(relays: &[&str], keys: Keys) -> Result<Client, WhitenoiseError> {
    let client = Client::default();
    for relay in relays {
        client.add_relay(*relay).await?;
    }

    client.connect().await;
    client.set_signer(keys).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    Ok(client)
}

pub async fn publish_test_metadata(
    client: &Client,
    name: &str,
    about: &str,
) -> Result<(), WhitenoiseError> {
    let metadata = Metadata {
        name: Some(name.to_string()),
        display_name: Some(name.to_string()),
        about: Some(about.to_string()),
        ..Default::default()
    };

    client
        .send_event_builder(EventBuilder::metadata(&metadata))
        .await?;
    Ok(())
}

pub async fn publish_relay_lists(
    client: &Client,
    relay_urls: Vec<String>,
) -> Result<(), WhitenoiseError> {
    let nip65_relay_tags: Vec<Tag> = relay_urls
        .iter()
        .map(|url| {
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::R)),
                [url.clone()],
            )
        })
        .collect();

    let relay_tags: Vec<Tag> = relay_urls
        .iter()
        .map(|url| Tag::custom(TagKind::Relay, [url.clone()]))
        .collect();

    client
        .send_event_builder(EventBuilder::new(Kind::RelayList, "").tags(nip65_relay_tags))
        .await?;
    client
        .send_event_builder(EventBuilder::new(Kind::InboxRelays, "").tags(relay_tags.clone()))
        .await?;
    client
        .send_event_builder(
            EventBuilder::new(Kind::MlsKeyPackageRelays, "").tags(relay_tags.clone()),
        )
        .await?;

    Ok(())
}
