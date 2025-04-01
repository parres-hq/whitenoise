<script lang="ts">
import NumberCircleOne from "phosphor-svelte/lib/NumberCircleOne";
import NumberCircleThree from "phosphor-svelte/lib/NumberCircleThree";
import NumberCircleTwo from "phosphor-svelte/lib/NumberCircleTwo";

let {
    currentStep = $bindable(),
    inboxRelaysPublished = $bindable(),
    keyPackageRelaysPublished = $bindable(),
    keyPackagePublished = $bindable(),
} = $props<{
    currentStep: number;
    inboxRelaysPublished: boolean;
    keyPackageRelaysPublished: boolean;
    keyPackagePublished: boolean;
}>();

let inboxRelaysColor = $derived(
    inboxRelaysPublished ? "text-green-500" : currentStep === 1 ? "text-white" : "text-gray-500"
);
let keyPackageRelaysColor = $derived(
    keyPackageRelaysPublished
        ? "text-green-500"
        : currentStep === 2
          ? "text-white"
          : "text-gray-500"
);
let keyPackageColor = $derived(
    keyPackagePublished ? "text-green-500" : currentStep === 3 ? "text-white" : "text-gray-500"
);
</script>

<div class="flex flex-row gap-4 items-center justify-center w-full mt-10">
    <NumberCircleOne size={40} class={inboxRelaysColor} />
    <NumberCircleTwo size={40} class={keyPackageRelaysColor} />
    <NumberCircleThree size={40} class={keyPackageColor} />
</div>
