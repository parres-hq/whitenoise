import type { Deletion } from "$lib/types/chat";
import type { NEvent } from "$lib/types/nostr";
import { findTargetId } from "./tags";

/**
 * Converts a Nostr event to a Deletion object.
 *
 * @param event - The Nostr event to convert
 * @returns A Deletion object or null if the event doesn't have a valid target ID
 */
export function eventToDeletion(event: NEvent): Deletion | null {
    const targetId = findTargetId(event);
    if (!targetId) return null;

    return {
        id: event.id,
        pubkey: event.pubkey,
        targetId,
        event,
    };
}
