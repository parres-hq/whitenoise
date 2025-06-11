use once_cell::sync::OnceCell;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::EnvFilter, fmt::Layer, prelude::*, registry::Registry};

use std::sync::Mutex;

mod accounts;
mod database;
mod error;
// mod media;
mod nostr_manager;
mod relays;
mod secrets_store;
mod types;
pub mod whitenoise;

// Re-export main types for library users
pub use accounts::{Account, AccountSettings, OnboardingState};
pub use error::WhitenoiseError;
pub use whitenoise::{Whitenoise, WhitenoiseConfig};

static TRACING_GUARDS: OnceCell<Mutex<Option<(WorkerGuard, WorkerGuard)>>> = OnceCell::new();
static TRACING_INIT: OnceCell<()> = OnceCell::new();

fn init_tracing(logs_dir: &std::path::Path) {
    TRACING_INIT.get_or_init(|| {
        let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("whitenoise")
            .filename_suffix("log")
            .build(logs_dir)
            .expect("Failed to create file appender");

        let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);
        let (non_blocking_stdout, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());

        TRACING_GUARDS
            .set(Mutex::new(Some((file_guard, stdout_guard))))
            .ok();

        let stdout_layer = Layer::new()
            .with_writer(non_blocking_stdout)
            .with_ansi(true)
            .with_target(true);

        let file_layer = Layer::new()
            .with_writer(non_blocking_file)
            .with_ansi(false)
            .with_target(true);

        Registry::default()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
            .with(stdout_layer)
            .with(file_layer)
            .init();
    });
}
