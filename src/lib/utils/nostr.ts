import { activeAccount } from "$lib/stores/accounts";
import { invoke } from "@tauri-apps/api/core";
import { decode as nip19Decode, npubEncode } from "nostr-tools/nip19";
import { get } from "svelte/store";
import type { EnrichedContact, NEvent, NMetadata } from "../types/nostr";

/**
 * Retrieves the display name from the given NMetadata object.
 *
 * @param metadata - The NMetadata object containing user information.
 * @param pubkey - Optional public key to use as fallback for display name.
 * @param truncate - Whether to truncate the npub if used as fallback (default: true).
 * @returns The display name in the following priority order:
 *          1. display_name
 *          2. name
 *          3. truncated npub of the pubkey (if available)
 */
export function nameFromMetadata(metadata: NMetadata, pubkey?: string, truncate = true): string {
    return (
        metadata.display_name ||
        metadata.name ||
        (pubkey ? (truncate ? truncatedNpub(pubkey) : npubFromPubkey(pubkey)) : "")
    ).trim();
}

/**
 * Converts a public key to its npub (Nostr public key) representation.
 * @param pubkey - The public key to convert.
 * @returns The npub representation of the public key.
 */
export function npubFromPubkey(pubkey: string): string {
    return npubEncode(pubkey);
}

/**
 * Generates a truncated npub from a public key.
 * @param pubkey - The public key to convert and truncate.
 * @param length - The desired length of the truncated npub. Defaults to 20.
 * @returns A truncated npub representation of the public key.
 */
export function truncatedNpub(pubkey: string, length = 20): string {
    return `${npubFromPubkey(pubkey).slice(0, length)}...`;
}

/**
 * Checks if a Nostr event is considered insecure from a messaging standpoint.
 *
 * @param event - The Nostr event to check.
 * @returns True if the event is considered insecure, false otherwise.
 *
 * @remarks
 * This function considers events with kinds 4 and 14 as insecure.
 * Kind 4 typically represents encrypted direct messages, which leak metadata.
 * kind 14 is often used for encrypted and gift-wrapped direct messages, which have no
 * PCS or forward secrecy.
 */
export function isInsecure(event: NEvent): boolean {
    const insecureKinds = [4, 14];
    return insecureKinds.includes(event.kind);
}

/**
 * Validates if a given URL is a valid WebSocket URL.
 *
 * @param url - The URL string to validate
 * @returns True if the URL is a valid WebSocket URL (ws: or wss: protocol), false otherwise
 *
 * @remarks
 * This function checks if the URL:
 * - Is a valid URL format
 * - Uses either the 'ws:' or 'wss:' protocol
 * Returns false for any invalid URL format or non-WebSocket protocols
 */
export function isValidWebSocketURL(url: string): boolean {
    try {
        const wsURL = new URL(url);
        return wsURL.protocol === "ws:" || wsURL.protocol === "wss:";
    } catch {
        return false;
    }
}

/**
 * Generates a preview of the latest message in a chat conversation.
 *
 * @param messageId - The ID of the message to preview. If undefined, returns "New chat"
 * @returns A formatted string containing the message preview in the format:
 *         - "New chat" if no messageId is provided
 *         - "You: [content]" if the message is from the current user
 *         - "[sender name]: [content]" if the message is from another user
 *         - Empty string if no message is found
 *
 * @remarks
 * This function:
 * - Queries the message using the provided messageId
 * - Fetches the sender's metadata if the message is from another user
 * - Uses the nameFromMetadata function to format the sender's name
 * - Handles cases where the message or sender information is not available
 */
export async function latestMessagePreview(messageId: string | undefined): Promise<string> {
    if (!messageId) {
        return "New chat";
    }

    const event = (await invoke("query_message", { messageId })) as NEvent;
    if (!event) {
        return "";
    }

    if (event.pubkey === get(activeAccount)?.pubkey) {
        return `You: ${event.content}`;
    }

    const user: EnrichedContact = await invoke("query_enriched_contact", {
        pubkey: event.pubkey,
        updateAccount: false,
    });
    const otherAuthorMetadata = user.metadata;
    return `${nameFromMetadata(otherAuthorMetadata)}: ${event.content}`;
}

/**
 * Checks if a string is a valid npub (Nostr public key in bech32 format).
 * @param str - The string to check.
 * @returns True if the string is a valid npub, false otherwise.
 */
export function isValidNpub(str: string): boolean {
    return /^npub1[a-z\d]{58}$/.test(str || "");
}

/**
 * Checks if a string is a valid hex key. (public or private)
 * @param str - The string to check.
 * @returns True if the string is a valid hex key, false otherwise.
 */
export function isValidHexKey(str: string): boolean {
    // Hex key should be 64 characters long and contain only hex characters
    return /^[0-9a-f]{64}$/i.test(str);
}

/**
 * Checks if a string is a valid nsec (Nostr secret key in bech32 format).
 * @param str - The string to check.
 * @returns True if the string is a valid nsec, false otherwise.
 */
export function isValidNsec(str: string): boolean {
    return /^nsec1[a-z\d]{58}$/.test(str || "");
}

/**
 * Converts an npub to its hex public key representation.
 * @param npub - The npub to convert.
 * @returns The hex public key.
 * @throws Error if the npub is invalid.
 */
export function hexKeyFromNpub(npub: string): string {
    const decoded = nip19Decode(npub);
    if (!decoded || decoded.type !== "npub") {
        throw new Error("Invalid npub");
    }
    return decoded.data;
}
