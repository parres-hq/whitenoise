<script lang="ts" module>
import { accounts, activeAccount } from "$lib/stores/accounts";
import { defineMeta } from "@storybook/addon-svelte-csf";
import { get } from "svelte/store";
import Sidebar from "./Sidebar.svelte";

accounts.set([
    {
        pubkey: "user1_pubkey",
        metadata: {
            name: "User 1",
            display_name: "User One",
            picture: "https://api.dicebear.com/7.x/avataaars/svg?seed=user1",
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
    },
    {
        pubkey: "user2_pubkey",
        metadata: {
            name: "User 2",
            display_name: "User Two",
            picture: "https://api.dicebear.com/7.x/avataaars/svg?seed=user2",
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
        last_used: Date.now() - 1000,
        active: false,
    },
]);

activeAccount.set(get(accounts)[0]);

const { Story } = defineMeta({
    title: "Components/Sidebar",
    component: Sidebar,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
                height: "400px",
            },
        },
        layout: "fullscreen",
    },
    argTypes: {
        activeTab: {
            control: "select",
            options: ["chats", "calls", "settings"],
            description: "The currently active tab",
        },
    },
});
</script>

<Story 
  name="Default" 
  args={{
    activeTab: "chats"
  }} 
/>

<Story 
  name="Calls" 
  args={{
    activeTab: "calls"
  }} 
/>

<Story 
  name="Settings" 
  args={{
    activeTab: "settings"
  }} 
/>
