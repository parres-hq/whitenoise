<script lang="ts">
import { goto } from "$app/navigation";
import { activeAccount, updateAccountsStore } from "$lib/stores/accounts";
import { isValidHexKey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";

onMount(async () => {
    updateAccountsStore().then(async () => {
        if ($activeAccount?.pubkey && isValidHexKey($activeAccount?.pubkey)) {
            await invoke("init_nostr_for_current_user");
            console.log("Initialized Nostr for current user");
            setTimeout(() => {
                goto("/chats");
            }, 1500);
        } else {
            goto("/login");
        }
    });
});
</script>

<div class="flex flex-col items-center w-screen h-svh">
    <div class="w-full h-2/3 flex flex-col items-center bg-background">
        <div class="relative w-full h-full">
            <img src="images/login-splash.webp" alt="login splash" class="w-full h-full object-cover animate-pulse" />
            <div class="absolute inset-0 bg-gradient-to-t from-background via-transparent from-10% to-transparent"></div>
        </div>
        <div class="flex flex-col self-start mx-4 text-foreground">
            <h2 class="text-5xl font-normal">Welcome to</h2>
            <h1 class="text-5xl font-semibold">White Noise</h1>
            <p class="text-xl mt-4 font-normal text-muted-foreground">Secure. Distributed. Uncensorable.</p>
        </div>
    </div>
</div>
