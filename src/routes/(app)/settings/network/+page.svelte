<script lang="ts">
import { goto } from "$app/navigation";
import Header from "$lib/components/Header.svelte";
import HeaderToolbar from "$lib/components/HeaderToolbar.svelte";
import { colorForRelayStatus, relays } from "$lib/stores/accounts";
import { CaretLeft } from "phosphor-svelte";
import { HardDrives } from "phosphor-svelte";

function goBack() {
    goto("/settings");
}
</script>

<HeaderToolbar>
    {#snippet left()}
        <button class="flex flex-row gap-0.5 items-center" onclick={goBack}>
            <CaretLeft size={24} weight="bold" />
            <span class="font-medium text-lg">Back</span>
        </button>
    {/snippet}
    {#snippet center()}
        <h1>Network Settings</h1>
    {/snippet}
</HeaderToolbar>

<Header title="Network Settings" />
<main class="px-4 flex flex-col">
    <h2 class="section-title">Relays</h2>
    <div class="section">
        <ul class="section-list">
            {#each Object.entries($relays) as [url, status]}
                <li class="section-list-item">
                    <button class="row-button">
                        <HardDrives size={24} class={colorForRelayStatus(status)} />
                        <span>
                            {url} -
                            <span class="text-sm font-light">{status}</span>
                        </span>
                    </button>
                </li>
            {/each}
        </ul>
    </div>
</main>
