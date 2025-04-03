<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { EnrichedContact, NostrMlsGroup } from "$lib/types/nostr";
import { hexMlsGroupId } from "$lib/utils/group";
import { nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
import Information from "carbon-icons-svelte/lib/Information.svelte";
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
let isInviting = $state(false);
let toastState = getToastState();

async function startChat() {
    if (!pubkey || isCreatingChat) return;

    isCreatingChat = true;

    try {
        // Create a DM group with just this contact
        const group: NostrMlsGroup = await invoke("create_group", {
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
        window.location.href = `/chats/${hexMlsGroupId(group.mls_group_id)}`;
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

async function inviteContact() {
    if (!pubkey || isInviting) return;

    isInviting = true;

    try {
        await invoke("invite_to_white_noise", { pubkey });
    } catch (error) {
        console.error("Failed to invite contact:", error);
    } finally {
        isInviting = false;
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

    <div class="flex flex-col justify-start items-center pt-40 flex-1 gap-4">
        <Avatar
            pubkey={pubkey}
            picture={contact?.metadata?.picture}
            pxSize={80}
        />

        <h3 class="text-2xl font-medium px-6">
            {nameFromMetadata(contact?.metadata, pubkey)}
        </h3>

        {#if contact?.metadata?.nip05}
            <div class="text-sm font-normal font-muted-foreground px-6">
                {contact.metadata.nip05}
            </div>
        {/if}

        <div class="mt-2 text-center px-6">
            <FormattedNpub npub={npubFromPubkey(pubkey)} showCopy={false} centered={true} />
        </div>

        {#if !contact?.nip104}
            <div class="flex flex-row gap-3 items-start bg-accent p-4 text-accent-foreground mt-12 mx-6">
                <Information size={24} class="shrink-0" />
                <div class="flex flex-col gap-2">
                    <h3 class="text-lg/6 font-medium">{nameFromMetadata(contact?.metadata, pubkey)} is not yet set up to use secure messaging</h3>
                    <p>
                        Do you want to invite them to White Noise? We'll send them a legacy direct message via Nostr with a link to download the app.
                    </p>
                </div>
            </div>
        {/if}
    </div>

    {#if !contact?.nip104}
        <Button
            variant="default"
            size="lg"
            class="text-base font-medium w-full h-fit absolute bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))] md:relative md:left-auto md:right-auto md:mt-6"
            disabled={isInviting}
            onclick={inviteContact}>
            {isInviting ? "Sending invite..." : "Invite to White Noise"}
        </Button>
    {:else}
        <Button
            variant="default"
            size="lg"
            class="text-base font-medium w-full h-fit absolute bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))] md:relative md:left-auto md:right-auto md:mt-6"
            disabled={isCreatingChat}
            onclick={startChat}>
            {isCreatingChat ? "Creating chat..." : "Start Chat & Send Invite"}
        </Button>
    {/if}
</div>
