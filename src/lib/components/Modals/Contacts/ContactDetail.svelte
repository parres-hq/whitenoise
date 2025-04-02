<script lang="ts">
import Loader from "$lib/components/Loader.svelte";
import { activeAccount } from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import type { CloseModal } from "$lib/types/modal";
import type { EnrichedContact } from "$lib/types/nostr";
import { nameFromMetadata } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import Warning from "phosphor-svelte/lib/Warning";
import Alert from "../../Alert.svelte";
import Avatar from "../../Avatar.svelte";
import Name from "../../Name.svelte";

let toastState = getToastState();

let { pubkey, contact, closeModal } = $props<{
    pubkey: string;
    contact: EnrichedContact;
    closeModal: CloseModal;
}>();

let isLoading = $state(false);
let showInviteAlert = $state(false);

async function startSecureChat() {
    isLoading = true;
    await invoke("create_group", {
        creatorPubkey: $activeAccount?.pubkey,
        memberPubkeys: [pubkey],
        adminPubkeys: [$activeAccount?.pubkey, pubkey],
        groupName: "Secure DM",
        description: "",
    })
        .then((group) => {
            console.log("Group created", group);
            toastState.add("Group created", "Group created successfully", "success");
            setTimeout(() => {
                closeModal();
            }, 1000);
        })
        .catch((e) => {
            toastState.add("Error creating group", e.toString(), "error");
            console.error("Error creating group", e);
        })
        .finally(() => {
            isLoading = false;
        });
}

async function inviteToWhiteNoise() {
    // TODO: await invoke("invite_to_white_noise", { pubkey });
    console.log("Invite to White Noise not implemented");
    toastState.add("Invite to White Noise", "Not implemented", "info");
}
</script>

{#if showInviteAlert}
    <Alert
        title="Invite {nameFromMetadata(contact.metadata, pubkey)} to White Noise?"
        body="We'll send a legacy-style direct message to {nameFromMetadata(contact.metadata, pubkey)} to invite them to White Noise. The message will say, 'Hi, I'm using White Noise to chat securely on Nostr. Join me!' and contain a link to download the app."
        acceptFn={async () => {
            invoke("invite_to_white_noise", { pubkey })
                .then(() => {
                    toastState.add("Message sent", `Invite sent to ${nameFromMetadata(contact.metadata, pubkey)}.`, "success");
                    showInviteAlert = false;
                })
                .catch((e) => {
                    toastState.add("Error sending message", `Failed to send message: ${e.toString()}`, "error");
                    console.error(e);
                });
        }}
        acceptText="Yes, send invite"
        acceptStyle="primary"
        cancelText="Cancel"
        bind:showAlert={showInviteAlert}
    />
{/if}

<div>
    <div class="flex flex-col items-center justify-start gap-2">
        <Avatar {pubkey} picture={contact.metadata.picture} pxSize={80} />
        <div class="text-4xl font-bold mt-6 w-full truncate text-center">
            <Name {pubkey} metadata={contact.metadata} unstyled={false} extraClasses="text-[2rem] text-center" />
        </div>
        <p class="text-gray-500 text-center">
            {contact.metadata.about}
        </p>
    </div>

    <div class="flex flex-col gap-10 mt-10 items-center w-full md:w-2/3 lg:w-1/2 mx-auto">
        {#if contact.nip104}
            <p>
                White Noise uses MLS (messaging layer security). Your messages are end-to-end encrypted and can only be
                read by you and the other participant.
            </p>
            <p class="mt-4">
                Ready to invite {nameFromMetadata(contact.metadata, pubkey)} to start a secure chat?
            </p>
            <button class="button-primary {isLoading ? 'opacity-50 cursor-not-allowed' : ''}" disabled={isLoading} onclick={startSecureChat}> Start secure chat </button>
        {:else}
            <p class="flex flex-col md:flex-row items-center gap-2 text-center w-4/5 md:w-full">
                <Warning class="text-red-500" weight="bold" size={24} />
                {nameFromMetadata(contact.metadata, pubkey)} is not yet set up to use secure MLS messaging.
            </p>

            <button class="button-primary" onclick={() => showInviteAlert = true}>
                Invite {nameFromMetadata(contact.metadata, pubkey)} to White Noise
            </button>
        {/if}
        {#if isLoading}
            <Loader size={40} fullscreen={false} />
        {/if}
    </div>
</div>
