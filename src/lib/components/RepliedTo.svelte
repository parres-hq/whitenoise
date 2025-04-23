<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import { type ChatMessage } from "$lib/types/chat";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import { _ as t } from "svelte-i18n";
import MessageTokens from "./MessageTokens.svelte";
import Name from "./Name.svelte";
let {
    message,
    isDeleted = $bindable(false),
}: {
    message: ChatMessage | undefined;
    isDeleted?: boolean;
} = $props();

function scrollToMessage() {
    if (!message) return;
    const messageNode = document.getElementById(message?.id);
    if (messageNode) {
        messageNode.scrollIntoView({ behavior: "smooth", block: "center" });
    }
}
</script>

{#if message}
    <button 
        class="flex flex-col gap-1 bg-primary-foreground text-primary rounded-sm p-2 border-l-4 border-l-white dark:border-l-black pl-4 mb-2 text-sm"
        onclick={scrollToMessage}
        aria-label={$t("chats.scrollToMessage")}
    >
            {#if message.pubkey === $activeAccount?.pubkey}
                <span class="font-medium">{$t("chats.you")}</span>
            {:else}
                <span class="font-medium truncate">
                    <Name pubkey={message.pubkey} unstyled={true} />
                </span>
            {/if}
        {#if isDeleted}
            <div class="inline-flex flex-row items-center gap-2 bg-gray-200 rounded-full px-3 py-1 w-fit text-black">
                <TrashCan size={20} /><span class="italic opacity-60">{$t("chats.messageDeleted")}</span>
            </div>
        {:else}
            <MessageTokens tokens={message.tokens} reply={true} />
        {/if}
    </button>
{:else}
    <div class="flex flex-col gap-1 bg-primary-foreground text-primary rounded-sm p-2 border-l-4 border-l-white dark:border-l-black pl-4 mb-2 text-sm">
        <span class="font-medium">
          <span>{$t("shared.loading")}</span>
        </span>
    </div>
{/if}
