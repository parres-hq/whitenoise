<script lang="ts">
import type { EnrichedContact, NWelcome } from "$lib/types/nostr";
import { NostrMlsGroupType } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { _ as t } from "svelte-i18n";
import { nameFromMetadata } from "../utils/nostr";
import GroupAvatar from "./GroupAvatar.svelte";
import Sheet from "./Sheet.svelte";
import InviteDetail from "./WelcomeDetail.svelte";
import Button from "./ui/button/button.svelte";

let { welcome }: { welcome: NWelcome } = $props();

let showSheet = $state(false);
let counterpartyPubkey = $derived(welcome.welcomer);
let enrichedInviter: EnrichedContact | undefined = $state(undefined);
let groupName = $state("");
let groupType = $state(
    welcome.member_count === 2 ? NostrMlsGroupType.DirectMessage : NostrMlsGroupType.Group
);

let inviteDescription = $derived(
    welcome.member_count === 2
        ? $t("chats.privateChatInvite")
        : $t("chats.groupChatInvite", { values: { memberCount: welcome.member_count } })
);

$effect(() => {
    if (welcome.welcomer && !enrichedInviter) {
        invoke("query_enriched_contact", {
            pubkey: counterpartyPubkey,
            updateAccount: false,
        }).then((value) => {
            enrichedInviter = value as EnrichedContact;
        });
    }
    if (groupType === NostrMlsGroupType.DirectMessage && welcome.welcomer && enrichedInviter) {
        groupName = nameFromMetadata(
            (enrichedInviter as EnrichedContact).metadata,
            counterpartyPubkey
        );
    } else {
        groupName = welcome.group_name;
    }
});
</script>

<Button
    size="lg"
    variant="ghost"
    class="flex flex-row gap-2 items-center justify-start py-10 px-4 w-full"
    onclick={() => showSheet = true}
>
    <GroupAvatar
        bind:groupType
        bind:groupName
        bind:counterpartyPubkey
        bind:enrichedCounterparty={enrichedInviter}
        pxSize={56}
    />
    <div class="flex flex-col gap-0 items-start">
        <span class="text-lg font-medium">{groupName}</span>
        <span class="text-sm text-muted-foreground">{inviteDescription}</span>
    </div>
</Button>
<Sheet bind:open={showSheet} class="h-[90svh]">
    {#snippet title()}{$t("chats.invitation")}{/snippet}
    <div class="pb-0 px-0">
        <InviteDetail {welcome} bind:enrichedInviter bind:showSheet />
    </div>
</Sheet>
