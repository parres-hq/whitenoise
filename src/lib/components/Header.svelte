<script lang="ts">
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
import type { Snippet } from "svelte";
let {
    backLocation,
    title,
    variant = "normal",
    children,
}: {
    backLocation?: string;
    title?: string;
    variant?: "normal" | "reverse";
    children?: Snippet;
} = $props();
</script>

<div class="sticky top-0 left-0 right-0 z-40 flex flex-row items-center gap-4 p-4 pt-16 {variant === 'reverse' ? 'bg-secondary text-secondary-foreground' : 'bg-primary text-primary-foreground'} ">
    {#if backLocation}
        <a href={backLocation}>
            <ChevronLeft size={24} />
        </a>
    {/if}
    {#if title}
        <h1 class="text-2xl">{title}</h1>
    {/if}
    {@render children?.()}
</div>
