<script lang="ts">
import { NostrMlsGroupType } from "$lib/types/nostr";
import type { EnrichedContact } from "$lib/types/nostr";
import { generateWhiteNoiseAvatar } from "$lib/utils/avatar";
import Avatar from "./Avatar.svelte";

let {
    groupType = $bindable(),
    groupName = $bindable(),
    counterpartyPubkey = $bindable(),
    enrichedCounterparty = $bindable(),
    pxSize,
}: {
    groupType: NostrMlsGroupType;
    groupName: string | undefined;
    counterpartyPubkey: string | undefined;
    enrichedCounterparty: EnrichedContact | undefined;
    pxSize: number;
} = $props();

// Generate a white noise avatar for the group
function getGroupWhiteNoiseAvatar() {
    // Use groupName as seed if available, otherwise use a combination of type and counterparty
    const seed = groupName || `${groupType}_${counterpartyPubkey || "unknown"}`;
    return generateWhiteNoiseAvatar(seed, pxSize * 2, 0.8, 2.5);
}
</script>

{#if groupType === NostrMlsGroupType.DirectMessage && counterpartyPubkey && enrichedCounterparty}
    <Avatar picture={enrichedCounterparty?.metadata.picture} pubkey={counterpartyPubkey} {pxSize} />
{:else}
    <div
        class="flex flex-col items-center justify-center rounded-full bg-gray-900"
        style="width: {pxSize}px; height: {pxSize}px; min-width: {pxSize}px; min-height: {pxSize}px;"
    >
        <img src={getGroupWhiteNoiseAvatar()} alt="group avatar" class="shrink-0 w-full h-full rounded-full object-cover" />
    </div>
{/if}
