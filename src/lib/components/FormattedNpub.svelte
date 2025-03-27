<script lang="ts">
import { getToastState } from "$lib/stores/toast-state.svelte";
import { copyToClipboard } from "$lib/utils/clipboard";
import Copy from "carbon-icons-svelte/lib/Copy.svelte";

let toastState = getToastState();
let { npub, showCopy = false }: { npub: string; showCopy?: boolean } = $props();

let highlightButton = $state(false);

async function copyNpub() {
    if (await copyToClipboard(npub, "npub")) {
        console.log("copied npub");
        highlightButton = true;
        setTimeout(() => {
            highlightButton = false;
        }, 2000);
    } else {
        toastState.add(
            "Error copying npub",
            "There was an error copying your npub, please try again.",
            "error"
        );
    }
}
</script>

    <span class="flex flex-row flex-wrap gap-x-1.5 text-accent-dark dark:text-accent-light text-xs font-mono">
        {#each (npub.match(/.{1,5}/g) || []) as block, idx}
            {#if idx % 2 === 1}
                <span class="opacity-60">{block}</span>
            {:else}
                <span>{block}</span>
            {/if}
        {/each}
        {#if showCopy}
            <button class="ml-2 transition-colors duration-200 text-muted-foreground-light dark:text-muted-foreground-dark {highlightButton ? 'text-green-500!' : ''}" onclick={copyNpub}>
                <Copy size={16} />
            </button>
        {/if}
    </span>
