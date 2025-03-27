<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import GroupListItem from "$lib/components/GroupListItem.svelte";
import Header from "$lib/components/Header.svelte";
import InviteListItem from "$lib/components/InviteListItem.svelte";
import Loader from "$lib/components/Loader.svelte";
import ContactsList from "$lib/components/Modals/Contacts/ContactsList.svelte";
import Modal from "$lib/components/Modals/Modal.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { Invite, InvitesWithFailures, NostrMlsGroup, ProcessedInvite } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import AddLarge from "carbon-icons-svelte/lib/AddLarge.svelte";
import Chat from "carbon-icons-svelte/lib/Chat.svelte";
import Search from "carbon-icons-svelte/lib/Search.svelte";
import { PlusCircle, Warning } from "phosphor-svelte";
import { onDestroy, onMount } from "svelte";

let unlistenAccountChanging: UnlistenFn;
let unlistenAccountChanged: UnlistenFn;
let unlistenNostrReady: UnlistenFn;
let unlistenGroupAdded: UnlistenFn;
let unlistenInviteAccepted: UnlistenFn;
let unlistenInviteDeclined: UnlistenFn;
let unlistenInviteProcessed: UnlistenFn;
let unlistenInviteFailedToProcess: UnlistenFn;

let toastState = getToastState();

let showModal = $state(false);

let isLoading = $state(true);
let loadingError = $state<string | null>(null);

let groups = $state<NostrMlsGroup[]>([]);
let invites = $state<Invite[]>([]);
let failures = $state<[string, string | undefined][]>([]);
let failuresExpanded = $state(false);

async function loadEvents() {
    isLoading = true;
    try {
        const [groupsResponse, invitesResponse] = await Promise.all([
            invoke("get_groups"),
            invoke("get_invites"),
        ]);
        groups = (groupsResponse as NostrMlsGroup[]).sort(
            (a, b) => b.last_message_at - a.last_message_at
        );

        invites = (invitesResponse as InvitesWithFailures).invites;
        failures = (invitesResponse as InvitesWithFailures).failures;
    } catch (error) {
        loadingError = error as string;
        console.log(error);
    } finally {
        isLoading = false;
    }
}

onMount(async () => {
    if ($activeAccount) {
        await loadEvents();
    }

    if (!unlistenAccountChanging) {
        unlistenAccountChanging = await listen<string>("account_changing", async (_event) => {
            console.log("Event received on chats page: account_changing");
            isLoading = true;
            groups = [];
            invites = [];
        });
    }

    if (!unlistenAccountChanged) {
        unlistenAccountChanged = await listen<string>("account_changed", async (_event) => {
            console.log("Event received on chats page: account_changed");
        });
    }

    if (!unlistenNostrReady) {
        unlistenNostrReady = await listen<string>("nostr_ready", async (_event) => {
            console.log("Event received on chats page: nostr_ready");
            if ($activeAccount) {
                await loadEvents();
            }
        });
    }

    if (!unlistenGroupAdded) {
        unlistenGroupAdded = await listen<NostrMlsGroup>("group_added", (event) => {
            const addedGroup = event.payload as NostrMlsGroup;
            console.log("Event received on chats page: group_added", addedGroup);
            groups = [...groups, addedGroup];
        });
    }

    if (!unlistenInviteAccepted) {
        unlistenInviteAccepted = await listen<Invite>("invite_accepted", (event) => {
            const acceptedInvite = event.payload as Invite;
            console.log("Event received on chats page: invite_accepted", acceptedInvite);
            invites = invites.filter((invite) => invite.event.id !== acceptedInvite.event.id);
        });
    }

    if (!unlistenInviteDeclined) {
        unlistenInviteDeclined = await listen<Invite>("invite_declined", (event) => {
            const declinedInvite = event.payload as Invite;
            console.log("Event received on chats page: invite_declined", declinedInvite);
            invites = invites.filter((invite) => invite.event.id !== declinedInvite.event.id);
        });
    }

    if (!unlistenInviteProcessed) {
        unlistenInviteProcessed = await listen<Invite>("invite_processed", async (_event) => {
            let invitesResponse = await invoke("get_invites");
            invites = (invitesResponse as InvitesWithFailures).invites;
            failures = (invitesResponse as InvitesWithFailures).failures;
        });
    }

    if (!unlistenInviteFailedToProcess) {
        unlistenInviteFailedToProcess = await listen<ProcessedInvite>(
            "invite_failed_to_process",
            (event) => {
                const failedInvite = event.payload as ProcessedInvite;
                console.log("Event received on chats page: invite_failed_to_process", failedInvite);
                failures = [...failures, [failedInvite.event_id, failedInvite.failure_reason]];
            }
        );
    }
});

