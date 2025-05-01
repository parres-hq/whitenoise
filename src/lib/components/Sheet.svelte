<script lang="ts">
import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
import Button from "$lib/components/ui/button/button.svelte";
import CloseLarge from "carbon-icons-svelte/lib/CloseLarge.svelte";
import type { Snippet } from "svelte";

interface SheetProps {
    open?: boolean;
    title?: Snippet;
    description?: Snippet;
    children: Snippet;
    class?: string;
    style?: string;
    /**
     * If true, disables keyboard avoidance (sheet will not move up when keyboard appears).
     * Useful for tall/scrolling sheets.
     */
    disableKeyboardAvoidance?: boolean;
}

let {
    open = $bindable(false),
    title,
    description,
    children,
    class: sheetClass = "",
    style: sheetStyle = "",
    disableKeyboardAvoidance = false,
}: SheetProps = $props();

let isClosing = $state(false);
let overlayVisible = $state(false);
let overlayOpacity = $state(0);
let dragStartY: number | null = null;
let dragCurrentY: number | null = null;
let dragOffset = $state(0);
let isDragging = $state(false);
const DRAG_CLOSE_THRESHOLD = 80;

function closeSheet() {
    if (isClosing) return;
    isClosing = true;
    overlayOpacity = 0;
    setTimeout(() => {
        open = false;
        isClosing = false;
        overlayVisible = false;
        dragOffset = 0;
    }, 150);
}

function onDragStart(e: TouchEvent | MouseEvent) {
    // Prevent dragging if the event target is inside an interactive element like input or button
    const target = e.target as HTMLElement;
    if (target.closest("input, textarea, button, a")) {
        return;
    }
    isDragging = true;
    if (typeof TouchEvent !== "undefined" && e instanceof TouchEvent) {
        dragStartY = e.touches[0].clientY;
    } else {
        dragStartY = (e as MouseEvent).clientY;
    }
    dragCurrentY = dragStartY;
    window.addEventListener("touchmove", onDragMove, { passive: false });
    window.addEventListener("mousemove", onDragMove);
    window.addEventListener("touchend", onDragEnd);
    window.addEventListener("mouseup", onDragEnd);
}

function onDragMove(e: TouchEvent | MouseEvent) {
    if (!isDragging || dragStartY === null) return;
    let clientY: number;
    if (typeof TouchEvent !== "undefined" && e instanceof TouchEvent) {
        if (e.touches.length === 0) {
            onDragEnd();
            return;
        }
        clientY = e.touches[0].clientY;
    } else {
        clientY = (e as MouseEvent).clientY;
    }
    dragCurrentY = clientY;
    dragOffset = Math.max(0, clientY - dragStartY);
    if (dragOffset > 0) {
        e.preventDefault?.();
    }
}

function onDragEnd() {
    if (!isDragging) return;
    if (dragOffset > DRAG_CLOSE_THRESHOLD) {
        closeSheet();
    } else {
        dragOffset = 0;
    }
    isDragging = false;
    dragStartY = null;
    dragCurrentY = null;
    window.removeEventListener("touchmove", onDragMove);
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("touchend", onDragEnd);
    window.removeEventListener("mouseup", onDragEnd);
}

$effect(() => {
    if (open) {
        overlayVisible = true;
        dragOffset = 0;
        setTimeout(() => {
            overlayOpacity = 1;
        }, 0);
    } else if (!isClosing) {
        overlayOpacity = 0;
        setTimeout(() => {
            overlayVisible = false;
        }, 150);
    }
});
</script>

{#if overlayVisible}
    <div
        class="fixed inset-0 z-50 bg-glitch-950/50 backdrop-blur-sm transition-opacity duration-150"
        style={`opacity: ${overlayOpacity};`}
        onclick={closeSheet}
        onkeydown={e => (e.key === 'Enter' || e.key === ' ') && closeSheet()}
        role="button"
        tabindex="0"
        aria-label="Close sheet overlay"
    ></div>
{/if}

{#if open || isClosing}
    <div
        class="fixed left-0 right-0 bottom-0 z-50 flex justify-center items-end pointer-events-none"
        style="min-height: 0;"
        aria-modal="true"
        role="dialog"
    >
        <KeyboardAvoidingView withSheet={true} strategy="transform" disableKeyboardAvoidance={disableKeyboardAvoidance}>
            <div
                class={`w-full bg-background shadow-2xl pointer-events-auto flex flex-col max-h-[85svh] border-t border-secondary ${isClosing ? 'animate-slideDown' : 'animate-slideUp'} ${sheetClass}`}
                style={`transform: translateY(${dragOffset}px); transition: ${isDragging ? 'none' : 'transform 0.2s cubic-bezier(0.4,0,0.2,1)'};${sheetStyle ? ' ' + sheetStyle : ''}`}
                role="document"
                tabindex="-1"
            >
                <div class="w-full py-2" onmousedown={onDragStart} ontouchstart={onDragStart} role="button" tabindex="0">
                    <div class="w-12 h-1.5 bg-secondary rounded-full mx-auto mt-3 mb-2 cursor-grab block sm:hidden"></div>
                </div>
                {#if title || description}
                    <div class="pt-4 pb-4 flex items-start justify-between mx-4 md:mx-8 shrink-0">
                        <div class="flex-1 text-left">
                            {#if title}
                                <h2 class="text-2xl font-normal">{@render title()}</h2>
                            {/if}
                            {#if description}
                                <p class="text-base text-muted-foreground mt-2">{@render description()}</p>
                            {/if}
                        </div>
                        <Button
                            variant="ghost"
                            size="icon"
                            aria-label="Close"
                            onclick={closeSheet}
                        >
                            <CloseLarge size={32}/>
                        </Button>
                    </div>
                {/if}

                <div class="flex-1">
                    {@render children()}
                </div>
            </div>
        </KeyboardAvoidingView>
    </div>
{/if}

<style>
@keyframes slideUp {
  from { transform: translateY(100%); }
  to { transform: translateY(0); }
}
@keyframes slideDown {
  from { transform: translateY(0); }
  to { transform: translateY(100%); }
}
.animate-slideUp {
  animation: slideUp 0.25s cubic-bezier(0.4,0,0.2,1);
}
.animate-slideDown {
  animation: slideDown 0.15s cubic-bezier(0.4,0,0.2,1);
}
</style>
