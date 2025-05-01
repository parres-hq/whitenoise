<script lang="ts">
import { goto } from "$app/navigation";
import { initI18n } from "$lib/locales/i18n";
import { activeAccount, updateAccountsStore } from "$lib/stores/accounts";
import { isValidHexKey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";

let isI18nLoading = $state(true);
let loading = $state(true);

onMount(async () => {
    await initI18n();
    isI18nLoading = false;

    updateAccountsStore().then(async () => {
        if ($activeAccount?.pubkey && isValidHexKey($activeAccount?.pubkey)) {
            await invoke("init_nostr_for_current_user");
            console.log("Initialized Nostr for current user");
            setTimeout(() => {
                loading = false;
                goto("/chats");
            }, 500);
        } else {
            goto("/login");
        }
    });
});
</script>

<div class="flex flex-col w-screen bg-background pl-safe-left pr-safe-right relative h-screen">
    <div class="flex flex-col text-foreground">
        <div class="relative top-0 left-0 right-0 w-full flex justify-center z-0">
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
    </div>
    {#if !isI18nLoading}
        <div class="flex flex-col mx-4 text-foreground">
            <h2 class="text-5xl font-normal">{$t("login.welcomeTo")}</h2>
            <h1 class="text-5xl font-semibold">White Noise</h1>
            <p class="text-xl mt-4 font-normal text-muted-foreground">{$t("login.slogan")}</p>
        </div>
    {/if}
</div>
