<script lang="ts">
import { goto } from "$app/navigation";
import Header from "$lib/components/Header.svelte";
import HeaderToolbar from "$lib/components/HeaderToolbar.svelte";
import {
    NostrWalletConnectError,
    hasNostrWalletConnectUri,
    removeNostrWalletConnectUri,
    setNostrWalletConnectUri,
} from "$lib/stores/accounts";
import { CaretLeft, Lightning } from "phosphor-svelte";
import { onMount } from "svelte";

let hasWallet = false;
let nwcUri = "";
let error = "";
let loading = false;

async function checkWalletStatus() {
    try {
        hasWallet = await hasNostrWalletConnectUri();
        error = "";
    } catch (e) {
        if (e instanceof NostrWalletConnectError) {
            error = e.message;
        } else {
            error = "An unexpected error occurred";
        }
    }
}

async function handleSetWallet() {
    if (!nwcUri) return;

    loading = true;
    try {
        await setNostrWalletConnectUri(nwcUri);
        await checkWalletStatus();
        nwcUri = "";
        error = "";
    } catch (e) {
        if (e instanceof NostrWalletConnectError) {
            error = e.message;
        } else {
            error = "An unexpected error occurred";
        }
    } finally {
        loading = false;
    }
}

async function handleRemoveWallet() {
    loading = true;
    try {
        await removeNostrWalletConnectUri();
        await checkWalletStatus();
        error = "";
    } catch (e) {
        if (e instanceof NostrWalletConnectError) {
            error = e.message;
        } else {
            error = "An unexpected error occurred";
        }
    } finally {
        loading = false;
    }
}

function goBack() {
    goto("/settings");
}

onMount(() => {
    checkWalletStatus();
});
</script>

<HeaderToolbar>
    {#snippet left()}
        <button class="flex flex-row gap-0.5 items-center" on:click={goBack}>
            <CaretLeft size={24} weight="bold" />
            <span class="font-medium text-lg">Back</span>
        </button>
    {/snippet}
    {#snippet center()}
        <h1>Lightning Settings</h1>
    {/snippet}
</HeaderToolbar>

<Header title="Lightning Settings" />
<main class="px-4 flex flex-col">
    <section class="flex flex-col gap-4">
        <h2 class="section-title flex items-center gap-2">
            <Lightning size={24} weight="bold" />
            Nostr Wallet Connect
        </h2>
        
        {#if error}
            <div class="text-red-500 text-sm">{error}</div>
        {/if}

        {#if hasWallet}
            <div class="flex flex-col gap-4">
                <p class="text-green-500">
                    You have already configured your Nostr Wallet Connect
                </p>
                <button
                    class="flex flex-row gap-2 items-center px-2 py-3 hover:bg-gray-700 w-full"
                    on:click={handleRemoveWallet}
                    disabled={loading}
                >
                    {loading ? 'Removing...' : 'Remove Wallet Connection'}
                </button>
            </div>
        {:else}
            <div class="flex flex-col gap-4">
                <div class="form-control w-full">
                    <label class="label" for="nwc-uri">
                        <span class="label-text">Nostr Wallet Connect URI</span>
                    </label>
                    <input
                        type="text"
                        id="nwc-uri"
                        class="w-full bg-transparent border-gray-700 rounded-md"
                        placeholder="nostr+walletconnect://"
                        bind:value={nwcUri}
                    />
                </div>
                <button
                    class="flex flex-row gap-2 items-center px-2 py-3 hover:bg-gray-700 w-full"
                    on:click={handleSetWallet}
                    disabled={!nwcUri || loading}
                >
                    {loading ? 'Saving...' : 'Save Wallet Connection'}
                </button>
            </div>
        {/if}
    </section>
</main>
    