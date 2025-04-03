<script lang="ts">
import type { EnrichedContact, Invite } from "$lib/types/nostr";
import { NostrMlsGroupType } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { nameFromMetadata } from "../utils/nostr";
import GroupAvatar from "./GroupAvatar.svelte";
import InviteDetail from "./Modals/Invites/InviteDetail.svelte";
import Button from "./ui/button/button.svelte";
import * as Sheet from "./ui/sheet";

let { invite }: { invite: Invite } = $props();

let showSheet = $state(false);
let enrichedInviter: EnrichedContact | undefined = $state(undefined);
let groupName = $state("");
let groupType = $state(
    invite.member_count === 2 ? NostrMlsGroupType.DirectMessage : NostrMlsGroupType.Group
);

let inviteDescription = $derived(
    invite.member_count === 2 ? "private chat" : `group with ${invite.member_count} members`
);

$effect(() => {
    if (invite.inviter && !enrichedInviter) {
        invoke("query_enriched_contact", { pubkey: invite.inviter, updateAccount: false }).then(
            (value) => {
                enrichedInviter = value as EnrichedContact;
            }
        );
    }
    if (groupType === NostrMlsGroupType.DirectMessage && invite.inviter && enrichedInviter) {
        groupName = nameFromMetadata((enrichedInviter as EnrichedContact).metadata, invite.inviter);
    } else {
        groupName = invite.group_name;
    }
});
</script>

<Sheet.Root bind:open={showSheet}>
    <Sheet.Trigger>
        <Button
            size="lg"
            variant="ghost"
            class="flex flex-row gap-2 items-center justify-start py-10 px-4 w-full"
        >
            <GroupAvatar
                bind:groupType
                bind:groupName
                bind:counterpartyPubkey={invite.inviter}
                bind:enrichedCounterparty={enrichedInviter}
                pxSize={40}
            />
            <div class="flex flex-col gap-0 items-start">
                <span class="text-lg font-medium">{groupName}</span>
                <span class="text-sm text-muted-foreground">You're invited to join a {inviteDescription}</span>
            </div>
        </Button>
    </Sheet.Trigger>
    <Sheet.Content side="bottom" class="pb-0 px-0 h-[90%]">
        <InviteDetail {invite} bind:enrichedInviter bind:showSheet />
    </Sheet.Content>
</Sheet.Root>


