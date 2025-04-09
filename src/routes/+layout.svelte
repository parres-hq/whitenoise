<script lang="ts">
import { Toaster } from "$lib/components/ui/sonner";
import type { Snippet } from "svelte";
import "../app.css";
import "../lib/locales/i18n";

let { children }: { children: Snippet } = $props();
</script>

<Toaster position="top-center" richColors />
{@render children()}
