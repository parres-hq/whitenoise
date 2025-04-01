<script lang="ts">
import { cn } from "$lib/utils.js";
import { Accordion as AccordionPrimitive } from "bits-ui";

type $$Props = AccordionPrimitive.ItemProps;

let className: $$Props["class"] = undefined;
export let value: $$Props["value"];
export { className as class };
</script>

<AccordionPrimitive.Item {value} class={cn("border-b", className)} {...$$restProps}>
	<slot />
</AccordionPrimitive.Item>
