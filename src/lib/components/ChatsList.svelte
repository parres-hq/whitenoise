<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import type { EnrichedContact, EnrichedContactsMap, Invite, NostrMlsGroup } from "$lib/types/nostr";
import { hexKeyFromNpub, isValidHexKey, isValidNpub, npubFromPubkey } from "$lib/utils/nostr";
import { nameFromMetadata } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import AddLarge from "carbon-icons-svelte/lib/AddLarge.svelte";
import Chat from "carbon-icons-svelte/lib/Chat.svelte";
import Search from "carbon-icons-svelte/lib/Search.svelte";
import WarningAlt from "carbon-icons-svelte/lib/WarningAlt.svelte";
import { onDestroy, onMount } from "svelte";
import Avatar from "./Avatar.svelte";
import FormattedNpub from "./FormattedNpub.svelte";
import GroupListItem from "./GroupListItem.svelte";
import Header from "./Header.svelte";
import InviteListItem from "./InviteListItem.svelte";
import Loader from "./Loader.svelte";
import StartSecureChat from "./StartSecureChat.svelte";
import Button from "./ui/button/button.svelte";
import Input from "./ui/input/input.svelte";
import * as Sheet from "./ui/sheet";

type ChatsListProps = {
    isLoading?: boolean;
    loadingError?: string | null;
    invites?: Invite[];
    groups?: NostrMlsGroup[];
    selectedChatId?: string | null;
};

let {
    isLoading = $bindable(false),
    loadingError = $bindable(null),
    invites = $bindable([]),
    groups = $bindable([]),
    selectedChatId = $bindable(null),
}: ChatsListProps = $props();

let unlistenAccountChanging: UnlistenFn;
let unlistenAccountChanged: UnlistenFn;
let unlistenNostrReady: UnlistenFn;

let contactsLoading = $state(true);
let contactsLoadingError = $state<string | null>(null);

let isSearching = $state(false);
let searchResults = $state<EnrichedContactsMap>({});

let contacts = $state<EnrichedContactsMap>({});
let contactsSearch = $state("");
let filteredContacts = $state<EnrichedContactsMap>({});

let isValidKey = $state(false);
let validKeyPubkey = $state<string | null>(null);
let validKeyContact = $state<EnrichedContact | null>(null);

let newChatSheetOpen = $state(false);
let selectedContactPubkey = $state<string | null>(null);
let selectedContact = $state<EnrichedContact | null>(null);
let showStartChatView = $state(false);

async function loadContacts() {
    try {
        const contactsResponse = await invoke("query_enriched_contacts");
        // Sort contacts by name
        contacts = Object.fromEntries(
            Object.entries(contactsResponse as EnrichedContactsMap).sort(
                ([_keyA, contactA], [_keyB, contactB]) => {
                    const nameA =
                        contactA.metadata.display_name ||
                        contactA.metadata.name ||
                        contactA.metadata.nip05 ||
                        "";
                    const nameB =
                        contactB.metadata.display_name ||
                        contactB.metadata.name ||
                        contactB.metadata.nip05 ||
                        "";
                    // If either name is empty, sort it to the bottom
                    if (!nameA && !nameB) return 0;
                    if (!nameA) return 1;
                    if (!nameB) return -1;
                    // Otherwise do normal string comparison
                    return nameA.localeCompare(nameB);
                }
            )
        );
        contactsLoading = false;
    } catch (error) {
        console.error("Error loading contacts:", error);
        contactsLoadingError = error as string;
        contactsLoading = false;
    }
}

async function fetchEnrichedContact(pubkey: string): Promise<EnrichedContact | null> {
    try {
        const contact = (await invoke("fetch_enriched_contact", {
            pubkey,
            updateAccount: false,
        })) as EnrichedContact;
        return contact;
    } catch (e) {
        console.error("Failed to fetch enriched contact:", e);
        return null;
    }
}

async function searchRelays(): Promise<void> {
    isSearching = true;
    console.log(`Searching relays for "${contactsSearch}"...`);
    invoke("search_for_enriched_contacts", { query: contactsSearch }).then((contact_map) => {
        searchResults = contact_map as EnrichedContactsMap;
        isSearching = false;
    });
}

function startChatWithContact(pubkey: string, contact: EnrichedContact | null): void {
    selectedContactPubkey = pubkey;
    selectedContact = contact;
    showStartChatView = true;
}

function resetChatSheet(): void {
    showStartChatView = false;
    selectedContactPubkey = null;
    selectedContact = null;
    contactsSearch = "";
    searchResults = {};
}

function closeNewChatSheet(): void {
    newChatSheetOpen = false;
    resetChatSheet();
}

