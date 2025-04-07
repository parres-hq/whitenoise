<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import Header from "$lib/components/Header.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import Copy from "carbon-icons-svelte/lib/Copy.svelte";
import View from "carbon-icons-svelte/lib/View.svelte";
import ViewOff from "carbon-icons-svelte/lib/ViewOff.svelte";
import Warning from "carbon-icons-svelte/lib/Warning.svelte";
import { onMount } from "svelte";
import { toast } from "svelte-sonner";

let showPrivateKey = $state(false);
let nsec = $state("");

onMount(async () => {
    if (!$activeAccount) return;
    await invoke<string>("export_nsec", {
        pubkey: $activeAccount.pubkey,
    })
        .then((value: string) => {
            nsec = value;
        })
        .catch((error) => {
            console.error(error);
        });
});

async function copyPublicKey() {
    if (!$activeAccount) return;
    const npub = npubFromPubkey($activeAccount.pubkey);
    await navigator.clipboard.writeText(npub);
    toast.success("Public key copied to clipboard");
}

async function copyPrivateKey() {
    await navigator.clipboard.writeText(nsec);
    toast.success("Private key copied to clipboard");
}
</script>

<Header backLocation="/settings" title="Nostr Keys" />

<div class="px-4 flex flex-col gap-12 py-6">
    <section class="flex flex-col gap-4">
        <h2 class="text-2xl font-normal">Public Key</h2>
        <p class="text-base text-muted-foreground">
            Your public key is your unique identifier in the Nostr network, enabling others to verify and recognize your messages. Share it openly!
        </p>

        <div class="flex items-center gap-3">
            <Avatar pubkey={$activeAccount!.pubkey} pxSize={48} />
            <FormattedNpub npub={npubFromPubkey($activeAccount!.pubkey)} showCopy={false} />
        </div>
        <Button
            size="lg"
            variant="secondary"
            class="w-full flex flex-row items-center justify-center gap-2"
            onclick={copyPublicKey}
        >
            <Copy size={20} />
            Copy Public Key
        </Button>
    </section>

    <section class="flex flex-col gap-4">
        <h2 class="text-2xl font-normal">Private Key</h2>
        <p class="text-base text-muted-foreground">
            Private key works like a secret password that grants access to your Nostr identity.
        </p>

        <div class="bg-orange-600/10 text-orange-600 p-4 flex flex-row gap-4 items-start">
            <Warning size={20} class="flex-shrink-0" />
            <div class="flex flex-col gap-2 text-base">
                <span class="font-medium">Keep your private key safe!</span>
                <span>
                    Don't share your private key publicly, and use it only to log in to other Nostr apps.
                </span>
            </div>
        </div>

        <div class="flex items-center gap-2">
            <Input
                type={showPrivateKey ? "text" : "password"}
                class="flex-1 font-mono text-lg break-all focus-visible:ring-input text-xs truncate"
                bind:value={nsec}
            />
            <Button size="icon" variant="outline" onclick={copyPrivateKey} class="p-2 shrink-0">
                <Copy size={20} class="w-5 h-5 shrink-0" />
            </Button>
            <Button size="icon" variant="outline" onclick={() => showPrivateKey = !showPrivateKey} class="p-2 shrink-0">
                {#if showPrivateKey}
                    <ViewOff size={20} class="w-5 h-5 shrink-0" />
                {:else}
                    <View size={20} class="w-5 h-5 shrink-0" />
                {/if}
            </Button>
        </div>
    </section>
</div>
