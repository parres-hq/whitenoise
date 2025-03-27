<script lang="ts">
import { goto } from "$app/navigation";
import { page } from "$app/state";
import { accounts, activeAccount, setActiveAccount } from "$lib/stores/accounts";
import { ChatsCircle, Gear, Phone } from "phosphor-svelte";
import Avatar from "./Avatar.svelte";
let { activeTab } = $props();

function handleAccountChange(pubkey: string) {
    if (page.url.pathname.startsWith("/chats/")) {
        goto("/chats");
    }
    setTimeout(() => {
        setActiveAccount(pubkey);
    }, 50);
}
</script>

<div class="hidden md:flex">
    <div
        class="sticky top-0 left-0 bottom-0 border-r border-gray-700 bg-gray-800 h-dvh p-2 gap-4 flex flex-col justify-between z-10"
    >
        <div class="flex flex-col gap-4 items-center">
            <a href="/chats" class="p-2">
                <span class="sidebar-link {activeTab === 'chats' ? 'active' : ''}">
                    <ChatsCircle size={32} weight={activeTab === "chats" ? "fill" : "light"} />
                </span>
            </a>
            <a href="/calls" class="p-2">
                <span class="sidebar-link {activeTab === 'calls' ? 'active' : ''}">
                    <Phone size={32} weight={activeTab === "calls" ? "fill" : "light"} />
                </span>
            </a>
        </div>
        <div class="flex flex-col gap-4 justify-between items-center mt-auto mb-0">
            {#each $accounts as account (account.pubkey)}
                <button onclick={() => handleAccountChange(account.pubkey)}>
                    <Avatar
                        pubkey={account.pubkey}
                        picture={account.metadata.picture}
                        pxSize={32}
                        showRing={account.pubkey === $activeAccount?.pubkey}
                    />
                </button>
            {/each}
            <a href="/settings" class="p-2">
                <span class="sidebar-link {activeTab === 'settings' ? 'active' : ''}">
                    <Gear size={32} weight={activeTab === "settings" ? "fill" : "light"} />
                </span>
            </a>
        </div>
    </div>
</div>
