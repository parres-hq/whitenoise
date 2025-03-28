<script lang="ts" module>
import { InviteState, NostrMlsGroupType } from "$lib/types/nostr";
import { defineMeta } from "@storybook/addon-svelte-csf";
import InviteListItem from "./InviteListItem.svelte";

const { Story } = defineMeta({
    title: "Components/InviteListItem",
    component: InviteListItem,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        invite: {
            control: "object",
            description: "The invite object containing all necessary information",
        },
    },
});
</script>

<Story 
  name="Default" 
  args={{
    invite: {
        event: {
            id: "test_event_id",
            pubkey: "test_pubkey",
            created_at: Date.now(),
            kind: 444,
            tags: [],
            content: "Test invite content"
        },
        mls_group_id: "test_mls_group_id",
        group_name: "Test Group",
        group_description: "A test group for storybook",
        group_admin_pubkeys: ["test_admin_pubkey"],
        group_relays: ["wss://relay.example.com"],
        inviter: "test_inviter_pubkey",
        member_count: 5,
        state: InviteState.Pending
    }
  }} 
/> 
