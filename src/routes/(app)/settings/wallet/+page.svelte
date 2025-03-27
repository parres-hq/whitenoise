<script lang="ts">
import { goto } from "$app/navigation";
import Button from "$lib/components/Button.svelte";
import Header from "$lib/components/Header.svelte";
import Loader from "$lib/components/Loader.svelte";
import {
    NostrWalletConnectError,
    getNostrWalletConnectBalance,
    hasNostrWalletConnectUri,
    removeNostrWalletConnectUri,
    setNostrWalletConnectUri,
} from "$lib/stores/accounts";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
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

onMount(() => {
    checkWalletStatus();
});
</script>

<Header>
    <div class="flex flex-row gap-4 items-center">
        <button class="header-back-button" onclick={() => goto("/settings")} aria-label="Back to settings">
            <ChevronLeft size={24} />
        </button>
        <h1 class="header-title">Wallet</h1>
    </div>
</Header>
<main class="px-4 py-6 flex flex-col gap-4 min-h-[calc(100vh-128px)] relative">
    {#if hasWallet}
        <h2 class="text-xl/7">Lightning Wallet Connected</h2>
        <div class="flex flex-row items-center justify-between">
            <span class="text-lg text-muted-foreground-light dark:text-muted-foreground-dark">Balance:</span>
            <span class="text-lg">
                {#if balanceLoading}
                    <span class="text-lg text-muted-foreground-light dark:text-muted-foreground-dark animate-pulse">Loading...</span>
                {:else}
                    {balance.toLocaleString()} sats
                {/if}
            </span>
        </div>

        <Button size="lg" variant="outline" handleClick={handleRemoveWallet} disabled={loading}>{loading ? 'Removing...' : 'Remove Wallet Connection'}</Button>
    {:else}
        <h2 class="text-xl/7">Connect your bitcoin lightning wallet to send and receive payments in White Noise.</h2>
        <div class="flex flex-col gap-0">
            <label for="nwc-uri">Connection String</label>
            <div class="flex flex-row gap-2">
                <input bind:value={nwcUri} type="text" id="nwc-uri" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="nostr+walletconnect://..." class="grow"/>
                <button class="border border-input-light dark:border-input-dark p-2 w-10 h-10 flex items-center justify-center" onclick={() => console.log("scan")}>
                    <ScanAlt size={16}  />
                </button>
                <button class="border border-input-light dark:border-input-dark p-2 w-10 h-10 flex items-center justify-center" onclick={() => console.log("paste")}>
                    <Paste size={16} />
                </button>
            </div>
            <div class="text-destructive-light dark:text-destructive-dark text-sm mt-2 min-h-[1.25rem]">
                {error}
            </div>
        </div>
        <div class="flex flex-row gap-3 items-start bg-accent-light dark:bg-accent-dark p-4 text-accent-foreground-light dark:text-accent-foreground-dark mt-12">
            <Information size={24} class="shrink-0" />
            <div class="flex flex-col gap-2">
                <h3 class="text-lg/6 font-medium">Which wallets can I connect?</h3>
                <p>
                    You can connect any wallet that supports Nostr Wallet Connect. See a full list <a href="https://github.com/getAlby/awesome-nwc/blob/master/README.md#nwc-wallets" target="_blank">here</a>.
                </p>
            </div>
        </div>

        <div class="mt-auto pt-6">
            <Button size="lg" handleClick={handleSetWallet} disabled={!nwcUri || loading}>Connect Wallet</Button>
        </div>
    {/if}

    <!-- <section class="flex flex-col gap-4">
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
                    onclick={handleRemoveWallet}
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
                    onclick={handleSetWallet}
                    disabled={!nwcUri || loading}
                >
                    {loading ? 'Saving...' : 'Save Wallet Connection'}
                </button>
            </div>
        {/if}
    </section> -->
</main>


