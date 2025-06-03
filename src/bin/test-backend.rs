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
    let whitenoise: Whitenoise = match Whitenoise::initialize_whitenoise(config).await {
        Ok(whitenoise) => whitenoise,
        Err(err) => {
            eprintln!("Failed to initialize Whitenoise: {}", err);
            std::process::exit(1);
        }
    };

    println!("WHITENOISE: {:?}", whitenoise);

    let account = whitenoise.create_identity().await?;

    println!("ACCOUNT: {:?}", account);

    Ok(())
}
