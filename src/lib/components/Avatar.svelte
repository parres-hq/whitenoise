<script lang="ts">
import type { EnrichedContact } from "$lib/types/nostr";
import { generateWhiteNoiseAvatar } from "$lib/utils/avatar";
import { invoke } from "@tauri-apps/api/core";

interface Props {
    picture?: string;
    pubkey: string;
    pxSize?: number;
}
let { pubkey, picture, pxSize = 32 }: Props = $props();

let user: EnrichedContact | undefined = $state(undefined);
let avatarImage: string | undefined = $state(picture);
let userFetched: boolean = $state(false);
let previousPubkey: string | undefined = $state(undefined);

$effect(() => {
    // Only reset state if the pubkey actually changed
    if (previousPubkey !== pubkey) {
        previousPubkey = pubkey;
        user = undefined;
        avatarImage = picture;
        userFetched = false;
    }

    // Update avatar image when picture prop changes
    if (picture) {
        avatarImage = picture;
    }

    if (!avatarImage && !userFetched) {
        invoke("query_enriched_contact", {
            pubkey,
            updateAccount: false,
        })
            .then((userResp) => {
                user = userResp as EnrichedContact;
                avatarImage = user.metadata.picture;
                userFetched = true;
            })
            .catch((e) => console.error(e));
    }
});

// Generate a white noise avatar as fallback
function getWhiteNoiseAvatar() {
    return generateWhiteNoiseAvatar(pubkey, pxSize * 2);
}
</script>

<div
    class="flex flex-col items-center justify-center rounded-full bg-gray-900"
    style="width: {pxSize}px; height: {pxSize}px; min-width: {pxSize}px; min-height: {pxSize}px;"
>
    {#if avatarImage}
        <img src={avatarImage} alt="avatar" class="shrink-0 w-full h-full rounded-full object-cover" />
    {:else}
        <img src={getWhiteNoiseAvatar()} alt="avatar" class="shrink-0 w-full h-full rounded-full object-cover" />
    {/if}
</div>
