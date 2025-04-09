<script lang="ts">
import Header from "$lib/components/Header.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import * as Sheet from "$lib/components/ui/sheet";
import { activeAccount, colorForRelayStatus } from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import { invoke } from "@tauri-apps/api/core";
import AddLarge from "carbon-icons-svelte/lib/AddLarge.svelte";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";
import { toast } from "svelte-sonner";

// Inbox relay list
let inboxRelays: string[] | undefined = $derived($activeAccount?.inbox_relays);
// Key Package relay list
let keyPackageRelays: string[] | undefined = $derived($activeAccount?.key_package_relays);

// State for add relay sheet
let showAddRelaySheet = $state(false);
let newRelayUrl = $state("wss://");
let urlError = $state("");
let currentRelayType = $state<"inbox" | "key_package">("inbox");
let isLoading = $state(false);
let relayStatuses = $state<Record<string, string>>({});

// Fetch relay lists
async function loadRelays() {
    // Fetch all connected relays with status and relay lists in parallel
    try {
        isLoading = true;
        const [statuses, inboxRelaysResult, keyPackageRelaysResult] = await Promise.all([
            invoke<Record<string, string>>("fetch_relays"),
            invoke<string[]>("fetch_relays_list", { kind: 10050 }),
            invoke<string[]>("fetch_relays_list", { kind: 10051 }),
        ]);

        relayStatuses = statuses;
        inboxRelays = inboxRelaysResult;
        keyPackageRelays = keyPackageRelaysResult;
    } catch (error) {
        toast.error($t("network.fetchRelayDataError"));
        console.error(error);
    } finally {
        isLoading = false;
    }
}

$inspect($activeAccount);
$inspect(inboxRelays);
$inspect(keyPackageRelays);
$inspect(relayStatuses);

// Open add relay sheet
function openAddRelaySheet(type: "inbox" | "key_package") {
    currentRelayType = type;
    newRelayUrl = "wss://";
    urlError = "";
    showAddRelaySheet = true;
}

// Close add relay sheet
function closeAddRelaySheet() {
    showAddRelaySheet = false;
}

// Add a new relay
async function addRelay() {
    if (!newRelayUrl.startsWith("wss://") && !newRelayUrl.startsWith("ws://")) {
        urlError = $t("network.invalidRelayUrlFormat");
        return;
    }

    // Check for duplicate URL in the specific relay list being modified
    if (currentRelayType === "inbox" && inboxRelays?.includes(newRelayUrl)) {
        urlError = $t("network.inboxRelayAlreadyConfigured");
        return;
    }
    if (currentRelayType === "key_package" && keyPackageRelays?.includes(newRelayUrl)) {
        urlError = $t("network.keyPackageRelayAlreadyConfigured");
        return;
    }

    if (!$activeAccount) {
        toast.error($t("network.noActiveAccountError"));
        return;
    }

    isLoading = true;
    try {
        if (currentRelayType === "inbox") {
            inboxRelays = [...(inboxRelays || []), newRelayUrl];
            await invoke("publish_relay_list", { relays: inboxRelays, kind: 10050 });
        } else if (currentRelayType === "key_package") {
            keyPackageRelays = [...(keyPackageRelays || []), newRelayUrl];
            await invoke("publish_relay_list", { relays: keyPackageRelays, kind: 10051 });
        }
        closeAddRelaySheet();
        await loadRelays();
    } catch (error) {
        toast.error($t("network.addRelayError"));
        console.error(error);
    } finally {
        isLoading = false;
        showAddRelaySheet = false;
    }
}

// Remove a relay
async function removeRelay(type: "inbox" | "key_package", relay_url: string) {
    if (type === "inbox") {
        inboxRelays = inboxRelays?.filter((url) => url !== relay_url);
        await invoke("publish_relay_list", { relays: inboxRelays, kind: 10050 });
    } else if (type === "key_package") {
        keyPackageRelays = keyPackageRelays?.filter((url) => url !== relay_url);
        await invoke("publish_relay_list", { relays: keyPackageRelays, kind: 10051 });
    }
}

async function handlePaste() {
    try {
        const text = await readFromClipboard();
        if (text) {
            newRelayUrl = text;
        } else {
            urlError = $t("clipboard.emptyTextError");
        }
    } catch (e) {
        urlError = $t("clipboard.readError");
    }
}

onMount(async () => {
    await loadRelays();
});
</script>

<Header backLocation="/settings" title={$t("network.title")} />

