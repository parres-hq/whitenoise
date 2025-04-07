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
import { _ as t } from "svelte-i18n";

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
            error = $t("wallet.unexpectedError");
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
            error = $t("wallet.unexpectedError");
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
            error = $t("wallet.unexpectedError");
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
            error = $t("clipboard.emptyTextError");
        }
    } catch (e) {
        error = $t("clipboard.readError");
    }
}

onMount(async () => {
    checkWalletStatus();
    showScanButton = await invoke("is_mobile");
});
</script>

<Header backLocation="/settings" title={$t("wallet.title")} />

<div class="px-4 py-6 pb-16 md:pb-6 flex flex-col gap-4">
    {#if hasWallet}
        <h2 class="text-xl/7">{$t("wallet.lightningWalletConnected")}</h2>
        <div class="flex flex-row items-center justify-between">
            <span class="text-lg text-muted-foreground">{$t("wallet.balance")}</span>
            <span class="text-lg">
                {#if balanceLoading}
                    <span class="text-lg text-muted-foreground animate-pulse">{$t("shared.loading")}</span>
                {:else}
                    {balance.toLocaleString()} sats
                {/if}
            </span>
        </div>

        <Button size="lg" variant="outline" onclick={handleRemoveWallet} disabled={loading}>
            {loading ? $t("wallet.removing") : $t("wallet.disconnectWallet")}
        </Button>
    {:else}
        <h2 class="text-xl/7">{$t("wallet.connectionDescription")}</h2>
        <div class="flex flex-col gap-0">
            <label for="nwc-uri">{$t("wallet.connectionString")}</label>
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
                <h3 class="text-lg/6 font-medium">
                    {$t("wallet.informationQuestion")}
                </h3>
                <p>
                   {$t("wallet.informationAnswer")}
                   <a href="https://github.com/getAlby/awesome-nwc/blob/master/README.md#nwc-wallets" target="_blank" class="underline">
                    {$t("wallet.informationAnswerLink")}
                   </a>.
                </p>
            </div>
        </div>

        <Button
            size="lg"
            onclick={handleSetWallet}
            disabled={!nwcUri || loading}
            class="text-base font-medium w-full h-fit fixed bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))] md:relative md:left-auto md:right-auto md:mt-6"
        >
            {$t("wallet.connectWallet")}
        </Button>
    {/if}
</div>
