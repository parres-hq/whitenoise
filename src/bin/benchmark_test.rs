use std::path::PathBuf;

use clap::Parser;

use ::whitenoise::integration_tests::benchmarks::registry::BenchmarkRegistry;
use ::whitenoise::*;

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

    tracing::info!("=== Starting Whitenoise Performance Benchmark Suite ===");

    let config = WhitenoiseConfig::new(&args.data_dir, &args.logs_dir);
    if let Err(err) = Whitenoise::initialize_whitenoise(config).await {
        tracing::error!("Failed to initialize Whitenoise: {}", err);
        std::process::exit(1);
    }

    let whitenoise = Whitenoise::get_instance()?;

    BenchmarkRegistry::run_all_benchmarks(whitenoise).await?;

    tracing::info!("=== All Performance Benchmarks Completed Successfully ===");

    Ok(())
}