onMount(async () => {
    await loadContacts();

    if (!unlistenAccountChanging) {
        unlistenAccountChanging = await listen<string>("account_changing", async (_event) => {
            console.log("Event received in contacts list: account_changing");
            contacts = {};
        });
    }

    if (!unlistenAccountChanged) {
        unlistenAccountChanged = await listen<string>("account_changed", async (_event) => {
            console.log("Event received in contacts list: account_changed");
        });
    }

    if (!unlistenNostrReady) {
        unlistenNostrReady = await listen<string>("nostr_ready", async (_event) => {
            console.log("Event received in contacts list: nostr_ready");
            await loadContacts();
        });
    }
});

onDestroy(() => {
    unlistenAccountChanging?.();
    unlistenAccountChanged?.();
    unlistenNostrReady?.();
});

$effect(() => {
    if (!contactsSearch || contactsSearch === "") {
        filteredContacts = contacts;
        isValidKey = false;
        validKeyPubkey = null;
        validKeyContact = null;
    } else {
        // Check if input is a valid npub or hex key
        if (isValidNpub(contactsSearch)) {
            isValidKey = true;
            validKeyPubkey = hexKeyFromNpub(contactsSearch);
        } else if (isValidHexKey(contactsSearch)) {
            isValidKey = true;
            validKeyPubkey = contactsSearch;
        } else {
            isValidKey = false;
            validKeyPubkey = null;
            validKeyContact = null;
        }

        // If we have a valid key, try to fetch the contact info
        if (validKeyPubkey) {
            fetchEnrichedContact(validKeyPubkey).then((contact) => {
                validKeyContact = contact;

                // Add the contact to search results if it's valid
                if (contact && validKeyPubkey) {
                    searchResults = {
                        ...searchResults,
                        [validKeyPubkey as string]: contact,
                    };
                }
            });
        }

        filteredContacts = Object.fromEntries(
            Object.entries(contacts as EnrichedContactsMap).filter(
                ([pubkey, contact]) =>
                    contact.metadata.name
                        ?.toLowerCase()
                        .trim()
                        .includes(contactsSearch.toLowerCase().trim()) ||
                    contact.metadata.display_name
                        ?.toLowerCase()
                        .trim()
                        .includes(contactsSearch.toLowerCase().trim()) ||
                    contact.metadata.nip05
                        ?.toLowerCase()
                        .trim()
                        .includes(contactsSearch.toLowerCase().trim()) ||
                    pubkey.toLowerCase().trim().includes(contactsSearch.toLowerCase().trim()) ||
                    npubFromPubkey(pubkey)
                        .toLowerCase()
                        .trim()
                        .includes(contactsSearch.toLowerCase().trim())
            )
        );
    }
});
</script>

