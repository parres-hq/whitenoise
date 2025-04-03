<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { EnrichedContact } from "$lib/types/nostr";
import { nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
import { onDestroy } from "svelte";
import Button from "../ui/button/button.svelte";
import * as Sheet from "../ui/sheet";

let { contact, pubkey, onBack, onClose } = $props<{
    contact: EnrichedContact | null;
    pubkey: string;
    onBack: () => void;
    onClose: () => void;
}>();

let isCreatingChat = $state(false);
let toastState = getToastState();

async function startChat() {
    if (!pubkey || isCreatingChat) return;

    isCreatingChat = true;

    try {
        // Create a DM group with just this contact
        const newGroup = await invoke("create_group", {
            creatorPubkey: $activeAccount?.pubkey,
            memberPubkeys: [pubkey],
            adminPubkeys: [$activeAccount?.pubkey, pubkey],
            groupName: "Secure DM",
            description: "",
        });

        toastState.add(
            "Chat created",
            `Started a secure chat with ${nameFromMetadata(contact?.metadata, pubkey)}`,
            "success"
        );

        // Navigate to the new chat
        // Safely extract the ID using type assertion to a record with string keys
        const group = newGroup as Record<string, string>;
        window.location.href = `/chat/${group.nostr_mls_group_id}`;
    } catch (error) {
        console.error("Failed to create chat:", error);
        toastState.add(
            "Failed to create chat",
            typeof error === "string" ? error : "An unexpected error occurred",
            "error"
        );
    } finally {
        isCreatingChat = false;
        onClose();
    }
}

onDestroy(() => {
    toastState.cleanup();
});
</script>

<div class="flex flex-col h-full relative">
    <Sheet.Header class="text-left flex flex-row items-start gap-x-1 -mt-0.5">
        <Button variant="link" size="icon" class="p-0 shrink-0" onclick={onBack}>
            <ChevronLeft size={24} class="shrink-0 !h-6 !w-6" />
        </Button>
        <Sheet.Title>Start secure chat</Sheet.Title>
    </Sheet.Header>

    <div class="flex flex-col justify-start items-center pt-[30%] flex-1 gap-4">
        <Avatar
            pubkey={pubkey}
            picture={contact?.metadata?.picture}
            pxSize={80}
        />

        <h3 class="text-2xl font-medium">
            {nameFromMetadata(contact?.metadata, pubkey)}
        </h3>

        {#if contact?.metadata?.nip05}
            <div class="text-sm font-normal font-muted-foreground">
                {contact.metadata.nip05}
            </div>
        {/if}

        <div class="mt-2 text-center">
            <FormattedNpub npub={npubFromPubkey(pubkey)} showCopy={false} centered={true} />
        </div>
    </div>

    <Button
        variant="default"
        size="lg"
        class="text-base font-medium w-full h-fit absolute bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))] md:relative md:left-auto md:right-auto md:mt-6"
        disabled={isCreatingChat}
        onclick={startChat}>
        {isCreatingChat ? "Creating chat..." : "Start Chat & Send Invite"}
    </Button>

</div>
