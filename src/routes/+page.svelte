<script lang="ts">
import { goto } from "$app/navigation";
import Loader from "$lib/components/Loader.svelte";
import { activeAccount, updateAccountsStore } from "$lib/stores/accounts";
import { isValidHexPubkey } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";

onMount(async () => {
    updateAccountsStore().then(async () => {
        if ($activeAccount?.pubkey && isValidHexPubkey($activeAccount?.pubkey)) {
            await invoke("init_nostr_for_current_user");
            console.log("Initialized Nostr for current user");
            setTimeout(() => {
                goto("/chats");
            }, 2000);
        } else {
            goto("/login");
        }
    });
});
</script>

<div class="flex flex-col items-center w-screen h-dvh">
    <div class="w-full h-2/3 flex flex-col items-center bg-background-light dark:bg-background-dark">
        <div class="relative w-full h-full">
            <img src="images/login-splash.webp" alt="login splash" class="w-full h-full object-cover animate-pulse" />
            <div class="absolute inset-0 bg-gradient-to-t from-background-light dark:from-background-dark via-transparent from-10% to-transparent"></div>
        </div>
        <div class="flex flex-col self-start mx-4 text-foreground-light dark:text-foreground-dark">
            <h2 class="text-5xl font-normal">Welcome to</h2>
            <h1 class="text-5xl font-semibold">White Noise</h1>
            <p class="text-xl mt-4 font-normal text-muted-foreground-light dark:text-muted-foreground-dark">Secure. Distributed. Uncensorable.</p>
        </div>
    </div>
</div>
