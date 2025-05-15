<script lang="ts">
import Sheet from "$lib/components/Sheet.svelte";
import { toDataURL } from "qrcode";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";

interface QrCodeSheetProps {
    open: boolean;
    npub: string;
}

let { open = $bindable(false), npub }: QrCodeSheetProps = $props();
let qrCodeUrl = $state("");

onMount(async () => {
    try {
        qrCodeUrl = await toDataURL(npub);
    } catch (error) {
        console.error("Error generating QR code:", error);
    }
});
</script>

<Sheet bind:open>
    {#snippet title()}{$t("settings.qrCode")}{/snippet}
    <div class="flex flex-col items-center justify-center p-8">
        {#if qrCodeUrl}
            <img src={qrCodeUrl} alt="QR Code" class="w-72 h-72" />
        {/if}
    </div>
</Sheet>
