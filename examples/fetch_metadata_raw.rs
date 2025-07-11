use nostr_sdk::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let db_path = &PathBuf::from("dev/data/examples/fetch_metadata_raw");

    // Delete the database
    if db_path.exists() {
        std::fs::remove_dir_all(db_path.clone()).unwrap();
    }

    let opts = ClientOptions::default();

    let full_path = db_path.join("nostr_lmdb");
    let db = NostrLMDB::builder(full_path)
        .map_size(1024 * 1024 * 512)
        .build()
        .expect("Failed to open Nostr database");
    let client = Client::builder().database(db).opts(opts).build();

    let relays = vec![
        "wss://relay.damus.io".to_string(),
        "wss://relay.primal.net".to_string(),
        "wss://nos.lol".to_string(),
    ];

    for relay in relays {
        client.add_relay(relay).await.expect("Error adding relay");
    }

    client.connect().await;

    let timeout = Duration::from_secs(3);
    let jeff = PublicKey::parse("npub1zuuajd7u3sx8xu92yav9jwxpr839cs0kc3q6t56vd5u9q033xmhsk6c2uc")
        .unwrap();
    let wn_support =
        PublicKey::parse("npub1zymqqmvktw8lkr5dp6zzw5xk3fkdqcynj4l3f080k3amy28ses6setzznv")
            .unwrap();
    let soapminer =
        PublicKey::parse("npub1zzmxvr9sw49lhzfx236aweurt8h5tmzjw7x3gfsazlgd8j64ql0sexw5wy")
            .unwrap();

    let jeff_contacts = client
        .fetch_events(
            Filter::new().author(jeff).kind(Kind::ContactList).limit(1),
            timeout,
        )
        .await
        .expect("Error fetching contacts")
        .first()
        .unwrap()
        .clone();

    let jeff_contacts_pubkeys = jeff_contacts
        .tags
        .iter()
        .filter(|tag| tag.kind() == TagKind::p())
        .filter_map(|tag| tag.content().map(|c| PublicKey::from_hex(c).unwrap()))
        .collect::<Vec<_>>();

    println!(
        "jeff_contacts_pubkeys length: {:?}",
        jeff_contacts_pubkeys.len()
    );

    let metadata_filter = Filter::new()
        .authors(jeff_contacts_pubkeys)
        .kind(Kind::Metadata);

    let _ = client
        .fetch_events(metadata_filter, timeout)
        .await
        .expect("Error fetching metadata");

    let wn_support_metadata = client
        .database()
        .query(Filter::new().author(wn_support).kind(Kind::Metadata))
        .await
        .expect("Error fetching metadata");
    let soapminer_metadata = client
        .database()
        .query(Filter::new().author(soapminer).kind(Kind::Metadata))
        .await
        .expect("Error fetching metadata");

    // Helper function to print metadata nicely
    fn print_metadata(name: &str, events: &Events) {
        if let Some(event) = events.first() {
            match Metadata::from_json(&event.content) {
                Ok(metadata) => {
                    println!("=== {} Metadata ===", name);
                    println!("Name: {:?}", metadata.name);
                    println!("Display Name: {:?}", metadata.display_name);
                    println!("About: {:?}", metadata.about);
                    println!("Picture: {:?}", metadata.picture);
                    println!("NIP-05: {:?}", metadata.nip05);
                    println!("Website: {:?}", metadata.website);
                    println!();
                }
                Err(e) => println!("Failed to parse {} metadata: {}", name, e),
            }
        } else {
            println!("No {} metadata found", name);
        }
    }

    print_metadata("WN Support", &wn_support_metadata);
    print_metadata("Soapminer", &soapminer_metadata);
}
