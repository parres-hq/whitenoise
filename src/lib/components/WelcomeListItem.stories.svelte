<script lang="ts" module>
import { NWelcomeState } from "$lib/types/nostr";
import { defineMeta } from "@storybook/addon-svelte-csf";
import WelcomeListItem from "./WelcomeListItem.svelte";

const { Story } = defineMeta({
    title: "Components/WelcomeListItem",
    component: WelcomeListItem,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        welcome: {
            control: "object",
            description: "The welcome object containing all necessary information",
        },
    },
});
</script>

<Story
  name="Default"
  args={{
    welcome: {
        id: "5d22aa659c7922d784b27f47b9971bc652b1f8a4224a8eca9162397ba701c9f2",
        event: {
            id: "test_event_id",
            pubkey: "7d92b004bce83a1fa097bbbf04c68436c5003b5d1b0c3f5121e59c2c32d17135",
            created_at: Date.now(),
            kind: 444,
            tags: [],
            content: "Test invite content"
        },
        mls_group_id: {
            value: { vec: new Uint8Array([1, 2, 3]) }
        },
        nostr_group_id: new Uint8Array([1, 2, 3]),
        wrapper_event_id: "7c2a4aba1246de3142ddaf8a5b11f3ee0bbc4ec2808a6d06454df800c365a9ac",
        group_name: "Test Group",
        group_description: "A test group for storybook",
        group_admin_pubkeys: ["7d92b004bce83a1fa097bbbf04c68436c5003b5d1b0c3f5121e59c2c32d17135"],
        group_relays: [{
            relay_url: "wss://relay.example.com",
            mls_group_id: {
                value: { vec: new Uint8Array([1, 2, 3]) }
            }
        }],
        welcomer: "7d92b004bce83a1fa097bbbf04c68436c5003b5d1b0c3f5121e59c2c32d17135",
        member_count: 5,
        state: NWelcomeState.Pending
    }
  }}
/>
