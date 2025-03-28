<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import Avatar from "./Avatar.svelte";

const { Story } = defineMeta({
    title: "Components/Avatar",
    component: Avatar,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        picture: {
            control: "text",
            description: "URL of the avatar image",
        },
        pubkey: {
            control: "text",
            description: "Public key of the user",
        },
        pxSize: {
            control: "number",
            description: "Size of the avatar in pixels",
        },
        showRing: {
            control: "boolean",
            description: "Whether to show a ring around the active account's avatar",
        },
    },
});
</script>

<Story 
  name="Default" 
  args={{
    pubkey: "7f5c2b32e5f23a",
    pxSize: 64,
    showRing: false
  }} 
/>

<Story 
  name="Picture" 
  args={{
    pubkey: "7f5c2b32e5f23a",
    picture: "https://api.dicebear.com/7.x/avataaars/svg?seed=John&backgroundColor=ffffff",
    pxSize: 64,
    showRing: true
  }} 
/>
