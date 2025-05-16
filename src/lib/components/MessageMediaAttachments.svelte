<script lang="ts">
import type { MediaAttachment } from "$lib/types/chat";
import { calculateGridColumns, calculateVisibleAttachments } from "$lib/utils/media";

let { mediaAttachments } = $props<{ mediaAttachments: MediaAttachment[] }>();

let {
    visible: visibleMediaAttachments,
    hiddenCount: hiddenMediaAttachmentsCount,
    hasHidden: hasHiddenMediaAttachments,
} = $derived(calculateVisibleAttachments(mediaAttachments));

let gridCols = $derived(
    calculateGridColumns(visibleMediaAttachments.length, hasHiddenMediaAttachments)
);
</script>
  
<div class="grid gap-2" style="grid-template-columns: repeat({gridCols}, minmax(0, 1fr));">
    {#each visibleMediaAttachments as mediaAttachment}
      {#if mediaAttachment.type === "image"}
        <img 
          alt={mediaAttachment.alt} 
          class="w-full h-full object-cover rounded-lg" 
          src={mediaAttachment.blurhashSvg} 
        />  
      {/if}
    {/each}
    {#if hasHiddenMediaAttachments}
      <div class="w-full h-full rounded-lg bg-gradient-to-b from-gray-400 to-gray-700 items-center justify-center flex aspect-square">
        <p class="text-white text-3xl text-center"> + {hiddenMediaAttachmentsCount} </p>
      </div>
    {/if}
</div>
  
