<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import type { Message } from "$lib/types/chat";
import type { NEvent, NostrMlsGroup, NostrMlsGroupWithRelays } from "$lib/types/nostr";
import { hexMlsGroupId } from "$lib/utils/group";
import { invoke } from "@tauri-apps/api/core";
import { PaperPlaneTilt, X } from "phosphor-svelte";
import { TrashSimple } from "phosphor-svelte";
import { onMount } from "svelte";
import Loader from "./Loader.svelte";

let {
    group,
    replyToMessage = $bindable(),
    handleNewMessage,
    isReplyToMessageDeleted = $bindable(false),
}: {
    group: NostrMlsGroup;
    replyToMessage?: Message;
    handleNewMessage: (message: NEvent) => void;
    isReplyToMessageDeleted?: boolean;
} = $props();

let message = $state("");
let textarea: HTMLTextAreaElement;
let sendingMessage: boolean = $state(false);

function adjustTextareaHeight() {
    textarea.style.height = "auto";
    textarea.style.height = `${textarea.scrollHeight}px`;
}

function handleInput() {
    adjustTextareaHeight();
}

async function sendMessage() {
    if (message.length === 0) return;

    let kind = 9;
    let tags = [];
    if (replyToMessage) {
        let groupWithRelays: NostrMlsGroupWithRelays = await invoke("get_group", {
            groupId: hexMlsGroupId(group.mls_group_id),
        });
        tags.push(["q", replyToMessage.id, groupWithRelays.relays[0], replyToMessage.pubkey]);
    }
    let tmpMessage = {
        id: "temp",
        content: message,
        created_at: Math.floor(Date.now() / 1000),
        pubkey: $activeAccount?.pubkey,
        kind,
        tags,
    };

    handleNewMessage(tmpMessage as NEvent);
    sendingMessage = true;

    await invoke("send_mls_message", {
        group,
        message,
        kind,
        tags,
    })
        .then((messageEvent) => {
            handleNewMessage(messageEvent as NEvent);
            message = "";
            setTimeout(adjustTextareaHeight, 0);
        })
        .finally(() => {
            replyToMessage = undefined;
            sendingMessage = false;
        });
}

function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
        sendMessage();
    }
}

onMount(() => {
    const visualViewport = window.visualViewport;
    if (visualViewport) {
        const onResize = () => {
            const isKeyboardVisible = visualViewport.height < window.innerHeight;
            console.log("isKeyboardVisible", isKeyboardVisible);
            document.body.classList.toggle("keyboard-visible", isKeyboardVisible);
        };
        visualViewport.addEventListener("resize", onResize);
        return () => visualViewport.removeEventListener("resize", onResize);
    }
});
</script>

<div class="messagebar sticky bottom-0 left-0 right-0 bg-gray-900 drop-shadow-message-bar">
    {#if replyToMessage}
        <div class="w-full py-4 px-6 pl-8 bg-blue-700/50 text-white backdrop-blur-sm border-t border-gray-700 border-l-4 border-l-blue-500 flex flex-row gap-2 items-start justify-between rounded-t-xl">
            {#if isReplyToMessageDeleted}
                <div class="inline-flex flex-row items-center gap-2 bg-gray-200 rounded-full px-3 py-1 w-fit text-black">
                    <TrashSimple size={20} /><span class="italic opacity-60">Message deleted</span>
                </div>
            {:else}
                <span>{replyToMessage.content}</span>
            {/if}
            <button onclick={() => replyToMessage = undefined} class="p-1 bg-white/50 hover:bg-white rounded-full mr-0">
                <X size={12} class="text-blue-700" />
            </button>
        </div>
    {/if}
    <div class="flex flex-row px-8 py-4 gap-4 items-center border-t border-gray-700">
        <textarea
            id="newMessageInput"
            bind:this={textarea}
            class="px-4 py-2 w-full bg-transparent ring-1 ring-gray-700 rounded-lg min-h-[2.5rem] max-h-[12rem] resize-none overflow-y-auto"
            rows="1"
            bind:value={message}
            oninput={handleInput}
            onkeydown={handleKeydown}
        ></textarea>
        <button
            class="p-3 bg-blue-700 rounded-full text-white ring-1 ring-blue-500 hover:bg-blue-600 disabled:hidden"
            onclick={sendMessage}
            disabled={sendingMessage}
        >
            <PaperPlaneTilt size={24} />
        </button>
        <div
            class="p-3 bg-blue-700 rounded-full text-white ring-1 ring-blue-500"
            class:hidden={!sendingMessage}
        >
            <Loader fullscreen={false} size={24} />
        </div>
    </div>
</div>

<style>
    :global(body.keyboard-visible) .messagebar {
        position: fixed;
        bottom: 0;
        width: 100%;
    }
</style>
