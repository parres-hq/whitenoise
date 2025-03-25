// These are types that map to the rust-nostr types from the rust backend
export type HexPubkey = string & { readonly __brand: unique symbol };
export type Npub = string & { readonly __brand: unique symbol };
export type Nsec = string & { readonly __brand: unique symbol };

export function isValidHexPubkey(value: string): value is HexPubkey {
    return /^[a-fA-F0-9]{64}$/.test(value);
}

export function isValidNpub(value: string): value is Npub {
    return /^npub1[a-zA-Z0-9]{59}$/.test(value);
}

export function isValidNsec(value: string): value is Nsec {
    return /^nsec1[a-zA-Z0-9]{58}$/.test(value);
}

export type EnrichedContact = {
    metadata: NMetadata;
    nip17: boolean;
    nip104: boolean;
    nostr_relays: string[];
    inbox_relays: string[];
    key_package_relays: string[];
};

export type EnrichedContactsMap = {
    [keys: string]: EnrichedContact;
};

export type MetadataMap = {
    [keys: string]: NMetadata;
};

export type NMetadata = {
    name?: string;
    display_name?: string;
    about?: string;
    picture?: string;
    banner?: string;
    website?: string;
    nip05?: string;
    lud06?: string;
    lud16?: string;
};

export type NChats = {
    [key: string]: NChat;
};

export type NLegacies = {
    [key: string]: NEvent[];
};

export type NChat = {
    latest: number;
    metadata: NMetadata;
    events: NEvent[];
};

export type NEvent = {
    id: string;
    pubkey: string;
    created_at: number;
    kind: number;
    tags: string[][];
    content: string;
    sig?: string;
};

export type Invite = {
    event: NEvent;
    mls_group_id: string;
    group_name: string;
    group_description: string;
    group_admin_pubkeys: string[];
    group_relays: string[];
    inviter: string;
    member_count: number;
    state: InviteState;
};

export type ProcessedInvite = {
    event_id: string;
    invite_event_id: string | undefined;
    account_pubkey: string;
    processed_at: number;
    state: ProcessedInviteState;
    failure_reason: string | undefined;
};

export enum InviteState {
    Pending = "Pending",
    Accepted = "Accepted",
    Declined = "Declined",
}

export enum ProcessedInviteState {
    Processed = "Processed",
    Failed = "Failed",
}

export type InvitesWithFailures = {
    invites: Invite[];
    failures: [string, string][];
};

export type NostrMlsWelcomeGroupData = {
    mls_group_id: Uint8Array;
    name: string;
    description: string;
    admin_pubkeys: string[];
    relays: string[];
};

export type NostrMlsGroup = {
    mls_group_id: Uint8Array;
    nostr_group_id: string;
    name: string;
    description: string;
    admin_pubkeys: string[];
    last_message_at: number;
    last_message_id: string;
    group_type: NostrMlsGroupType;
};

export type NostrMlsGroupWithRelays = {
    group: NostrMlsGroup;
    relays: string[];
};

export enum NostrMlsGroupType {
    DirectMessage = "DirectMessage",
    Group = "Group",
}

export type SerializableToken =
    | { Nostr: string }
    | { Url: string }
    | { Hashtag: string }
    | { Text: string }
    | { LineBreak: null }
    | { Whitespace: null };
