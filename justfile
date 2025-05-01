######################
# Development
######################

# Run development in tmux
dev-tmux:
    ./scripts/tmux_local_dev.sh

# Run the development server on your local machine
dev:
    RUST_LOG=info,whitenoise=debug,nostr_mls=debug bun tauri dev

# Run android dev in tmux
dev-and-tmux:
    ./scripts/tmux_and_dev.sh

# Run the development server on Android
dev-and:
    bun tauri android dev

# This will log the output from the android app to the console, includes all the Rust logs and the Tauri console (JS) logs
log-and:
    adb logcat | grep -E "RustStdoutStderr|Tauri\/Console|WebView"


# Stop any running tmux "whitenoise-dev" session
stop-tmux:
    ./scripts/tmux_stop_dev.sh


######################
# Build
######################

# Build the android release APKs
build-apk:
    bun tauri android build --apk --split-per-abi


######################
# Utilities
######################

# Publish a NIP-89 handler
publish-nip89:
    ./scripts/publish_nip89_handler.sh
