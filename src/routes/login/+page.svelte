<script lang="ts">
import { goto } from "$app/navigation";
import LoginSheet from "$lib/components/LoginSheet.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import { activeAccount, createAccount, updateAccountsStore } from "$lib/stores/accounts";
import { isValidHexKey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";

let loading = $state(true);

let unlistenAccountChanged: UnlistenFn;
let unlistenNostrReady: UnlistenFn;

onMount(async () => {
    if (!unlistenAccountChanged) {
        unlistenAccountChanged = await listen<string>("account_changed", (_event) => {
            updateAccountsStore().then(async () => {
                console.log("Event received on root page: account_changed");
                loading = false;
                goto("/chats");
            });
        });
    }

    if (!unlistenNostrReady) {
        unlistenNostrReady = await listen<string>("nostr_ready", async (_event) => {
            console.log("Event received on root page: nostr_ready");
        });
    }

    updateAccountsStore().then(async () => {
        loading = false;
        if ($activeAccount?.pubkey && isValidHexKey($activeAccount?.pubkey)) {
            await invoke("init_nostr_for_current_user");
            console.log("Initialized Nostr for current user");
        }
    });
});

onDestroy(() => {
    unlistenAccountChanged?.();
    unlistenNostrReady?.();
});

async function handleCreateAccount() {
    if (loading) return;
    loading = true;
    createAccount().catch((_e) => {
        loading = false;
    });
}
</script>

<div class="flex flex-col items-center w-screen bg-background relative">
    <div class="w-full h-svh flex flex-col items-center bg-background">
        <div class="relative w-full">
            <img src="images/login-splash.webp" alt="login splash" class="max-h-[700px] w-full object-cover {loading ? 'animate-pulse' : ''}" />
            <div class="absolute inset-0 bg-gradient-to-t from-background via-transparent from-10% to-transparent"></div>
        </div>
        <div class="flex flex-col self-start mx-4 text-foreground mb-16">
            <h2 class="text-5xl font-normal">Welcome to</h2>
            <h1 class="text-5xl font-semibold">White Noise</h1>
            <p class="text-xl mt-4 font-normal text-muted-foreground">Secure. Distributed. Uncensorable.</p>
        </div>
        <div class="flex flex-col gap-0 w-full px-0 absolute bottom-0 left-0 right-0">
            <LoginSheet {loading}>
                <Button variant="ghost" class="w-full h-fit text-base font-medium py-4">Sign in with Nostr key</Button>
            </LoginSheet>
            <Button size="lg" variant="default" onclick={handleCreateAccount} disabled={loading} class="w-full h-fit text-base font-medium pt-4 pb-[calc(1rem+var(--sab))]">
                Create a new Nostr key
            </Button>
        </div>
    </div>
</div>
