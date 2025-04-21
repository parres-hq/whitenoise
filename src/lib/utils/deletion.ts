import type { DeletionMessage, Message } from "$lib/types/chat";
import { findTargetId } from "./tags";

/**
 * Converts a Message to a DeletionMessage object.
 *
 * @param message - The Message to convert
 * @returns A DeletionMessage object or null if the message's event doesn't have a valid target ID
 */
export function messageToDeletionMessage(message: Message): DeletionMessage | null {
    const event = message.event;
    const targetId = findTargetId(event);
    if (!targetId) return null;

    return {
        id: event.id,
        pubkey: event.pubkey,
        targetId,
        event,
    };
}
