<script lang="ts">
import { goto } from "$app/navigation";
import { initI18n } from "$lib/locales/i18n";
import { activeAccount, updateAccountsStore } from "$lib/stores/accounts";
import { isValidHexKey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";

let isI18nLoading = $state(true);

onMount(async () => {
    await initI18n();
    isI18nLoading = false;

    updateAccountsStore().then(async () => {
        if ($activeAccount?.pubkey && isValidHexKey($activeAccount?.pubkey)) {
            await invoke("init_nostr_for_current_user");
            console.log("Initialized Nostr for current user");
            setTimeout(() => {
                goto("/chats");
            }, 500);
        } else {
            goto("/login");
        }
    });
});
</script>

<div class="flex flex-col h-dvh items-center justify-between w-screen bg-background pl-safe-left pr-safe-right relative">
    <div class="relative w-full">
        <img src="images/login-splash.webp" alt="login splash" class="max-h-[330px] sm:max-h-[400px] md:max-h-[600px] w-full object-cover" />
        <div class="absolute inset-0 bg-gradient-to-t from-background via-transparent from-10% to-transparent"></div>
    </div>
    <div class="flex flex-col self-start mx-4 text-foreground mb-16">
        {#if !isI18nLoading}
            <h2 class="text-5xl font-normal">{$t("login.welcomeTo")}</h2>
            <h1 class="text-5xl font-semibold">White Noise</h1>
            <p class="text-xl mt-4 font-normal text-muted-foreground">{$t("login.slogan")}</p>
        {/if}
    </div>
</div>