<Header>
    <div class="flex flex-row items-center justify-between w-full">
        <a href="/settings">
            <Avatar pubkey={$activeAccount!.pubkey} />
        </a>
        <div class="flex flex-row items-center gap-4 md:gap-2">
            <Sheet.Root>
                <Sheet.Trigger>
                    <Button variant="link" size="icon" class="p-2 shrink-0 text-primary-foreground">
                        <Search size={24} class="shrink-0 !h-6 !w-6" />
                    </Button>
                </Sheet.Trigger>
                <Sheet.Content side="bottom" class="pb-0 px-0 h-[90%]">
                    <Sheet.Header class="text-left mb-6 px-6">
                        <Sheet.Title>Search</Sheet.Title>
                    </Sheet.Header>
                    <div class="flex flex-col gap-2 px-6">
                        <Input type="search" placeholder="Search contacts or chats&hellip;" class="focus-visible:ring-0" />
                    </div>
                    <div class="flex flex-col gap-2 px-6 mt-6 text-destructive">Not implemented yet</div>
                </Sheet.Content>
            </Sheet.Root>

            <Sheet.Root bind:open={newChatSheetOpen} onOpenChange={() => {
                if (!newChatSheetOpen) {
                    resetChatSheet();
                }
            }}>
                <Sheet.Trigger>
                    <Button variant="link" size="icon" class="p-2 shrink-0 text-primary-foreground">
                        <AddLarge size={24} class="shrink-0 !h-6 !w-6" />
                    </Button>
                </Sheet.Trigger>
                <Sheet.Content side="bottom" class="pb-0 px-0 h-[90%]">
                    <div class="flex flex-col h-full relative">
                        {#if showStartChatView && selectedContactPubkey && selectedContact}
                            <StartSecureChat
                                bind:pubkey={selectedContactPubkey}
                                bind:contact={selectedContact}
                                onBack={resetChatSheet}
                                onClose={closeNewChatSheet}
                            />
                        {:else}
                            <div class="sticky top-0">
                                <Sheet.Header class="text-left mb-6 px-6">
                                    <Sheet.Title>New chat</Sheet.Title>
                                </Sheet.Header>

                                <div class="flex flex-row gap-4 px-6 mb-4">
                                    <form onsubmit={searchRelays} class="flex flex-row gap-2 items-center w-full">
                                        <Input
                                            type="search"
                                            placeholder="Search contact or public key..."
                                            bind:value={contactsSearch}
                                            class="focus-visible:ring-0"
                                        />
                                        <Button type="submit" variant="outline" size="icon" class="shrink-0">
                                            <Search size={24} class="shrink-0 !h-6 !w-6" />
                                        </Button>
                                    </form>
                                </div>
                            </div>

                            <div class="flex flex-col flex-1 overflow-hidden">
                                <div class="flex flex-col gap-2 overflow-y-auto flex-1">
                                    {#if contactsLoading}
                                        <div class="flex justify-center items-center flex-1 py-10">
                                            <Loader size={40} fullscreen={false} />
                                        </div>
                                    {:else if contactsLoadingError}
                                        <div class="text-destructive font-medium flex flex-col gap-2 px-6">
                                            <span>Sorry, we couldn't load contacts because of an error.</span>
                                            <pre class="font-mono p-2 bg-destructive/10 text-xs">{contactsLoadingError || "Unknown error"}</pre>
                                        </div>
                                    {:else}
                                        <h2 class="text-xl font-normal mb-2 px-6">Contacts</h2>
                                        {#if filteredContacts && Object.keys(filteredContacts).length > 0}
                                            <div class="px-0">
                                                {#each Object.entries(filteredContacts) as [pubkey, contact] (pubkey)}
                                                    <Button variant="ghost" size="lg" class="w-full h-fit flex flex-row gap- px-6 items-center min-w-0 w-full py-2 focus-visible:outline-none focus-visible:ring-0" onclick={() => startChatWithContact(pubkey, contact)}>
                                                        <Avatar
                                                            pubkey={pubkey}
                                                            picture={contact.metadata?.picture}
                                                            pxSize={56}
                                                        />
                                                        <div class="flex flex-col gap-0 min-w-0 justify-start text-left truncate w-full">
                                                            <div class="truncate text-lg font-medium">
                                                                {nameFromMetadata(contact.metadata, pubkey)}
                                                            </div>
                                                            <div class="flex gap-4 items-center">
                                                                <FormattedNpub npub={npubFromPubkey(pubkey)} showCopy={false} />
                                                            </div>
                                                        </div>
                                                    </Button>
                                                {/each}
                                            </div>
                                        {:else}
                                            <span class="text-gray-400 px-6 text-center">No contacts found</span>
                                        {/if}
                                        <div class="mt-4">
                                            {#if isSearching}
                                                <h2 class="text-xl font-normal mb-2 px-6">Searching&hellip;</h2>
                                                <div class="px-6">
                                                    <Loader size={40} fullscreen={false} />
                                                </div>
                                            {:else if searchResults && Object.keys(searchResults).length > 0}
                                                <h2 class="text-xl font-normal mb-2 px-6">Search results</h2>
                                                {#each Object.entries(searchResults) as [pubkey, contact] (pubkey)}
                                                    <Button variant="ghost" size="lg" class="w-full h-fit flex flex-row gap-3 px-6 items-center min-w-0 w-full py-2 focus-visible:outline-none focus-visible:ring-0" onclick={() => startChatWithContact(pubkey, contact)}>
                                                        <Avatar
                                                            pubkey={pubkey}
                                                            picture={contact.metadata?.picture}
                                                            pxSize={56}
                                                        />
                                                        <div class="flex flex-col gap-0 min-w-0 justify-start text-left truncate w-full">
                                                            <div class="truncate text-lg font-medium">
                                                                {nameFromMetadata(contact.metadata, pubkey)}
                                                            </div>
                                                            <div class="flex gap-4 items-center">
                                                                <FormattedNpub npub={npubFromPubkey(pubkey)} showCopy={false} />
                                                            </div>
                                                        </div>
                                                    </Button>
                                                {/each}
                                            {/if}
                                        </div>
                                    {/if}
                                </div>
                            </div>
                        {/if}
                    </div>
                </Sheet.Content>
            </Sheet.Root>
        </div>
    </div>
</Header>

<!-- Chat list -->
{#if isLoading}
    <div class="flex justify-center items-center mt-20 w-full">
        <Loader size={40} fullscreen={false} />
    </div>
{:else if loadingError}
    <div class="flex flex-col gap-2 items-center justify-center flex-1 pt-40 text-destructive">
        <WarningAlt size={32} />
        <span>Error loading chats</span>
        <span>{loadingError}</span>
    </div>
{:else}
    <div class="flex flex-col gap-2">
        {#if invites.length === 0 && groups.length === 0}
            <div class="flex flex-col gap-2 items-center justify-center flex-1 pt-40 text-muted-foreground">
                <Chat size={32} />
                <span>No chats found</span>
                <span>Click the "+" button to start a new chat</span>
            </div>
        {/if}
        {#each invites as invite}
            <InviteListItem {invite} />
        {/each}
        {#each groups as group}
            <GroupListItem {group} bind:selectedChatId />
        {/each}
    </div>
{/if}
