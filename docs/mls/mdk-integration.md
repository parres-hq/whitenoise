# MDK Crate Integration

## Overview

Whitenoise uses the `mdk-core` crate for MLS protocol implementation. This document details the integration patterns and usage within our project.

## Dependencies

From `Cargo.toml`:
```toml
mdk-core = { version = "0.5.0", git="https://github.com/parres-hq/mdk" }
mdk-sqlite-storage = { version = "0.5.0", git="https://github.com/parres-hq/mdk" }
```

## Core Integration Points

### MDK Initialization

The `mdk` instance is created per account:

```rust
// From src/whitenoise/accounts.rs
let mdk = Account::create_mdk(account.pubkey, &self.config.data_dir)?;
```

### Message Processing

MLS messages are processed through the nostr-mls interface:

```rust
// From src/whitenoise/event_processor/event_handlers/handle_mls_message.rs
match mdk.process_message(&event) {
    Ok(result) => {
        // Handle successful processing
    }
    Err(e) => {
        // Handle MLS errors
        Err(WhitenoiseError::MdkCoreError(e))
    }
}
```

### Storage Backend

We use SQLite storage for MLS state persistence:
- Database location: `{data_dir}/mls/`
- Per-account storage isolation
- Integrated with main application database

### Key Package Management

Key packages are generated and managed through the mdk interface:

```rust
// Key package generation for group joining
let key_package = mdk.generate_key_package()?;
```

### Group Operations

Core group operations available:

1. **Create Group**: `mdk.create_group()`
2. **Join Group**: `mdk.join_group(welcome_message)`
3. **Add Member**: `mdk.add_member(key_package)`
4. **Remove Member**: `mdk.remove_member(member_id)`
5. **Send Message**: `mdk.send_message(content)`

### Error Handling

MLS errors are wrapped in our custom error type:

```rust
#[derive(Debug, thiserror::Error)]
pub enum WhitenoiseError {
    #[error("MDK error: {0}")]
    MdkCoreError(#[from] mdk_core::Error),
    // ... other error types
}
```

## Configuration

### MLS Provider Configuration

The mdk crate is configured with:
- Crypto provider: Default MLS crypto implementation
- Storage provider: SQLite backend
- Transport: Nostr event system
