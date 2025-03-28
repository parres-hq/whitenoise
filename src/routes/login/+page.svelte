<script lang="ts">
import { goto } from "$app/navigation";
import Button from "$lib/components/ui/button/button.svelte";
import {
    LoginError,
    activeAccount,
    createAccount,
    login,
    updateAccountsStore,
} from "$lib/stores/accounts";
import { isValidHexPubkey } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { expoInOut } from "svelte/easing";
import { type FlyParams, fly } from "svelte/transition";

let nsecOrHex = $state("");
let loading = $state(true);
let loginError = $state<LoginError | null>(null);
let flyParams: FlyParams = { duration: 150, easing: expoInOut, y: window.innerHeight };

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
        if ($activeAccount?.pubkey && isValidHexPubkey($activeAccount?.pubkey)) {
            await invoke("init_nostr_for_current_user");
            console.log("Initialized Nostr for current user");
        }
    });
});

onDestroy(() => {
    unlistenAccountChanged?.();
    unlistenNostrReady?.();
});

async function handleLogin() {
    console.log("clicked login");
    if (loading) return;
    loading = true;
    login(nsecOrHex).catch((error) => {
        console.log("login error", error);
        loginError = error;
        loading = false;
    });
}

async function handleCreateAccount() {
    if (loading) return;
    loading = true;
    createAccount().catch((error) => {
        loginError = error;
        loading = false;
    });
}
</script>

<div class="flex flex-col items-center w-screen h-dvh bg-background">
    <div class="w-full h-2/3 flex flex-col items-center bg-background">
        <div class="relative w-full h-full">
            <img src="images/login-splash.webp" alt="login splash" class="w-full h-full object-cover {loading ? 'animate-pulse' : ''}" />
            <div class="absolute inset-0 bg-gradient-to-t from-background via-transparent from-10% to-transparent"></div>
        </div>
        <div class="flex flex-col self-start mx-4 text-foreground mb-16">
            <h2 class="text-5xl font-normal">Welcome to</h2>
            <h1 class="text-5xl font-semibold">White Noise</h1>
            <p class="text-xl mt-4 font-normal text-muted-foreground">Secure. Distributed. Uncensorable.</p>
        </div>
        <div class="flex flex-col gap-4 w-full px-4 mt-18">
            <Button size="lg" variant="default" onclick={handleLogin} disabled={loading} >
                Sign in with Nostr key
            </Button>
            <Button size="lg" variant="ghost" onclick={handleCreateAccount} disabled={loading}>
                Create a new Nostr key
            </Button>
        </div>
    </div>
</div>
