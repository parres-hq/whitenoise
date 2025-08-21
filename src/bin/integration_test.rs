#[path = "../integration_tests/mod.rs"]
mod integration_tests;

use ::whitenoise::*;
use anyhow::Result;
use clap::Parser;
use integration_tests::registry::ScenarioRegistry;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    #[clap(long, value_name = "PATH", required = true)]
    data_dir: PathBuf,

    #[clap(long, value_name = "PATH", required = true)]
    logs_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), WhitenoiseError> {
    let args = Args::parse();

    tracing::info!("=== Starting Whitenoise Integration Test Suite ===");

    let config = WhitenoiseConfig::new(&args.data_dir, &args.logs_dir);
    if let Err(err) = Whitenoise::initialize_whitenoise(config).await {
        tracing::error!("Failed to initialize Whitenoise: {}", err);
        std::process::exit(1);
    }

    let whitenoise = Whitenoise::get_instance()?;

    ScenarioRegistry::run_all_scenarios(whitenoise).await?;

    tracing::info!("=== All Integration Test Scenarios Completed Successfully ===");
    Ok(())
}
