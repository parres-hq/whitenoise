<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import Toast from "./Toast.svelte";

const { Story } = defineMeta({
    title: "Components/Toast",
    component: Toast,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        toast: {
            control: "object",
            description: "The toast object containing title, message, type, and id",
        },
    },
});

const mockToast = {
    id: "1",
    title: "Success",
    message: "Operation completed successfully",
    type: "success" as const,
};

const mockErrorToast = {
    id: "2",
    title: "Error",
    message: "Something went wrong",
    type: "error" as const,
};

const mockInfoToast = {
    id: "3",
    title: "Information",
    message: "Here's something you should know",
    type: "info" as const,
};
</script>

<Story 
  name="Success" 
  args={{
    toast: mockToast
  }} 
/>

<Story 
  name="Error" 
  args={{
    toast: mockErrorToast
  }} 
/>

<Story 
  name="Info" 
  args={{
    toast: mockInfoToast
  }} 
/> 
