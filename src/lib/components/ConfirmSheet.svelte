<script lang="ts">
import Sheet from "$lib/components/Sheet.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import CloseLarge from "carbon-icons-svelte/lib/CloseLarge.svelte";

interface ConfirmSheetProps {
    title: string;
    description: string;
    acceptText: string;
    cancelText: string;
    open: boolean;
    acceptFn: () => void;
}

let {
    title,
    description,
    acceptText,
    cancelText,
    open = $bindable(false),
    acceptFn,
}: ConfirmSheetProps = $props();
</script>

<Sheet bind:open={open}>
    {#snippet title()}{title}{/snippet}
    {#snippet description()} {@html description} {/snippet}
    <div class="flex flex-col gap-2 w-full px-4 md:px-8 pb-8 bg-background">
        <Button variant="ghost" size="lg" class="w-full h-fit text-base font-medium py-4 px-0 focus-visible:ring-0 disabled:cursor-not-allowed" onclick={() => open = false}>{cancelText}</Button>
        <Button id="accept-button" variant="default" size="lg" class="text-base font-medium w-full h-fit mx-0 py-4 px-1 focus-visible:ring-0 disabled:cursor-not-allowed" onclick={acceptFn}>{acceptText}</Button>
    </div>
</Sheet>
