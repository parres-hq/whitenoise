<script lang="ts" module>
import type { Message } from "$lib/types/chat";
import { NostrMlsGroupType } from "$lib/types/nostr";
import { defineMeta } from "@storybook/addon-svelte-csf";
import MessageBar from "./MessageBar.svelte";

const { Story } = defineMeta({
    title: "Components/MessageBar",
    component: MessageBar,
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
            description: "The NostrMlsGroup object containing group information",
        },
        replyToMessage: {
            control: "object",
            description: "Optional message being replied to",
        },
        handleNewMessage: {
            control: false,
            description: "Callback function when a new message is created",
        },
        isReplyToMessageDeleted: {
            control: "boolean",
            description: "Whether the replied to message has been deleted",
        },
    },
});

const mockGroup = {
    mls_group_id: new Uint8Array([1, 2, 3, 4]),
    nostr_group_id: "test_group_id",
    name: "Test Group",
    description: "A test group for storybook",
    admin_pubkeys: ["test_admin_pubkey"],
    last_message_at: Date.now(),
    last_message_id: "test_message_id",
    group_type: NostrMlsGroupType.Group,
};

const mockReplyMessage = {
    id: "test_message_id",
    pubkey: "test_pubkey",
    content: "This is a test message being replied to",
    createdAt: Date.now(),
    reactions: [],
    isSingleEmoji: false,
    isMine: false,
    tokens: [{ Text: "This is a test message being replied to" }],
    event: {
        id: "test_event_id",
        pubkey: "test_pubkey",
        created_at: Date.now(),
        kind: 1,
        tags: [],
        content: "This is a test message being replied to",
    },
};

function handleNewMessage(message: Message) {
    console.log("New message:", message);
}
</script>

<Story 
  name="Default" 
  args={{
    group: mockGroup,
    handleNewMessage: handleNewMessage
  }} 
/>

<Story 
  name="Reply" 
  args={{
    group: mockGroup,
    replyToMessage: mockReplyMessage,
    handleNewMessage: handleNewMessage,
    isReplyToMessageDeleted: false
  }} 
/>

<Story 
  name="Deleted Reply" 
  args={{
    group: mockGroup,
    replyToMessage: mockReplyMessage,
    handleNewMessage: handleNewMessage,
    isReplyToMessageDeleted: true
  }} 
/>
