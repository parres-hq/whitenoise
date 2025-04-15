<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import type { CachedMessage, Message } from "$lib/types/chat";
import type { NostrMlsGroup, NostrMlsGroupWithRelays } from "$lib/types/nostr";
import { hexMlsGroupId } from "$lib/utils/group";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import AddLarge from "carbon-icons-svelte/lib/AddLarge.svelte";
import ArrowUp from "carbon-icons-svelte/lib/ArrowUp.svelte";
import Checkmark from "carbon-icons-svelte/lib/Checkmark.svelte";
import CloseLarge from "carbon-icons-svelte/lib/CloseLarge.svelte";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";
import { toast } from "svelte-sonner";
import Loader from "./Loader.svelte";
import Button from "./ui/button/button.svelte";
import Textarea from "./ui/textarea/textarea.svelte";
let {
    group,
    replyToMessage = $bindable(),
    handleNewMessage,
    isReplyToMessageDeleted = $bindable(false),
}: {
    group: NostrMlsGroup;
    replyToMessage?: Message;
    handleNewMessage: (message: CachedMessage) => void;
    isReplyToMessageDeleted?: boolean;
} = $props();

let message = $state("");
let media = $state<
    Array<{
        file: File;
        status: "uploading" | "error" | "success";
    }>
>([]);
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
    if (message.length === 0 && media.length === 0) return;

    // Check if any uploads are still in progress
    if (media.some((item) => item.status === "uploading")) {
        toast.info($t("chats.waitMediaUpload"));
        return;
    }

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

    handleNewMessage(tmpMessage as unknown as CachedMessage);
    sendingMessage = true;

    await invoke("send_mls_message", {
        group,
        message,
        kind,
        tags,
        uploadedFiles: await Promise.all(
            media
                .filter((item) => item.status === "success")
                .map(async (item) => {
                    const arrayBuffer = await item.file.arrayBuffer();
                    return {
                        filename: item.file.name,
                        mime_type: item.file.type,
                        data: Array.from(new Uint8Array(arrayBuffer)),
                    };
                })
        ),
    })
        .then((cachedMessage) => {
            handleNewMessage(cachedMessage as CachedMessage);
            message = "";
            media = []; // Clear media after successful send
            setTimeout(adjustTextareaHeight, 0);
        })
        .catch((error) => {
            console.error("Error sending message:", error);
            toast.error(`Failed to send message: ${error}`);
        })
        .finally(() => {
            replyToMessage = undefined;
            sendingMessage = false;
        });
}

function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        sendMessage();
    }
}

async function handleFileUpload() {
    const filePath = await open({
        multiple: false,
        directory: false,
        mimeTypes: ["image/*", "video/*", "audio/*", "application/pdf"],
    });
    if (!filePath) return;

    try {
        const fileData = await readFile(filePath);
        // Create a File object from the array buffer
        const file = new File([fileData], filePath.split("/").pop() || "file", {
            type: getMimeType(filePath),
        });

        // Add file to media array and start upload
        media = [...media, { file, status: "uploading" }];
        await uploadFile(file);
    } catch (error) {
        console.error("Error reading file:", error);
        toast.error("Failed to read file");
    }
}

async function uploadFile(file: File) {
    try {
        const arrayBuffer = await file.arrayBuffer();
        const fileData = {
            filename: file.name,
            mime_type: file.type,
            data: Array.from(new Uint8Array(arrayBuffer)),
        };

        await invoke("upload_file", {
            groupId: group.mls_group_id,
            file: fileData,
        });

        // Update status to success
        media = media.map((item) =>
            item.file.name === file.name ? { ...item, status: "success" } : item
        );
    } catch (error) {
        console.error("Error uploading file:", error);
        media = media.map((item) =>
            item.file.name === file.name ? { ...item, status: "error" } : item
        );
        toast.error(`Failed to upload ${file.name}`);
    }
}

// Helper function to determine MIME type from file extension
function getMimeType(filePath: string): string {
    const extension = filePath.split(".").pop()?.toLowerCase();
    const mimeTypes: Record<string, string> = {
        jpg: "image/jpeg",
        jpeg: "image/jpeg",
        png: "image/png",
        gif: "image/gif",
        mp4: "video/mp4",
        mp3: "audio/mpeg",
        pdf: "application/pdf",
        // Add more as needed
    };
    return mimeTypes[extension || ""] || "application/octet-stream";
}

