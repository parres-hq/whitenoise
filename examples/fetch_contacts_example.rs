use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use whitenoise::{Whitenoise, WhitenoiseConfig, WhitenoiseError};

/// Example demonstrating how to compare query_contacts vs fetch_contacts for debugging
///
/// This example shows how to:
/// 1. Initialize Whitenoise with real relay connections
/// 2. Login with your private key (nsec)
/// 3. Wait for background contact fetching to complete
/// 4. Compare contacts from database (query_contacts) vs relays (fetch_contacts)
/// 5. Identify potential data synchronization issues
///
/// To use this example with real data:
/// 1. Create a .env file with NOSTR_NSEC=your_private_key
/// 2. Or set the NOSTR_NSEC environment variable directly
/// 3. Or replace the demo nsec in the code with your actual private key
// Compare contact data between two contact lists and report any discrepancies
fn compare_contact_data(
    query_contacts: &HashMap<PublicKey, Option<Metadata>>,
    fetch_contacts: &HashMap<PublicKey, Option<Metadata>>,
) -> (usize, usize, Vec<String>) {
    let mut mismatches = Vec::new();
    let mut total_compared = 0;
    let mut total_mismatches = 0;

    // Get all unique contact pubkeys from both lists
    let mut all_pubkeys = std::collections::HashSet::new();
    all_pubkeys.extend(query_contacts.keys());
    all_pubkeys.extend(fetch_contacts.keys());

    // Compare each contact
    for pubkey in all_pubkeys {
        total_compared += 1;
        let query_meta = query_contacts.get(pubkey);
        let fetch_meta = fetch_contacts.get(pubkey);

        let mut contact_mismatches = Vec::new();

        match (query_meta, fetch_meta) {
            (Some(Some(query_m)), Some(Some(fetch_m))) => {
                // Compare name
                if query_m.name != fetch_m.name {
                    contact_mismatches.push(format!(
                        "    NAME: query={:?} vs fetch={:?}",
                        query_m.name, fetch_m.name
                    ));
                }

                // Compare display_name
                if query_m.display_name != fetch_m.display_name {
                    contact_mismatches.push(format!(
                        "    DISPLAY_NAME: query={:?} vs fetch={:?}",
                        query_m.display_name, fetch_m.display_name
                    ));
                }

                // Compare about
                if query_m.about != fetch_m.about {
                    let query_about_short = query_m.about.as_ref().map(|s| {
                        if s.len() > 50 {
                            format!("{}...", &s[..50])
                        } else {
                            s.clone()
                        }
                    });
                    let fetch_about_short = fetch_m.about.as_ref().map(|s| {
                        if s.len() > 50 {
                            format!("{}...", &s[..50])
                        } else {
                            s.clone()
                        }
                    });
                    contact_mismatches.push(format!(
                        "    ABOUT: query={:?} vs fetch={:?}",
                        query_about_short, fetch_about_short
                    ));
                }

                // Compare picture
                if query_m.picture != fetch_m.picture {
                    contact_mismatches.push(format!(
                        "    PICTURE: query={:?} vs fetch={:?}",
                        query_m.picture, fetch_m.picture
                    ));
                }
            }
            (Some(Some(_query_m)), Some(None)) => {
                // contact_mismatches.push(format!(
                //     "    AVAILABILITY: query found metadata (name={:?}) but fetch returned None",
                //     query_m.name
                // ));
            }
            (Some(Some(_query_m)), None) => {
                // contact_mismatches.push(format!(
                //     "    CONTACT PRESENCE: query found contact with metadata (name={:?}) but fetch didn't find contact at all",
                //     query_m.name
                // ));
            }
            (Some(None), Some(Some(_fetch_m))) => {
                // contact_mismatches.push(format!(
                //     "    AVAILABILITY: fetch found metadata (name={:?}) but query returned None",
                //     fetch_m.name
                // ));
            }
            (None, Some(Some(_fetch_m))) => {
                // contact_mismatches.push(format!(
                //     "    CONTACT PRESENCE: fetch found contact with metadata (name={:?}) but query didn't find contact at all",
                //     fetch_m.name
                // ));
            }
            (Some(None), Some(None)) => {
                // Both found the contact but neither has metadata - this is fine
            }
            (Some(None), None) => {
                // contact_mismatches.push(format!(
                //     "    CONTACT PRESENCE: query found contact (no metadata) but fetch didn't find contact at all"
                // ));
            }
            (None, Some(None)) => {
                // contact_mismatches.push(format!(
                //     "    CONTACT PRESENCE: fetch found contact (no metadata) but query didn't find contact at all"
                // ));
            }
            (None, None) => {
                // Neither found the contact - this shouldn't happen since we're iterating over keys from both maps
                // unreachable!("Contact key came from one of the maps but not found in either");
            }
        }

        if !contact_mismatches.is_empty() {
            total_mismatches += 1;
            let npub = pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex());
            mismatches.push(format!(
                "❌ CONTACT DATA MISMATCH for {}:\n{}",
                npub,
                contact_mismatches.join("\n")
            ));
        }
    }

    (total_compared, total_mismatches, mismatches)
}

