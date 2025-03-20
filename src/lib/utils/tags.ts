import type { NEvent } from "$lib/types/nostr";

/**
 * Finds the target event ID in a Nostr event's tags.
 * Looks for the first "e" tag
 *
 * @param event - The Nostr event to search for target ID
 * @returns The target event ID if found, undefined otherwise
 */
export function findTargetId(event: NEvent): string | undefined {
    return event.tags.find((t) => t[0] === "e")?.[1];
}

/**
 * Finds the bolt11 Lightning invoice tag in a Nostr event.
 * Used for identifying Lightning Network payment info.
 *
 * @param event - The Nostr event to search for bolt11 tag
 * @returns The bolt11 tag array if found, undefined otherwise
 */
export function findBolt11Tag(event: NEvent): string[] | undefined {
    return event.tags.find((t) => t[0] === "bolt11");
}

/**
 * Finds the payment preimage in a Nostr event's tags.
 * The preimage is used as proof of payment in Lightning Network transactions.
 *
 * @param event - The Nostr event to search for preimage
 * @returns The preimage string if found, undefined otherwise
 */
export function findPreimage(event: NEvent): string | undefined {
    return event.tags.find((t) => t[0] === "preimage")?.[1];
}

/**
 * Finds the reply-to event ID in a Nostr event's tags.
 * Looks for the first "q" tag
 *
 * @param event - The Nostr event to search for reply-to ID
 * @returns The ID of the event being replied to if found, undefined otherwise
 */
export function findReplyToId(event: NEvent): string | undefined {
    return event.tags.find((t) => t[0] === "q")?.[1];
}
