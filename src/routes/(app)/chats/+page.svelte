<script lang="ts">
import { pushState } from "$app/navigation";
import { page } from "$app/stores";
import ChatsList from "$lib/components/ChatsList.svelte";
import * as Resizable from "$lib/components/ui/resizable";
import { activeAccount } from "$lib/stores/accounts";
import type { Invite, InvitesWithFailures, NostrMlsGroup, ProcessedInvite } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import Tree from "carbon-icons-svelte/lib/Tree.svelte";
import { onDestroy, onMount } from "svelte";
import { _ as t } from "svelte-i18n";
import ChatPage from "./[id]/+page.svelte";
import InfoPage from "./[id]/info/+page.svelte";

let unlistenAccountChanging: UnlistenFn;
let unlistenAccountChanged: UnlistenFn;
let unlistenNostrReady: UnlistenFn;
let unlistenGroupAdded: UnlistenFn;
let unlistenInviteAccepted: UnlistenFn;
let unlistenInviteDeclined: UnlistenFn;
let unlistenInviteProcessed: UnlistenFn;
let unlistenInviteFailedToProcess: UnlistenFn;

let selectedChatId: string | null = $state(null);
let showInfoPage: boolean = $state(false);
let isLoading = $state(true);
let loadingError: string | null = $state(null);
let groups: NostrMlsGroup[] = $state([]);
let invites: Invite[] = $state([]);
let failures: [string, string | undefined][] = $state([]);

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
});

$effect(() => {
    if (selectedChatId && $page.state.selectedChatId !== selectedChatId) {
        // Update URL without navigation on desktop
        if (window.innerWidth >= 768) {
            // md breakpoint
            const href = `/chats/${selectedChatId}`;
            pushState(href, { selectedChatId });
        }
    }
});
</script>


<!-- On desktop, we show the chats list and the chat page side by side -->
<div class="hidden md:block">
    <Resizable.PaneGroup direction="horizontal">
        <Resizable.Pane defaultSize={35} minSize={20}>
            <div class="flex w-full h-svh">
                <div class="w-full overflow-y-auto overscroll-none">
                    <div class="max-w-full">
                        <ChatsList bind:invites bind:groups bind:selectedChatId />
                    </div>
                </div>
            </div>
        </Resizable.Pane>
        <Resizable.Handle class="bg-muted-foreground" />
        <Resizable.Pane defaultSize={65} minSize={50}>
            <div class="flex w-full h-svh">
                <div class="w-full overflow-y-auto">
                    {#if selectedChatId}
                        {#if showInfoPage}
                            <InfoPage bind:selectedChatId bind:showInfoPage />
                        {:else}
                            <ChatPage bind:selectedChatId bind:showInfoPage />
                        {/if}
                    {:else}
                        <div class="sticky top-0 left-0 right-0 z-40 flex flex-row items-center gap-4 p-4 pt-14 bg-primary text-primary-foreground"></div>
                        <div class="flex flex-row gap-2 items-center justify-center h-full">
                            <Tree size={32} class="text-muted-foreground mb-4" />
                            <h1 class="text-base font-normal text-muted-foreground">{$t("chats.noChatSelected")}</h1>
                        </div>
                    {/if}
                </div>
            </div>
        </Resizable.Pane>
    </Resizable.PaneGroup>
</div>

<!-- On mobile, show just the chats list -->
<div class="md:hidden">
    <div class="max-w-full">
        <ChatsList bind:invites bind:groups />
    </div>
</div>
