<script lang="ts">
import type { ModalView } from "$lib/types/modal";
import CaretLeft from "phosphor-svelte/lib/CaretLeft";
import X from "phosphor-svelte/lib/X";
import { onDestroy } from "svelte";
import type { Component } from "svelte";
import { fade, fly } from "svelte/transition";
import HeaderToolbar from "../HeaderToolbar.svelte";

let {
    initialComponent,
    modalProps = {},
    showModal = $bindable(),
}: {
    initialComponent: Component;
    modalProps: Record<string, unknown>;
    showModal: boolean;
} = $props();

// Stack to keep track of views/pages
let viewStack: ModalView[] = $state([
    {
        component: initialComponent,
        modalProps,
    },
]);
let currentView: ModalView = $derived(viewStack[viewStack.length - 1]);

// Navigation methods
export function pushView(component: Component, modalProps: Record<string, unknown> = {}): void {
    viewStack = [...viewStack, { component, modalProps }];
}

export function popView(): void {
    if (viewStack.length > 1) {
        viewStack = viewStack.slice(0, -1);
    }
}

export function closeModal(): void {
    showModal = false;
}

// Lock/unlock scroll when modal opens/closes
$effect(() => {
    if (showModal) {
        document.body.style.overflow = "hidden";
    } else {
        document.body.style.overflow = "auto";
    }
});

// Cleanup on component destroy
onDestroy(() => {
    document.body.style.overflow = "auto";
});
</script>

<div
    class="fixed inset-0 -bottom-10 md:p-8 bg-black bg-opacity-50 flex items-end z-20"
    transition:fade={{ duration: 100 }}
>
    <div
        class="flex flex-col gap-4 bg-gray-800 ring-2 ring-gray-700 rounded-t-xl w-full h-full lg:h-[90vh] z-30 pb-40 overflow-y-scroll"
        transition:fly={{ y: 800, duration: 300 }}
    >
        <HeaderToolbar bgColor="bg-gray-800">
            {#snippet left()}
                {#if viewStack.length > 1}
                    <button class="flex flex-row gap-0.5 items-end" onclick={popView}>
                        <CaretLeft size={24} />
                        <span class="text-xl font-medium">Back</span>
                    </button>
                {/if}
            {/snippet}

            {#snippet center()}<h1>{currentView.modalProps?.title ?? ""}</h1>{/snippet}

            {#snippet right()}
                <div>
                    <button onclick={() => (showModal = false)}>
                        <X size={30} />
                    </button>
                </div>
            {/snippet}
        </HeaderToolbar>
        <div class="p-4">
            <currentView.component {...currentView.modalProps} {pushView} {popView} {closeModal} />
        </div>
    </div>
</div>
