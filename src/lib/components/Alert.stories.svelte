<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import { fn } from "@storybook/test";
import Alert from "./Alert.svelte";

const { Story } = defineMeta({
    title: "Components/Alert",
    component: Alert,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
                height: "400px",
            },
        },
    },
    argTypes: {
        title: {
            control: "text",
            description: "The title of the alert",
        },
        body: {
            control: "text",
            description: "The body text of the alert",
        },
        acceptText: {
            control: "text",
            description: "The text for the accept button",
        },
        acceptStyle: {
            control: { type: "select" },
            options: ["primary", "outline", "warning"],
            description: "The style of the accept button",
        },
        cancelText: {
            control: "text",
            description: "The text for the cancel button",
        },
        showAlert: {
            description:
                "Whether to show the alert. This is a bindable prop and its visibility should be controlled by the parent component.",
            control: false,
            type: "boolean",
        },
    },
    args: {
        acceptFn: fn(),
        showAlert: true,
    },
});
</script>

<Story
  name="Default"
  args={{
    title: 'Confirm Action',
    body: 'Are you sure you want to proceed with this action?',
    acceptText: 'Confirm',
    acceptStyle: 'primary',
    cancelText: 'Cancel',
    showAlert: true
  }}
/>
