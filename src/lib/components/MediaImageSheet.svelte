<script lang="ts">
import Sheet from "$lib/components/Sheet.svelte";
import { _ as t } from "svelte-i18n";

let {
    isOpen = $bindable(false),
    imageUrl,
    alt,
} = $props<{
    isOpen: boolean;
    imageUrl: string;
    alt: string;
}>();
</script>

<Sheet bind:open={isOpen} class="opacity-100">
    {#snippet title()}
        <span class="text-foreground px-2 text-base">
            {$t("media.image")}
        </span>
    {/snippet}
    <div class="relative w-full h-full flex items-center justify-center overflow-hidden">
        <div class="w-full h-full flex items-center justify-center p-4">
            <div class="relative w-full h-full flex items-center justify-center">
                <img
                    src={imageUrl}
                    alt={alt}
                    class="max-h-[70vh] rounded-lg object-contain"
                />
            </div>
        </div>
    </div>
</Sheet> 
