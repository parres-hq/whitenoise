import type { ChatMessage } from "$lib/types/chat";
import type { MediaAttachment } from "$lib/types/media";
import type { MessageWithTokens, SerializableToken } from "$lib/types/nostr";
import { eventToLightningInvoice, eventToLightningPayment } from "./lightning";
import { findMediaAttachments } from "./media";
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
 * Removes linebreaks and whitespace tokens from the end of a message.
 *
 * @param tokens - Array of SerializableToken to process
 * @returns Array of SerializableToken with trailing linebreaks and whitespace removed
 */
function removeTrailingWhitespace(tokens: SerializableToken[]): SerializableToken[] {
    let endIndex = tokens.length - 1;
    while (endIndex >= 0) {
        const token = tokens[endIndex];
        if ("LineBreak" in token || "Whitespace" in token) {
            endIndex--;
        } else {
            break;
        }
    }
    return tokens.slice(0, endIndex + 1);
}

/**
 * Converts a Message object to a Message object.
 *
 * @param message - The Message object to convert
 * @param currentPubkey - The current user's public key, used to determine if the message belongs to the current user
 * @returns A formatted Message object
 */
export function messageToChatMessage(
    messageAndTokens: MessageWithTokens,
    currentPubkey: string | undefined
): ChatMessage {
    const event = messageAndTokens.message.event;
    const replyToId = findReplyToId(event);
    const isMine = currentPubkey === event.pubkey;
    const lightningInvoice = eventToLightningInvoice(event);
    const lightningPayment = eventToLightningPayment(event);
    const content = contentToShow({ content: event.content, invoice: lightningInvoice?.invoice });
    const mediaAttachments: MediaAttachment[] = findMediaAttachments(messageAndTokens.message);
    const mediaAttachmentsUrls = new Set(mediaAttachments.map((attachment) => attachment.url));
    const tokens: SerializableToken[] = removeTrailingWhitespace(
        messageAndTokens.tokens.filter(
            (token: SerializableToken) => !mediaAttachmentsUrls.has(token.Url)
        )
    );

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
        mediaAttachments,
    };
}
