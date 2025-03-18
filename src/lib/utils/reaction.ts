import type { NEvent } from "$lib/types/nostr";
import type { Reaction } from '$lib/types/chat';
import { findTargetId } from './tags';

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
        event
    };
}
