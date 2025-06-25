# Nostr Health Check Utility

This directory contains a Rust binary specifically designed to test the health of Nostr relays and Blossom servers during CI workflows.

## Purpose

The health check utility ensures that Docker Compose services are fully functional before running the main test suite, providing more reliable CI runs by validating actual Nostr protocol compliance rather than basic connectivity.

## What it tests

### Nostr Relays
- **WebSocket Connection**: Establishes proper WebSocket connections
- **Protocol Compliance**: Sends valid Nostr `REQ` messages per NIP-01
- **Response Validation**: Waits for proper `EOSE`, `EVENT`, or `NOTICE` responses
- **Clean Cleanup**: Sends `CLOSE` messages to properly terminate subscriptions

### Blossom Server
- **HTTP Endpoint**: Tests basic HTTP connectivity
- **Status Validation**: Accepts both success responses and 404 (normal for root)

## Usage

### In CI (Automated)
The health check runs automatically in GitHub Actions:
```bash
cd .github/workflows/cargo
timeout 60 cargo run --bin nostr_health_check
```

### Local Testing
You can run the health check locally to verify your development environment:

```bash
# Start your services first
docker-compose up -d

# Run the health check
cd .github/workflows/cargo
cargo run --bin nostr_health_check
```

## Output Examples

**✅ Successful Run:**
```
Testing Nostr relay: ws://localhost:8080
✓ ws://localhost:8080 is healthy
Testing Nostr relay: ws://localhost:7777  
✓ ws://localhost:7777 is healthy
Testing Blossom server: http://localhost:3000
✓ Blossom server is healthy
All services are healthy!
```

**❌ Failure Example:**
```
Testing Nostr relay: ws://localhost:8080
✗ ws://localhost:8080 failed health check: Connection refused
Some services failed health checks
```

## Configuration

The health check is configured to test:
- `ws://localhost:8080` (nostr-rs-relay)
- `ws://localhost:7777` (strfry-nostr-relay)  
- `http://localhost:3000` (blossom server)

To modify which services are tested, edit the `relays` vector in `src/main.rs`.

## Dependencies

- `tokio` - Async runtime
- `tokio-tungstenite` - WebSocket client
- `serde_json` - JSON message formatting
- `futures-util` - Stream utilities
- `reqwest` - HTTP client

## Timeout Behavior

- **Per-relay timeout**: 10 seconds
- **HTTP timeout**: 5 seconds  
- **Overall CI timeout**: 60 seconds (with fallback)
- **Message polling**: 100ms intervals

The health check is designed to be fast but thorough, providing quick feedback while ensuring actual protocol compliance.