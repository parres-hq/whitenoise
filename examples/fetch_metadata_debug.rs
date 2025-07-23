use nostr_sdk::prelude::*;
use std::path::PathBuf;
use whitenoise::{Account, Whitenoise, WhitenoiseConfig, WhitenoiseError};

/// Example demonstrating how to fetch and compare metadata for two specific npubs
///
/// This example helps debug issues where different npubs are returning the same metadata.
/// It fetches metadata for both provided npubs and performs detailed comparison of all fields.
///
/// To use this example:
/// 1. Make sure you have a .env file with NOSTR_NSEC=your_private_key for authentication
/// 2. Run: cargo run --example fetch_metadata_debug
/// 3. The example will show detailed comparison of metadata for both npubs
// Helper function to compare metadata and report differences
fn compare_metadata(
    npub1: &str,
    metadata1: &Option<Metadata>,
    npub2: &str,
    metadata2: &Option<Metadata>,
) -> (bool, Vec<String>) {
    let mut differences = Vec::new();
    let mut has_differences = false;

    match (metadata1, metadata2) {
        (Some(meta1), Some(meta2)) => {
            // Compare name
            if meta1.name != meta2.name {
                differences.push(format!(
                    "NAME differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.name, npub2, meta2.name
                ));
                has_differences = true;
            }

            // Compare display_name
            if meta1.display_name != meta2.display_name {
                differences.push(format!(
                    "DISPLAY_NAME differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.display_name, npub2, meta2.display_name
                ));
                has_differences = true;
            }

            // Compare about
            if meta1.about != meta2.about {
                let about1_short = meta1.about.as_ref().map(|s| {
                    if s.len() > 100 {
                        format!("{}...", &s[..100])
                    } else {
                        s.clone()
                    }
                });
                let about2_short = meta2.about.as_ref().map(|s| {
                    if s.len() > 100 {
                        format!("{}...", &s[..100])
                    } else {
                        s.clone()
                    }
                });
                differences.push(format!(
                    "ABOUT differs: {} = {:?}, {} = {:?}",
                    npub1, about1_short, npub2, about2_short
                ));
                has_differences = true;
            }

            // Compare picture
            if meta1.picture != meta2.picture {
                differences.push(format!(
                    "PICTURE differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.picture, npub2, meta2.picture
                ));
                has_differences = true;
            }

            // Compare banner
            if meta1.banner != meta2.banner {
                differences.push(format!(
                    "BANNER differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.banner, npub2, meta2.banner
                ));
                has_differences = true;
            }

            // Compare nip05
            if meta1.nip05 != meta2.nip05 {
                differences.push(format!(
                    "NIP05 differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.nip05, npub2, meta2.nip05
                ));
                has_differences = true;
            }

            // Compare lud16
            if meta1.lud16 != meta2.lud16 {
                differences.push(format!(
                    "LUD16 differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.lud16, npub2, meta2.lud16
                ));
                has_differences = true;
            }

            // Compare website
            if meta1.website != meta2.website {
                differences.push(format!(
                    "WEBSITE differs: {} = {:?}, {} = {:?}",
                    npub1, meta1.website, npub2, meta2.website
                ));
                has_differences = true;
            }

            // If all fields are identical
            if !has_differences {
                differences.push(
                    "⚠️  All metadata fields are IDENTICAL between the two npubs!".to_string(),
                );
                differences.push(
                    "   This suggests there might be a caching issue or data corruption."
                        .to_string(),
                );
            }
        }
        (Some(_), None) => {
            differences.push(format!(
                "AVAILABILITY: {} has metadata, {} has none",
                npub1, npub2
            ));
            has_differences = true;
        }
        (None, Some(_)) => {
            differences.push(format!(
                "AVAILABILITY: {} has metadata, {} has none",
                npub2, npub1
            ));
            has_differences = true;
        }
        (None, None) => {
            differences.push("Both npubs have NO metadata available".to_string());
        }
    }

    (has_differences, differences)
}

