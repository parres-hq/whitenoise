import type {
    NEvent,
    NostrMlsGroup,
    NostrMlsGroupWithRelays,
    SerializableToken,
} from "$lib/types/nostr";

/**
 * Represents a cached message from the local database.
 * We get these back from invoke calls and events emitted from the backend.
 * @property {string} event_id - The ID of the Nostr event
 * @property {string} account_pubkey - The public key of the account that sent the message
 * @property {string} author_pubkey - The public key of the author of the message
 * @property {string} mls_group_id - The ID of the MLS group the message belongs to
 * @property {number} event_kind - The kind of the Nostr event
 * @property {number} created_at - The timestamp when the message was created
 * @property {string} content - The content of the message
 * @property {NEvent} event - The original Nostr unsigned event data
 * @property {SerializableToken[]} tokens - The tokenized message content
 * @property {string} outer_event_id - The ID of the outer event, if applicable
 */
export type CachedMessage = {
    event_id: string;
    account_pubkey: string;
    author_pubkey: string;
    mls_group_id: string;
    event_kind: number;
    created_at: number;
    content: string;
    event: NEvent;
    tokens: SerializableToken[];
    outer_event_id: string;
};

/**
 * Represents a chat message in the front-end application
 * @property {string} id - Unique identifier for the message
 * @property {string} pubkey - Public key of the message sender
 * @property {string} content - Text content of the message
 * @property {number} createdAt - Unix timestamp when the message was created
 * @property {string} [replyToId] - ID of the message this is replying to, if applicable
 * @property {Reaction[]} reactions - Array of reactions to this message
 * @property {LightningInvoice} [lightningInvoice] - Lightning invoice details if message contains one
 * @property {LightningPayment} [lightningPayment] - Lightning payment details if message is a payment
 * @property {boolean} isSingleEmoji - Whether the message consists of only a single emoji
 * @property {boolean} isMine - Whether the current user is the author of this message
 * @property {NEvent} event - The original Nostr event data
 * @property {SerializableToken[]} tokens - The tokenized message content
 */
export type Message = {
    id: string;
    pubkey: string;
    content: string;
    createdAt: number;
    replyToId?: string;
    reactions: Reaction[];
    lightningInvoice?: LightningInvoice;
    lightningPayment?: LightningPayment;
    isSingleEmoji: boolean;
    isMine: boolean;
    event: NEvent;
    tokens: SerializableToken[];
};

/**
 * Represents a reaction to a message
 * @property {string} id - Unique identifier for the reaction
 * @property {string} pubkey - Public key of the user who reacted
 * @property {string} content - The reaction content (typically an emoji)
 * @property {number} createdAt - Unix timestamp when the reaction was created
 * @property {string} targetId - ID of the message this reaction targets
 * @property {boolean} isMine - Whether the current user is the author of this reaction
 * @property {NEvent} event - The original Nostr event data
 */
export type Reaction = {
    id: string;
    pubkey: string;
    content: string;
    createdAt: number;
    targetId: string;
    isMine: boolean;
    event: NEvent;
};

/**
 * Summary of reactions to a message, grouping by emoji
 * @property {string} emoji - The emoji used in the reaction
 * @property {number} count - Number of users who reacted with this emoji
 */
export type ReactionSummary = {
    emoji: string;
    count: number;
};

/**
 * Represents an emoji reaction in a message
 * @property {string} emoji - The emoji used in the reaction
 * @property {string} [name] - Optional name for the reaction
 */
export type ReactionEmoji = {
    emoji: string;
    name?: string;
};

/**
 * Represents a Lightning Network invoice in a message
 * @property {string} invoice - The Lightning invoice string (BOLT11 format)
 * @property {number} amount - The amount in satoshis
 * @property {string} [description] - Optional description of what the invoice is for
 * @property {boolean} isPaid - Whether the invoice has been paid
 */
export type LightningInvoice = {
    invoice: string;
    amount: number;
    description?: string;
    isPaid: boolean;
};

/**
 * Represents a Lightning Network payment
 * @property {string} preimage - Payment preimage (proof of payment)
 * @property {boolean} isPaid - Whether the payment was successful
 */
export type LightningPayment = {
    preimage: string;
    isPaid: boolean;
};

/**
 * Represents a message deletion event
 * @property {string} id - Unique identifier for the deletion event
 * @property {string} pubkey - Public key of the user who deleted the message
 * @property {string} targetId - ID of the message that was deleted
 * @property {NEvent} event - The original Nostr event data
 */
export type Deletion = {
    id: string;
    pubkey: string;
    targetId: string;
    event: NEvent;
};

/**
 * Map of message IDs to Message objects for efficient lookup
 */
export type MessagesMap = Map<string, Message>;

/**
 * Map of reaction IDs to Reaction objects for efficient lookup
 */
export type ReactionsMap = Map<string, Reaction>;

/**
 * Map of deletion target IDs to Deletion objects for efficient lookup
 */
export type DeletionsMap = Map<string, Deletion>;

/**
 * State and methods for managing a chat conversation
 * @property {Message[]} messages - Array of messages in the chat, sorted by creation time
 * @property {function} handleEvent - Processes a single Nostr event and updates state
 * @property {function} handleEvents - Processes multiple Nostr events and updates state
 * @property {function} clear - Clears all messages and state
 * @property {function} findMessage - Finds a message by its ID
 * @property {function} findReaction - Finds a reaction by its ID
 * @property {function} findReplyToMessage - Finds the message that another message is replying to
 * @property {function} isDeleted - Checks if a message has been deleted
 * @property {function} getMessageReactionsSummary - Gets a summary of reactions for a message
 * @property {function} hasReactions - Checks if a message has any reactions
 * @property {function} clickReaction - Toggles a reaction on a message
 * @property {function} deleteMessage - Deletes a message
 * @property {function} payLightningInvoice - Pays a Lightning invoice in a message
 * @property {function} isMessageDeletable - Checks if a message can be deleted
 * @property {function} isMessageCopyable - Checks if a message content can be copied
 */
export type ChatState = {
    messages: Message[];
    handleCachedMessage: (cachedMessage: CachedMessage) => void;
    handleCachedMessages: (cachedMessages: CachedMessage[]) => void;
    clear: () => void;
    findMessage: (id: string) => Message | undefined;
    findReaction: (id: string) => Reaction | undefined;
    findReplyToMessage: (message: Message) => Message | undefined;
    isDeleted: (eventId: string) => boolean;
    getMessageReactionsSummary: (messageId: string) => ReactionSummary[];
    hasReactions: (message: Message) => boolean;
    clickReaction: (
        group: NostrMlsGroup,
        reaction: string,
        messageId: string
    ) => Promise<CachedMessage | null>;
    deleteMessage: (group: NostrMlsGroup, messageId: string) => Promise<CachedMessage | null>;
    payLightningInvoice: (
        groupWithRelays: NostrMlsGroupWithRelays,
        message: Message
    ) => Promise<CachedMessage | null>;
    isMessageDeletable: (messageId: string) => boolean;
    isMessageCopyable: (messageId: string) => boolean;
};
