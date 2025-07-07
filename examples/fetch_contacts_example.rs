use nostr_sdk::prelude::*;
use std::path::PathBuf;
use whitenoise::{Whitenoise, WhitenoiseConfig, WhitenoiseError};

/// Example demonstrating how to fetch your own contacts using the fetch_contacts method
///
/// This example shows how to:
/// 1. Initialize Whitenoise with real relay connections
/// 2. Login with your private key (nsec)
/// 3. Fetch your own contacts from the Nostr network
/// 4. Display the contact list with metadata
///
/// To use this example with real data:
/// 1. Replace the demo nsec with your actual private key
/// 2. Or set the NOSTR_NSEC environment variable
#[tokio::main]
async fn main() -> Result<(), WhitenoiseError> {
    // Initialize Whitenoise with real configuration
    let config = WhitenoiseConfig::new(
        &PathBuf::from("./example_data"),
        &PathBuf::from("./example_logs"),
    );

    println!("🔧 Initializing Whitenoise...");
    Whitenoise::initialize_whitenoise(config).await?;
    let whitenoise = Whitenoise::get_instance()?;

    // Get the private key - you can either:
    // 1. Set the NOSTR_NSEC environment variable with your nsec
    // 2. Or replace this with your actual nsec
    let nsec = match std::env::var("NOSTR_NSEC") {
        Ok(nsec) => {
            println!("🔑 Using private key from NOSTR_NSEC environment variable");
            nsec
        }
        Err(_) => {
            println!("⚠️  No NOSTR_NSEC environment variable found.");
            println!("   Creating a demo account instead.");
            println!("   To use your real account, set NOSTR_NSEC=your_private_key");
            println!("   Example: NOSTR_NSEC=nsec1... cargo run --example fetch_contacts_example");

            // Generate a demo key for demonstration
            let demo_keys = Keys::generate();
            demo_keys.secret_key().to_secret_hex()
        }
    };

    println!("\n🔑 Logging in with private key...");
    let account = whitenoise.login(nsec).await?;

    println!("✅ Successfully logged in!");
    println!("   📊 Account pubkey: {}", account.pubkey.to_hex());
    println!("   📡 Account npub: {}", account.pubkey.to_bech32().unwrap_or_else(|_| "Invalid".to_string()));

    // Wait a moment for relay connections to stabilize
    println!("\n⏳ Waiting for relay connections to stabilize...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Fetch contacts for the logged-in account
    println!("📡 Fetching contacts from Nostr relays...");

    match whitenoise.fetch_contacts(account.pubkey).await {
        Ok(contacts) => {
            println!("\n🎉 Successfully fetched contacts!");
            println!("📊 Total contacts found: {}", contacts.len());

            if contacts.is_empty() {
                println!("📝 No contacts found for this account.");
                println!("   This could mean:");
                println!("   • You haven't followed anyone yet (or haven't published a contact list)");
                println!("   • Your contact list isn't available on the connected relays");
                println!("   • This is a new account");

                println!("\n💡 How to add contacts:");
                println!("   • Use a Nostr client to follow other users");
                println!("   • Or use the Whitenoise API: whitenoise.add_contact(&account, contact_pubkey)");
            } else {
                println!("\n📋 Your Contact List:");
                println!("====================");

                for (i, (contact_pubkey, metadata)) in contacts.iter().enumerate() {
                    println!("\n👤 Contact #{}", i + 1);
                    println!("   📡 npub: {}", contact_pubkey.to_bech32().unwrap_or_else(|_| "Invalid".to_string()));
                    println!("   🔑 hex:  {}", contact_pubkey.to_hex());

                    match metadata {
                        Some(meta) => {
                            if let Some(name) = &meta.name {
                                println!("   📝 Name: {}", name);
                            }
                            if let Some(display_name) = &meta.display_name {
                                println!("   🏷️  Display Name: {}", display_name);
                            }
                            if let Some(about) = &meta.about {
                                let truncated_about = if about.len() > 100 {
                                    format!("{}...", &about[..100])
                                } else {
                                    about.clone()
                                };
                                println!("   ℹ️  About: {}", truncated_about);
                            }
                            if let Some(picture) = &meta.picture {
                                println!("   🖼️  Picture: {}", picture);
                            }
                            if let Some(nip05) = &meta.nip05 {
                                println!("   ✅ NIP-05: {}", nip05);
                            }
                            if let Some(website) = &meta.website {
                                println!("   🌐 Website: {}", website);
                            }
                        }
                        None => {
                            println!("   📝 No metadata available");
                        }
                    }
                }

                // Summary statistics
                let contacts_with_metadata = contacts.values().filter(|meta| meta.is_some()).count();
                let contacts_with_names = contacts.values()
                    .filter_map(|meta| meta.as_ref())
                    .filter(|meta| meta.name.is_some() || meta.display_name.is_some())
                    .count();
                let contacts_with_pictures = contacts.values()
                    .filter_map(|meta| meta.as_ref())
                    .filter(|meta| meta.picture.is_some())
                    .count();
                let contacts_with_nip05 = contacts.values()
                    .filter_map(|meta| meta.as_ref())
                    .filter(|meta| meta.nip05.is_some())
                    .count();

                println!("\n📈 Summary Statistics:");
                println!("   • Total contacts: {}", contacts.len());
                println!("   • Contacts with metadata: {}", contacts_with_metadata);
                println!("   • Contacts with names: {}", contacts_with_names);
                println!("   • Contacts with pictures: {}", contacts_with_pictures);
                println!("   • Contacts with NIP-05 verification: {}", contacts_with_nip05);
                println!("   • Contacts without metadata: {}", contacts.len() - contacts_with_metadata);
            }
        }
        Err(e) => {
            eprintln!("❌ Error fetching contacts: {}", e);
            eprintln!("\nPossible causes:");
            eprintln!("• Network connectivity issues");
            eprintln!("• Account hasn't published a contact list yet");
            eprintln!("• Relay connectivity problems");
            return Err(e);
        }
    }

    // Show which relays were used
    println!("\n🌐 Relay Information:");
    let relay_status = whitenoise.fetch_relay_status(account.pubkey).await?;
    if relay_status.is_empty() {
        println!("   ⚠️  No relays found for this account");
        println!("   📝 This is normal for accounts that haven't configured custom relays");
        println!("   📝 Whitenoise is using default relays for queries");
    } else {
        println!("   📡 Found {} relay(s) for this account:", relay_status.len());
        for (relay_url, status) in &relay_status {
            println!("      • {} - Status: {:?}", relay_url, status);
        }
    }

    // Show how to use the contact management APIs
    println!("\n📚 Contact Management APIs Available:");
    println!("====================================");
    println!("• fetch_contacts(pubkey) - Get all contacts for logged-in user");
    println!("• add_contact(account, contact_pubkey) - Add a new contact");
    println!("• remove_contact(account, contact_pubkey) - Remove a contact");
    println!("• update_contacts(account, contact_list) - Replace entire contact list");

    println!("\n📝 Return Type Details:");
    println!("• HashMap<PublicKey, Option<Metadata>>");
    println!("• PublicKey: The contact's public key (can convert to npub)");
    println!("• Metadata: name, display_name, about, picture, nip05, website, etc.");
    println!("• Option<Metadata>: Some(data) if metadata available, None if not");

    println!("\n💡 Pro Tips:");
    println!("• Export your npub: whitenoise.export_account_npub(account)");
    println!("• Export your nsec: whitenoise.export_account_nsec(account)");
    println!("• Convert pubkey to npub: pubkey.to_bech32()");
    println!("• Parse npub to pubkey: PublicKey::parse(npub)");

    println!("\n✨ Contact fetching example completed!");

    // Show usage instructions
    if std::env::var("NOSTR_NSEC").is_err() {
        println!("\n🔄 To use with your real account:");
        println!("   NOSTR_NSEC=your_nsec_here cargo run --example fetch_contacts_example");
        println!("   (Replace 'your_nsec_here' with your actual private key)");
    }

    Ok(())
}
