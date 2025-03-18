import type { NEvent, NostrMlsGroup, NostrMlsGroupWithRelays } from './nostr';

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
}

export type Reaction = {
    id: string;
    pubkey: string;
    content: string;
    createdAt: number;
    targetId: string;
    isMine: boolean;
    event: NEvent;
}

export type ReactionSummary = {
    emoji: string;
    count: number;
}

export type LightningInvoice = {
    invoice: string;
    amount: number;
    description?: string;
    isPaid: boolean;
};

export type LightningPayment = {
    preimage: string;
    isPaid: boolean;
}

export type Deletion = {
    id: string;
    pubkey: string;
    targetId: string;
    event: NEvent;
}

export type MessagesMap = Map<string, Message>;

export type ReactionsMap = Map<string, Reaction>;

export type DeletionsMap = Map<string, Deletion>;

export type ChatState = {
    messages: Message[];
    handleEvent: (event: NEvent) => void;
    handleEvents: (events: NEvent[]) => void;
    clear: () => void;
    findMessage: (id: string) => Message | undefined;
    findReplyToMessage: (message: Message) => Message | undefined;
    isDeleted: (eventId: string) => boolean;
    getMessageReactionsSummary: (messageId: string) => ReactionSummary[];
    hasReactions: (message: Message) => boolean;
    clickReaction: (group: NostrMlsGroup, reaction: string, messageId: string) => Promise<NEvent | null>;
    deleteMessage: (group: NostrMlsGroup, messageId: string) => Promise<NEvent | null>;
    payLightningInvoice: (groupWithRelays: NostrMlsGroupWithRelays, message: Message) => Promise<NEvent | null>;
    isMessageDeletable: (messageId: string) => boolean;
    isMessageCopyable: (messageId: string) => boolean;
};
