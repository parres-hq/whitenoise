<script lang="ts">
import { page } from "$app/state";
import GroupAvatar from "$lib/components/GroupAvatar.svelte";
import Header from "$lib/components/Header.svelte";
import MessageBar from "$lib/components/MessageBar.svelte";
import MessageTokens from "$lib/components/MessageTokens.svelte";
import RepliedTo from "$lib/components/RepliedTo.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import { DEFAULT_REACTION_EMOJIS } from "$lib/constants/reactions";
import { activeAccount, hasLightningWallet } from "$lib/stores/accounts";
import { createChatStore } from "$lib/stores/chat";
import type { CachedMessage, Message } from "$lib/types/chat";
import {
    type EnrichedContact,
    type NEvent,
    type NostrMlsGroup,
    NostrMlsGroupType,
    type NostrMlsGroupWithRelays,
} from "$lib/types/nostr";
import { copyToClipboard } from "$lib/utils/clipboard";
import { hexMlsGroupId } from "$lib/utils/group";
import { lightningInvoiceToQRCode } from "$lib/utils/lightning";
import { nameFromMetadata } from "$lib/utils/nostr";
import { formatMessageTime } from "$lib/utils/time";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import CheckmarkOutline from "carbon-icons-svelte/lib/CheckmarkOutline.svelte";
import CircleDash from "carbon-icons-svelte/lib/CircleDash.svelte";
import Copy from "carbon-icons-svelte/lib/Copy.svelte";
import OverflowMenuHorizontal from "carbon-icons-svelte/lib/OverflowMenuHorizontal.svelte";
import Reply from "carbon-icons-svelte/lib/Reply.svelte";
import Search from "carbon-icons-svelte/lib/Search.svelte";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import { onDestroy, onMount, tick } from "svelte";
import { type PressCustomEvent, press } from "svelte-gestures";
import { _ as t } from "svelte-i18n";
import { toast } from "svelte-sonner";

let {
    selectedChatId = $bindable(),
    showInfoPage = $bindable(false),
}: { selectedChatId?: string; showInfoPage?: boolean } = $props();

let unlistenMlsMessageReceived: UnlistenFn;
let unlistenMlsMessageProcessed: UnlistenFn;

const chatStore = createChatStore();

let group: NostrMlsGroup | undefined = $state(undefined);
let counterpartyPubkey: string | undefined = $state(undefined);
let enrichedCounterparty: EnrichedContact | undefined = $state(undefined);
let groupName = $state("");
let cachedMessages: CachedMessage[] | undefined = $state(undefined);
let showMessageMenu = $state(false);
let selectedMessageId: string | null | undefined = $state(undefined);
let messageMenuPosition = $state({ x: 0, y: 0 });
let messageMenuExtendedPosition = $state({ x: 0, y: 0 });
let replyToMessage: Message | undefined = $state(undefined);
let isReplyToMessageDeleted = $state(false);

$effect(() => {
    if (replyToMessage?.id) {
        isReplyToMessageDeleted = chatStore.isDeleted(replyToMessage.id);
    } else {
        isReplyToMessageDeleted = false;
    }
});

$effect(() => {
    if (
        group &&
        group.group_type === NostrMlsGroupType.DirectMessage &&
        counterpartyPubkey &&
        enrichedCounterparty
    ) {
        groupName = nameFromMetadata(enrichedCounterparty.metadata, counterpartyPubkey);
    } else if (group) {
        groupName = group.name;
    }
});

$effect(() => {
    if (selectedChatId && group && hexMlsGroupId(group.mls_group_id) !== selectedChatId) {
        loadGroup();
    }
});

async function loadGroup() {
    const groupId = selectedChatId || page.params.id;
    invoke("get_group_and_messages", { groupId }).then(async (groupResponse) => {
        const groupData: NostrMlsGroup = (
            groupResponse as { group: NostrMlsGroup; messages: CachedMessage[] }
        ).group;
        const cachedMessagesData: CachedMessage[] = (
            groupResponse as { group: NostrMlsGroup; messages: CachedMessage[] }
        ).messages;

        group = groupData;
        cachedMessages = cachedMessagesData;
        // Add messages to the chat store
        chatStore.clear();
        for (const cachedMessage of cachedMessages) {
            chatStore.handleCachedMessage(cachedMessage);
        }

        if (!counterpartyPubkey) {
            counterpartyPubkey =
                group.group_type === NostrMlsGroupType.DirectMessage
                    ? group.admin_pubkeys.filter((pubkey) => pubkey !== $activeAccount?.pubkey)[0]
                    : undefined;
        }
        if (counterpartyPubkey) {
            invoke("query_enriched_contact", {
                pubkey: counterpartyPubkey,
                updateAccount: false,
            }).then((value) => {
                enrichedCounterparty = value as EnrichedContact;
            });
        }
        await scrollToBottom();
    });
}

