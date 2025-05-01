<script lang="ts">
import { goto, pushState } from "$app/navigation";
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import { activeAccount } from "$lib/stores/accounts";
import type { EnrichedContact, NGroup } from "$lib/types/nostr";
import { hexMlsGroupId } from "$lib/utils/group";
import { nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import ChevronLeft from "carbon-icons-svelte/lib/ChevronLeft.svelte";
import Information from "carbon-icons-svelte/lib/Information.svelte";
import { _ as t } from "svelte-i18n";
import { toast } from "svelte-sonner";
import Loader from "./Loader.svelte";
import Button from "./ui/button/button.svelte";

let {
    contact = $bindable(null),
    pubkey = $bindable(""),
    onBack,
    onClose,
} = $props<{
    contact: EnrichedContact | null;
    pubkey: string;
    onBack: () => void;
    onClose: () => void;
}>();

let isCreatingChat = $state(false);
let isInviting = $state(false);

async function startChat() {
    if (!pubkey || isCreatingChat) return;

    isCreatingChat = true;

    try {
        // Create a DM group with just this contact
        const group: NGroup = await invoke("create_group", {
            creatorPubkey: $activeAccount?.pubkey,
            memberPubkeys: [pubkey],
            adminPubkeys: [$activeAccount?.pubkey, pubkey],
            groupName: "Secure DM",
            description: "",
        });

        toast.success(`Started a secure chat with ${nameFromMetadata(contact?.metadata, pubkey)}`);

        // Get the group ID
        const groupId = hexMlsGroupId(group.mls_group_id);

        // Close the sheet first
        onClose();

        // Check if on desktop (md breakpoint) or mobile
        if (window.innerWidth >= 768) {
            // On desktop: Navigate to chats, then update the state to select the chat
            // We need to delay the pushState to make sure it comes after navigation
            goto("/chats").then(() => {
                // After navigation, use pushState to set the selectedChatId in page state
                const href = `/chats/${groupId}`;
                setTimeout(() => {
                    pushState(href, { selectedChatId: groupId });
                }, 100);
            });
        } else {
            // On mobile: navigate directly to the chat page
            goto(`/chats/${groupId}`);
        }
    } catch (error) {
        toast.error("Failed to create chat");
        console.error(error);
    } finally {
        isCreatingChat = false;
    }
}

async function inviteContact() {
    if (!pubkey || isInviting) return;

    isInviting = true;

    try {
        await invoke("invite_to_white_noise", { pubkey });
    } catch (error) {
        toast.error("Failed to invite contact");
        console.error(error);
    } finally {
        isInviting = false;
    }
}
</script>

<div class="flex flex-col h-full relative">
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
                    <h3 class="text-lg/6 font-medium">{nameFromMetadata(contact?.metadata, pubkey)} {$t("chats.contactNotSetUp")}</h3>
                    <p>{$t("chats.sendInviteQuestion")}</p>
                </div>
            </div>
        {/if}
    </div>

    <div class="flex flex-col gap-2 w-full px-4 md:px-8 pb-8 bg-background">
        {#if !contact?.nip104}
            <Button
                variant="default"
                size="lg"
                class="text-base font-medium w-full py-3 px-0 focus-visible:ring-0 disabled:cursor-not-allowed"
                disabled={isInviting}
                onclick={inviteContact}>
                {#if isInviting}
                    <Loader size={16} fullscreen={false} />
                    {$t("chats.sendingInvite")}
                {:else}
                    {$t("chats.sendInvite")}
                {/if}
            </Button>
        {:else}
            <Button
                variant="default"
                size="lg"
                class="text-base font-medium w-full py-3 px-0 focus-visible:ring-0 disabled:cursor-not-allowed"
                disabled={isCreatingChat}
                onclick={startChat}>
                {#if isCreatingChat}
                    <Loader size={16} fullscreen={false} />
                    {$t("chats.creatingChat")}
                {:else}
                    {$t("chats.startChatAndSendInvite")}
                {/if}
            </Button>
        {/if}
    </div>
</div>
