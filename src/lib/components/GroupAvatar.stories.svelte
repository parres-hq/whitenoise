<script lang="ts" module>
import { NostrMlsGroupType } from "$lib/types/nostr";
import { defineMeta } from "@storybook/addon-svelte-csf";
import GroupAvatar from "./GroupAvatar.svelte";

const { Story } = defineMeta({
    title: "Components/GroupAvatar",
    component: GroupAvatar,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        groupType: {
            control: "select",
            options: Object.values(NostrMlsGroupType),
            description: "Type of the group",
        },
        groupName: {
            control: "text",
            description: "Name of the group",
        },
        counterpartyPubkey: {
            control: "text",
            description: "Public key of the counterparty for DMs",
        },
        pxSize: {
            control: "number",
            description: "Size of the avatar in pixels",
        },
    },
});
</script>

<Story 
  name="Default" 
  args={{
    groupType: NostrMlsGroupType.Group,
    groupName: "Test Group",
    pxSize: 64
  }} 
/> 
