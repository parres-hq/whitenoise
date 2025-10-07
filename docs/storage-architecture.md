# Media Files Storage Architecture

## Overview

The media files system is organized into three distinct layers, each with a specific responsibility:

```
┌─────────────────────────────────────────────────────────────┐
│                    whitenoise/media_files                    │
│                   (High-level orchestration)                 │
│  - Coordinates between storage, database, and Blossom       │
│  - Handles encryption/decryption                            │
│  - Manages business logic                                   │
└────────────┬──────────────┬──────────────┬──────────────────┘
             │              │              │
             ▼              ▼              ▼
    ┌────────────┐  ┌──────────────┐  ┌──────────────┐
    │     DB     │  │      FS      │  │   Blossom    │
    └────────────┘  └──────────────┘  └──────────────┘
    database/       storage/          BlossomClient
    media_files     media_files       (nostr_sdk)
```

## Layer Responsibilities

### 1. `whitenoise/media_files.rs` (Orchestration Layer)
**Purpose:** High-level coordination between storage and database

**Responsibilities:**
- Coordinate between storage layer and database layer
- Provide convenience methods for common operations
- Combine filesystem and database operations atomically
- Business logic and validation

**Does NOT:**
- Touch the filesystem directly (delegates to storage layer)
- Execute SQL directly (delegates to database layer)
- Handle network operations (use BlossomClient)
- Handle encryption/decryption (MDK's responsibility)

**Key Types:**
- `MediaFiles<'a>` - Orchestrator with lifetime-bound references to storage and database

**Key Methods:**
- `store_and_record()` - Store file and record metadata in one operation
- `record_in_database()` - Record existing file metadata
- `find_file_with_prefix()` - Delegate to storage layer
- `get_file_path()` - Delegate to storage layer

### 2. `database/media_files.rs` (Left Branch)
**Purpose:** Database operations for media file metadata

**Responsibilities:**
- Store and retrieve media file records
- Track file metadata (hash, mime type, dimensions, blurhash)
- Update accessed_at timestamps
- Query files by group and hash

**Does NOT:**
- Touch the filesystem
- Download or upload files
- Handle encryption/decryption

**Key Types:**
- `MediaFile` - Database record representation
- `MediaFileParams` - Parameters for saving records
- `MediaFileRow` - Internal database row type

### 3. `storage/media_files.rs` (Middle Branch)
**Purpose:** Filesystem operations for media files

**Responsibilities:**
- Create directory structures (`<cache_dir>/<group_id_hex>/<subdirectory>/`)
- Store files atomically (write to `.tmp`, then rename)
- Retrieve file paths
- Check file existence
- Find files by prefix (useful when extension is unknown)

**Does NOT:**
- Interact with the database
- Download or upload files
- Handle encryption/decryption
- Know about Blossom URLs

**Key Types:**
- `MediaFileStorage` - Filesystem storage manager
- `Storage` - Wrapper struct for all storage subsystems

**Directory Structure:**
```
<data_dir>/media_cache/
  <mls_group_id_hex>/
    group_images/          # Group profile images (marmot extension)
      <hash>.<ext>
    media/                 # Chat media files
      <hash>.<ext>
```

### 4. `BlossomClient` (Right Branch)
**Purpose:** Network operations with Blossom servers

**Provided by:** `nostr_sdk` crate

**Responsibilities:**
- Upload blobs to Blossom servers
- Download blobs from Blossom servers
- Handle authentication with Nostr keys

**Does NOT:**
- Touch the filesystem directly
- Manage database records
- Handle caching

## Usage Patterns

### Group Images (Marmot Data Extension)

Group images are part of the Marmot protocol's data extension and are stored in the `group_images` subdirectory.

**Using the orchestration layer (recommended):**
```rust
// Store and record in one operation
let path = whitenoise
    .media_files()
    .store_and_record(
        &account_pubkey,
        &group_id,
        "group_images",
        "abc123.jpg",
        &decrypted_data,
        &hash,
        "image/jpeg",
        "group_image",
        Some("https://..."),
        None,
        None,
    )
    .await?;
```

**Using layers directly (for more control):**
```rust
// Store file to filesystem
let path = whitenoise.storage.media_files
    .store_file(&group_id, "group_images", "abc123.jpg", &decrypted_data)
    .await?;

// Record in database
use whitenoise::database::media_files::{MediaFile, MediaFileParams};
MediaFile::save(
    &whitenoise.database,
    &group_id,
    &account_pubkey,
    MediaFileParams {
        file_path: &path,
        file_hash: &hash,
        mime_type: "image/jpeg",
        media_type: "group_image",
        blossom_url: Some("https://..."),
        dimensions: None,
        blurhash: None,
    },
).await?;
```

### Chat Media Files

Regular chat media (images, videos, etc.) would be stored in the `media` subdirectory.

**Using the orchestration layer (recommended):**
```rust
// Store and record in one operation
let path = whitenoise
    .media_files()
    .store_and_record(
        &account_pubkey,
        &group_id,
        "media",
        "xyz789.mp4",
        &decrypted_data,
        &hash,
        "video/mp4",
        "chat_media",
        Some("https://..."),
        Some("1920x1080"),
        Some("LKO2?U%2Tw=w]~RBVZRi};RPxuwH"),
    )
    .await?;
```
