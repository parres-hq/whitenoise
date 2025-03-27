<script lang="ts">
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { PushView } from "$lib/types/modal";
import type { EnrichedContact, EnrichedContactsMap } from "$lib/types/nostr";
import { npubFromPubkey } from "$lib/utils/nostr";
import { hexKeyFromNpub, isValidHexKey, isValidNpub } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { CaretRight } from "phosphor-svelte";
import { onDestroy, onMount } from "svelte";
import Avatar from "../../Avatar.svelte";
import Loader from "../../Loader.svelte";
import Name from "../../Name.svelte";
import ContactDetail from "./ContactDetail.svelte";

let toastState = getToastState();

let unlistenAccountChanging: UnlistenFn;
let unlistenAccountChanged: UnlistenFn;
let unlistenNostrReady: UnlistenFn;

let { pushView } = $props<{
    pushView?: PushView;
}>();

let isLoading = $state(true);
// TODO: create a load error state
let loadingError = $state<string | null>(null);

let isSearching = $state(false);
let searchResults = $state<EnrichedContactsMap>({});

let contacts = $state<EnrichedContactsMap>({});
let search = $state("");
let filteredContacts = $state<EnrichedContactsMap>({});

let isValidKey = $state(false);
let validKeyPubkey = $state<string | null>(null);
let validKeyContact = $state<EnrichedContact | null>(null);

async function loadContacts() {
    const contactsResponse = await invoke("fetch_enriched_contacts");
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
    isLoading = false;
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
    toastState.cleanup();
});

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

$effect(() => {
    if (!search || search === "") {
        filteredContacts = contacts;
        isValidKey = false;
        validKeyPubkey = null;
        validKeyContact = null;
    } else {
        // Check if input is a valid npub or hex key
        if (isValidNpub(search)) {
            isValidKey = true;
            validKeyPubkey = hexKeyFromNpub(search);
        } else if (isValidHexKey(search)) {
            isValidKey = true;
            validKeyPubkey = search;
        } else {
            isValidKey = false;
            validKeyPubkey = null;
            validKeyContact = null;
        }

        // If we have a valid key, try to fetch the contact info
        if (validKeyPubkey) {
            fetchEnrichedContact(validKeyPubkey).then((contact) => {
                validKeyContact = contact;
            });
        }

        filteredContacts = Object.fromEntries(
            Object.entries(contacts as EnrichedContactsMap).filter(
                ([pubkey, contact]) =>
                    contact.metadata.name
                        ?.toLowerCase()
                        .trim()
                        .includes(search.toLowerCase().trim()) ||
                    contact.metadata.display_name
                        ?.toLowerCase()
                        .trim()
                        .includes(search.toLowerCase().trim()) ||
                    contact.metadata.nip05
                        ?.toLowerCase()
                        .trim()
                        .includes(search.toLowerCase().trim()) ||
                    pubkey.toLowerCase().trim().includes(search.toLowerCase().trim()) ||
                    npubFromPubkey(pubkey)
                        .toLowerCase()
                        .trim()
                        .includes(search.toLowerCase().trim())
            )
        );
    }
});

function viewContact(pubkey: string, contact: EnrichedContact): void {
    pushView(ContactDetail, { pubkey, contact });
}

async function searchRelays(): Promise<void> {
    isSearching = true;
    console.log(`Searching relays for "${search}"...`);
    invoke("search_for_enriched_contacts", { query: search }).then((contact_map) => {
        searchResults = contact_map as EnrichedContactsMap;
        isSearching = false;
    });
}
</script>

<div class="flex flex-row gap-4">
    <form onsubmit={searchRelays} class="flex flex-row gap-2 items-center w-full" >
        <input
            type="search"
            placeholder="Search..."
            bind:value={search}
            class="bg-transparent ring-1 ring-gray-700 rounded-md px-3 py-1.5 w-full"
        />
        <button type="submit" class="button-primary">Search</button>
    </form>
</div>
<div class="flex flex-col mt-10">

    {#if isLoading }
        <Loader size={40} fullscreen={false} />
    {:else}
        <h2 class="section-title">Your contacts</h2>
        <div class="section !p-0 divide-y divide-gray-700">
            {#if filteredContacts && Object.keys(filteredContacts).length > 0}
                {#each Object.entries(filteredContacts) as [pubkey, contact] (pubkey)}
                    <button
                        onclick={() => viewContact(pubkey, contact)}
                        class="flex flex-row gap-2 items-center px-2 py-3 hover:bg-gray-700 w-full"
                    >
                        <Avatar {pubkey} picture={contact.metadata.picture} pxSize={40} />
                        <div class="flex flex-col items-start justify-start truncate">
                            <Name {pubkey} metadata={contact.metadata} />
                            <span class="text-gray-400 text-sm font-mono">{npubFromPubkey(pubkey)}</span>
                        </div>
                        <CaretRight size={20} class="ml-auto" />
                    </button>
                {/each}
            {:else}
                <div class="flex flex-col gap-6 items-center justify-center h-full">
                    <span class="text-gray-400">No contacts found</span>

                </div>
            {/if}
        </div>
        <h2 class="section-title">Other people</h2>
        <div class="section !p-0 divide-y divide-gray-700">
            {#if isSearching }
                <div class="my-4">
                    <Loader size={40} fullscreen={false} />
                </div>
            {:else if searchResults && Object.keys(searchResults).length > 0}
                {#each Object.entries(searchResults) as [pubkey, contact] (pubkey)}
                    <button
                        onclick={() => viewContact(pubkey, contact)}
                        class="flex flex-row gap-2 items-center px-2 py-3 hover:bg-gray-700 w-full"
                    >
                        <Avatar {pubkey} picture={contact.metadata.picture} pxSize={40} />
                        <div class="flex flex-col items-start justify-start truncate">
                            <Name {pubkey} metadata={contact.metadata} />
                            <span class="text-gray-400 text-sm font-mono">{npubFromPubkey(pubkey)}</span>
                        </div>
                        <CaretRight size={20} class="ml-auto" />
                    </button>
                {/each}
            {:else if isValidKey && validKeyPubkey !== null}
                <button
                    onclick={() => viewContact(validKeyPubkey as string, validKeyContact ?? {
                        metadata: {},
                        nip17: false,
                        nip104: false,
                        nostr_relays: [],
                        inbox_relays: [],
                        key_package_relays: []
                    })}
                    class="flex flex-row gap-2 items-center px-2 py-3 hover:bg-gray-700 w-full"
                >
                    <Avatar pubkey={validKeyPubkey as string} picture={validKeyContact?.metadata?.picture} pxSize={40} />
                    <div class="flex flex-col items-start justify-start truncate">
                        <Name pubkey={validKeyPubkey as string} metadata={validKeyContact?.metadata} />
                        <span class="text-gray-400 text-sm">Click to start a new group with this user</span>
                    </div>
                    <CaretRight size={20} class="ml-auto" />
                </button>
            {:else}
                <div class="flex flex-col gap-6 items-center justify-center h-full">
                    <span class="text-gray-400 my-4">Submit a search to find other people</span>
                </div>
            {/if}
        </div>
    {/if}

</div>
