<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import Header from "./Header.svelte";

const { Story } = defineMeta({
    title: "Components/Header",
    component: Header,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        title: {
            control: "text",
            description: "The title to display in the header",
        },
    },
});
</script>

<Story 
  name="Default" 
  args={{
    title: "Example Header"
  }} 
/>
