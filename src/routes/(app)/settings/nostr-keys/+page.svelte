<script lang="ts">
import { goto } from "$app/navigation";
import Header from "$lib/components/Header.svelte";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
</script>

<Header>
    <div class="flex flex-row gap-4 items-center">
        <button class="header-back-button" onclick={() => goto("/settings")} aria-label="Back to settings">
            <ChevronLeft size={24} />
        </button>
        <h1 class="header-title">Nostr Keys</h1>
    </div>
</Header>

<main class="px-4 flex flex-col">

</main>
