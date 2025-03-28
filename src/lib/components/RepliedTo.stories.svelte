<script lang="ts" module>
import { activeAccount } from "$lib/stores/accounts";
import { defineMeta } from "@storybook/addon-svelte-csf";
import RepliedTo from "./RepliedTo.svelte";

// Mock the activeAccount store
activeAccount.set({
    pubkey: "own_pubkey",
    metadata: {
        name: "Test User",
        display_name: "Test User",
    },
    nostr_relays: [],
    inbox_relays: [],
    key_package_relays: [],
    mls_group_ids: [],
    settings: {
        darkTheme: false,
        devMode: false,
        lockdownMode: false,
    },
    onboarding: {
        inbox_relays: false,
        key_package_relays: false,
        publish_key_package: false,
    },
    last_used: Date.now(),
    active: true,
});

const { Story } = defineMeta({
    title: "Components/RepliedTo",
    component: RepliedTo,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        message: {
            control: "object",
            description: "The message being replied to",
        },
        isDeleted: {
            control: "boolean",
            description: "Whether the replied to message has been deleted",
        },
    },
});

const mockMessage = {
    id: "test_message_id",
    pubkey: "7f5c2b32e5f23a",
    content: "This is the original message that was replied to",
    createdAt: Date.now(),
    reactions: [],
    isSingleEmoji: false,
    isMine: false,
    event: {
        id: "test_event_id",
        pubkey: "7f5c2b32e5f23a",
        created_at: Date.now(),
        kind: 1,
        tags: [],
        content: "This is the original message that was replied to",
    },
};

const mockOwnMessage = {
    ...mockMessage,
    pubkey: "own_pubkey",
    isMine: true,
};
</script>

<Story 
  name="Default" 
  args={{
    message: mockMessage,
    isDeleted: false
  }} 
/>

<Story 
  name="Own Message" 
  args={{
    message: mockOwnMessage,
    isDeleted: false
  }} 
/>

<Story 
  name="Deleted Message" 
  args={{
    message: mockMessage,
    isDeleted: true
  }} 
/>

<Story 
  name="Loading" 
  args={{
    message: undefined
  }} 
/> 
