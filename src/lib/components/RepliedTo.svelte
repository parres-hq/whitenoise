<script lang="ts">
import { activeAccount } from "$lib/stores/accounts";
import { type ChatMessage } from "$lib/types/chat";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import { _ as t } from "svelte-i18n";
import Name from "./Name.svelte";

let {
    message,
    isDeleted = $bindable(false),
}: {
    message: ChatMessage | undefined;
    isDeleted?: boolean;
} = $props();
</script>

{#if message}
    <div class="flex flex-col gap-1 bg-primary-foreground text-primary rounded-sm p-2 border-l-4 border-l-white dark:border-l-black pl-4 mb-2 text-sm">
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
            <span class="break-words-smart">{message.content}</span>
        {/if}
    </div>
{:else}
    <div class="flex flex-col gap-1 bg-primary-foreground text-primary rounded-sm p-2 border-l-4 border-l-white dark:border-l-black pl-4 mb-2 text-sm">
        <span class="font-medium">
          <span>{$t("shared.loading")}</span>
        </span>
    </div>
{/if}
