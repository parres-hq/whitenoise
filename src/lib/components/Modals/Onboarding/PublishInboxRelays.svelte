<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { PushView } from "$lib/types/modal";
import { isValidWebSocketURL } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash } from "phosphor-svelte";
import OnboardingNumbers from "./OnboardingNumbers.svelte";
import PublishKeyPackageRelays from "./PublishKeyPackageRelays.svelte";

let toastState = getToastState();

let inboxRelays: string[] = $state(
    import.meta.env.DEV ? ["ws://localhost:8080", "ws://localhost:7777"] : ["wss://auth.nostr1.com"]
);
let newInboxRelay: string = $state("");
let inputError: string | null = $state(null);

let {
    pushView,
    inboxRelaysPublished = $bindable(),
    keyPackageRelaysPublished = $bindable(),
    keyPackagePublished = $bindable(),
} = $props<{
    pushView: PushView;
    inboxRelaysPublished: boolean;
    keyPackageRelaysPublished: boolean;
    keyPackagePublished: boolean;
}>();

function goToKeyPackageRelays(): void {
    pushView(PublishKeyPackageRelays, {
        inboxRelaysPublished,
        keyPackageRelaysPublished,
        keyPackagePublished,
    });
}

function addInboxRelay(): void {
    if (!newInboxRelay.trim()) {
        inputError = "Please enter a relay URL";
        return;
    }

    if (!isValidWebSocketURL(newInboxRelay)) {
        inputError = "Please enter a valid WebSocket URL (ws:// or wss://)";
        return;
    }

    inboxRelays = [...inboxRelays, newInboxRelay];
    newInboxRelay = "";
    inputError = null;
}

async function publishInboxRelays() {
    await invoke("publish_relay_list", {
        relays: inboxRelays,
        kind: 10050,
    })
        .then(async () => {
            inboxRelaysPublished = true;
            await invoke("update_account_onboarding", {
                pubkey: $activeAccount?.pubkey,
                inboxRelays: true,
                keyPackageRelays: !!keyPackageRelaysPublished,
                publishKeyPackage: !!keyPackagePublished,
            });
            goToKeyPackageRelays();
        })
        .catch((e) => {
            toastState.add("Couldn't publish inbox relays", e, "error");
            console.error(e);
        });
}
</script>

<div class="flex flex-col gap-10 mt-10 items-center w-full md:w-2/3 lg:w-1/2 mx-auto">
    <div>
        <OnboardingNumbers currentStep={1} {inboxRelaysPublished} {keyPackageRelaysPublished} {keyPackagePublished} />
        <p class="mt-4">
            First, we'll need to specify your inbox relays. These are the relays where other users can send you messages
            and only you can read events meant for you.
        </p>
    </div>
    <div class="w-full">
        <h3 class="text-lg border-b border-gray-700 mb-2 font-medium text-white">Inbox relays</h3>
        {#each inboxRelays as relay}
            <div class="flex flex-row gap-2">
                <div class="text-white">{relay}</div>
                <button class="button-secondary" onclick={() => (inboxRelays = inboxRelays.filter((r) => r !== relay))}>
                    <Trash size={20} />
                </button>
            </div>
        {/each}
        <div class="flex flex-col gap-2 mt-8">
            <div class="flex flex-row gap-2">
                <input
                    type="text"
                    bind:value={newInboxRelay}
                    class="w-full bg-transparent border-gray-700 rounded-md"
                    class:border-red-500={inputError}
                />
                <button class="button-outline flex flex-row gap-2 items-center whitespace-nowrap" onclick={addInboxRelay}>
                    <Plus size={18} /> Add relay
                </button>
            </div>
            {#if inputError}
                <span class="text-red-500 text-sm">{inputError}</span>
            {/if}
        </div>
    </div>
    <button class="button-primary" onclick={publishInboxRelays}> Publish a new inbox relays event </button>
    <button class="button-outline" onclick={goToKeyPackageRelays}> Skip this step </button>
</div>
