<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
import Button from "$lib/components/ui/button/button.svelte";
import * as Sheet from "$lib/components/ui/sheet";
import type { EnrichedContact, NWelcome } from "$lib/types/nostr";
import { nameFromMetadata, npubFromPubkey, pubkeyFromBytes } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { _ as t } from "svelte-i18n";
import { toast } from "svelte-sonner";

type InviteDetailProps = {
    welcome: NWelcome;
    enrichedInviter?: EnrichedContact;
    showSheet: boolean;
};

let {
    welcome,
    enrichedInviter = $bindable(),
    showSheet = $bindable(),
}: InviteDetailProps = $props();

let isAcceptingInvite = $state(false);
let isDecliningInvite = $state(false);

let welcomerPubkey = $derived(pubkeyFromBytes(welcome.welcomer));

let subhead = $derived(
    welcome.member_count === 2
        ? "has invited you to join a secure chat."
        : `has invited you to join ${welcome.group_name}, a group with ${welcome.member_count} members.`
);

async function acceptInvite() {
    if (!enrichedInviter) {
        return;
    }
    isAcceptingInvite = true;
    invoke("accept_welcome", { welcome_event_id: welcome.event.id })
        .then(() => {
            const event = new CustomEvent("inviteAccepted", { detail: welcome.mls_group_id });
            window.dispatchEvent(event);
            toast.success(
                $t("chats.inviteAccepted", {
                    values: { name: nameFromMetadata(enrichedInviter.metadata) },
                })
            );
        })
        .catch((e) => {
            toast.error("Error accepting invite");
            console.error(e);
            isAcceptingInvite = false;
        })
        .finally(() => {
            isAcceptingInvite = false;
            showSheet = false;
        });
}

async function declineInvite() {
    if (!enrichedInviter) {
        return;
    }
    isDecliningInvite = true;
    invoke("decline_welcome", { welcome_event_id: welcome.event.id })
        .then(() => {
            toast.info(
                $t("chats.inviteDeclined", {
                    values: { name: nameFromMetadata(enrichedInviter.metadata) },
                })
            );
        })
        .catch((e) => {
            toast.error("Error declining invite");
            console.error(e);
            isDecliningInvite = false;
        })
        .finally(() => {
            isDecliningInvite = false;
            showSheet = false;
        });
}
</script>

{#if enrichedInviter}
    <KeyboardAvoidingView withSheet={true}>
        <div class="flex flex-col h-full relative">
            <Sheet.Header class="text-left flex flex-row items-start gap-x-1 px-6">
                <Sheet.Title>{$t("chats.invitation")}</Sheet.Title>
            </Sheet.Header>

            <div class="flex flex-col justify-start items-center pt-24 flex-1 gap-4">
                <h2 class="text-lg font-normal px-6 mb-10">{nameFromMetadata(enrichedInviter?.metadata)} {subhead}</h2>
                <Avatar
                    pubkey={welcomerPubkey}
                    picture={enrichedInviter?.metadata?.picture}
                    pxSize={80}
                />

                <h3 class="text-2xl font-medium px-6">
                    {nameFromMetadata(enrichedInviter?.metadata, welcomerPubkey)}
                </h3>

                {#if enrichedInviter?.metadata?.nip05}
                    <div class="text-sm font-normal font-muted-foreground px-6">
                        {enrichedInviter?.metadata?.nip05}
                    </div>
                {/if}

                <div class="mt-2 text-center px-6">
                    <FormattedNpub npub={npubFromPubkey(welcomerPubkey)} showCopy={false} centered={true} />
                </div>
            </div>

            <div class="flex flex-col gap-0 w-full px-0 fixed bottom-0 left-0 right-0 bg-background">
                <Button
                    variant="ghost"
                    size="lg"
                    tabindex="1"
                    class="text-base font-medium w-full py-6 focus-visible:ring-0"
                    disabled={isDecliningInvite}
                    onclick={declineInvite}>
                    {isDecliningInvite ? $t("chats.decliningInvite") : $t("chats.declineInvite")}
                </Button>
                <Button
                    variant="default"
                    size="lg"
                    tabindex="0"
                    class="text-base font-medium w-full pb-[calc(1.5rem+var(--sab))] pt-6 focus-visible:ring-0"
                    disabled={isAcceptingInvite}
                    onclick={acceptInvite}>
                        {isAcceptingInvite ? $t("chats.acceptingInvite") : $t("chats.acceptInvite")}
                </Button>
            </div>
        </div>
    </KeyboardAvoidingView>
{/if}
