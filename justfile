# Publish a NIP-89 handler
publish-nip89:
    ./scripts/publish_nip89_handler.sh

# Run the development server
dev:
    RUST_LOG=info,whitenoise=debug,nostr_mls=debug bun tauri dev

# Run the development server on Android
dev-and:
    bun tauri android dev

# Build the android release APKs
build-apk:
    bun tauri android build --apk --split-per-abi

