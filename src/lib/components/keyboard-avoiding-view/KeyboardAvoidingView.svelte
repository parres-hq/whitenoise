<script lang="ts">
import { browser } from "$app/environment";
import { onDestroy, onMount } from "svelte";
import type { Snippet } from "svelte";
import { spring } from "svelte/motion";

/**
 * KeyboardAvoidingView - A component that adjusts its content when the mobile keyboard appears
 *
 * Props:
 * - class: Additional CSS classes to add to the container
 * - withSheet: Set to true if this is used with a sheet component
 * - bottomOffset: Additional space to add below the content (default: 0)
 * - strategy: How to handle keyboard appearance
 *   - "padding": Adds padding to push content up (default)
 *   - "position": Adjusts position of the container
 *   - "transform": Uses transform to move content (good for sheets)
 * - minKeyboardThreshold: Minimum height difference to consider keyboard visible (default: 100)
 * - adjustmentDelay: Delay before keyboard adjustments take effect (default: 350 for sheets, 0 for non-sheets)
 */
interface KeyboardAvoidingViewProps {
    class?: string;
    withSheet?: boolean;
    bottomOffset?: number;
    strategy?: "padding" | "position" | "transform";
    minKeyboardThreshold?: number;
    adjustmentDelay?: number;
    children: Snippet;
}

let {
    class: className = "",
    withSheet = false,
    bottomOffset = 0,
    strategy = withSheet ? "transform" : "padding",
    minKeyboardThreshold = 100,
    adjustmentDelay = withSheet ? 50 : 0,
    children,
}: KeyboardAvoidingViewProps = $props();

let container: HTMLElement;
let keyboardHeight = $state(0);
let isKeyboardVisible = $state(false);
let cleanup: (() => void) | undefined;
let initialWindowHeight = $state(0);
let previousViewportHeight = $state(0);
let isAndroid = $state(false);
let isIOS = $state(false);

// Spring animation for keyboard height
// const keyboardSpring = spring(0, {
//     stiffness: 1,
//     damping: 1,
// });

// // Subscribe to the spring store to update keyboard height
// $effect(() => {
//     keyboardHeight = $keyboardSpring;
// });

// Helper function to determine platform
function detectPlatform() {
    if (!browser) return { isAndroid: false, isIOS: false };
    isAndroid = /Android/i.test(navigator.userAgent);
    isIOS = /iPhone|iPad|iPod/i.test(navigator.userAgent);
    return { isAndroid, isIOS };
}

