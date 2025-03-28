<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import Name from "./Name.svelte";

const { Story } = defineMeta({
    title: "Components/Name",
    component: Name,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        pubkey: {
            control: "text",
            description: "The public key of the user",
        },
        metadata: {
            control: "object",
            description: "Optional metadata containing display_name or name",
        },
        extraClasses: {
            control: false,
            description: "Additional CSS classes to apply to the name",
        },
        unstyled: {
            control: "boolean",
            description: "Whether to render without default styling",
        },
    },
});

const mockPubkey = "7f5c2b32e5f23a";
const mockMetadata = {
    display_name: "Alice",
    name: "alice_nostr",
    about: "Nostr enthusiast",
    picture: "https://example.com/avatar.jpg",
};
</script>

<Story 
  name="Default" 
  args={{
    pubkey: mockPubkey,
  }} 
/>

<Story 
  name="Metadata" 
  args={{
    pubkey: mockPubkey,
    metadata: mockMetadata
  }} 
/>
