# Whitenoise Examples

This directory contains examples demonstrating how to use the Whitenoise library.

## Examples

### `fetch_contacts_example.rs`

Demonstrates how to fetch your own contact list from the Nostr network.

**Features shown:**
- Initializing Whitenoise with real relay connections
- Logging in with your private key (nsec)
- Fetching contacts using `fetch_contacts()`
- Displaying contact metadata (names, pictures, NIP-05, etc.)
- Checking relay connection status

**Usage:**

```bash
# With your own private key
NOSTR_NSEC=nsec1your_private_key_here cargo run --example fetch_contacts_example

# Or run with demo account (no real data)
cargo run --example fetch_contacts_example
```

**What you'll see:**
- Your complete contact list with metadata
- Summary statistics (total contacts, verified profiles, etc.)
- Relay connection status
- API usage instructions

### `onboarding_workflow.rs`

Demonstrates the complete onboarding workflow for new accounts.

**Features shown:**
- Account creation and login
- Background data fetching
- Automatic onboarding completion
- Manual onboarding triggers

**Usage:**

```bash
cargo run --example onboarding_workflow
```

## Setting up your environment

1. Get your Nostr private key (nsec) from your Nostr client
2. Set it as an environment variable:
   ```bash
   export NOSTR_NSEC=nsec1your_private_key_here
   ```
3. Run any example that uses real data

## Common APIs demonstrated

- `whitenoise.login(nsec)` - Login with private key
- `whitenoise.fetch_contacts(pubkey)` - Get contact list
- `whitenoise.add_contact(account, contact_pubkey)` - Add a contact
- `whitenoise.remove_contact(account, contact_pubkey)` - Remove a contact
- `whitenoise.fetch_relay_status(pubkey)` - Check relay status
- `PublicKey::parse(npub)` - Convert npub to PublicKey
- `pubkey.to_bech32()` - Convert PublicKey to npub

## Security Notes

- Never hardcode private keys in source code
- Use environment variables for private keys
- The examples create temporary accounts when no real key is provided
- Real contact data only appears when using your actual private key

## Need help?

Check the API documentation in the source code for detailed information about each method.
