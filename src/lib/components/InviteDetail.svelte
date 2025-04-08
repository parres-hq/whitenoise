<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
import Button from "$lib/components/ui/button/button.svelte";
import * as Sheet from "$lib/components/ui/sheet";
import type { EnrichedContact, Invite } from "$lib/types/nostr";
import { nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "svelte-sonner";

type InviteDetailProps = {
    invite: Invite;
    enrichedInviter?: EnrichedContact;
    showSheet: boolean;
};

let {
    invite,
    enrichedInviter = $bindable(),
    showSheet = $bindable(),
}: InviteDetailProps = $props();

let isAcceptingInvite = $state(false);
let isDecliningInvite = $state(false);

let subhead = $derived(
    invite.member_count === 2
        ? "has invited you to join a secure chat."
        : `has invited you to join ${invite.group_name}, a group with ${invite.member_count} members.`
);

async function acceptInvite() {
    if (!enrichedInviter) {
        return;
    }
    isAcceptingInvite = true;
    invoke("accept_invite", { invite })
        .then(() => {
            const event = new CustomEvent("inviteAccepted", { detail: invite.mls_group_id });
            window.dispatchEvent(event);
            toast.success(
                `You've accepted an invite to join a secure chat with ${nameFromMetadata(enrichedInviter.metadata)}`
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
    invoke("decline_invite", { invite })
        .then(() => {
            toast.info(
                `You've declined an invite to join a secure chat with ${nameFromMetadata(enrichedInviter.metadata)}`
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
                <Sheet.Title>Invitation</Sheet.Title>
            </Sheet.Header>

            <div class="flex flex-col justify-start items-center pt-24 flex-1 gap-4">
                <h2 class="text-lg font-normal px-6 mb-10">{nameFromMetadata(enrichedInviter?.metadata)} {subhead}</h2>
                <Avatar
                    pubkey={invite.inviter}
                    picture={enrichedInviter?.metadata?.picture}
                    pxSize={80}
                />

                <h3 class="text-2xl font-medium px-6">
                    {nameFromMetadata(enrichedInviter?.metadata, invite.inviter)}
                </h3>

                {#if enrichedInviter?.metadata?.nip05}
                    <div class="text-sm font-normal font-muted-foreground px-6">
                        {enrichedInviter?.metadata?.nip05}
                    </div>
                {/if}

                <div class="mt-2 text-center px-6">
                    <FormattedNpub npub={npubFromPubkey(invite.inviter)} showCopy={false} centered={true} />
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
                    {isDecliningInvite ? "Declining invite..." : "Decline invite"}
                </Button>
                <Button
                    variant="default"
                    size="lg"
                    tabindex="0"
                    class="text-base font-medium w-full pb-[calc(1.5rem+var(--sab))] pt-6 focus-visible:ring-0"
                    disabled={isAcceptingInvite}
                    onclick={acceptInvite}>
                        {isAcceptingInvite ? "Accepting invite..." : "Accept invite"}
                </Button>
            </div>
        </div>
    </KeyboardAvoidingView>
{/if}
