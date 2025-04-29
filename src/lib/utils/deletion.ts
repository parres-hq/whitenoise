import type { DeletionMessage } from "$lib/types/chat";
import type { MessageWithTokens } from "$lib/types/nostr";
import { findTargetId } from "./tags";

/**
 * Converts a Message to a DeletionMessage object.
 *
 * @param message - The Message to convert
 * @returns A DeletionMessage object or null if the message's event doesn't have a valid target ID
 */
export function messageToDeletionMessage(
    messageAndTokens: MessageWithTokens
): DeletionMessage | null {
    const event = messageAndTokens.message.event;
    const targetId = findTargetId(event);
    if (!targetId) return null;

    return {
        id: event.id,
        pubkey: event.pubkey,
        targetId,
        event,
    };
}
