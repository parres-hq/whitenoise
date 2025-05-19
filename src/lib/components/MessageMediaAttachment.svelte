<script lang="ts">
import type { createMediaStore } from "$lib/stores/media";
import type { MediaAttachment } from "$lib/types/media";
import type { NGroup } from "$lib/types/nostr";
import Download from "carbon-icons-svelte/lib/Download.svelte";
import { toast } from "svelte-sonner";
import Loader from "./Loader.svelte";

let { src, mediaAttachment, isInitialLoading, group, mediaStore } = $props<{
    src?: string;
    mediaAttachment: MediaAttachment;
    isInitialLoading: boolean;
    group: NGroup;
    mediaStore: ReturnType<typeof createMediaStore>;
}>();

let isDownloading = $state(isInitialLoading);

$effect(() => {
    if (src && isDownloading) {
        isDownloading = false;
    }
});

async function downloadMedia(mediaAttachment: MediaAttachment) {
    try {
        isDownloading = true;
        await mediaStore.downloadMedia(group, mediaAttachment);
    } catch (error) {
        console.error("Error downloading media:", error);
        isDownloading = false;
        toast.error("Failed to download media");
    }
}
</script>
<div class="relative">
  {#if mediaAttachment.type === "image"}
    <img 
      alt={mediaAttachment.alt} 
      class="w-40 h-full rounded-lg aspect-square object-cover" 
      src={src || mediaAttachment.blurhashSvg}
    />
    {#if isDownloading || isInitialLoading}
      <div
        class="absolute inset-0 flex items-center justify-center"
      >
        <div class="bg-gray-800/50 rounded-full p-1">
          <Loader fullscreen={false} size={30} />
        </div>
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
  
