import type {
    CachedMessage,
    ChatState,
    DeletionsMap,
    Message,
    MessagesMap,
    Reaction,
    ReactionSummary,
    ReactionsMap,
} from "$lib/types/chat";
import type { NostrMlsGroup, NostrMlsGroupWithRelays } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { derived, get, writable } from "svelte/store";
import { activeAccount } from "./accounts";

import { eventToDeletion } from "$lib/utils/deletion";
import { cachedMessageToMessage } from "$lib/utils/message";
import { eventToReaction } from "$lib/utils/reaction";

/**
 * Creates a chat store to manage messages, reactions, and deletions.
 * @returns {Object} A Svelte store wrapping ChatState with methods to interact with messages, reactions, and deletions
 */
export function createChatStore() {
    const messagesMap = writable<MessagesMap>(new Map());
    const reactionsMap = writable<ReactionsMap>(new Map());
    const deletionsMap = writable<DeletionsMap>(new Map());
    const currentPubkey = get(activeAccount)?.pubkey;

    const messages = derived(messagesMap, ($messagesMap) => {
        return Array.from($messagesMap.values()).sort((a, b) => a.createdAt - b.createdAt);
    });

    const { subscribe, update } = writable<ChatState>({
        messages: get(messages),
        handleCachedMessage,
        handleCachedMessages,
        clear,
        findMessage,
        findReaction,
        findReplyToMessage,
        isDeleted,
        getMessageReactionsSummary,
        hasReactions,
        clickReaction,
        deleteMessage,
        payLightningInvoice,
        isMessageDeletable,
        isMessageCopyable,
    });

    messages.subscribe((sorted) => {
        update((state) => ({
            ...state,
            messages: sorted,
        }));
    });

    const eventHandlers = {
        handleChatEvent: (cachedMessage: CachedMessage) => {
            const newMessage = cachedMessageToMessage(cachedMessage, currentPubkey);
            const messagesToUpdate = [newMessage];
            const replyToMessage = newMessage.replyToId
                ? findMessage(newMessage.replyToId)
                : undefined;

            if (replyToMessage?.lightningInvoice && newMessage.lightningPayment) {
                newMessage.lightningPayment.isPaid = true;
                replyToMessage.lightningInvoice.isPaid = true;
                messagesToUpdate.push(replyToMessage);
            }

            messagesMap.update((messages) => {
                for (const message of messagesToUpdate) {
                    messages.set(message.id, message);
                }
                return messages;
            });
        },
        handleDeletionEvent: (cachedMessage: CachedMessage) => {
            const deletion = eventToDeletion(cachedMessage.event);
            if (!deletion) return;
            deletionsMap.update((deletions) => {
                deletions.set(deletion.targetId, deletion);
                return deletions;
            });
        },
        handleReactionEvent: (cachedMessage: CachedMessage) => {
            const reaction = eventToReaction(cachedMessage.event, currentPubkey);
            if (!reaction) return;
            reactionsMap.update((reactions) => {
                reactions.set(reaction.id, reaction);
                return reactions;
            });

            const message = findMessage(reaction.targetId);
            if (!message) return;
            message.reactions = [...message.reactions, reaction];
            messagesMap.update((messages) => {
                messages.set(message.id, message);
                return messages;
            });
        },
    };

    const eventHandlerMap: Record<number, (cachedMessage: CachedMessage) => void> = {
        5: eventHandlers.handleDeletionEvent,
        7: eventHandlers.handleReactionEvent,
        9: eventHandlers.handleChatEvent,
    };

    /**
     * Deletes temporary events from the message and reaction maps
     */
    function deleteTempEvents() {
        messagesMap.update((messages) => {
            messages.delete("temp");
            return messages;
        });
        reactionsMap.update((reactions) => {
            reactions.delete("temp");
            return reactions;
        });
    }

    function handleCachedMessage(cachedMessage: CachedMessage, deleteTemp = true) {
        if (deleteTemp) deleteTempEvents();

        const handler = eventHandlerMap[cachedMessage.event_kind];
        if (handler) handler(cachedMessage);
    }

    /**
     * Handles multiple Nostr events and their tokens, sorting them by creation time and updating the chat store state
     * @param {EventAndTokens[]} eventsAndTokens - Array of Nostr events and tokens to handle
     */
    function handleCachedMessages(cachedMessages: CachedMessage[]) {
        deleteTempEvents();
        const sortedEvents = cachedMessages.sort((a, b) => a.created_at - b.created_at);
        for (const cachedMessage of sortedEvents) {
            handleCachedMessage(cachedMessage, false);
        }
    }

    /**
     * Clears all messages and deletions from the chatstore
     */
    function clear() {
        messagesMap.set(new Map());
        deletionsMap.set(new Map());
        reactionsMap.set(new Map());
    }

    /**
     * Finds a message by its ID
     * @param {string} id - The ID of the message to find
     * @returns {Message | undefined} The found message or undefined
     */
    function findMessage(id: string): Message | undefined {
        const messages = get(messagesMap);
        return messages.get(id);
    }
    /**
     * Finds a reaction by its ID
     * @param {string} id - The ID of the reaction to find
     * @returns {Reaction | undefined} The found reaction or undefined
     */
    function findReaction(id: string): Reaction | undefined {
        const reactions = get(reactionsMap);
        return reactions.get(id);
    }

    /**
     * Finds a user's reaction to a message with specific content
     * @param {Message} message - The message to search reactions for
     * @param {string} content - The reaction content to find
     * @returns {Reaction | undefined} The found reaction or undefined
     */
    function findMyMessageReaction(message: Message, content: string): Reaction | undefined {
        return message.reactions.find(
            (reaction) => reaction.content === content && reaction.isMine && !isDeleted(reaction.id)
        );
    }

    /**
     * Finds the message that a given message is replying to
     * @param {Message} message - The message to find the reply-to message for
     * @returns {Message | undefined} The message being replied to, or undefined
     */
    function findReplyToMessage(message: Message): Message | undefined {
        const replyToId = message.replyToId;
        if (replyToId) return findMessage(replyToId);
    }

    /**
     * Checks if an event has been deleted
     * @param {string} eventId - The ID of the event to check
     * @returns {boolean} True if the event has been deleted, false otherwise
     */
    function isDeleted(eventId: string): boolean {
        const deletions = get(deletionsMap);
        return deletions.has(eventId);
    }

    /**
     * Gets a summary of reactions for a message
     * @param {string} messageId - The ID of the message to get reactions for
     * @returns {ReactionSummary[]} Array of reaction summaries (emoji and count)
     */
    function getMessageReactionsSummary(messageId: string): ReactionSummary[] {
        const message = findMessage(messageId);
        const reactions = message?.reactions || [];
        const reactionsCounter: { [key: string]: number } = {};
        for (const reaction of reactions) {
            if (!isDeleted(reaction.id)) {
                reactionsCounter[reaction.content] = (reactionsCounter[reaction.content] || 0) + 1;
            }
        }
        return Object.entries(reactionsCounter).map(([emoji, count]) => ({ emoji, count }));
    }

    /**
     * Checks if a message has any reactions
     * @param {Message} message - The message to check for reactions
     * @returns {boolean} True if the message has reactions, false otherwise
     */
    function hasReactions(message: Message): boolean {
        const reactionsSummary = getMessageReactionsSummary(message.id);
        return reactionsSummary.length > 0;
    }

    /**
     * Checks if a message can be deleted by the current user
     * @param {string} messageId - The ID of the message to check
     * @returns {boolean} True if the message can be deleted, false otherwise
     */
    function isMessageDeletable(messageId: string): boolean {
        const message = findMessage(messageId);
        if (!message || message.lightningPayment || isDeleted(messageId)) return false;
        return message.isMine;
    }

    /**
     * Checks if a message content can be copied
     * @param {string} messageId - The ID of the message to check
     * @returns {boolean} True if the message can be copied, false otherwise
     */
    function isMessageCopyable(messageId: string): boolean {
        const message = findMessage(messageId);
        if (!message) return false;
        return !isDeleted(message.id);
    }

    /**
     * Adds a reaction to a message if current user hasn't reacted with same emoji, otherwise deletes the reaction
     * @param {NostrMlsGroup} group - The group the message belongs to
     * @param {string} content - The reaction content (emoji)
     * @param {string} messageId - The ID of the message to react to
     * @returns {Promise<CachedMessage | null>} The created event or null if operation failed
     */
    async function clickReaction(
        group: NostrMlsGroup,
        content: string,
        messageId: string
    ): Promise<CachedMessage | null> {
        const message = findMessage(messageId);
        if (!message) return null;

        const existingReaction = findMyMessageReaction(message, content);

        if (existingReaction) {
            return await deleteEvent(group, existingReaction.pubkey, existingReaction.id);
        }
        return await addReaction(group, message, content);
    }

    /**
     * Adds a reaction to a message
     * @param {NostrMlsGroup} group - The group the message belongs to
     * @param {Message} message - The message to react to
     * @param {string} content - The reaction content (emoji)
     * @returns {Promise<CachedMessage | null>} The created event or null if operation failed
     */
    async function addReaction(
        group: NostrMlsGroup,
        message: Message,
        content: string
    ): Promise<CachedMessage | null> {
        const tags = [
            ["e", message.id],
            ["p", message.pubkey],
        ];
        try {
            const reactionMessage = await invoke<CachedMessage>("send_mls_message", {
                group,
                message: content,
                kind: 7,
                tags,
            });
            handleCachedMessage(reactionMessage);
            return reactionMessage;
        } catch (error) {
            console.error("Error sending reaction:", error);
            return null;
        }
    }

    /**
     * Deletes a message
     * @param {NostrMlsGroup} group - The group the message belongs to
     * @param {string} messageId - The ID of the message to delete
     * @returns {Promise<CachedMessage | null>} The deletion event or null if deletion fails
     */
    async function deleteMessage(
        group: NostrMlsGroup,
        messageId: string
    ): Promise<CachedMessage | null> {
        const message = findMessage(messageId);
        if (!message) return null;

        return deleteEvent(group, message.pubkey, message.id);
    }

    /**
     * Deletes an event (message or reaction)
     * @param {NostrMlsGroup} group - The group the event belongs to
     * @param {string} pubkey - The public key of the event author
     * @param {string} eventId - The ID of the event to delete
     * @returns {Promise<CachedMessage | null>} The deletion event or null if deletion fails
     */
    async function deleteEvent(
        group: NostrMlsGroup,
        pubkey: string,
        eventId: string
    ): Promise<CachedMessage | null> {
        if (pubkey !== currentPubkey) return null;

        try {
            const deletionMessage = await invoke<CachedMessage>("delete_message", {
                group,
                messageId: eventId,
            });
            if (deletionMessage) {
                handleCachedMessage(deletionMessage);
            }
            return deletionMessage;
        } catch (error) {
            console.error("Error deleting message:", error);
            return null;
        }
    }

    /**
     * Pays a lightning invoice attached to a message
     * @param {NostrMlsGroupWithRelays} groupWithRelays - The group with relay information
     * @param {Message} message - The message with the lightning invoice to pay
     * @returns {Promise<CachedMessage | null>} The payment event or null if operation failed
     */
    async function payLightningInvoice(
        groupWithRelays: NostrMlsGroupWithRelays,
        message: Message
    ): Promise<CachedMessage | null> {
        if (!message.lightningInvoice) {
            console.error("Message does not have a lightning invoice");
            return null;
        }

        const tags = [["q", message.id, groupWithRelays.relays[0], message.pubkey]];

        const paymentMessage = await invoke<CachedMessage>("pay_invoice", {
            group: groupWithRelays.group,
            tags: tags,
            bolt11: message.lightningInvoice.invoice,
        });
        handleCachedMessage(paymentMessage);
        return paymentMessage;
    }

    return {
        subscribe,
        handleCachedMessage,
        handleCachedMessages,
        clear,
        findMessage,
        findReaction,
        findReplyToMessage,
        isDeleted,
        getMessageReactionsSummary,
        hasReactions,
        clickReaction,
        deleteMessage,
        payLightningInvoice,
        isMessageDeletable,
        isMessageCopyable,
    };
}
