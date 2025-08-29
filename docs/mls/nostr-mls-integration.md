# nostr-mls Crate Integration

## Overview

Whitenoise uses the `nostr-mls` crate for MLS protocol implementation. This document details the integration patterns and usage within our project.

## Dependencies

From `Cargo.toml`:
```toml
nostr-mls = { version = "0.43.0", git="https://github.com/rust-nostr/nostr", rev = "84b1a016cffc30625567a03e2d3bcae86463f075" }
nostr-mls-sqlite-storage = { version = "0.43.0", git="https://github.com/rust-nostr/nostr", rev = "84b1a016cffc30625567a03e2d3bcae86463f075" }
```

## Core Integration Points

### NostrMls Initialization

The `nostr-mls` instance is created per account:

```rust
// From src/whitenoise/accounts.rs
let nostr_mls = Account::create_nostr_mls(account.pubkey, &self.config.data_dir)?;
```

### Message Processing

MLS messages are processed through the nostr-mls interface:

```rust
// From src/whitenoise/event_processor/event_handlers/handle_mls_message.rs
match nostr_mls.process_message(&event) {
    Ok(result) => {
        // Handle successful processing
    }
    Err(e) => {
        // Handle MLS errors
        Err(WhitenoiseError::NostrMlsError(e))
    }
}
```

### Storage Backend

We use SQLite storage for MLS state persistence:
- Database location: `{data_dir}/mls/`
- Per-account storage isolation
- Integrated with main application database

### Key Package Management

Key packages are generated and managed through the nostr-mls interface:

```rust
// Key package generation for group joining
let key_package = nostr_mls.generate_key_package()?;
```

### Group Operations

Core group operations available:

1. **Create Group**: `nostr_mls.create_group()`
2. **Join Group**: `nostr_mls.join_group(welcome_message)`
3. **Add Member**: `nostr_mls.add_member(key_package)`
4. **Remove Member**: `nostr_mls.remove_member(member_id)`
5. **Send Message**: `nostr_mls.send_message(content)`

### Error Handling

MLS errors are wrapped in our custom error type:

```rust
#[derive(Debug, thiserror::Error)]
pub enum WhitenoiseError {
    #[error("Nostr MLS error: {0}")]
    NostrMlsError(#[from] nostr_mls::Error),
    // ... other error types
}
```

## Configuration

### MLS Provider Configuration

The nostr-mls crate is configured with:
- Crypto provider: Default MLS crypto implementation
- Storage provider: SQLite backend
- Transport: Nostr event system
