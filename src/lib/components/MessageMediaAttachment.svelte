<script lang="ts">
import type { MediaAttachment } from "$lib/types/media";
import Download from "carbon-icons-svelte/lib/Download.svelte";
import Loader from "./Loader.svelte";

let { src, mediaAttachment, single } = $props<{
    src?: string;
    mediaAttachment: MediaAttachment;
}>();

let isDownloading = $state(false);

async function downloadMedia(mediaAttachment: MediaAttachment) {
    isDownloading = true;
    console.log("Downloading:", mediaAttachment.url);
}
</script>
<div class="relative w-full max-w-full" style="aspect-ratio: {mediaAttachment.width} / {mediaAttachment.height}">
  {#if mediaAttachment.type === "image"}
    <img 
      alt={mediaAttachment.alt} 
      class="w-40 h-full rounded-lg aspect-square object-cover" 
      src={src || mediaAttachment.blurhashSvg}
    />
    {#if isDownloading}
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
  
