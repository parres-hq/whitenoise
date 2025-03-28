<script lang="ts" module>
import { setToastState } from "$lib/stores/toast-state.svelte";
import { defineMeta } from "@storybook/addon-svelte-csf";
import Toaster from "./Toaster.svelte";

const { Story } = defineMeta({
    title: "Components/Toaster",
    component: Toaster,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
                height: "400px",
            },
        },
    },
});
</script>

<script lang="ts">
const toastState = setToastState();
if (toastState) {
    toastState.add("Success", "Operation completed successfully", "success");
    toastState.add("Error", "Something went wrong", "error");
    toastState.add("Information", "Here's something you should know", "info");
}
</script>

<Story name="Default">
  <Toaster />
</Story> 
