<script lang="ts">
import { cn } from "$lib/utils.js";
import { Dialog as SheetPrimitive } from "bits-ui";
import CloseLarge from "carbon-icons-svelte/lib/CloseLarge.svelte";
import { onDestroy, onMount } from "svelte";
import { fly } from "svelte/transition";
import { SheetOverlay, SheetPortal, type Side, sheetTransitions, sheetVariants } from "./index.js";

type $$Props = SheetPrimitive.ContentProps & {
    side?: Side;
    keyboardAware?: boolean;
};

let className: $$Props["class"] = undefined;
export let side: $$Props["side"] = "right";
export let keyboardAware: $$Props["keyboardAware"] = true;
export { className as class };
export let inTransition: $$Props["inTransition"] = fly;
export let inTransitionConfig: $$Props["inTransitionConfig"] = sheetTransitions[side ?? "right"].in;
export let outTransition: $$Props["outTransition"] = fly;
export let outTransitionConfig: $$Props["outTransitionConfig"] =
    sheetTransitions[side ?? "right"].out;

// Keyboard handling
let isKeyboardVisible = false;
let cleanup: (() => void) | undefined;

onMount(() => {
    if (!keyboardAware || side !== "bottom") return;

    const visualViewport = window.visualViewport;
    if (visualViewport) {
        const onResize = () => {
            isKeyboardVisible = visualViewport.height < window.innerHeight;
            if (isKeyboardVisible) {
                const keyboardHeight = window.innerHeight - visualViewport.height;
                document.documentElement.style.setProperty(
                    "--keyboard-height",
                    `${keyboardHeight}px`
                );
                document.body.classList.add("keyboard-visible");
            } else {
                document.documentElement.style.removeProperty("--keyboard-height");
                document.body.classList.remove("keyboard-visible");
            }
        };
        visualViewport.addEventListener("resize", onResize);
        cleanup = () => visualViewport.removeEventListener("resize", onResize);
    }
});

onDestroy(() => {
    if (cleanup) cleanup();
});
</script>

<SheetPortal>
	<SheetOverlay />
	<SheetPrimitive.Content
		{inTransition}
		{inTransitionConfig}
		{outTransition}
		{outTransitionConfig}
		class={cn(
            sheetVariants({ side }),
            keyboardAware && side === "bottom" ? "keyboard-aware-sheet" : "",
            className
        )}
		{...$$restProps}
	>
		<slot />
		<SheetPrimitive.Close
			class="ring-offset-background focus:ring-ring data-[state=open]:bg-secondary absolute right-4 top-4 rounded-sm opacity-70 transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-offset-2 disabled:pointer-events-none"
		>
			<CloseLarge size={24} />
			<span class="sr-only">Close</span>
		</SheetPrimitive.Close>
	</SheetPrimitive.Content>
</SheetPortal>

<style>
    :global(.keyboard-aware-sheet) {
        position: fixed !important;
    }

    :global(body.keyboard-visible .keyboard-aware-sheet) {
        bottom: var(--keyboard-height, 0px) !important;
        transition: bottom 0.2s ease-out;
    }
</style>
