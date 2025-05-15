<script lang="ts" module>
import { NostrMlsGroupState, NostrMlsGroupType } from "$lib/types/nostr";
import { defineMeta } from "@storybook/addon-svelte-csf";
import GroupListItem from "./GroupListItem.svelte";

const { Story } = defineMeta({
    title: "Components/GroupListItem",
    component: GroupListItem,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        group: {
            control: "object",
            description: "The group object containing all necessary information",
        },
    },
});
</script>

<Story
  name="Default"
  args={{
    group: {
        mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
        nostr_group_id: new Uint8Array([1, 2, 3, 4]),
        name: "Test Group",
        description: "A test group for storybook",
        admin_pubkeys: ["7f5c2b32e5f23a"],
        last_message_at: Date.now(),
        last_message_id: "test_message_id",
        group_type: NostrMlsGroupType.Group,
        state: NostrMlsGroupState.Active,
        epoch: 1,
    }
  }}
/>
