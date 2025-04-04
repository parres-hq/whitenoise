<script lang="ts">
import { goto } from "$app/navigation";
import { page } from "$app/state";
import Avatar from "$lib/components/Avatar.svelte";
import GroupAvatar from "$lib/components/GroupAvatar.svelte";
import HeaderToolbar from "$lib/components/HeaderToolbar.svelte";
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
import CaretLeft from "phosphor-svelte/lib/CaretLeft";
import HardDrives from "phosphor-svelte/lib/HardDrives";
import LockKey from "phosphor-svelte/lib/LockKey";
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
    // rotatingKey = true;
    // await invoke("rotate_key_in_group", { groupId: page.params.id })
    //     .then(() => {
    //         document.getElementById("rotate-key-icon")?.style.setProperty("color", "green");
    //         setTimeout(() => {
    //             document.getElementById("rotate-key-icon")?.style.setProperty("color", "white");
    //         }, 2000);
    //     })
    //     .catch((e) => {
    //         console.error(e);
    //         toast.error("Error rotating key");
    //         rotatingKey = false;
    //         document.getElementById("rotate-key-icon")?.style.setProperty("color", "red");
    //     })
    //     .finally(() => {
    //         rotatingKey = false;
    //     });
}
</script>

{#if group}
    <HeaderToolbar>
        {#snippet left()}
            <button onclick={() => goto(`/chats/${page.params.id}`)} class="p-2 -mr-2">
                <CaretLeft size={30} />
            </button>
        {/snippet}
    </HeaderToolbar>
    <div class="flex flex-col items-center justify-center gap-2 p-4 mb-8">
        <GroupAvatar groupType={group.group_type} {groupName} {counterpartyPubkey} {enrichedCounterparty} pxSize={80} />
        <h1 class="text-2xl font-bold">{groupName}</h1>
        <p class="text-gray-500 flex flex-row items-center gap-2">
            <LockKey size={20} />
            {group.description || "A secure chat"}
        </p>
    </div>
    <div class="mx-4">
    <h2 class="section-title">{members.length} Members</h2>
    <div class="section">
        <ul class="flex flex-col">
            {#each members as member}
                <li class="flex flex-row items-center gap-4 border-b border-gray-700 py-2 last:border-b-0">
                    <Avatar pubkey={member} />
                    <span class="text-base font-medium"><Name pubkey={member} unstyled={true} /></span>
                    {#if admins.includes(member)}
                        <span class="text-xs font-medium text-white bg-purple-600 outline outline-1 outline-purple-300/50 px-2 pt-0.5 rounded-full">Admin</span>
                    {/if}
                </li>
            {/each}
        </ul>
    </div>
    <h2 class="section-title">Group Relays</h2>
    <div class="section">
        <ul class="flex flex-col items-center gap-0">
            {#each Object.entries(groupRelaysWithStatus) as [url, status]}
                <li class="flex flex-row items-center gap-4 py-3 w-full border-b border-gray-700 last:border-b-0">
                    <HardDrives size={24} class={colorForRelayStatus(status)} />
                    <span>
                        {url} -
                        <span class="text-sm font-light">{status}</span>
                    </span>
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