#[tokio::main]
async fn main() -> Result<(), WhitenoiseError> {
    // Load environment variables from .env file if it exists
    match dotenvy::dotenv() {
        Ok(_) => println!("🔧 Loaded environment variables from .env file"),
        Err(_) => println!("🔧 No .env file found, using system environment variables only"),
    }

    // Initialize Whitenoise with real configuration
    let config = WhitenoiseConfig::new(
        &PathBuf::from("dev/data/examples/data"),
        &PathBuf::from("dev/data/examples/logs"),
    );

    println!("🔧 Initializing Whitenoise...");
    Whitenoise::initialize_whitenoise(config).await?;
    let whitenoise = Whitenoise::get_instance()?;

    // Get the private key
    let nsec = match std::env::var("NOSTR_NSEC") {
        Ok(nsec) => {
            println!("🔑 Using private key from NOSTR_NSEC environment variable");
            nsec
        }
        Err(_) => {
            println!("⚠️  No NOSTR_NSEC environment variable found.");
            println!("   Creating a demo account instead.");
            println!("   To use your real account:");
            println!("   • Create a .env file with: NOSTR_NSEC=your_private_key");
            println!("   • Or set environment variable: NOSTR_NSEC=your_private_key");
            println!("   • Or run: NOSTR_NSEC=nsec1... cargo run --example fetch_contacts_example");

            // Generate a demo key for demonstration
            let demo_keys = Keys::generate();
            demo_keys.secret_key().to_secret_hex()
        }
    };

    println!("\n🔑 Logging in with private key...");
    let account = whitenoise.login(nsec).await?;

    println!("✅ Successfully logged in!");
    println!("   📊 Account pubkey: {}", account.pubkey.to_hex());
    println!(
        "   📡 Account npub: {}",
        account
            .pubkey
            .to_bech32()
            .unwrap_or_else(|_| "Invalid".to_string())
    );

    // Wait for background contact fetching to complete
    println!("\n⏳ Waiting 3 seconds for background contact fetching to complete...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    println!("\n🔍 CONTACT DATA CONSISTENCY TEST");
    println!("=================================");

    // Test Method 1: query_contacts (database/cache)
    println!("\n1️⃣  Fetching contacts from database using query_contacts...");
    let start_time = std::time::Instant::now();
    let query_contacts = whitenoise.query_contacts(account.pubkey).await?;
    let query_duration = start_time.elapsed();

    let query_with_metadata = query_contacts
        .values()
        .filter(|meta| meta.is_some())
        .count();
    println!(
        "   ✅ Query method: {}/{} contacts have metadata (took {:?})",
        query_with_metadata,
        query_contacts.len(),
        query_duration
    );

    // Test Method 2: fetch_contacts (relays)
    println!("\n2️⃣  Fetching contacts from relays using fetch_contacts...");
    let start_time = std::time::Instant::now();
    let fetch_contacts = whitenoise.fetch_contacts(&account).await?;
    let fetch_duration = start_time.elapsed();

    let fetch_with_metadata = fetch_contacts
        .values()
        .filter(|meta| meta.is_some())
        .count();
    println!(
        "   ✅ Fetch method: {}/{} contacts have metadata (took {:?})",
        fetch_with_metadata,
        fetch_contacts.len(),
        fetch_duration
    );

    // Compare results
    println!("\n3️⃣  Comparing results...");
    let (total_compared, total_mismatches, mismatches) =
        compare_contact_data(&query_contacts, &fetch_contacts);

    println!("📊 COMPARISON RESULTS:");
    println!("   • Total unique contacts: {}", total_compared);
    println!("   • Contacts with data mismatches: {}", total_mismatches);
    println!(
        "   • Query method found: {} total contacts, {} with metadata",
        query_contacts.len(),
        query_with_metadata
    );
    println!(
        "   • Fetch method found: {} total contacts, {} with metadata",
        fetch_contacts.len(),
        fetch_with_metadata
    );

    if total_mismatches == 0 {
        println!("✅ EXCELLENT! No contact data mismatches found between methods.");
        println!("   Both query_contacts and fetch_contacts returned consistent results.");
    } else {
        println!("⚠️  FOUND {} CONTACT DATA MISMATCHES!", total_mismatches);
        println!(
            "   This suggests potential synchronization issues between database and relay data."
        );

        // Show detailed mismatches (limit to first 10 to avoid overwhelming output)
        let show_count = std::cmp::min(mismatches.len(), 10);
        println!(
            "\n📋 DETAILED MISMATCHES (showing first {} of {}):",
            show_count,
            mismatches.len()
        );

        for (i, mismatch) in mismatches.iter().take(show_count).enumerate() {
            println!("\n{}) {}", i + 1, mismatch);
        }

        if mismatches.len() > show_count {
            println!(
                "\n... and {} more mismatches",
                mismatches.len() - show_count
            );
        }
    }

    println!("\n🎯 SUMMARY:");
    if total_mismatches == 0 {
        println!("✅ No contact data synchronization issues detected.");
        println!("   Database and relay data are consistent.");
    } else {
        println!(
            "❌ Found {} contact data synchronization issues.",
            total_mismatches
        );
        println!("   This suggests there may be a bug in contact data synchronization.");
        println!("   Consider investigating the contact fetching and caching implementation.");
    }

    // Performance comparison
    println!("\n⚡ PERFORMANCE COMPARISON:");
    println!("   • Query (database): {:?}", query_duration);
    println!("   • Fetch (relays): {:?}", fetch_duration);
    if query_duration < fetch_duration {
        println!(
            "   📈 Database query was {:.2}x faster than relay fetch",
            fetch_duration.as_secs_f64() / query_duration.as_secs_f64()
        );
    } else {
        println!(
            "   📈 Relay fetch was {:.2}x faster than database query",
            query_duration.as_secs_f64() / fetch_duration.as_secs_f64()
        );
    }

    Ok(())
}
