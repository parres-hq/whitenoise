<script lang="ts">
import { goto } from "$app/navigation";
import { page } from "$app/state";
import { activeAccount, updateAccountsStore } from "$lib/stores/accounts";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { onDestroy, onMount } from "svelte";

let { children } = $props();

let isLoadingAccounts = $state(true);

let unlistenNostrReady: UnlistenFn;
let unlistenAccountUpdated: UnlistenFn;

// Start with true so we don't show until the preflight checks are done
let keyPackagePublished = $state(true);
let keyPackageRelaysPublished = $state(true);
let inboxRelaysPublished = $state(true);

let showPreflightModal = $state(false);
$effect(() => {
    showPreflightModal =
        !keyPackageRelaysPublished || !inboxRelaysPublished || !keyPackagePublished;
});

async function checkPreflight() {
    await updateAccountsStore();
    isLoadingAccounts = false;

    if (!$activeAccount) {
        goto("/login");
    }

    if ($activeAccount) {
        if (!$activeAccount.metadata.display_name || !$activeAccount.metadata.picture) {
            await invoke("query_enriched_contact", {
                pubkey: $activeAccount.pubkey,
                updateAccount: true,
            });
        }
        inboxRelaysPublished = $activeAccount.onboarding.inbox_relays;
        keyPackageRelaysPublished = $activeAccount.onboarding.key_package_relays;
        keyPackagePublished = $activeAccount.onboarding.publish_key_package;
    }
}

onMount(async () => {
    if (!unlistenNostrReady) {
        unlistenNostrReady = await listen<string>("nostr_ready", async (_event) => {
            console.log("Event received on layout page: nostr_ready");
            checkPreflight();
        });
    }

    if (!unlistenAccountUpdated) {
        unlistenAccountUpdated = await listen<string>("account_updated", async (_event) => {
            console.log("Event received on layout page: account_updated");
            await updateAccountsStore();
        });
    }

    checkPreflight();

    // Do you have permission to send a notification?
    let permissionGranted = await isPermissionGranted();

    // If not we need to request it
    if (!permissionGranted) {
        console.log("Requesting permission");
        const permission = await requestPermission();
        permissionGranted = permission === "granted";
    }
});

onDestroy(() => {
    unlistenNostrReady?.();
});
</script>

<main class="flex flex-col min-h-dvh pb-safe-bottom pl-safe-left pr-safe-right">
    {@render children()}
</main>

