<script lang="ts">
import { goto } from "$app/navigation";
import LoginSheet from "$lib/components/LoginSheet.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import { activeAccount, createAccount, updateAccountsStore } from "$lib/stores/accounts";
import { isValidHexKey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { _ as t } from "svelte-i18n";

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

<div class="flex flex-col h-dvh items-center justify-start w-screen bg-background pl-safe-left pr-safe-right relative">
    <div class="relative w-full flex justify-center">
        <!-- Glitchy background image -->
        <img src="images/login-splash-bg.webp" alt="login splash" class="absolute inset-0 w-full h-full object-cover pointer-events-none select-none {loading ? 'animate-pulse' : ''}" />
        <!-- Overlay logo image, centered -->
        <div class="absolute inset-0 flex items-center justify-center">
            <img src="images/login-logo.webp" alt="White Noise Logo" class="w-40 md:w-48 drop-shadow-lg" style="max-width: 40vw;" />
        </div>
        <!-- Gradient overlay for fade effect -->
        <div class="absolute inset-0 bg-gradient-to-t from-background via-transparent from-10% to-transparent"></div>
        <!-- Spacer for aspect ratio -->
        <div class="invisible w-full pt-[120%] sm:pt-[80%] md:pt-[60%] lg:pt-[50%] xl:pt-[40%]"></div>
    </div>
    <div class="flex flex-col self-start mx-4 text-foreground">
        <h2 class="text-5xl font-normal">{$t("login.welcomeTo")}</h2>
        <h1 class="text-5xl font-semibold">White Noise</h1>
        <p class="text-xl mt-4 font-normal text-muted-foreground">{$t("login.slogan")}</p>
    </div>
    <div class="w-full flex flex-col gap-0 mb-0 mt-auto">
        <LoginSheet {loading}>
            <Button variant="ghost" class="w-full h-fit text-base font-medium py-4">{$t("login.signInWithNostrKey")}</Button>
        </LoginSheet>
        <Button size="lg" variant="default" onclick={handleCreateAccount} disabled={loading} class="w-full h-fit text-base font-medium pt-4 pb-[calc(1rem+var(--sab))]">
            {$t("login.createNewNostrKey")}
        </Button>
    </div>
</div>
