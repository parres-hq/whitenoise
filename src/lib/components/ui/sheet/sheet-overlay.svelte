<script lang="ts">
import { cn } from "$lib/utils.js";
import { Dialog as SheetPrimitive } from "bits-ui";
import { fade } from "svelte/transition";

type $$Props = SheetPrimitive.OverlayProps;

let className: $$Props["class"] = undefined;
export let transition: $$Props["transition"] = fade;
export let transitionConfig: $$Props["transitionConfig"] = {
    duration: 150,
};
export { className as class };
</script>

<SheetPrimitive.Overlay
	{transition}
	{transitionConfig}
	class={cn("bg-glitch-950/50 fixed inset-0 z-50 backdrop-blur-sm ", className)}
	{...$$restProps}
/>