onMount(() => {
    const visualViewport = window.visualViewport;
    if (visualViewport) {
        const onResize = () => {
            const isKeyboardVisible = visualViewport.height < window.innerHeight;
            document.body.classList.toggle("keyboard-visible", isKeyboardVisible);
        };
        visualViewport.addEventListener("resize", onResize);
        return () => visualViewport.removeEventListener("resize", onResize);
    }
});
</script>

<div class="messagebar sticky bottom-0 left-0 right-0 bg-background drop-shadow-message-bar">
    {#if replyToMessage}
        <div class="w-full py-4 px-6 pl-8 bg-muted backdrop-blur-sm flex flex-row gap-2 items-start justify-between ">
            {#if isReplyToMessageDeleted}
                <div class="inline-flex flex-row items-center gap-2 bg-gray-200 rounded-full px-3 py-1 w-fit text-black">
                    <TrashCan size={20} /><span class="italic opacity-60">{$t("chats.messageDeleted")}</span>
                </div>
            {:else}
                <span>{replyToMessage.content}</span>
            {/if}
            <button onclick={() => replyToMessage = undefined} class="p-1 bg-primary hover:bg-primary/80 rounded-full mr-0">
                <CloseLarge size={16} class="text-primary-foreground" />
            </button>
        </div>
    {/if}
    {#if media.length > 0}
        <div class="w-full py-2 px-6 pt-4 bg-muted backdrop-blur-sm flex flex-row gap-2 items-center overflow-x-auto">
            {#each media as item, index}
                <div class="relative group">
                    {#if item.file.type.startsWith('image/')}
                        <img
                            src={URL.createObjectURL(item.file)}
                            alt="Preview"
                            class="h-16 w-16 object-cover rounded-lg"
                        />
                    {:else if item.file.type.startsWith('video/')}
                        <div class="h-16 w-16 bg-gray-700 rounded-lg flex items-center justify-center">
                            <span class="text-white text-sm">{$t("chats.video")}</span>
                        </div>
                    {:else if item.file.type.startsWith('audio/')}
                        <div class="h-16 w-16 bg-gray-700 rounded-lg flex items-center justify-center">
                            <span class="text-white text-sm">{$t("chats.audio")}</span>
                        </div>
                    {:else}
                        <div class="h-16 w-16 bg-gray-700 rounded-lg flex items-center justify-center">
                            <span class="text-white text-sm">{$t("chats.pdf")}</span>
                        </div>
                    {/if}
                    <div class="absolute inset-0 bg-black/50 rounded-lg flex items-center justify-center">
                        {#if item.status === 'uploading'}
                            <div class="w-12 h-12">
                                <Loader fullscreen={false} size={48} />
                            </div>
                        {:else if item.status === 'error'}
                            <div class="text-destructive">
                                <CloseLarge size={24} />
                            </div>
                        {:else if item.status === 'success'}
                            <div class="text-green-500">
                                <Checkmark size={24} />
                            </div>
                        {/if}
                    </div>
                    <button
                        class="absolute -top-2 -right-2 bg-destructive text-destructive-foreground rounded-full p-1"
                        onclick={() => {
                            media = media.filter((_, i) => i !== index);
                        }}
                    >
                        <CloseLarge size={16} />
                    </button>
                </div>
            {/each}
        </div>
    {/if}
    <div class="flex flex-row p-4 gap-3 items-center border-t border-accent">
        <button
            class="p-2"
            onclick={handleFileUpload}
            disabled={false}
        >
            <AddLarge size={24} class="w-6 h-6" />
        </button>
        <textarea
            id="newMessageInput"
            class="px-4 py-2 w-full bg-input focus-visible:outline-none focus-visible:ring-input-foreground min-h-[2.5rem] max-h-[200px] resize-none overflow-y-auto"
            rows={1}
            bind:value={message}
            bind:this={textarea}
            oninput={handleInput}
            onkeydown={handleKeydown}
        ></textarea>
        <button
            class="p-2 bg-primary text-primary-foreground w-10 h-10"
            onclick={sendMessage}
            disabled={sendingMessage || media.some(item => item.status === "uploading")}
        >
            {#if sendingMessage}
                <Loader fullscreen={false} size={24} />
            {:else}
                <ArrowUp size={24} class="w-6 h-6" />
            {/if}
        </button>
    </div>
</div>

<style>
    :global(body.keyboard-visible) .messagebar {
        position: fixed;
        bottom: 0;
        width: 100%;
    }
</style>
