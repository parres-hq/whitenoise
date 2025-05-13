<script lang="ts">
import { createMediaStore } from "$lib/stores/media";
import type { MediaAttachment } from "$lib/types/media";
import { calculateGridColumns, calculateVisibleAttachments } from "$lib/utils/media";
import MessageMediaAttachment from "./MessageMediaAttachment.svelte";

let { mediaAttachments, mediaStore } = $props<{
    mediaAttachments: MediaAttachment[];
    mediaStore: ReturnType<typeof createMediaStore>;
}>();

let showAll = $state(false);

let {
    visible: visibleMediaAttachments,
    hiddenCount: hiddenMediaAttachmentsCount,
    hasHidden: hasHiddenMediaAttachments,
} = $derived(calculateVisibleAttachments(mediaAttachments));

let displayAttachments = $derived(showAll ? mediaAttachments : visibleMediaAttachments);

let gridCols = $derived(
    calculateGridColumns(displayAttachments.length, hasHiddenMediaAttachments && !showAll)
);

let mediaFilesMap = $state(mediaStore.mediaFilesMap);

$effect(() => {
    const unsubscribe = mediaStore.subscribe((state: { mediaFilesMap: Map<string, string> }) => {
        mediaFilesMap = state.mediaFilesMap;
    });
    return unsubscribe;
});

function toggleShowAll() {
    showAll = !showAll;
}
</script>
  
<div class="grid gap-2" style="grid-template-columns: repeat({gridCols}, minmax(0, 1fr));">
    {#each displayAttachments as mediaAttachment}
      <MessageMediaAttachment
        src={mediaFilesMap.get(mediaAttachment.url)}
        mediaAttachment={mediaAttachment}
      />
    {/each}
    {#if hasHiddenMediaAttachments && !showAll}
      <button 
        onclick={toggleShowAll}
        class="rounded-lg bg-gradient-to-b from-gray-400 to-gray-700 items-center justify-center flex aspect-square hover:from-gray-500 hover:to-gray-800 transition-colors"
      >
        <p class="text-white text-3xl text-center"> + {hiddenMediaAttachmentsCount} </p>
      </button>
    {/if}
</div>
  
