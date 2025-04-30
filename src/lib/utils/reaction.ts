import type { ReactionMessage } from "$lib/types/chat";
import type { MessageWithTokens } from "$lib/types/nostr";
import { findTargetId } from "./tags";

/**
 * Converts a Nostr event to a ReactionMessage object.
 *
 * @param event - The Nostr event to convert
 * @param currentPubkey - The current user's public key, used to determine if the reaction belongs to the current user
 * @returns A ReactionMessage object or null if the event doesn't have a valid target ID
 */
export function messageToReactionMessage(
    messageAndTokens: MessageWithTokens,
    currentPubkey: string | undefined
): ReactionMessage | null {
    const event = messageAndTokens.message.event;
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