// Set up listeners for Visual Viewport API
function setupViewportListeners() {
    if (!browser) return;

    // Initialize heights
    initialWindowHeight = window.innerHeight;

    // Check if Visual Viewport API is available
    const visualViewport = window.visualViewport;
    if (!visualViewport) {
        // Fallback to resize events if visual viewport not available
        const resizeHandler = () => {
            const currentHeight = window.innerHeight;
            if (initialWindowHeight - currentHeight > minKeyboardThreshold) {
                const newKeyboardHeight = initialWindowHeight - currentHeight + bottomOffset;
                setTimeout(() => {
                    // keyboardSpring.set(newKeyboardHeight);
                    isKeyboardVisible = true;
                }, adjustmentDelay);
            } else {
                setTimeout(() => {
                    // keyboardSpring.set(bottomOffset);
                    isKeyboardVisible = false;
                }, adjustmentDelay);
            }
        };

        window.addEventListener("resize", resizeHandler);
        return () => {
            window.removeEventListener("resize", resizeHandler);
        };
    }

    // Initial values
    previousViewportHeight = visualViewport.height;

    const viewportHandler = () => {
        // Current viewport height
        const viewportHeight = visualViewport.height;
        // Full window height
        const windowHeight = window.innerHeight;

        // Device-specific adjustments
        if (isAndroid) {
            console.log("isAndroid");
            // Check if viewport height decreased significantly (keyboard appeared)
            // OR if there is a significant difference between window and viewport height
            const heightDecrease = previousViewportHeight - viewportHeight;
            const windowViewportDiff = windowHeight - viewportHeight;

            if (
                heightDecrease > minKeyboardThreshold ||
                windowViewportDiff > minKeyboardThreshold
            ) {
                // For Android, sometimes the height can fluctuate, so we check the rate of change
                const newKeyboardHeight =
                    Math.max(windowViewportDiff, heightDecrease) + bottomOffset;
                setTimeout(() => {
                    // keyboardSpring.set(newKeyboardHeight);
                    isKeyboardVisible = true;
                }, adjustmentDelay);
            } else if (
                viewportHeight > previousViewportHeight ||
                Math.abs(windowHeight - viewportHeight) < minKeyboardThreshold
            ) {
                // Keyboard likely disappeared
                setTimeout(() => {
                    // keyboardSpring.set(bottomOffset);
                    isKeyboardVisible = false;
                }, adjustmentDelay);
            }

            // Update previous height for next comparison
            previousViewportHeight = viewportHeight;
        } else if (isIOS) {
            console.log("isIOS");
            // Standard detection for iOS
            if (windowHeight - viewportHeight > minKeyboardThreshold) {
                // Add extra offset for iOS sheets which need more space
                const iosExtraOffset = isIOS && withSheet ? 50 : 0;
                const newKeyboardHeight =
                    windowHeight - viewportHeight + bottomOffset + iosExtraOffset;
                setTimeout(() => {
                    // keyboardSpring.set(newKeyboardHeight);
                    isKeyboardVisible = true;
                }, adjustmentDelay);
            } else {
                setTimeout(() => {
                    // keyboardSpring.set(bottomOffset);
                    isKeyboardVisible = false;
                }, adjustmentDelay);
            }
        }
    };

    // Add focus/blur event listeners for input elements
    const inputFocusHandler = () => {
        // Position viewport at the focused input
        if (withSheet && isIOS) {
            setTimeout(() => {
                // Force scroll to make sure the input is visible
                const activeElement = document.activeElement as HTMLElement;
                if (activeElement && activeElement.tagName === "INPUT") {
                    activeElement.scrollIntoView({ behavior: "smooth", block: "center" });
                }
            }, 300);
        }
        // Slight delay to ensure keyboard has appeared
        setTimeout(viewportHandler, 300);
    };

    const inputBlurHandler = () => {
        // Slight delay to ensure keyboard has disappeared
        setTimeout(() => {
            // keyboardSpring.set(bottomOffset);
            isKeyboardVisible = false;
        }, 100);
    };

    // Find all focusable elements within container when it's mounted
    const watchForInputs = () => {
        const inputs: Element[] = [];
        if (container) {
            const inputElements = container.querySelectorAll(
                'input, textarea, [contenteditable="true"]'
            );
            // Add event listeners to each input element
            for (const input of inputElements) {
                input.addEventListener("focus", inputFocusHandler);
                input.addEventListener("blur", inputBlurHandler);
                inputs.push(input);
            }
        }
        return inputs;
    };

    // Initial setup
    const inputs = watchForInputs();

    // For mutation observer to detect dynamically added inputs
    const observer = new MutationObserver((mutations) => {
        // Clean up old listeners
        for (const input of inputs) {
            input.removeEventListener("focus", inputFocusHandler);
            input.removeEventListener("blur", inputBlurHandler);
        }
        // Setup new listeners
        watchForInputs();
    });

    if (container) {
        observer.observe(container, { childList: true, subtree: true });
    }

    visualViewport.addEventListener("resize", viewportHandler);
    visualViewport.addEventListener("scroll", viewportHandler);

    return () => {
        visualViewport.removeEventListener("resize", viewportHandler);
        visualViewport.removeEventListener("scroll", viewportHandler);
        observer.disconnect();

        // Clean up input listeners
        for (const input of inputs) {
            input.removeEventListener("focus", inputFocusHandler);
            input.removeEventListener("blur", inputBlurHandler);
        }
    };
}

// On mount, set up viewport listeners
onMount(() => {
    detectPlatform();
    cleanup = setupViewportListeners();
});

onDestroy(() => {
    // Remove event listeners
    if (cleanup) cleanup();
});
</script>

<div
  bind:this={container}
  class="keyboard-avoiding-view {className} {isKeyboardVisible ? 'keyboard-visible' : ''} {withSheet ? 'with-sheet' : ''}"
  style={strategy === "padding" ?
    `padding-bottom: ${keyboardHeight}px;` :
    strategy === "position" ?
    `bottom: ${keyboardHeight}px;` :
    `transform: translateY(-${keyboardHeight}px);`}
>
    {@render children()}
</div>

<style>
  .keyboard-avoiding-view {
    display: flex;
    flex-direction: column;
    position: relative;
    width: 100%;
    transition: transform 0.2s ease-out;
  }

  /* Special handling for sheet content */
  .with-sheet {
    height: 100%;
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
  }

  /* Add any additional styling needed for keyboard visibility state */
  .keyboard-visible.with-sheet {
    overflow-y: auto;
    -webkit-overflow-scrolling: touch;
  }
</style>
