<script lang="ts" module>
import { setToastState } from "$lib/stores/toast-state.svelte";
import { defineMeta } from "@storybook/addon-svelte-csf";
import ContactsList from "./ContactsList.svelte";

const { Story } = defineMeta({
    title: "Components/ContactsList",
    component: ContactsList,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
});
</script>

<script lang="ts">
setToastState();
</script>

<Story name="Default">
    <ContactsList pushView={() => {}} />
</Story> 
