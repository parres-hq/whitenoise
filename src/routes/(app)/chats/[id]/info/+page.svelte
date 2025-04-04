<script lang="ts">
import { page } from "$app/state";
import Avatar from "$lib/components/Avatar.svelte";
import GroupAvatar from "$lib/components/GroupAvatar.svelte";
import Header from "$lib/components/Header.svelte";
import Name from "$lib/components/Name.svelte";
import { activeAccount, colorForRelayStatus, fetchRelays, relays } from "$lib/stores/accounts";
import {
    type NostrMlsGroup,
    NostrMlsGroupType,
    type NostrMlsGroupWithRelays,
} from "$lib/types/nostr";
import type { EnrichedContact, NEvent } from "$lib/types/nostr";
import { nameFromMetadata } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import Security from "carbon-icons-svelte/lib/Security.svelte";
import { onMount } from "svelte";

let groupWithRelays: NostrMlsGroupWithRelays | undefined = $state(undefined);
let group: NostrMlsGroup | undefined = $state(undefined);
let groupRelays: string[] = $state([]);
let groupRelaysWithStatus: Record<string, string> = $state({});
let counterpartyPubkey: string | undefined = $state(undefined);
let enrichedCounterparty: EnrichedContact | undefined = $state(undefined);
let groupName = $state("");
let members: string[] = $state([]);
let admins: string[] = $state([]);
let rotatingKey = $state(false);

$effect(() => {
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
    let groupResponses = Promise.all([
        invoke("get_group", { groupId: page.params.id }),
        invoke("get_group_members", { groupId: page.params.id }),
        invoke("get_group_admins", { groupId: page.params.id }),
    ]);
    let [groupResponse, membersResponse, adminsResponse] = await groupResponses;
    groupWithRelays = groupResponse as NostrMlsGroupWithRelays;
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
</script>

{#if group}
    <Header backLocation={`/chats/${page.params.id}`} title="Chat details" />
    <div class="flex flex-col items-center justify-center gap-2 p-4 mb-8 mt-12">
        <GroupAvatar groupType={group.group_type} {groupName} {counterpartyPubkey} {enrichedCounterparty} pxSize={80} />
        <h1 class="text-2xl font-bold">{groupName}</h1>
        <p class="text-gray-500 flex flex-row items-center gap-2">
            <Security size={20} />
            {group.description || "A secure chat"}
        </p>
    </div>
    <div class="mx-6">
        <h2 class="text-2xl font-normal mb-4">{members.length} Members</h2>
        <div class="mb-12">
            <ul class="flex flex-col">
                {#each members as member}
                    <li class="flex flex-row items-center gap-4 border-b border-gray-700 py-2 last:border-b-0">
                        <Avatar pubkey={member} pxSize={40} />
                        <span class="text-base font-medium"><Name pubkey={member} unstyled={true} /></span>
                        {#if admins.includes(member)}
                            <span class="text-xs text-violet-50 bg-violet-600 border border-violet-400 px-2 pt-0.5 rounded-full">admin</span>
                        {/if}
                    </li>
                {/each}
            </ul>
        </div>
        <h2 class="text-2xl font-normal mb-4">Group Relays</h2>
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
        <!-- <h2 class="section-title">Actions</h2>
        <div class="section">
            <div class="flex flex-col items-center gap-0">
                <button class="flex flex-row items-center gap-4 py-3 w-full border-b border-gray-700 last:border-b-0" onclick={rotateKey}><Key size={24} class="transition-all duration-300 ease-in-out {rotatingKey ? 'animate-spin': ''}" id="rotate-key-icon" />Rotate Your Key</button>
                <button class="text-red-500 flex flex-row items-center gap-4 py-3 w-full border-b border-gray-700 last:border-b-0" onclick={leaveGroup}><SignOut size={24} />Leave Group</button>
                <button class="text-red-500 flex flex-row items-center gap-4 py-3 w-full border-b border-gray-700 last:border-b-0" onclick={reportSpam}><WarningOctagon size={24} />Report Spam</button>
            </div>
        </div> -->
    </div>
{/if}
