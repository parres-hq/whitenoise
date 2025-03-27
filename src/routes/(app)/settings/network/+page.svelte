<script lang="ts">
import { goto } from "$app/navigation";
import Header from "$lib/components/Header.svelte";
import { colorForRelayStatus, relays } from "$lib/stores/accounts";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
import { HardDrives } from "phosphor-svelte";
</script>

<Header>
    <div class="flex flex-row gap-4 items-center">
        <button class="header-back-button" onclick={() => goto("/settings")} aria-label="Back to settings">
            <ChevronLeft size={24} />
        </button>
        <h1 class="header-title">Network</h1>
    </div>
</Header>
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
