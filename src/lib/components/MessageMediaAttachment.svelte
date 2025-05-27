<script lang="ts">
import type { createMediaStore } from "$lib/stores/media";
import type { MediaAttachment } from "$lib/types/media";
import type { NGroup } from "$lib/types/nostr";
import Close from "carbon-icons-svelte/lib/Close.svelte";
import Download from "carbon-icons-svelte/lib/Download.svelte";
import { _ as t } from "svelte-i18n";
import Loader from "./Loader.svelte";
import MediaImageSheet from "./MediaImageSheet.svelte";

let { src, mediaAttachment, isInitialLoading, group, mediaStore, isMine } = $props<{
    src?: string;
    mediaAttachment: MediaAttachment;
    isInitialLoading: boolean;
    group: NGroup;
    mediaStore: ReturnType<typeof createMediaStore>;
    isMine: boolean;
}>();

let isDownloading = $state(isInitialLoading);
let downloadFailed = $state(false);
let isSheetOpen = $state(false);
const showLoader = $derived((isDownloading || isInitialLoading) && !src);

function handleImageClick() {
    if (src) {
        isSheetOpen = true;
    }
}

function handleSheetClose() {
    isSheetOpen = false;
}

$effect(() => {
    if (src && (isDownloading || isInitialLoading)) {
        isDownloading = false;
        return;
    }
    if (isMine && !src && !isDownloading && !isInitialLoading && !downloadFailed) {
        downloadMedia(mediaAttachment);
        return;
    }
});

async function downloadMedia(mediaAttachment: MediaAttachment) {
    try {
        isDownloading = true;
        await mediaStore.downloadMedia(group, mediaAttachment);
    } catch (error) {
        console.error("Error downloading media:", error);
        isDownloading = false;
        downloadFailed = true;
    } finally {
        isDownloading = false;
    }
}
</script>
<div class="relative">
  {#if mediaAttachment.type === "image"}
    <button
      onclick={handleImageClick}
      class="max-w-40 h-full rounded-lg aspect-square overflow-hidden"
    >
      <img 
        alt={mediaAttachment.alt} 
        class="w-full h-full object-cover" 
        src={src || mediaAttachment.blurhashSvg}
      />
    </button>
    {#if showLoader}
      <div
        class="absolute inset-0 flex items-center justify-center"
      >
        <div class="bg-gray-800/50 rounded-full p-1">
          <Loader fullscreen={false} size={30} />
        </div>
      </div>
    {:else if downloadFailed && !src}
      <div
        class="absolute inset-0 flex flex-col items-center justify-center gap-1"
      >
        <div class="bg-red-600/80 rounded-full p-2">
          <Close class="w-4 h-4 md:w-5 md:h-5 text-white" />
        </div>
        <span class="text-sm text-white bg-gray-800/80 px-2 py-1 rounded">
          {$t("media.downloadError")}
        </span>
      </div>
    {:else if !src}
      <button
        onclick={() => downloadMedia(mediaAttachment)}
        class="absolute inset-0 flex items-center justify-center"
        title="Download image"
      >
        <div class="bg-gray-800/80 rounded-full p-2">
          <Download class="w-4 h-4 md:w-5 md:h-5 text-white" />
        </div>
      </button>
    {/if}
  {/if}
</div>

{#if isSheetOpen}
    <MediaImageSheet
        bind:isOpen={isSheetOpen}
        imageUrl={src}
        alt={mediaAttachment.alt}
    />
{/if}
  
