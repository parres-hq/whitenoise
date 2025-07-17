#!/usr/bin/env bash
set -e

RELAY_URLS=("ws://localhost:8080" "ws://localhost:7777")
MAX_ATTEMPTS=20
WAIT_INTERVAL=0.5
CONNECTION_TIMEOUT=3

check_relay() {
    local relay_url=$1
    local attempts=0
    local connected=false

    echo "Testing relay: $relay_url"

    while [ $attempts -lt $MAX_ATTEMPTS ] && [ "$connected" = false ]; do
        attempts=$((attempts + 1))

                # Convert ws:// to http:// for curl test
        local http_url=${relay_url/ws:/http:}
        local websocket_key=$(openssl rand -base64 16)

        # Test WebSocket upgrade capability using curl
        # Look for "101 Switching Protocols" response which indicates successful WebSocket handshake
        if curl -s -v \
            -H "Connection: Upgrade" \
            -H "Upgrade: websocket" \
            -H "Sec-WebSocket-Key: $websocket_key" \
            -H "Sec-WebSocket-Version: 13" \
            --max-time $CONNECTION_TIMEOUT \
            "$http_url" 2>&1 | grep -q "HTTP/1.1 101 Switching Protocols"; then
            echo "✓ Relay $relay_url is ready (attempt $attempts)"
            connected=true
        else
            if [ $attempts -le 2 ] || [ $((attempts % 5)) -eq 0 ]; then
                echo "  Attempt $attempts/$MAX_ATTEMPTS: Relay $relay_url not ready"
            fi

            if [ $attempts -lt $MAX_ATTEMPTS ]; then
                sleep $WAIT_INTERVAL
            fi
        fi
    done

    if [ "$connected" = false ]; then
        echo "✗ Relay $relay_url failed to become ready after $MAX_ATTEMPTS attempts"
        return 1
    fi
    return 0
}

echo "Waiting for Nostr relays to be ready..."

# Start all relay checks concurrently
pids=()
for relay_url in "${RELAY_URLS[@]}"; do
    check_relay "$relay_url" &
    pids+=($!)
done

# Wait for all checks to complete and verify they all succeeded
failed=false
for pid in "${pids[@]}"; do
    if ! wait "$pid"; then
        failed=true
    fi
done

if [ "$failed" = true ]; then
    echo "✗ One or more relays failed to become ready"
    exit 1
fi

echo "✓ All relays are ready!"
