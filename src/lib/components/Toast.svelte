<script lang="ts">
import type { Toast } from "$lib/stores/toast-state.svelte";
import { getToastState } from "$lib/stores/toast-state.svelte";
import CheckCircle from "phosphor-svelte/lib/CheckCircle";
import Info from "phosphor-svelte/lib/Info";
import Warning from "phosphor-svelte/lib/Warning";
import X from "phosphor-svelte/lib/X";
import { fly } from "svelte/transition";

let toastState = getToastState();
let { toast }: { toast: Toast } = $props();

let textColor: string = $derived.by(() => {
    if (toast.type === "error") return "text-red-600";
    if (toast.type === "success") return "text-green-600";
    return "text-blue-600";
});
</script>

<div
    transition:fly={{ duration: 200, y: -20 }}
    class="bg-gray-950/95 opacity-[.97] text-white rounded-md p-4 shadow-lg ring-1 ring-gray-700 relative flex flex-row gap-2 justify-between"
>
    <div class="flex flex-col gap-2">
        <h3 class="text-lg font-bold flex flex-row gap-2 items-center {textColor}">
            {#if toast.type === "error"}
                <Warning size={20} />
            {:else if toast.type === "success"}
                <CheckCircle size={20} />
            {:else}
                <Info size={20} />
            {/if}
            {toast.title}
        </h3>
        <p>{toast.message}</p>
    </div>
    <X
        size={24}
        class="cursor-pointer text-gray-400 hover:text-white whitespace-nowrap shrink-0"
        onclick={() => toastState.remove(toast.id)}
    />
</div>
