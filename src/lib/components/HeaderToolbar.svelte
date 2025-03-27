<script lang="ts">
import type { Snippet } from "svelte";

let { left, center, right, alwaysShowCenter, bgColor } = $props<{
    left?: () => Snippet;
    center?: () => Snippet;
    right?: () => Snippet;
    alwaysShowCenter?: boolean;
    bgColor?: string;
}>();

// Fade the center text in when scrolling down
let centerOpacity = $state(alwaysShowCenter ? 1 : 0);
let headerOpacity = $state(0);
window.addEventListener("scroll", () => {
    if (!alwaysShowCenter) {
        centerOpacity = Math.min(window.scrollY / 200, 1);
    }
    headerOpacity = Math.min(window.scrollY / 200, 1) * 0.8;
});
let headerBorderOpacity = $derived(Math.min(headerOpacity * 3, 1));
</script>

<div
    class="flex flex-row justify-between items-center py-8 px-4 sticky h-16 top-0 left-0 right-0 backdrop-blur-sm z-10 {bgColor} md:pt-8 pt-safe-top"
    style="border-bottom: 1px solid rgba(55, 65, 81, {headerBorderOpacity}); background-color: rgba(3, 7, 18, {headerOpacity})"
>
    {#if left}
        {@render left()}
    {:else}
        <span></span>
    {/if}

    {#if center}
        <div class="font-bold text-xl" style="opacity:{centerOpacity};">
            {@render center()}
        </div>
    {:else}
        <span></span>
    {/if}

    {#if right}
        {@render right()}
    {:else}
        <span></span>
    {/if}
</div>