async function scrollToBottom() {
    await tick();
    const messagesContainer = document.getElementById("messagesContainer");
    const screenHeight = window.innerHeight;
    if (messagesContainer && screenHeight < messagesContainer.scrollHeight) {
        const lastMessage = messagesContainer.lastElementChild;
        lastMessage?.scrollIntoView({ behavior: "instant" });
    }
    if (messagesContainer) {
        messagesContainer.style.opacity = "1";
    }
}

onMount(async () => {
    if (!unlistenMlsMessageProcessed) {
        unlistenMlsMessageProcessed = await listen<[NostrMlsGroup, CachedMessage]>(
            "mls_message_processed",
            ({ payload: [_updatedGroup, cachedMessage] }) => {
                const message = chatStore.findMessage(cachedMessage.event_id);
                if (!message) {
                    console.log("pushing message to transcript");
                    chatStore.handleCachedMessage(cachedMessage);
                }
                scrollToBottom();
            }
        );
    }

    if (!unlistenMlsMessageReceived) {
        unlistenMlsMessageReceived = await listen<NEvent>(
            "mls_message_received",
            ({ payload: _message }) => {
                console.log("mls_message_received event received");
                loadGroup();
            }
        );
    }

    await loadGroup();
});

function handleNewEvent(cachedMessage: CachedMessage) {
    chatStore.handleCachedMessage(cachedMessage);
}

function handlePress(event: PressCustomEvent | MouseEvent) {
    const target = event.target as HTMLElement;
    const messageContainer = target.closest("[data-message-container]");
    const messageId = messageContainer?.getAttribute("data-message-id");
    const isCurrentUser = messageContainer?.getAttribute("data-is-current-user") === "true";
    selectedMessageId = messageId;
    const messageBubble = messageContainer?.parentElement?.querySelector(
        "[data-message-container]:not(button)"
    );
    const rect = messageBubble?.getBoundingClientRect() || target.getBoundingClientRect();

    // Temporarily make menus visible but with measuring class
    const reactionMenu = document.getElementById("messageMenu");
    const extendedMenu = document.getElementById("messageMenuExtended");
    if (reactionMenu) reactionMenu.classList.replace("invisible", "visible");
    if (extendedMenu) extendedMenu.classList.replace("invisible", "visible");

    // Add measuring class
    if (reactionMenu) reactionMenu.classList.add("measuring");
    if (extendedMenu) extendedMenu.classList.add("measuring");

    // Use setTimeout to ensure the menus are rendered before measuring
    setTimeout(() => {
        const reactionMenuWidth = reactionMenu?.getBoundingClientRect().width || 0;
        const extendedMenuWidth = extendedMenu?.getBoundingClientRect().width || 0;

        // Remove measuring class
        if (reactionMenu) reactionMenu.classList.remove("measuring");
        if (extendedMenu) extendedMenu.classList.remove("measuring");

        // Calculate viewport-relative positions
        const viewportX = isCurrentUser ? rect.right - reactionMenuWidth : rect.left;
        const viewportY = rect.top - 60;

        messageMenuPosition = {
            x: viewportX,
            y: viewportY,
        };

        messageMenuExtendedPosition = {
            x: isCurrentUser ? rect.right - extendedMenuWidth : rect.left,
            y: rect.bottom + 10,
        };

        showMessageMenu = true;

        // Apply animation to the message bubble
        if (messageBubble instanceof HTMLElement) {
            messageBubble.style.transform = "scale(1.10)";
            messageBubble.style.transformOrigin = isCurrentUser ? "right" : "left";
            messageBubble.style.transition = "transform 0.06s ease-out";

            setTimeout(() => {
                messageBubble.style.transform = "scale(1)";
            }, 100);

            messageBubble.addEventListener(
                "pointerup",
                () => {
                    messageBubble.style.transform = "scale(1)";
                },
                { once: true }
            );
        }
    }, 0);
}

function handleOutsideClick() {
    showMessageMenu = false;
    selectedMessageId = undefined;
}

async function clickReaction(reaction: string, messageId: string | null | undefined) {
    if (!group) {
        console.error("no group found");
        return;
    }
    if (!messageId) {
        console.error("no message selected");
        return;
    }
    chatStore
        .clickReaction(group, reaction, messageId)
        .catch((err) => {
            console.error("Failed to apply reaction:", err);
        })
        .finally(() => {
            showMessageMenu = false;
        });
}

