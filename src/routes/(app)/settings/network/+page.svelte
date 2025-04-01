<script lang="ts">
import Header from "$lib/components/Header.svelte";
import { colorForRelayStatus, relays } from "$lib/stores/accounts";
import HardDrives from "phosphor-svelte/lib/HardDrives";
</script>

<Header backLocation="/settings" title="Network" />

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
