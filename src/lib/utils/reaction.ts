import type { Reaction } from "$lib/types/chat";
import type { NEvent } from "$lib/types/nostr";
import { findTargetId } from "./tags";

/**
 * Converts a Nostr event to a Reaction object.
 *
 * @param event - The Nostr event to convert
 * @param currentPubkey - The current user's public key, used to determine if the reaction belongs to the current user
 * @returns A Reaction object or null if the event doesn't have a valid target ID
 */
export function eventToReaction(event: NEvent, currentPubkey: string | undefined): Reaction | null {
    const targetId = findTargetId(event);
    if (!targetId) return null;
    const isMine = currentPubkey === event.pubkey;

    return {
        id: event.id,
        pubkey: event.pubkey,
        content: event.content,
        createdAt: event.created_at,
        targetId,
        isMine,
        event,
    };
}