async function copyMessage() {
    if (selectedMessageId) {
        const message = chatStore.findMessage(selectedMessageId);
        if (message) {
            await writeText(message.content);
            const button = document.querySelector("[data-copy-button]");
            button?.classList.add("copy-success");
            setTimeout(() => {
                button?.classList.remove("copy-success");
                showMessageMenu = false;
            }, 1000);
        }
    }
}

async function payLightningInvoice(message: Message) {
    if (!group) {
        console.error("no group found");
        return;
    }

    if (!message.lightningInvoice) {
        toast.error("Message does not have a lightning invoice");
        return;
    }

    if (!$hasLightningWallet) {
        toast.error("Lightning wallet not connected");
        return;
    }

    let groupWithRelays: NostrMlsGroupWithRelays = await invoke("get_group", {
        groupId: hexMlsGroupId(group.mls_group_id),
    });

    if (!groupWithRelays) {
        console.error("no group with relays found");
        return;
    }

    chatStore
        .payLightningInvoice(groupWithRelays, message)
        .then(
            (_paymentEvent: CachedMessage | null) => {
                toast.success("Payment success", {
                    description: "Successfully sent payment to invoice",
                });
            },
            (e) => {
                toast.error("Error sending payment");
                console.error(e);
            }
        )
        .finally(() => {
            showMessageMenu = false;
        });
}

async function copyInvoice(message: Message) {
    const invoice = message.lightningInvoice?.invoice;
    if (invoice) await copyToClipboard(invoice, "bolt11 invoice");
}

function reply() {
    if (selectedMessageId) {
        replyToMessage = chatStore.findMessage(selectedMessageId);
        document.getElementById("newMessageInput")?.focus();
        showMessageMenu = false;
    }
}

function editMessage() {
    console.log("editing message");
}

function deleteMessage() {
    if (!selectedMessageId) {
        console.error("No message selected");
        return;
    }
    if (!group) {
        console.error("No group found");
        return;
    }

    chatStore
        .deleteMessage(group, selectedMessageId)
        .then(() => {
            showMessageMenu = false;
        })
        .catch((e) => {
            toast.error("Error Deleting Message");
            console.error(e);
        });
}

function isSelectedMessageDeletable(): boolean {
    if (!selectedMessageId) return false;

    return chatStore.isMessageDeletable(selectedMessageId);
}

function isSelectedMessageCopyable(): boolean {
    if (!selectedMessageId) return false;

    return chatStore.isMessageCopyable(selectedMessageId);
}

function hasMessageReactions(message: Message): boolean {
    return chatStore.hasReactions(message);
}

onDestroy(() => {
    unlistenMlsMessageProcessed();
    unlistenMlsMessageReceived();
    chatStore.clear();
});

function navigateToInfo() {
    if (window.innerWidth >= 768) {
        // Desktop mode (md breakpoint)
        showInfoPage = true;
    } else {
        // Mobile: use regular navigation
        const url = `/chats/${page.params.id}/info`;
        window.location.href = url;
    }
}
</script>

