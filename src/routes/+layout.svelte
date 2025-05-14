<script lang="ts">
import { Toaster } from "$lib/components/ui/sonner";
import { initI18n } from "$lib/locales/i18n";
import type { Snippet } from "svelte";
import { onMount } from "svelte";
import "../app.css";
import "../lib/locales/i18n";

let { children }: { children: Snippet } = $props();
let isI18nLoading = $state(true);

onMount(async () => {
    await initI18n();
    isI18nLoading = false;
});
</script>

{#if !isI18nLoading}
    <Toaster position="top-center" richColors />
    {@render children()}
{/if}
