#!/bin/sh

# List of relays to publish to (space-separated)
RELAYS="wss://relay.damus.io wss://nos.lol wss://relay.nostr.band"

# Generate a random 32-byte hex string for the d tag
D_TAG=$(head -c 32 /dev/urandom | xxd -p -c 64)

# Prompt for secret key (never store it)
printf "Enter your Nostr secret key (hex, nsec, or ncryptsec): "
stty -echo
read SECRET_KEY
stty echo
echo

if [ -z "$SECRET_KEY" ]; then
  echo "Secret key is required. Aborting."
  exit 1
fi

nak event \
  --kind 31990 \
  --content '' \
  -d "$D_TAG" \
  -t k=443 \
  -t k=444 \
  -t k=445 \
  -t k=1059 \
  -t k=10050 \
  -t k=10051 \
  --prompt-sec \
  $RELAYS
