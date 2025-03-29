<script lang="ts">
import { goto } from "$app/navigation";
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import * as Sheet from "$lib/components/ui/sheet/index.js";
import {
    LoginError,
    activeAccount,
    createAccount,
    login,
    updateAccountsStore,
} from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import { isValidHexKey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import { onDestroy, onMount } from "svelte";

let nsecOrHex = $state("");
let loading = $state(true);
let loginError = $state<LoginError | null>(null);

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

async function handlePaste() {
    try {
        const text = await readFromClipboard();
        if (text) {
            nsecOrHex = text;
        } else {
            loginError = { name: "ClipboardError", message: "No text found in clipboard" };
        }
    } catch (e) {
        loginError = { name: "ClipboardError", message: "Failed to read from clipboard" };
    }
}
</script>

<div class="flex flex-col items-center w-screen bg-background">
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
            <Sheet.Root onOpenChange={(open) => {
                if (!open) {
                    loginError = null;
                    nsecOrHex = "";
                }
            }}>
                <Sheet.Trigger>
                    <Button variant="outline" class="w-full">Sign in with Nostr key</Button>
                </Sheet.Trigger>
                <Sheet.Content side="bottom" class="pb-safe-bottom">
                    <div class="max-h-[80vh] overflow-y-auto pb-8 px-1">
                        <Sheet.Header class="text-left mb-8">
                            <Sheet.Title>Sign in with your Nostr key</Sheet.Title>
                            <Sheet.Description>
                                Your key is encrypted and stored only on your device.
                            </Sheet.Description>
                        </Sheet.Header>
                        <div class="flex flex-col gap-x-4">
                            <div class="flex flex-row gap-2">
                                <Input
                                    bind:value={nsecOrHex}
                                    type="password"
                                    autofocus={false}
                                    placeholder="nsec1..."
                                    autocomplete="off"
                                    autocapitalize="off"
                                    autocorrect="off"
                                    class="mb-1"
                                />
                                <Button variant="outline" size="icon" onclick={handlePaste} class="shrink-0">
                                    <Paste size={16}/>
                                </Button>
                            </div>
                            <div class="h-8 text-sm text-destructive ml-1">
                                {loginError?.message}
                            </div>
                            <Button size="lg" variant="default" onclick={handleLogin} disabled={loading}>Sign in</Button>
                        </div>
                    </div>
                </Sheet.Content>
            </Sheet.Root>
            <Button size="lg" variant="default" onclick={handleCreateAccount} disabled={loading}>
                Create a new Nostr key
            </Button>
        </div>
    </div>
</div>
