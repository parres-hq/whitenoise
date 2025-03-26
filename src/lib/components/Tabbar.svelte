<script lang="ts">
import { ChatCircle, GearSix, Phone } from "phosphor-svelte";

let { activeTab = $bindable("chats") }: { activeTab: string } = $props();
</script>

<nav
    class="fixed inset-x-0 bottom-0 border-t border-gray-700 py-4 min-w-96 overflow-hidden md:hidden bg-gray-950/80 backdrop-blur-xs z-10 pb-safe-bottom"
>
    <div class="max-w-(--breakpoint-xl) mx-auto px-4 flex justify-around items-center">
        <a href="/chats">
            <span class="tabbar-link {activeTab === 'chats' ? 'active' : ''}">
                <ChatCircle size={30} weight={activeTab === "chats" ? "fill" : "light"} />
                Chats
            </span>
        </a>
        <a href="/calls">
            <span class="tabbar-link {activeTab === 'calls' ? 'active' : ''}">
                <Phone size={30} weight={activeTab === "calls" ? "fill" : "light"} />
                Calls
            </span>
        </a>
        <a href="/settings">
            <span class="tabbar-link {activeTab === 'settings' ? 'active' : ''}">
                <GearSix size={30} weight={activeTab === "settings" ? "fill" : "light"} />
                Settings
            </span>
        </a>
    </div>
</nav>

<style lang="postcss">
    .tabbar-link {
        @apply flex flex-col items-center font-medium  gap-2 text-sm;
    }
    .tabbar-link.active {
        @apply text-blue-500;
    }
</style>
