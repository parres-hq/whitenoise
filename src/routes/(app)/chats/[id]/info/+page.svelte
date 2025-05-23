<script lang="ts">
import { page } from "$app/stores";
import Avatar from "$lib/components/Avatar.svelte";
import GroupAvatar from "$lib/components/GroupAvatar.svelte";
import Header from "$lib/components/Header.svelte";
import Name from "$lib/components/Name.svelte";
import { activeAccount, colorForRelayStatus, fetchRelays, relays } from "$lib/stores/accounts";
import { type NGroup, type NGroupWithRelays, NostrMlsGroupType } from "$lib/types/nostr";
import type { EnrichedContact, NEvent } from "$lib/types/nostr";
import { nameFromMetadata } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import Security from "carbon-icons-svelte/lib/Security.svelte";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";

let {
    selectedChatId = $bindable(),
    showInfoPage = $bindable(false),
}: { selectedChatId?: string; showInfoPage?: boolean } = $props();

let groupWithRelays: NGroupWithRelays | undefined = $state(undefined);
let group: NGroup | undefined = $state(undefined);
let groupRelays: string[] = $state([]);
let groupRelaysWithStatus: Record<string, string> = $state({});
let counterpartyPubkey: string | undefined = $state(undefined);
let enrichedCounterparty: EnrichedContact | undefined = $state(undefined);
let groupName = $state("");
let members: string[] = $state([]);
let admins: string[] = $state([]);

$effect(() => {
    // Check if selectedChatId is in URL query params
    if (!selectedChatId && $page.url.searchParams.has("selectedChatId")) {
        const urlSelectedChatId = $page.url.searchParams.get("selectedChatId");
        if (urlSelectedChatId) {
            selectedChatId = urlSelectedChatId;
        }
    }

    if (group && !counterpartyPubkey && !enrichedCounterparty) {
        counterpartyPubkey =
            group.group_type === NostrMlsGroupType.DirectMessage
                ? group.admin_pubkeys.filter((pubkey) => pubkey !== $activeAccount?.pubkey)[0]
                : undefined;
        if (counterpartyPubkey) {
            invoke("query_enriched_contact", {
                pubkey: counterpartyPubkey,
                updateAccount: false,
            }).then((value) => {
                enrichedCounterparty = value as EnrichedContact;
            });
        }
    }

    if (
        group &&
        group.group_type === NostrMlsGroupType.DirectMessage &&
        counterpartyPubkey &&
        enrichedCounterparty
    ) {
        groupName = nameFromMetadata(enrichedCounterparty.metadata, counterpartyPubkey);
    } else if (group) {
        groupName = group.name;
    }
});

async function loadGroup() {
    // Use selectedChatId from props if available, otherwise use page.params.id
    const groupId = selectedChatId || $page.params.id;

    let groupResponses = Promise.all([
        invoke("get_group", { groupId }),
        invoke("get_group_members", { groupId }),
        invoke("get_group_admins", { groupId }),
    ]);
    let [groupResponse, membersResponse, adminsResponse] = await groupResponses;
    groupWithRelays = groupResponse as NGroupWithRelays;
    group = groupWithRelays.group;
    groupRelays = groupWithRelays.relays;

    fetchRelays().then(() => {
        // Extract matching relays and their status
        groupRelaysWithStatus = Object.fromEntries(
            groupRelays.filter((relay) => relay in $relays).map((relay) => [relay, $relays[relay]])
        );
    });

    members = membersResponse as string[];
    admins = adminsResponse as string[];
}

$effect(() => {
    if (selectedChatId) {
        loadGroup();
    }
});

onMount(async () => {
    await loadGroup();
});

function leaveGroup() {
    console.log("leaveGroup not implemented");
}

function reportSpam() {
    console.log("reportSpam not implemented");
}

async function rotateKey() {
    console.log("rotateKey not implemented");
}

function handleBack() {
    if (window.innerWidth >= 768 && showInfoPage !== undefined) {
        // In desktop mode with panel layout, just toggle the info page off
        showInfoPage = false;
    } else {
        // Normal navigation otherwise
        window.history.back();
    }
}
</script>

{#if group}
    <Header onBackAction={handleBack} title={$t("chats.chatDetails")}>
        <!-- Using onBackAction prop for custom back behavior -->
    </Header>
    <div class="flex flex-col items-center justify-center gap-2 p-4 mb-8 mt-12">
        <GroupAvatar groupType={group.group_type} {groupName} {counterpartyPubkey} {enrichedCounterparty} pxSize={80} />
        <h1 class="text-2xl font-bold">{groupName}</h1>
        <p class="text-gray-500 flex flex-row items-center gap-2">
            <Security size={20} />
            {group.description || "A secure chat"}
        </p>
    </div>
    <div class="mx-6">
        <h2 class="text-2xl font-normal mb-4">{members.length} {$t("chats.members")}</h2>
        <div class="mb-12">
            <ul class="flex flex-col">
                {#each members as member}
                    <li class="flex flex-row items-center gap-4 border-b border-gray-700 py-2 last:border-b-0">
                        <Avatar pubkey={member} pxSize={40} />
                        <span class="text-base font-medium truncate"><Name pubkey={member} unstyled={true} /></span>
                        {#if admins.includes(member)}
                            <span class="text-xs text-violet-50 bg-violet-600 border border-violet-400 px-2 pt-0.5 rounded-full">{$t("chats.admin")}</span>
                        {/if}
                    </li>
                {/each}
            </ul>
        </div>
        <h2 class="text-2xl font-normal mb-4">{$t("chats.groupRelays")}</h2>
        <div class="mb-12">
            <ul class="flex flex-col gap-1">
                {#each Object.entries(groupRelaysWithStatus) as [relay_url, relay_status]}
                <li class="flex items-center justify-between py-2">
                    <span class="text-lg">{relay_url}</span>
                        <div class="flex items-center gap-2">
                            <div class="w-2 h-2 rounded-full {colorForRelayStatus(relay_status)}"></div>
                            <span class="text-xs text-muted-foreground">{relay_status}</span>
                        </div>
                </li>
                {/each}
            </ul>
        </div>
    </div>
{/if}