onDestroy(() => {
    unlistenAccountChanging?.();
    unlistenAccountChanged?.();
    unlistenNostrReady?.();
    unlistenGroupAdded?.();
    unlistenInviteAccepted?.();
    unlistenInviteDeclined?.();
    unlistenInviteProcessed?.();
    unlistenInviteFailedToProcess?.();
    toastState.cleanup();
});
</script>

<Header>
    <a href="/settings" class="no-underline!">
        <Avatar pubkey={$activeAccount!.pubkey} />
    </a>
    <div class="flex flex-row items-center gap-6">
        <Search size={24} class="text-primary-dark dark:text-primary-light" />
        <AddLarge size={24} class="text-primary-dark dark:text-primary-light" />
    </div>
</Header>
<main class="">
    {#if isLoading}
        <div class="flex justify-center items-center mt-20 w-full">
            <Loader size={40} fullscreen={false} />
        </div>
    {:else if loadingError}
        <div class="text-red-500 px-4 font-medium flex flex-col gap-2">
            <span>Sorry, we couldn't load your chats because of an error.</span>
            <pre class="font-mono p-2 rounded-md ring-1 ring-red-500/30">{loadingError}</pre>
        </div>
    {:else}
        <div class="flex flex-col gap-0">
            {#if invites.length === 0 && groups.length === 0}
                <div class="flex flex-col gap-2 items-center justify-center flex-1 pt-40 text-muted-foreground-light dark:text-muted-foreground-dark">
                    <Chat size={32} class="text-muted-dark dark:text-muted-light" />
                    <span>No chats found</span>
                    <span>Click the "+" button to start a new chat</span>
                </div>
            {/if}
            {#each invites as invite}
                <InviteListItem {invite} />
            {/each}
            {#each groups as group}
                <GroupListItem {group} />
            {/each}
            {#if failures.length > 0}
                <div class="flex flex-col fixed bottom-[calc(4.75rem+env(safe-area-inset-bottom))] md:bottom-0 w-full bg-gray-900 border-t border-gray-700">
                    <button
                        class="flex flex-row gap-2 items-center px-4 py-3 border-b border-gray-700 hover:bg-gray-700"
                        onclick={() => failuresExpanded = !failuresExpanded}
                    >
                        <Warning size={20} class="text-yellow-500" />
                        <span>{failures.length} unprocessable {failures.length === 1 ? 'invite' : 'invites'}</span>
                        <span class="ml-auto text-sm text-gray-400">{failuresExpanded ? 'Hide' : 'Show'}</span>
                    </button>

                    {#if failuresExpanded}
                        {#each failures as failure}
                            <div class="flex flex-row gap-2 items-center px-4 py-3 border-b border-gray-700 hover:bg-gray-700 pl-8 bg-gray-800/50 truncate">
                                <span>{failure[1]}</span>
                            </div>
                        {/each}
                    {/if}
                </div>
            {/if}
        </div>
    {/if}
</main>

{#if showModal}
    <Modal initialComponent={ContactsList} modalProps={{}} bind:showModal />
{/if}
