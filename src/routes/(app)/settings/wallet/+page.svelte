<script lang="ts">
import Header from "$lib/components/Header.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import {
    NostrWalletConnectError,
    getNostrWalletConnectBalance,
    hasNostrWalletConnectUri,
    removeNostrWalletConnectUri,
    setNostrWalletConnectUri,
} from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import { invoke } from "@tauri-apps/api/core";
import Information from "carbon-icons-svelte/lib/Information.svelte";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import ScanAlt from "carbon-icons-svelte/lib/ScanAlt.svelte";
import { onMount } from "svelte";

let hasWallet = $state(false);
let balance = $state(0);
let nwcUri = $state("");
let error = $state("");
let loading = $state(false);
let balanceLoading = $state(false);
let showScanButton = $state(false);

// TODO: Show errors to the user if something goes wrong loading things
async function checkWalletStatus() {
    try {
        hasWallet = await hasNostrWalletConnectUri();
        if (hasWallet) {
            balanceLoading = true;
            balance = await getNostrWalletConnectBalance();
            balanceLoading = false;
        }
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

async function handlePaste() {
    try {
        const text = await readFromClipboard();
        if (text) {
            nwcUri = text;
        } else {
            error = "No text found in clipboard";
        }
    } catch (e) {
        error = "Failed to read from clipboard";
    }
}

onMount(async () => {
    checkWalletStatus();
    showScanButton = await invoke("is_mobile");
});
</script>

<Header backLocation="/settings" title="Wallet" />

<main class="px-4 py-6 flex flex-col gap-4 relative">
    {#if hasWallet}
        <h2 class="text-xl/7">Lightning Wallet Connected</h2>
        <div class="flex flex-row items-center justify-between">
            <span class="text-lg text-muted-foreground">Balance:</span>
            <span class="text-lg">
                {#if balanceLoading}
                    <span class="text-lg text-muted-foreground animate-pulse">Loading...</span>
                {:else}
                    {balance.toLocaleString()} sats
                {/if}
            </span>
        </div>

        <Button size="lg" variant="outline" onclick={handleRemoveWallet} disabled={loading}>{loading ? 'Removing...' : 'Disconnect Wallet'}</Button>
    {:else}
        <h2 class="text-xl/7">Connect your bitcoin lightning wallet to send and receive payments in White Noise.</h2>
        <div class="flex flex-col gap-0">
            <label for="nwc-uri">Connection String</label>
            <div class="flex flex-row gap-2">
                <input bind:value={nwcUri} type="text" id="nwc-uri" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="nostr+walletconnect://..." class="grow"/>
                {#if showScanButton}
                    <button class="border border-input p-2 w-10 h-10 flex items-center justify-center" onclick={() => console.log("scan")}>
                        <ScanAlt size={16}  />
                    </button>
                {/if}
                <Button variant="outline" size="icon" onclick={handlePaste}>
                    <Paste size={16} />
                </Button>
            </div>
            <div class="text-destructive text-sm mt-2 min-h-[1.25rem]">
                {error}
            </div>
        </div>
        <div class="flex flex-row gap-3 items-start bg-accent p-4 text-accent-foreground mt-12">
            <Information size={24} class="shrink-0" />
            <div class="flex flex-col gap-2">
                <h3 class="text-lg/6 font-medium">Which wallets can I connect?</h3>
                <p>
                    You can connect any wallet that supports Nostr Wallet Connect. See a full list <a href="https://github.com/getAlby/awesome-nwc/blob/master/README.md#nwc-wallets" target="_blank" class="underline">here</a>.
                </p>
            </div>
        </div>

        <div class="mt-auto pt-6">
            <Button size="lg" onclick={handleSetWallet} disabled={!nwcUri || loading} class="w-full">Connect Wallet</Button>
        </div>
    {/if}
</main>


