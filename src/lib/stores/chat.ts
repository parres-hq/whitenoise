import { writable, get, derived } from 'svelte/store';
import type { NEvent, NostrMlsGroup,  NostrMlsGroupWithRelays } from '$lib/types/nostr';
import type {
    ChatState,
    Message,
    Reaction,
    MessagesMap,
    DeletionsMap,
    ReactionsMap,
    ReactionSummary
} from '$lib/types/chat';
import { activeAccount } from './accounts';
import { invoke } from "@tauri-apps/api/core";


import { eventToMessage } from '$lib/utils/message';
import { eventToReaction } from '$lib/utils/reaction';
import { eventToDeletion } from '$lib/utils/deletion';

export function createChatStore() {
    const messagesMap = writable<MessagesMap>(new Map());
    const reactionsMap = writable<ReactionsMap>(new Map());
    const deletionsMap = writable<DeletionsMap>(new Map());
    const currentPubkey = get(activeAccount)?.pubkey;

    const messages = derived(messagesMap, ($messagesMap) => {
        return Array.from($messagesMap.values())
            .sort((a, b) => a.createdAt - b.createdAt);
    });
    
    const { subscribe, set, update } = writable<ChatState>({
        messages: get(messages),
        handleEvent,
        handleEvents,
        clear,
        findMessage,
        findReplyToMessage,
        isDeleted,
        getMessageReactionsSummary,
        hasReactions,
        clickReaction,
        deleteMessage,
        payLightningInvoice,
        isMessageDeletable,
        isMessageCopyable
    });
    
    messages.subscribe(sorted => {
        update(state => ({
            ...state,
            messages: sorted
        }));
    });
    
    const eventHandlers = {
        handleMessageEvent: (event: NEvent) => {
            const newMessage = eventToMessage(event, currentPubkey);
            const messagesToUpdate = [newMessage];
            const replyToMessage = newMessage.replyToId ? findMessage(newMessage.replyToId) : undefined;
            const isPaid = true;
            if(replyToMessage?.lightningInvoice && newMessage.lightningPayment && isPaid) {
                newMessage.lightningPayment.isPaid = true;
                replyToMessage.lightningInvoice.isPaid = true;
                messagesToUpdate.push(replyToMessage);
            }
            
            messagesMap.update(messages => {
                for (const message of messagesToUpdate) {
                    messages.set(message.id, message);
                }
                return messages;
            });
        },
        handleDeletionEvent: (event: NEvent) => {
            const deletion = eventToDeletion(event);
            if (!deletion) return;
            deletionsMap.update(deletions => {
                deletions.set(deletion.targetId, deletion);
                return deletions;
            });
        },
        handleReactionEvent: (event: NEvent) => {
            const reaction = eventToReaction(event,currentPubkey);
            if (!reaction) return;
            reactionsMap.update(reactions => {
                reactions.set(reaction.id, reaction);
                return reactions;
            });
            
            const message = findMessage(reaction.targetId);
            if (!message) return;
            message.reactions.push(reaction);
            messagesMap.update(messages => {
                messages.set(message.id, message);
                return messages;
            });
        }
    };

    // Event handler map with Nostr event kinds as keys
    const eventHandlerMap: Record<number, (event: NEvent) => void> = {
        5: eventHandlers.handleDeletionEvent,
        7: eventHandlers.handleReactionEvent,
        9: eventHandlers.handleMessageEvent
    };

    function deleteTempEvents() {
        messagesMap.update(messages => {
            messages.delete("temp");
            return messages;
        });
        reactionsMap.update(reactions => {
            reactions.delete("temp");
            return reactions;
        });
    }

    function handleEvent(event: NEvent, deleteTemp: boolean = true) {
        if (deleteTemp) deleteTempEvents();
        
        const handler = eventHandlerMap[event.kind];
        if (handler) handler(event);
    }

    function handleEvents(events: NEvent[]) {
        deleteTempEvents();
        const sortedEvents = events.sort((a, b) => a.created_at - b.created_at);
        for (const event of sortedEvents) {
            handleEvent(event, false);
        }
    }

    function clear() {
        messagesMap.set(new Map());
        deletionsMap.set(new Map());
    }

    function findMessage(id: string): Message | undefined {
        const messages = get(messagesMap); 
        return messages.get(id);
    }

    function findMyMessageReaction(message: Message, content: string): Reaction | undefined {
        return message.reactions.find(reaction => reaction.content === content && reaction.isMine && !isDeleted(reaction.id));
    }

    function findReplyToMessage(message: Message): Message | undefined {
        const replyToId = message.replyToId;
        if (replyToId) return findMessage(replyToId);
    }
    
    function isDeleted(eventId: string): boolean {
        const deletions = get(deletionsMap);
        return deletions.has(eventId);
    }

    function getMessageReactionsSummary(messageId: string): ReactionSummary[] {
        const message = findMessage(messageId);
        const reactions = message?.reactions || [];
        const reactionsCounter: {[key: string]: number}  = {};
        for (const reaction of reactions) {
            if(!isDeleted(reaction.id)) {
                reactionsCounter[reaction.content] = (reactionsCounter[reaction.content] || 0) + 1;
            }
        }
        return Object.entries(reactionsCounter).map(([emoji, count]) => ({ emoji, count }))
    }

    function hasReactions(message: Message): boolean {
        const reactionsSummary = getMessageReactionsSummary(message.id);
        return reactionsSummary.length > 0;
    }

    function isMessageDeletable(messageId: string): boolean {        
        const message = findMessage(messageId);
        if(!message || message.lightningPayment || isDeleted(messageId)) return false;
        return message.isMine;
    }

    function isMessageCopyable(messageId: string): boolean {
        const message = findMessage(messageId);
        if (!message) return false;
        return !isDeleted(message.id);
    }

    async function clickReaction(
        group: NostrMlsGroup,
        content: string,
        messageId: string
    ): Promise<NEvent | null> {
        const message = findMessage(messageId);
        if (!message) return null;

        const existingReaction = findMyMessageReaction(message, content);

        if(existingReaction) {
            return await deleteEvent(group, existingReaction.pubkey, existingReaction.id);
        } 
        return await addReaction(group, message, content);
    }

    async function addReaction(
        group: NostrMlsGroup,
        message: Message,
        content: string
    ): Promise<NEvent | null> {
        const tags = [["e", message.id], ["p", message.pubkey]]
        try {
            const reactionEvent = await invoke("send_mls_message", {
                group,
                message: content,
                kind: 7,
                tags
            }) as NEvent;
            handleEvent(reactionEvent);
            return reactionEvent;
        } catch (error) {
            console.error("Error sending reaction:", error);
            return null;
        }
    }

    async function deleteMessage(group: NostrMlsGroup, messageId: string): Promise<NEvent | null> {
        const message = findMessage(messageId);
        if (!message) return null;

        return deleteEvent(group, message.pubkey, message.id);
    }

    async function deleteEvent(
        group: NostrMlsGroup,
        pubkey: string, eventId: string
    ): Promise<NEvent | null> {
        if (pubkey !== currentPubkey) return null;

        try {
            const deletionEvent = await invoke<NEvent>("delete_message", {
                group,
                messageId: eventId,
            });
            if (deletionEvent) {
                handleEvent(deletionEvent);
            }
            return deletionEvent;
        } catch (error) {
            console.error("Error deleting message:", error);
            return null;
        }
    }

    async function payLightningInvoice(
        groupWithRelays: NostrMlsGroupWithRelays,
        message: Message
    ): Promise<NEvent | null> {
        if (!message.lightningInvoice) {
            console.error("Message does not have a lightning invoice");
            return null;
        }
        
        let tags = [["q", message.id, groupWithRelays.relays[0], message.pubkey]];
        
        const paymentEvent : NEvent = await invoke("pay_invoice", {
            group: groupWithRelays.group,
            tags: tags,
            bolt11: message.lightningInvoice.invoice,
        });
        handleEvent(paymentEvent);
        return paymentEvent;
    }

    return {
        subscribe,
        handleEvent,
        handleEvents,
        clear,
        findMessage,
        findReplyToMessage,
        isDeleted,
        getMessageReactionsSummary,
        hasReactions,
        clickReaction,
        deleteMessage,
        payLightningInvoice,
        isMessageDeletable,
        isMessageCopyable
    };
}
