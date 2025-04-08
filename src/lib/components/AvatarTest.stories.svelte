<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import AvatarTest from "./AvatarTest.svelte";

const { Story } = defineMeta({
    title: "Components/AvatarTest",
    component: AvatarTest,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
            description: {
                component: `
                    Demonstration of the TV static avatar generator with various parameters and effects.
                    This component showcases the capabilities of the avatar generator including:
                    - Input-driven unique TV effects for different identifiers
                    - Size and density variations
                    - Color variations
                    - TV static effects intensity
                `,
            },
        },
        layout: "fullscreen",
    },
});
</script>

<Story
  name="Default"
/>
