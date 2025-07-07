use nostr_sdk::prelude::*;
use std::path::PathBuf;
use whitenoise::{Whitenoise, WhitenoiseConfig};

/// Example demonstrating the complete onboarding workflow for new accounts
///
/// This shows the recommended pattern for handling account login with
/// automatic background data fetching and onboarding completion.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Whitenoise
    let config = WhitenoiseConfig::new(
        &PathBuf::from("./example_data"),
        &PathBuf::from("./example_logs"),
    );
    Whitenoise::initialize_whitenoise(config).await?;
    let whitenoise = Whitenoise::get_instance()?;

    // Simulate logging in with an existing private key
    let test_keys = Keys::generate();
    let privkey = test_keys.secret_key().to_secret_hex();

    println!("🔑 Logging in account: {}", test_keys.public_key().to_hex());

    // Step 1: Login - this triggers background data fetch automatically
    let account = whitenoise.login(privkey).await?;
    println!("✅ Account logged in successfully");
    println!("📊 Initial onboarding state:");
    println!("   - Inbox relays: {}", account.onboarding.inbox_relays);
    println!(
        "   - Key package relays: {}",
        account.onboarding.key_package_relays
    );
    println!(
        "   - Key package published: {}",
        account.onboarding.key_package_published
    );

    // Step 2: The background fetch is already running automatically,
    // and will complete onboarding steps when done.
    println!("\n⏳ Background data fetch and onboarding completion is running...");

    // Wait a moment for background tasks to potentially complete
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Step 3: Check the final onboarding state
    println!("\n🔄 Checking final onboarding state...");
    let final_account = whitenoise.fetch_account(&account.pubkey).await?;
    println!("📊 Final onboarding state:");
    println!(
        "   - Inbox relays: {}",
        final_account.onboarding.inbox_relays
    );
    println!(
        "   - Key package relays: {}",
        final_account.onboarding.key_package_relays
    );
    println!(
        "   - Key package published: {}",
        final_account.onboarding.key_package_published
    );

    // Step 4: Manual onboarding completion (if needed)
    // You can also manually trigger onboarding completion at any time:
    println!("\n🔧 Manually completing any remaining onboarding steps...");
    let completion_state = whitenoise
        .complete_pending_onboarding_steps(&account.pubkey)
        .await?;
    println!("📊 After manual completion:");
    println!("   - Inbox relays: {}", completion_state.inbox_relays);
    println!(
        "   - Key package relays: {}",
        completion_state.key_package_relays
    );
    println!(
        "   - Key package published: {}",
        completion_state.key_package_published
    );

    // Step 5: Check if account is fully onboarded
    if completion_state.inbox_relays
        && completion_state.key_package_relays
        && completion_state.key_package_published
    {
        println!("\n🎉 Account is fully onboarded and ready to use!");
    } else {
        println!("\n⚠️  Account onboarding is incomplete. This may be due to:");
        if !completion_state.inbox_relays {
            println!("   - No inbox relays configured or available");
        }
        if !completion_state.key_package_relays {
            println!("   - No key package relays configured or available");
        }
        if !completion_state.key_package_published {
            println!("   - Key package publication failed (may require key package relays)");
        }
    }

    // Step 6: Demonstrate refreshing onboarding state
    println!("\n🔄 Refreshing onboarding state...");
    let mut refreshed_account = whitenoise.fetch_account(&account.pubkey).await?;
    whitenoise
        .refresh_account_onboarding_state(&mut refreshed_account)
        .await?;
    println!("✅ Onboarding state refreshed successfully");

    println!("\n✨ Onboarding workflow demonstration complete!");

    Ok(())
}
