<script lang="ts">
import { goto } from "$app/navigation";
import Header from "$lib/components/Header.svelte";
import HeaderToolbar from "$lib/components/HeaderToolbar.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { invoke } from "@tauri-apps/api/core";
import { CaretLeft } from "phosphor-svelte";

function goBack() {
    goto("/settings");
}

async function refetchAccount() {
    await invoke("query_enriched_contact", {
        pubkey: $activeAccount?.pubkey,
        updateAccount: true,
    });
}
</script>

<HeaderToolbar>
    {#snippet left()}
        <button class="flex flex-row gap-0.5 items-end" onclick={goBack}>
            <CaretLeft size={24} />
            <span class="text-xl font-medium">Back</span>
        </button>
    {/snippet}
    {#snippet center()}
        <h1>Account</h1>
    {/snippet}
</HeaderToolbar>

<Header title="Account" />
<main class="px-4 flex flex-col">
    <button class="button-primary" onclick={refetchAccount}>Refetch Account</button>
</main>
