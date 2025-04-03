<script lang="ts">
import { page } from "$app/stores";
import ChatsList from "$lib/components/ChatsList.svelte";
import * as Resizable from "$lib/components/ui/resizable";
import { activeAccount } from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { Invite, InvitesWithFailures, NostrMlsGroup, ProcessedInvite } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import Tree from "carbon-icons-svelte/lib/Tree.svelte";
import { onDestroy, onMount } from "svelte";
import ChatPage from "./[id]/+page.svelte";

let unlistenAccountChanging: UnlistenFn;
let unlistenAccountChanged: UnlistenFn;
let unlistenNostrReady: UnlistenFn;
let unlistenGroupAdded: UnlistenFn;
let unlistenInviteAccepted: UnlistenFn;
let unlistenInviteDeclined: UnlistenFn;
let unlistenInviteProcessed: UnlistenFn;
let unlistenInviteFailedToProcess: UnlistenFn;

let toastState = getToastState();

let selectedChatId = $state<string | null>(null);

let isLoading = $state(true);
let loadingError = $state<string | null>(null);

let groups = $state<NostrMlsGroup[]>([]);
let invites = $state<Invite[]>([]);
let failures = $state<[string, string | undefined][]>([]);

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


<!-- On desktop, we can show the chats list and the chat page side by side -->
<div class="hidden md:block h-full">
    <Resizable.PaneGroup direction="horizontal">
        <Resizable.Pane defaultSize={35} minSize={20}>
            <ChatsList bind:invites bind:groups />
        </Resizable.Pane>
        <Resizable.Handle class="bg-muted-foreground" />
        <Resizable.Pane defaultSize={65} minSize={50}>
            {#if selectedChatId}
              <ChatPage />
            {:else}
                <div class="sticky top-0 left-0 right-0 z-40 flex flex-row items-center gap-4 p-4 pt-14 bg-primary text-primary-foreground"></div>
                <div class="flex flex-row gap-2 items-center justify-center h-full">
                    <Tree size={32} class="text-muted-foreground mb-4" />
                    <h1 class="text-base font-normal text-muted-foreground">Have you touched grass today?</h1>
                </div>
            {/if}

        </Resizable.Pane>
    </Resizable.PaneGroup>
</div>

<!-- On mobile, show just the chats list -->
<div class="md:hidden">
    <ChatsList bind:invites bind:groups />
</div>
