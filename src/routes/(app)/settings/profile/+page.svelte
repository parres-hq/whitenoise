<script lang="ts">
import Header from "$lib/components/Header.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { invoke } from "@tauri-apps/api/core";

async function refetchAccount() {
    await invoke("query_enriched_contact", {
        pubkey: $activeAccount?.pubkey,
        updateAccount: true,
    });
}
</script>

<Header backLocation="/settings" title="Profile" />

<main class="px-4 flex flex-col">
    <button class="button-primary" onclick={refetchAccount}>Refetch Account</button>
</main>
