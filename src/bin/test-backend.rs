use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use whitenoise::{Whitenoise, WhitenoiseConfig};

/// Test backend for Whitenoise
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Directory for application data
    #[clap(long, value_name = "PATH", required = true)]
    data_dir: PathBuf,

    /// Directory for application logs
    #[clap(long, value_name = "PATH", required = true)]
    logs_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let config = WhitenoiseConfig::new(&args.data_dir, &args.logs_dir);
    let mut whitenoise: Whitenoise = match Whitenoise::initialize_whitenoise(config).await {
        Ok(whitenoise) => whitenoise,
        Err(err) => {
            eprintln!("Failed to initialize Whitenoise: {}", err);
            std::process::exit(1);
        }
    };

    println!("INITIAL WHITENOISE: {:?}", whitenoise);
    assert_eq!(whitenoise.accounts.len(), 0);
    assert_eq!(whitenoise.active_account, None);


    let created_account = whitenoise.create_identity().await?;
    println!("CREATED ACCOUNT: {:?}", created_account);
    println!("WHITENOISE AFTER CREATING ACCOUNT: {:?}", whitenoise);

    assert_eq!(whitenoise.accounts.len(), 1);
    assert_eq!(whitenoise.active_account, Some(created_account.pubkey));

    // Create a second account - this should be set to the active account
    let created_account_2 = whitenoise.create_identity().await?;
    println!("CREATED ACCOUNT 2: {:?}", created_account_2);
    println!("WHITENOISE AFTER CREATING ACCOUNT 2: {:?}", whitenoise);

    assert_eq!(whitenoise.accounts.len(), 2);
    assert_eq!(whitenoise.active_account, Some(created_account_2.pubkey));

    // Logout the second account - this should set the first account to active
    whitenoise.logout(&created_account_2).await?;
    println!("WHITENOISE AFTER LOGGING OUT ACCOUNT 2: {:?}", whitenoise);

    assert_eq!(whitenoise.accounts.len(), 1);
    assert!(whitenoise.accounts.contains_key(&created_account.pubkey));
    assert!(!whitenoise.accounts.contains_key(&created_account_2.pubkey));
    assert_eq!(whitenoise.active_account, Some(created_account.pubkey));

    // TODO: Login with a known account

    // TODO: Logout one account

    Ok(())
}
