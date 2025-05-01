<script lang="ts">
import { copyToClipboard } from "$lib/utils/clipboard";
import Copy from "carbon-icons-svelte/lib/Copy.svelte";
import { toast } from "svelte-sonner";

let {
    npub,
    showCopy = false,
    centered = false,
}: { npub: string; showCopy?: boolean; centered?: boolean } = $props();

let highlightButton = $state(false);

async function copyNpub() {
    if (await copyToClipboard(npub, "npub")) {
        highlightButton = true;
        setTimeout(() => {
            highlightButton = false;
        }, 2000);
    } else {
        toast.error("Error copying npub");
    }
}
</script>

    <span class="flex flex-row z-0 flex-wrap gap-x-2 text-accent-dark dark:text-accent-light text-xs font-mono {centered ? 'justify-center text-center' : ''}">
        {#each (npub.match(/.{1,5}/g) || []) as block, idx}
            {#if idx % 2 === 1}
                <span class="opacity-60">{block}</span>
            {:else}
                <span>{block}</span>
            {/if}
        {/each}
        {#if showCopy}
            <button class="ml-2 transition-colors duration-200 text-muted-foreground-light dark:text-muted-foreground-dark {highlightButton ? 'text-green-500' : ''}" onclick={copyNpub}>
                <Copy size={16} />
            </button>
        {/if}
    </span>
