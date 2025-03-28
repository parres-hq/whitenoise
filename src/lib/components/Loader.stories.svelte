<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import Loader from "./Loader.svelte";

const { Story } = defineMeta({
    title: "Components/Loader",
    component: Loader,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        size: {
            control: "number",
            description: "Size of the loader in pixels",
        },
        fullscreen: {
            control: "boolean",
            description: "Whether to display the loader in fullscreen mode",
        },
    },
});
</script>

<Story 
  name="Default" 
  args={{
    size: 80,
    fullscreen: false
  }} 
/>
