<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import { type NostrMlsGroup, NostrMlsGroupType } from "$lib/types/nostr";
import type { EnrichedContact } from "$lib/types/nostr";
import { hexMlsGroupId } from "$lib/utils/group";
import { formatMessageTime } from "$lib/utils/time";
import { invoke } from "@tauri-apps/api/core";
import { latestMessagePreview, nameFromMetadata } from "../utils/nostr";
import GroupAvatar from "./GroupAvatar.svelte";

let { group } = $props<{
    group: NostrMlsGroup;
}>();

let counterpartyPubkey: string | undefined = $state(undefined);
let enrichedCounterparty: EnrichedContact | undefined = $state(undefined);
let picture: string | undefined = $state(undefined);
let groupName: string | undefined = $state(undefined);
let counterpartyQueried: boolean = $state(false);
let counterpartyFetched: boolean = $state(false);
let messagePreview: string = $state("");

$effect(() => {
    latestMessagePreview(group.last_message_id).then((preview: string) => {
        messagePreview = preview;
    });

    if (!counterpartyPubkey) {
        counterpartyPubkey =
            group.group_type === NostrMlsGroupType.DirectMessage
                ? group.admin_pubkeys.filter(
                      (pubkey: string) => pubkey !== $activeAccount?.pubkey
                  )[0]
                : undefined;
    }

    if (counterpartyPubkey && !counterpartyQueried) {
        invoke("query_enriched_contact", {
            pubkey: counterpartyPubkey,
            updateAccount: false,
        }).then((userResponse) => {
            enrichedCounterparty = userResponse as EnrichedContact;
            picture = enrichedCounterparty?.metadata?.picture;
            counterpartyQueried = true;
        });
    }

    if (
        counterpartyPubkey &&
        counterpartyQueried &&
        (!enrichedCounterparty?.metadata.picture ||
            !enrichedCounterparty?.metadata.display_name ||
            !enrichedCounterparty?.metadata.name) &&
        !counterpartyFetched
    ) {
        invoke("fetch_enriched_contact", {
            pubkey: counterpartyPubkey,
            updateAccount: false,
        }).then((userResponse) => {
            enrichedCounterparty = userResponse as EnrichedContact;
            picture = enrichedCounterparty?.metadata?.picture;
            counterpartyFetched = true;
        });
    }

    if (
        group.group_type === NostrMlsGroupType.DirectMessage &&
        counterpartyPubkey &&
        enrichedCounterparty
    ) {
        groupName = nameFromMetadata(
            (enrichedCounterparty as EnrichedContact).metadata,
            counterpartyPubkey
        );
    } else {
        groupName = group.name;
    }
});
</script>

<a
    href={`/chats/${hexMlsGroupId(group.mls_group_id)}/`}
    class="flex flex-row gap-2 items-center justify-between px-4 py-3 border-b border-gray-700 hover:bg-gray-700"
>
    <div class="flex flex-row gap-2 items-center">
        <GroupAvatar bind:groupType={group.group_type} bind:groupName bind:counterpartyPubkey bind:enrichedCounterparty pxSize={40} />
        <div class="flex flex-col gap-0">
            <span class="text-lg font-semibold">{groupName}</span>
            <span class="text-sm text-gray-400 {group.last_message_id ? "" : "text-gray-500"} line-clamp-2">{group.last_message_id ? messagePreview : "New chat"}</span>
        </div>
    </div>
    <span class="whitespace-nowrap">{group.last_message_at ? formatMessageTime(group.last_message_at) : ""}</span>
</a>
