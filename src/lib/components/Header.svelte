<script lang="ts">
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
import type { Snippet } from "svelte";
let {
    backLocation,
    title,
    children,
}: {
    backLocation?: string;
    title?: string;
    children?: Snippet;
} = $props();
</script>

<div class="sticky top-0 left-0 right-0 z-20 flex flex-row items-center gap-4 p-4 pt-16 md:pt-4 bg-primary text-primary-foreground">
    {#if backLocation}
        <a href={backLocation}>
            <ChevronLeft size={24} />
        </a>
    {/if}
    {#if title}
        <h1 class="text-2xl py-1">{title}</h1>
    {/if}
    {@render children?.()}
</div>
