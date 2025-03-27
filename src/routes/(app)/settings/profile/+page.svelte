<script lang="ts">
import { goto } from "$app/navigation";
import Header from "$lib/components/Header.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { invoke } from "@tauri-apps/api/core";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";

async function refetchAccount() {
    await invoke("query_enriched_contact", {
        pubkey: $activeAccount?.pubkey,
        updateAccount: true,
    });
}
</script>

<Header>
    <div class="flex flex-row gap-4 items-center">
        <button class="header-back-button" onclick={() => goto("/settings")} aria-label="Back to settings">
            <ChevronLeft size={24} />
        </button>
        <h1 class="header-title">Profile</h1>
    </div>
</Header>
<main class="px-4 flex flex-col">
    <button class="button-primary" onclick={refetchAccount}>Refetch Account</button>
</main>
