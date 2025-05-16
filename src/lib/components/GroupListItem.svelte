<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import { type NGroup, NostrMlsGroupType } from "$lib/types/nostr";
import type { EnrichedContact } from "$lib/types/nostr";
import { hexMlsGroupId } from "$lib/utils/group";
import { formatMessageTime } from "$lib/utils/time";
import { invoke } from "@tauri-apps/api/core";
import { _ as t } from "svelte-i18n";
import { latestMessagePreview, nameFromMetadata } from "../utils/nostr";
import GroupAvatar from "./GroupAvatar.svelte";
import Button from "./ui/button/button.svelte";

let { group, selectedChatId = $bindable(null) }: { group: NGroup; selectedChatId?: string | null } =
    $props();

let counterpartyPubkey: string | undefined = $state(undefined);
let enrichedCounterparty: EnrichedContact | undefined = $state(undefined);
let picture: string | undefined = $state(undefined);
let groupName: string | undefined = $state(undefined);
let counterpartyQueried: boolean = $state(false);
let counterpartyFetched: boolean = $state(false);
// TODO: This needs to listen for new messages and update the preview
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
});

$effect(() => {
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
});

$effect(() => {
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
});

$effect(() => {
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

function handleClick(e: MouseEvent) {
    const groupId = hexMlsGroupId(group.mls_group_id);
    // On desktop, update selectedChatId instead of navigating
    if (window.innerWidth >= 768) {
        e.preventDefault();
        selectedChatId = groupId;
    }
}
</script>

<Button
    size="lg"
    variant="ghost"
    href={`/chats/${hexMlsGroupId(group.mls_group_id)}/`}
    class="flex flex-row gap-2 items-center justify-between py-10 px-4 w-full {selectedChatId === hexMlsGroupId(group.mls_group_id) ? "bg-muted" : ""}"
    onclick={handleClick}
>
    <div class="flex flex-row gap-2 items-center flex-1 min-w-0">
        <GroupAvatar bind:groupType={group.group_type} bind:groupName bind:counterpartyPubkey bind:enrichedCounterparty pxSize={56} />
        <div class="truncate">
            <span class="text-lg font-medium truncate">{groupName}</span>
            <p class="text-sm text-muted-foreground whitespace-pre-wrap break-words w-full line-clamp-2">{group.last_message_id ? messagePreview : $t("chats.newChat")}</p>
        </div>
    </div>
    <span class="whitespace-nowrap text-sm text-muted-foreground ml-2">{group.last_message_at ? formatMessageTime(group.last_message_at) : ""}</span>
</Button>
