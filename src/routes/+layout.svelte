<script lang="ts">
import { Toaster } from "$lib/components/ui/sonner";
import type { Snippet } from "svelte";
import "../app.css";

let { children }: { children: Snippet } = $props();
</script>

<Toaster position="top-center" richColors />
{@render children()}
