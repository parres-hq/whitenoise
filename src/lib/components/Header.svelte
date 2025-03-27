<script lang="ts">
import type { Snippet } from "svelte";
let { children }: { children?: Snippet } = $props();
</script>

<div class="sticky top-0 left-0 right-0 flex flex-row items-center gap-2 justify-between p-4 pt-16 bg-primary-light dark:bg-primary-dark">
    {@render children?.()}
</div>