<main class="px-4 flex flex-col gap-12 py-6">
    <section class="flex flex-col gap-3">
        <div class="flex justify-between items-center">
            <h2 class="text-2xl font-normal">{$t("network.connectedRelays")}</h2>
        </div>

        {#if Object.keys(relayStatuses).length === 0}
            <p class="text-lg text-muted-foreground">{$t("network.noRelaysConnected")}</p>
        {:else}
            <ul class="flex flex-col gap-1">
                {#each Object.entries(relayStatuses) as [relay_url, relay_status]}
                <li class="flex items-center justify-between py-2">
                    <span class="text-lg">{relay_url}</span>
                        <div class="flex items-center gap-2">
                            <div class="w-2 h-2 rounded-full {colorForRelayStatus(relay_status)}"></div>
                            <span class="text-xs text-muted-foreground">{$t(`network.relayStatuses.${relay_status.toLowerCase()}`)}</span>
                        </div>
                </li>
                {/each}
            </ul>
        {/if}
    </section>

    <section class="flex flex-col gap-3">
        <div class="flex justify-between items-center">
            <h2 class="text-2xl font-normal">{$t("network.inboxRelayList")}</h2>
            <Button
                variant="ghost"
                size="icon"
                onclick={() => openAddRelaySheet('inbox')}
                class="shrink-0! p-0!"
                aria-label={$t("network.addInboxRelay")}
            >
                <AddLarge size={24} class="w-6! h-6! shrink-0"/>
            </Button>
        </div>

        {#if !inboxRelays ||inboxRelays.length === 0}
            <p class="text-sm text-muted-foreground">{$t("network.noInboxRelays")}</p>
        {:else}
            <ul class="flex flex-col">
                {#each inboxRelays! as relay_url}
                    <li class="flex items-center justify-between py-2 border-b border-border last:border-none">
                        <span class="text-base">{relay_url}</span>
                        <Button variant="ghost" size="icon" aria-label={$t("network.removeRelay")} onclick={() => removeRelay("inbox", relay_url)}>
                            <TrashCan size={20} />
                        </Button>
                    </li>
                {/each}
            </ul>
        {/if}
    </section>

    <!-- Key Package Relay List Section -->
    <section class="flex flex-col gap-3">
        <div class="flex justify-between items-center">
            <h2 class="text-2xl font-normal">{$t("network.keyPackageRelaysList")}</h2>
            <Button
                variant="ghost"
                size="icon"
                class="shrink-0! p-0!"
                onclick={() => openAddRelaySheet('key_package')}
                aria-label={$t("network.addKeyPackageRelay")}
            >
                <AddLarge size={24} class="w-6! h-6! shrink-0"/>
            </Button>
        </div>

        {#if !keyPackageRelays || keyPackageRelays.length === 0}
            <p class="text-sm text-muted-foreground">{$t("network.noKeyPackageRelays")}</p>
        {:else}
            <ul class="flex flex-col">
                {#each keyPackageRelays! as relay_url}
                    <li class="flex items-center justify-between py-2 border-b border-border last:border-none">
                        <span class="text-base">{relay_url}</span>
                        <Button
                            variant="ghost"
                            size="icon"
                            aria-label={$t("network.removeRelay")}
                            onclick={() => removeRelay("key_package", relay_url)}
                        >
                            <TrashCan size={20} />
                        </Button>
                    </li>
                {/each}
            </ul>
        {/if}
    </section>
</main>

<!-- Add Relay Sheet -->
<Sheet.Root bind:open={showAddRelaySheet}>
    <Sheet.Content side="bottom" class="pb-20">
        <Sheet.Header class="text-left mb-8">
            <Sheet.Title>{$t("network.addNewRelay")}</Sheet.Title>
        </Sheet.Header>
        <div class="flex flex-col gap-x-4 relative">
            <div class="flex flex-col gap-0">
                <div class="flex flex-row gap-2 pl-1">
                    <Input
                        bind:value={newRelayUrl}
                        placeholder="wss://..."
                        type="text"
                        class="w-full focus-visible:ring-0"
                    />
                    <Button variant="outline" size="icon" onclick={handlePaste}>
                        <Paste size={16} />
                    </Button>
                </div>
                <div class="text-destructive text-sm mt-2 min-h-[1.25rem]">
                    {urlError}
                </div>
            </div>
        </div>
        <Button size="lg" onclick={addRelay} disabled={isLoading || !newRelayUrl} class="text-base font-medium w-full h-fit fixed bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))] md:mt-6">{$t("network.addRelay")}</Button>
    </Sheet.Content>
</Sheet.Root>
