import type {
    ChatMessage,
    ChatMessagesMap,
    ChatState,
    DeletionMessagesMap,
    Message,
    ReactionMessage,
    ReactionMessagesMap,
    ReactionSummary,
} from "$lib/types/chat";
import type { MessageWithTokens, NGroup } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { derived, get, writable } from "svelte/store";
import { activeAccount } from "./accounts";

import { messageToDeletionMessage } from "$lib/utils/deletion";
import { messageToChatMessage } from "$lib/utils/message";
import { messageToReactionMessage } from "$lib/utils/reaction";

/**
 * Creates a chat store to manage chat messages, reaction messages, and deletion messages.
 * @returns {Object} A Svelte store wrapping ChatState with methods to interact with messages, reactionMessages, and deletions
 */
export function createChatStore() {
    const chatMessagesMap = writable<ChatMessagesMap>(new Map());
    const reactionMessagesMap = writable<ReactionMessagesMap>(new Map());
    const deletionMessagesMap = writable<DeletionMessagesMap>(new Map());
    const currentPubkey = get(activeAccount)?.pubkey;

    const chatMessages = derived(chatMessagesMap, ($chatMessagesMap) => {
        return Array.from($chatMessagesMap.values()).sort((a, b) => a.createdAt - b.createdAt);
    });

    const { subscribe, update } = writable<ChatState>({
        chatMessages: get(chatMessages),
        handleMessage,
        handleMessages,
        clear,
        findChatMessage,
        findReactionMessage,
        findReplyToChatMessage,
        isDeleted,
        getMessageReactionsSummary,
        hasReactions,
        clickReaction,
        deleteMessage,
        payLightningInvoice,
        isMessageDeletable,
        isMessageCopyable,
    });

    chatMessages.subscribe((sortedChatMessages) => {
        update((state) => ({
            ...state,
            chatMessages: sortedChatMessages,
        }));
    });

    const eventHandlers = {
        handleChatMessage: (messageAndTokens: MessageWithTokens) => {
            const newMessage = messageToChatMessage(messageAndTokens, currentPubkey);
            const messagesToUpdate = [newMessage];
            const replyToMessage = newMessage.replyToId
                ? findChatMessage(newMessage.replyToId)
                : undefined;

            if (replyToMessage?.lightningInvoice && newMessage.lightningPayment) {
                newMessage.lightningPayment.isPaid = true;
                replyToMessage.lightningInvoice.isPaid = true;
                messagesToUpdate.push(replyToMessage);
            }

            chatMessagesMap.update((chatMessages) => {
                for (const message of messagesToUpdate) {
                    chatMessages.set(message.id, message);
                }
                return chatMessages;
            });
        },
        handleDeletionMessage: (messageAndTokens: MessageWithTokens) => {
            const deletionMessage = messageToDeletionMessage(messageAndTokens);
            if (!deletionMessage) return;
            deletionMessagesMap.update((deletionMessages) => {
                deletionMessages.set(deletionMessage.targetId, deletionMessage);
                return deletionMessages;
            });
        },
        handleReactionMessage: (messageAndTokens: MessageWithTokens) => {
            const reactionMessage = messageToReactionMessage(messageAndTokens, currentPubkey);
            if (!reactionMessage) return;
            reactionMessagesMap.update((reactionMessages) => {
                reactionMessages.set(reactionMessage.id, reactionMessage);
                return reactionMessages;
            });

            const chatMessage = findChatMessage(reactionMessage.targetId);
            if (!chatMessage) return;
            chatMessage.reactions = [...chatMessage.reactions, reactionMessage];
            chatMessagesMap.update((chatMessages) => {
                chatMessages.set(chatMessage.id, chatMessage);
                return chatMessages;
            });
        },
    };

    const messageHandlerMap: Record<number, (messageAndTokens: MessageWithTokens) => void> = {
        5: eventHandlers.handleDeletionMessage,
        7: eventHandlers.handleReactionMessage,
        9: eventHandlers.handleChatMessage,
    };

    /**
     * Deletes temporary messages from the chat messages and reaction messages maps
     */
    function deleteTempMessages() {
        chatMessagesMap.update((chatMessages) => {
            chatMessages.delete("temp");
            return chatMessages;
        });
        reactionMessagesMap.update((reactionMessages) => {
            reactionMessages.delete("temp");
            return reactionMessages;
        });
    }

    function handleMessage(messageAndTokens: MessageWithTokens, deleteTemp = true) {
        if (deleteTemp) deleteTempMessages();

        const handler = messageHandlerMap[messageAndTokens.message.kind];
        if (handler) handler(messageAndTokens);
    }

    /**
     * Handles multiple Nostr events and their tokens, sorting them by creation time and updating the chat store state
     * @param {EventAndTokens[]} eventsAndTokens - Array of Nostr events and tokens to handle
     */
    function handleMessages(messagesAndTokens: MessageWithTokens[]) {
        deleteTempMessages();
        const sortedEvents = messagesAndTokens.sort(
            (a, b) => a.message.created_at - b.message.created_at
        );
        for (const message of sortedEvents) {
            handleMessage(message, false);
        }
    }

    /**
     * Clears all messages and deletions from the chatstore
     */
    function clear() {
        chatMessagesMap.set(new Map());
        deletionMessagesMap.set(new Map());
        reactionMessagesMap.set(new Map());
    }

    /**
     * Finds a message by its ID
     * @param {string} id - The ID of the message to find
     * @returns {ChatMessage | undefined} The found message or undefined
     */
    function findChatMessage(id: string): ChatMessage | undefined {
        const chatMessages = get(chatMessagesMap);
        return chatMessages.get(id);
    }
    /**
     * Finds a reaction by its ID
     * @param {string} id - The ID of the reaction to find
     * @returns {ReactionMessage | undefined} The found reaction message or undefined
     */
    function findReactionMessage(id: string): ReactionMessage | undefined {
        const reactionMessages = get(reactionMessagesMap);
        return reactionMessages.get(id);
    }

    /**
     * Finds a user's reaction to a message with specific content
     * @param {ChatMessage} message - The message to search reactionMessages for
     * @param {string} content - The reaction content to find
     * @returns {ReactionMessage | undefined} The found reaction message or undefined
     */
    function findMyMessageReaction(
        chatMessage: ChatMessage,
        content: string
    ): ReactionMessage | undefined {
        return chatMessage.reactions.find(
            (reaction) => reaction.content === content && reaction.isMine && !isDeleted(reaction.id)
        );
    }

    /**
     * Finds the message that a given message is replying to
     * @param {ChatMessage} message - The message to find the reply-to message for
     * @returns {ChatMessage | undefined} The message being replied to, or undefined
     */
    function findReplyToChatMessage(chatMessage: ChatMessage): ChatMessage | undefined {
        const replyToId = chatMessage.replyToId;
        if (replyToId) return findChatMessage(replyToId);
    }

    /**
     * Checks if an event has been deleted
     * @param {string} eventId - The ID of the event to check
     * @returns {boolean} True if the event has been deleted, false otherwise
     */
    function isDeleted(eventId: string): boolean {
        const deletions = get(deletionMessagesMap);
        return deletions.has(eventId);
    }

    /**
     * Gets a summary of reactionMessages for a message
     * @param {string} messageId - The ID of the message to get reactionMessages for
     * @returns {ReactionSummary[]} Array of reaction summaries (emoji and count)
     */
    function getMessageReactionsSummary(messageId: string): ReactionSummary[] {
        const message = findChatMessage(messageId);
        const reactionMessages = message?.reactions || [];
        const reactionMessagesCounter: { [key: string]: number } = {};
        for (const reaction of reactionMessages) {
            if (!isDeleted(reaction.id)) {
                reactionMessagesCounter[reaction.content] =
                    (reactionMessagesCounter[reaction.content] || 0) + 1;
            }
        }
        return Object.entries(reactionMessagesCounter).map(([emoji, count]) => ({ emoji, count }));
    }

    /**
     * Checks if a message has any reactionMessages
     * @param {ChatMessage} message - The message to check for reactionMessages
     * @returns {boolean} True if the message has reactionMessages, false otherwise
     */
    function hasReactions(chatMessage: ChatMessage): boolean {
        const reactionMessagesummary = getMessageReactionsSummary(chatMessage.id);
        return reactionMessagesummary.length > 0;
    }

    /**
     * Checks if a message can be deleted by the current user
     * @param {string} messageId - The ID of the message to check
     * @returns {boolean} True if the message can be deleted, false otherwise
     */
    function isMessageDeletable(messageId: string): boolean {
        const message = findChatMessage(messageId);
        if (!message || message.lightningPayment || isDeleted(messageId)) return false;
        return message.isMine;
    }

    /**
     * Checks if a message content can be copied
     * @param {string} messageId - The ID of the message to check
     * @returns {boolean} True if the message can be copied, false otherwise
     */
    function isMessageCopyable(messageId: string): boolean {
        const message = findChatMessage(messageId);
        if (!message) return false;
        return !isDeleted(message.id);
    }

    /**
     * Adds a reaction to a message if current user hasn't reacted with same emoji, otherwise deletes the reaction
     * @param {NGroup} group - The group the message belongs to
     * @param {string} content - The reaction content (emoji)
     * @param {string} messageId - The ID of the message to react to
     * @returns {Promise<MessageWithTokens | null>} The created event or null if operation failed
     */
    async function clickReaction(
        group: NGroup,
        content: string,
        messageId: string
    ): Promise<MessageWithTokens | null> {
        const chatMessage = findChatMessage(messageId);
        if (!chatMessage) return null;

        const existingReaction = findMyMessageReaction(chatMessage, content);

        if (existingReaction) {
            return await deleteEvent(group, existingReaction.pubkey, existingReaction.id);
        }
        return await addReaction(group, chatMessage, content);
    }

    /**
     * Adds a reaction to a message
     * @param {NGroup} group - The group the message belongs to
     * @param {ChatMessage} chatMessage - The message to react to
     * @param {string} content - The reaction content (emoji)
     * @returns {Promise<MessageWithTokens | null>} The created event or null if operation failed
     */
    async function addReaction(
        group: NGroup,
        chatMessage: ChatMessage,
        content: string
    ): Promise<MessageWithTokens | null> {
        const tags = [
            ["e", chatMessage.id],
            ["p", chatMessage.pubkey],
        ];
        try {
            const reactionMessage = await invoke<MessageWithTokens>("send_mls_message", {
                group,
                message: content,
                kind: 7,
                tags,
            });
            handleMessage(reactionMessage);
            return reactionMessage;
        } catch (error) {
            console.error("Error sending reaction:", error);
            return null;
        }
    }

    /**
     * Deletes a message
     * @param {NGroup} group - The group the message belongs to
     * @param {string} messageId - The ID of the message to delete
     * @returns {Promise<Message | null>} The deletion event or null if deletion fails
     */
    async function deleteMessage(
        group: NGroup,
        messageId: string
    ): Promise<MessageWithTokens | null> {
        const message = findChatMessage(messageId);
        if (!message) return null;

        return deleteEvent(group, message.pubkey, message.id);
    }

    /**
     * Deletes an event (message or reaction)
     * @param {NGroup} group - The group the event belongs to
     * @param {string} pubkey - The public key of the event author
     * @param {string} eventId - The ID of the event to delete
     * @returns {Promise<Message | null>} The deletion event or null if deletion fails
     */
    async function deleteEvent(
        group: NGroup,
        pubkey: string,
        eventId: string
    ): Promise<MessageWithTokens | null> {
        if (pubkey !== currentPubkey) return null;

        try {
            const deletionMessage = await invoke<MessageWithTokens>("delete_message", {
                group,
                messageId: eventId,
            });
            if (deletionMessage) {
                handleMessage(deletionMessage);
            }
            return deletionMessage;
        } catch (error) {
            console.error("Error deleting message:", error);
            return null;
        }
    }

    /**
     * Pays a lightning invoice attached to a message
     * @param {NGroup} group - The group with relay information
     * @param {ChatMessage} chatMessage - The message with the lightning invoice to pay
     * @returns {Promise<MessageWithTokens | null>} The payment event or null if operation failed
     */
    async function payLightningInvoice(
        group: NGroup,
        chatMessage: ChatMessage
    ): Promise<MessageWithTokens | null> {
        if (!chatMessage.lightningInvoice) {
            console.error("Message does not have a lightning invoice");
            return null;
        }

        const relays: string[] = await invoke("get_group_relays", {
            mlsGroupId: group.mls_group_id,
        });

        const tags = [["q", chatMessage.id, relays[0], chatMessage.pubkey]];

        const paymentMessage: MessageWithTokens = await invoke("pay_invoice", {
            group: group,
            tags: tags,
            bolt11: chatMessage.lightningInvoice.invoice,
        });
        handleMessage(paymentMessage);
        return paymentMessage;
    }

    return {
        subscribe,
        handleMessage,
        handleMessages,
        clear,
        findChatMessage,
        findReactionMessage,
        findReplyToChatMessage,
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
