// These are types that map to the rust-nostr types from the rust backend
export type HexPubkey = string & { readonly __brand: unique symbol };
export type Npub = string & { readonly __brand: unique symbol };
export type Nsec = string & { readonly __brand: unique symbol };

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

export type NGroup = {
    mls_group_id: MlsGroupId;
    nostr_group_id: Uint8Array;
    name: string;
    description: string;
    admin_pubkeys: string[];
    last_message_at: number | undefined;
    last_message_id: string | undefined;
    group_type: NostrMlsGroupType;
    epoch: number;
    state: NostrMlsGroupState;
};

export type MlsGroupId = {
    value: { vec: Uint8Array };
};

export enum NostrMlsGroupType {
    DirectMessage = "direct_message",
    Group = "group",
}

export enum NostrMlsGroupState {
    Active = "active",
    Inactive = "inactive",
    Pending = "pending",
}

export type NGroupRelay = {
    relay_url: string;
    mls_group_id: MlsGroupId;
};

export type NWelcome = {
    id: string;
    event: NEvent;
    mls_group_id: MlsGroupId;
    nostr_group_id: Uint8Array;
    group_name: string;
    group_description: string;
    group_admin_pubkeys: string[];
    group_relays: NGroupRelay[];
    welcomer: string;
    member_count: number;
    state: NWelcomeState;
    wrapper_event_id: string;
};

export enum NWelcomeState {
    Pending = "pending",
    Accepted = "accepted",
    Declined = "declined",
    Ignored = "ignored",
}

export type SerializableToken =
    | { Nostr: string }
    | { Url: string }
    | { Hashtag: string }
    | { Text: string }
    | { LineBreak: null }
    | { Whitespace: null };