{#if group}
    <Header backLocation={selectedChatId ? undefined : "/chats"}>
        <div class="flex flex-row items-center justify-between w-full">
            <button onclick={navigateToInfo} class="flex flex-row items-center gap-3">
                <GroupAvatar
                    groupType={group!.group_type}
                    {groupName}
                    {counterpartyPubkey}
                    {enrichedCounterparty}
                pxSize={40}
                />
                <span class="text-2xl font-medium">{groupName}</span>
            </button>
            <!-- TODO: Implement chat search -->
            <!-- <Search size={24} class="text-muted-foreground shrink-0 !w-6 !h-6"/> -->
        </div>
    </Header>

    <main id="mainContainer" class="flex flex-col relative min-h-svh">
        <div
            id="messagesContainer"
            class="flex-1 px-8 flex flex-col gap-2 pt-10 pb-24 overflow-y-auto opacity-100 transition-opacity ease-in-out duration-50"
        >
            {#each $chatStore.messages as message (message.id)}
                <div
                    class={`flex justify-end ${message.isMine ? "" : "flex-row-reverse"} items-center gap-4 group ${hasMessageReactions(message) ? "mb-6" : ""}`}
                >
                    <button
                        onclick={handlePress}
                        data-message-container
                        data-message-id={message.id}
                        data-is-current-user={message.isMine}
                        class="p-2 opacity-0 group-hover:opacity-100 transition-opacity duration-200"
                    >
                        <OverflowMenuHorizontal size={24} />
                    </button>
                    <div
                        use:press={()=>({ triggerBeforeFinished: true, timeframe: 100 })}
                        onpress={handlePress}
                        data-message-container
                        data-message-id={message.id}
                        data-is-current-user={message.isMine}
                        class={`font-normal text-base relative rounded-md max-w-[70%] ${message.lightningPayment ? "bg-opacity-10" : ""} ${!message.isSingleEmoji ? `${message.isMine ? `bg-primary text-primary-foreground` : `bg-muted text-accent-foreground`} p-3` : ''} ${showMessageMenu && message.id === selectedMessageId ? 'relative z-20' : ''}`}
                    >
                        {#if message.replyToId }
                            <RepliedTo
                                message={chatStore.findReplyToMessage(message)}
                                isDeleted={chatStore.isDeleted(message.replyToId)}
                            />
                        {/if}
                        <div class="flex {message.content.trim().length < 50 && !message.isSingleEmoji ? "flex-row gap-6" : "flex-col gap-2"} w-full {message.lightningPayment ? "items-center justify-center" : "items-end"}  {message.isSingleEmoji ? 'mb-4 my-6' : ''}">
                            <div class="break-words-smart w-full {message.lightningPayment ? 'flex justify-center' : ''} {message.isSingleEmoji ? 'text-7xl leading-none' : ''}">
                                {#if chatStore.isDeleted(message.id)}
                                    <div class="inline-flex flex-row items-center gap-2 px-3 py-1 w-fit text-muted-foreground">
                                        <span class="font-italic text-base opacity-60">{$t("chats.messageDeleted")}</span>
                                    </div>
                                {:else if message.content.trim().length > 0}
                                    {#if !message.lightningInvoice}
                                        <MessageTokens tokens={message.tokens} />
                                    {/if}
                                    {#if message.lightningInvoice }
                                    <div class="flex flex-col items-start gap-4">
                                        <div class="relative">
                                            {#await lightningInvoiceToQRCode(message.lightningInvoice.invoice)}
                                                <div class="max-w-64 max-h-64 shadow-lg flex items-center justify-center">
                                                    <CircleDash size={32} class="animate-spin-slow text-blue-600" />
                                                </div>
                                            {:then qrCodeUrl}
                                                {#if qrCodeUrl}
                                                    <Button
                                                        variant="ghost"
                                                        size="lg"
                                                        class="p-0 w-full h-auto aspect-square relative"
                                                        onclick={() => {
                                                            copyInvoice(message);
                                                            toast.success("Invoice copied to clipboard");
                                                        }}
                                                    >
                                                        <img
                                                            src={qrCodeUrl}
                                                            alt="QR Code"
                                                            class="max-w-full w-full h-auto {message.lightningInvoice.isPaid ? 'blur-sm' : ''}"
                                                        />
                                                    </Button>
                                                {/if}
                                            {:catch}
                                                <!-- Show nothing in case of error -->
                                            {/await}
                                            {#if message.lightningInvoice.description}
                                                <div class="mt-4">{message.lightningInvoice.description}</div>
                                            {/if}
                                            {#if message.lightningInvoice.isPaid }
                                                <CheckmarkOutline
                                                    size={32}
                                                    class="text-green-500 rounded-full opacity-80 absolute top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2"
                                                />
                                            {/if}
                                        </div>
                                        <div class="flex flex-col gap-4">
                                            <Button
                                                variant="outline"
                                                size="sm"
                                                onclick={(() => {
                                                    copyInvoice(message);
                                                    toast.success("Invoice copied to clipboard");
                                                })}
                                                class={`px-6 py-2 flex flex-row gap-4 items-center justify-center font-semibold grow ${message.isMine ? "bg-secondary-foreground" : ""}`}
                                            >
                                                {$t("chats.copyInvoice")}  <Copy size={20} />
                                            </Button>
                                            {#if $hasLightningWallet && !message.lightningInvoice.isPaid}
                                                <button
                                                    onclick={() => payLightningInvoice(message)}
                                                    class="transition-all bg-gradient-to-bl from-orange-500 to-orange-600 hover:from-orange-600 hover:to-orange-500  hover:shadow-xl duration-300 rounded-md px-6 py-2 flex flex-row gap-4 items-center justify-center font-semibold grow"
                                                >
                                                {$t("chats.paySats", { values: { amount :message.lightningInvoice.amount } })}
                                                </button>
                                            {/if}
                                        </div>
                                    </div>
                                    {/if}
                                {:else if message.lightningPayment}
                                    <div class="inline-flex flex-row items-center gap-2 bg-orange-400 rounded-full px-2 py-0 w-fit">
                                        <span>⚡️</span><span class="italic font-bold">{$t("chats.invoicePaid")}</span><span>⚡️</span>
                                    </div>
                                {:else}
                                    <span class="italic opacity-60">{$t("chats.noMessageContent")}</span>
                                {/if}
                                </div>
                                <div class="flex flex-row gap-2 items-center ml-auto {message.isMine ? "text-primary-foreground" : "text-primary"}">
                                    {#if message.id !== "temp"}
                                        <span><CheckmarkOutline size={16} /></span>
                                    {:else}
                                        <span><CircleDash size={16} class="animate-spin-slow"/></span>
                                    {/if}
                                    <span class="text-sm opacity-60 whitespace-nowrap">
                                        {formatMessageTime(message.createdAt)}
                                    </span>
                                </div>
                            </div>
                            <div class="reactions flex flex-row gap-2 absolute -bottom-6 right-0">
                                {#each chatStore.getMessageReactionsSummary(message.id) as {emoji, count}}
                                    <button onclick={() => clickReaction(emoji, message.id)} class="text-sm py-1 px-2 ring-1 ring-background {message.isMine ? 'bg-accent-foreground text-primary-foreground' : 'text-primary bg-input'} rounded-full flex flex-row gap-1 items-center">
                                        {emoji}
                                        {#if count > 1}
                                            <span class="text-sm">{count}</span>
                                        {/if}
                                    </button>
                                {/each}
                            </div>
                        </div>
                    </div>
            {/each}
        </div>
        <MessageBar {group} bind:replyToMessage={replyToMessage} handleNewMessage={handleNewEvent} bind:isReplyToMessageDeleted={isReplyToMessageDeleted} />
    </main>
{/if}

{#if showMessageMenu}
    <button
        type="button"
        class="fixed inset-0 backdrop-blur-sm z-10"
        onclick={handleOutsideClick}
        onkeydown={(e) => e.key === 'Escape' && handleOutsideClick()}
        aria-label={$t("chats.closeMessageMenu")}
    ></button>
{/if}

<div
    id="messageMenu"
    class="{showMessageMenu ? 'visible' : 'invisible'} fixed bg-background backdrop-blur-sm ring-1 ring-muted-foreground/20 drop-shadow-xl drop-shadow-black py-0 px-2 z-30 translate-x-0"
    style="left: {messageMenuPosition.x}px; top: {messageMenuPosition.y}px;"
    role="menu"
>
    <div class="flex flex-row gap-3 text-xl">
        {#each DEFAULT_REACTION_EMOJIS as reaction (reaction.name)}
            <button onclick={() => clickReaction(reaction.emoji, selectedMessageId)} class="p-3" title={reaction.name}>
                {reaction.emoji}
            </button>
        {/each}
    </div>
</div>

<div
    id="messageMenuExtended"
    class="{showMessageMenu ? 'opacity-100 visible' : 'opacity-0 invisible'} fixed bg-background backdrop-blur-sm ring-1 ring-muted-foreground/20 drop-shadow-xl drop-shadow-black z-30 translate-x-0 transition-opacity duration-200"
    style="left: {messageMenuExtendedPosition.x}px; top: {messageMenuExtendedPosition.y}px;"
    role="menu"
>
    <div class="flex flex-col justify-start items-between divide-y divide-muted min-w-48">
        {#if isSelectedMessageCopyable()}
            <Button variant="ghost" size="sm" data-copy-button onclick={copyMessage} class="text-base font-normal flex flex-row items-center justify-between">{$t("chats.copy")} <Copy size={24} /></Button>
        {/if}
        <Button variant="ghost" size="sm" data-copy-button onclick={reply} class="text-base font-normal flex flex-row items-center justify-between">{$t("chats.reply")} <Reply size={24} /></Button>
        <!-- <button onclick={editMessage} class="px-4 py-2 flex flex-row gap-20 items-center justify-between">Edit <PencilSimple size={20} /></button> -->
        {#if isSelectedMessageDeletable()}
            <Button variant="ghost" size="sm" data-copy-button onclick={deleteMessage} class="text-base font-normal flex flex-row items-center justify-between">{$t("chats.delete")} <TrashCan size={24} /></Button>
        {/if}
    </div>
</div>

<style>
    .measuring {
        position: fixed !important;
        visibility: hidden !important;
        top: -9999px !important;
        left: -9999px !important;
    }

    .copy-success {
        color: rgb(34 197 94); /* text-green-500 */
        transition: color 0.2s ease-in-out;
    }
    /* Ensure immediate visibility state change */
    .invisible {
        display: none;
    }
</style>