// Helper function to print detailed metadata
fn print_metadata_details(npub: &str, metadata: &Option<Metadata>) {
    match metadata {
        Some(meta) => {
            println!("📋 Metadata for {}:", npub);
            println!("   • Name: {:?}", meta.name);
            println!("   • Display Name: {:?}", meta.display_name);
            if let Some(about) = &meta.about {
                let about_short = if about.len() > 200 {
                    format!("{}...", &about[..200])
                } else {
                    about.clone()
                };
                println!("   • About: {:?}", about_short);
            } else {
                println!("   • About: None");
            }
            println!("   • Picture: {:?}", meta.picture);
            println!("   • Banner: {:?}", meta.banner);
            println!("   • NIP05: {:?}", meta.nip05);
            println!("   • LUD16: {:?}", meta.lud16);
            println!("   • Website: {:?}", meta.website);
        }
        None => {
            println!("📋 Metadata for {}: NONE", npub);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), WhitenoiseError> {
    // Delete the database
    let db_path = PathBuf::from("dev/data/examples/data/nostr_lmdb");
    if db_path.exists() {
        std::fs::remove_dir_all(db_path).unwrap();
    }

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

    // Get the private key for authentication
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
            println!("   • Or run: NOSTR_NSEC=nsec1... cargo run --example fetch_metadata_debug");

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

    // Wait for background processing to complete
    println!("\n⏳ Waiting 3 seconds for background processing to complete...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    println!("\n🔍 METADATA COMPARISON DEBUG TEST");
    println!("==================================");

    // The two npubs to test
    let npub1 = "npub1zymqqmvktw8lkr5dp6zzw5xk3fkdqcynj4l3f080k3amy28ses6setzznv";
    let npub2 = "npub1zzmxvr9sw49lhzfx236aweurt8h5tmzjw7x3gfsazlgd8j64ql0sexw5wy";

    println!("Testing npubs:");
    println!("   📍 NPub 1: {}", npub1);
    println!("   📍 NPub 2: {}", npub2);

    // Convert npubs to PublicKey objects
    let pubkey1 = match PublicKey::parse(npub1) {
        Ok(pk) => pk,
        Err(e) => {
            eprintln!("❌ Failed to parse npub1: {}", e);
            return Err(WhitenoiseError::InvalidPublicKey);
        }
    };

    let pubkey2 = match PublicKey::parse(npub2) {
        Ok(pk) => pk,
        Err(e) => {
            eprintln!("❌ Failed to parse npub2: {}", e);
            return Err(WhitenoiseError::InvalidPublicKey);
        }
    };

    // Verify they're different pubkeys
    if pubkey1 == pubkey2 {
        println!("❌ ERROR: Both npubs resolve to the same PublicKey!");
        println!("   This would explain why metadata is identical.");
        return Ok(());
    } else {
        println!("✅ Npubs resolve to different PublicKeys:");
        println!("   📍 PubKey 1: {}", pubkey1.to_hex());
        println!("   📍 PubKey 2: {}", pubkey2.to_hex());
    }

    let discovery_relays = Account::default_relays();

    println!("\n1️⃣  Fetching metadata for first npub...");
    let start_time = std::time::Instant::now();
    let metadata1 = whitenoise
        .fetch_metadata_from(discovery_relays.clone(), pubkey1)
        .await?;
    let duration1 = start_time.elapsed();
    println!("   ✅ Fetched in {:?}", duration1);

    println!("\n2️⃣  Fetching metadata for second npub...");
    let start_time = std::time::Instant::now();
    let metadata2 = whitenoise
        .fetch_metadata_from(discovery_relays.clone(), pubkey2)
        .await?;
    let duration2 = start_time.elapsed();
    println!("   ✅ Fetched in {:?}", duration2);

    // Print detailed metadata for both
    println!("\n📊 DETAILED METADATA COMPARISON:");
    println!("================================");

    print_metadata_details(npub1, &metadata1);
    println!();
    print_metadata_details(npub2, &metadata2);

    // Perform detailed comparison
    println!("\n🔬 COMPARISON ANALYSIS:");
    println!("======================");

    let (has_differences, differences) = compare_metadata(npub1, &metadata1, npub2, &metadata2);

    if has_differences {
        println!(
            "✅ GOOD! Found {} difference(s) between the metadata:",
            differences.len()
        );
        for (i, diff) in differences.iter().enumerate() {
            println!("   {}. {}", i + 1, diff);
        }
    } else {
        println!("❌ PROBLEM DETECTED!");
        for diff in differences {
            println!("   {}", diff);
        }
    }

    // Test cache consistency by fetching again
    println!("\n🔄 CACHE CONSISTENCY TEST:");
    println!("==========================");

    println!("Re-fetching both metadata to check for caching issues...");

    let metadata1_second = whitenoise
        .fetch_metadata_from(discovery_relays.clone(), pubkey1)
        .await?;
    let metadata2_second = whitenoise
        .fetch_metadata_from(discovery_relays.clone(), pubkey2)
        .await?;

    // Check if results are consistent between fetches
    let consistent1 = match (&metadata1, &metadata1_second) {
        (Some(m1), Some(m2)) => {
            m1.name == m2.name
                && m1.display_name == m2.display_name
                && m1.about == m2.about
                && m1.picture == m2.picture
        }
        (None, None) => true,
        _ => false,
    };

    let consistent2 = match (&metadata2, &metadata2_second) {
        (Some(m1), Some(m2)) => {
            m1.name == m2.name
                && m1.display_name == m2.display_name
                && m1.about == m2.about
                && m1.picture == m2.picture
        }
        (None, None) => true,
        _ => false,
    };

    if consistent1 && consistent2 {
        println!("✅ Cache consistency: Both npubs returned consistent metadata across fetches");
    } else {
        println!("❌ Cache inconsistency detected!");
        if !consistent1 {
            println!("   📍 {} metadata changed between fetches", npub1);
        }
        if !consistent2 {
            println!("   📍 {} metadata changed between fetches", npub2);
        }
    }

    println!("\n🎯 SUMMARY:");
    println!("===========");

    match (&metadata1, &metadata2) {
        (Some(_), Some(_)) => {
            if has_differences {
                println!("✅ SUCCESS: Both npubs have metadata and they are DIFFERENT");
                println!(
                    "   This is the expected behavior - each user should have unique metadata."
                );
            } else {
                println!("❌ ISSUE: Both npubs have metadata but it's IDENTICAL");
                println!("   This suggests a potential bug in metadata fetching or caching.");
                println!("   Possible causes:");
                println!("   • Cache key collision");
                println!("   • Database query returning wrong data");
                println!("   • Relay data corruption");
                println!("   • PublicKey parsing issue");
            }
        }
        (Some(_), None) => {
            println!("⚠️  PARTIAL: Only the first npub has metadata");
            println!("   This could be normal if the second user hasn't published metadata.");
        }
        (None, Some(_)) => {
            println!("⚠️  PARTIAL: Only the second npub has metadata");
            println!("   This could be normal if the first user hasn't published metadata.");
        }
        (None, None) => {
            println!("⚠️  NO DATA: Neither npub has metadata available");
            println!("   This could indicate:");
            println!("   • Users haven't published metadata");
            println!("   • Relay connectivity issues");
            println!("   • Database synchronization problems");
        }
    }

    println!("\n⚡ PERFORMANCE SUMMARY:");
    println!("   • First fetch: {:?}", duration1);
    println!("   • Second fetch: {:?}", duration2);

    if duration1.as_millis() > 1000 || duration2.as_millis() > 1000 {
        println!("   ⚠️  Some fetches took longer than 1 second - possible network issues");
    }

    Ok(())
}
