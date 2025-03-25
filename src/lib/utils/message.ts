import type { CachedMessage, Message } from "$lib/types/chat";
import type { NEvent, SerializableToken } from "$lib/types/nostr";
import { eventToLightningInvoice, eventToLightningPayment } from "./lightning";
import { findReplyToId } from "./tags";

/**
 * Determines if a string contains only a single emoji.
 *
 * @param str - The string to check
 * @returns True if the string contains only a single emoji, false otherwise
 */
function isSingleEmoji(str: string) {
    const trimmed = str.trim();
    const emojiRegex =
        /^(?:\p{Emoji_Presentation}|\p{Emoji}\uFE0F)\p{Emoji_Modifier}*(?:\u200D(?:\p{Emoji_Presentation}|\p{Emoji}\uFE0F)\p{Emoji_Modifier}*)*$/u;
    return emojiRegex.test(trimmed);
}

/**
 * Formats message content to hide full lightning invoices for display purposes.
 * If an invoice is present, it replaces the full invoice with a shortened version
 * showing only the first and last 15 characters.
 *
 * @param content - The message content
 * @param invoice - The lightning invoice string, if present
 * @returns Formatted content with shortened invoice (if applicable)
 */
function contentToShow({ content, invoice }: { content: string; invoice: string | undefined }) {
    if (!invoice) return content;
    const firstPart = invoice.substring(0, 15);
    const lastPart = invoice.substring(invoice.length - 15);
    return content.replace(invoice, `${firstPart}...${lastPart}`);
}

/**
 * Converts a Nostr event to a Message object.
 * Processes the event to extract relevant information and format it appropriately,
 * including handling lightning invoices/payments and emoji detection.
 *
 * @param event - The Nostr event to convert
 * @param currentPubkey - The current user's public key, used to determine if the message belongs to the current user
 * @returns A formatted Message object
 */
export function eventToMessage(event: NEvent, currentPubkey: string | undefined): Message {
    const replyToId = findReplyToId(event);
    const isMine = currentPubkey === event.pubkey;
    const lightningInvoice = eventToLightningInvoice(event);
    const lightningPayment = eventToLightningPayment(event);
    const content = contentToShow({ content: event.content, invoice: lightningInvoice?.invoice });
    const tokens: SerializableToken[] = [];

    return {
        id: event.id,
        pubkey: event.pubkey,
        content,
        createdAt: event.created_at,
        replyToId,
        reactions: [],
        lightningInvoice,
        isSingleEmoji: isSingleEmoji(content),
        lightningPayment,
        isMine,
        event,
        tokens,
    };
}

/**
 * Converts a CachedMessage object to a Message object.
 *
 * @param cachedMessage - The CachedMessage object to convert
 * @param currentPubkey - The current user's public key, used to determine if the message belongs to the current user
 * @returns A formatted Message object
 */
export function cachedMessageToMessage(
    cachedMessage: CachedMessage,
    currentPubkey: string | undefined
): Message {
    const message = eventToMessage(cachedMessage.event, currentPubkey);
    if (cachedMessage.tokens.length > 0) {
        message.tokens = cachedMessage.tokens;
    }
    return message;
}
