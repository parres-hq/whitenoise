<script lang="ts">
import { goto } from "$app/navigation";
import Avatar from "$lib/components/Avatar.svelte";
import ConfirmSheet from "$lib/components/ConfirmSheet.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import Header from "$lib/components/Header.svelte";
import LoginSheet from "$lib/components/LoginSheet.svelte";
import * as Accordion from "$lib/components/ui/accordion";
import Button from "$lib/components/ui/button/button.svelte";
import * as Sheet from "$lib/components/ui/sheet";
import {
    LogoutError,
    accounts,
    activeAccount,
    fetchRelays,
    logout,
    setActiveAccount,
    updateAccountsStore,
} from "$lib/stores/accounts";
import { nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import {
    isPermissionGranted,
    requestPermission,
    sendNotification,
} from "@tauri-apps/plugin-notification";
import AddLarge from "carbon-icons-svelte/lib/AddLarge.svelte";
import ChevronRight from "carbon-icons-svelte/lib/ChevronRight.svelte";
import ChevronSort from "carbon-icons-svelte/lib/ChevronSort.svelte";
import Logout from "carbon-icons-svelte/lib/Logout.svelte";
import Notification from "carbon-icons-svelte/lib/Notification.svelte";
import Password from "carbon-icons-svelte/lib/Password.svelte";
import Satellite from "carbon-icons-svelte/lib/Satellite.svelte";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import User from "carbon-icons-svelte/lib/User.svelte";
import Wallet from "carbon-icons-svelte/lib/Wallet.svelte";
import { onDestroy, onMount } from "svelte";
import { toast } from "svelte-sonner";

let showLoginSheet = $state(false);
let showSwitchAccountSheet = $state(false);
let addProfileLoading = $state(false);

let accordionOpenSection = $state("profile");

let unlisten: UnlistenFn;

onMount(async () => {
    if (!unlisten) {
        unlisten = await listen<string>("account_changed", (_event) => {
            updateAccountsStore().then(() => {
                console.log("account_changed & updateAccountStore from settings page.");
                fetchRelays();
                showSwitchAccountSheet = false;
                showLoginSheet = false;
            });
        });
    }

    fetchRelays();
});

onDestroy(() => {
    unlisten?.();
});

async function handleLogout(pubkey: string): Promise<void> {
    logout(pubkey)
        .then(() => toast.success("Successfully logged out"))
        .catch((e) => {
            if (e instanceof LogoutError) {
                goto("/");
            } else {
                toast.error("Failed to log out");
                console.error(e);
            }
        });
}

async function testNotification() {
    let permissionGranted = await isPermissionGranted();

    if (!permissionGranted) {
        permissionGranted = "granted" === (await requestPermission());
    }
    if (permissionGranted) {
        sendNotification({
            title: "White Noise",
            body: "Notification test successful!",
        });
    }
}

async function deleteAll() {
    invoke("delete_all_data")
        .then(() => {
            toast.info("All accounts, groups, and messages have been deleted");
            goto("/login");
        })
        .catch((e) => {
            toast.error("Error deleting data");
            console.error(e);
        });
}

function deleteAllKeyPackages() {
    invoke("delete_all_key_packages")
        .then(() => toast.success("Key Packages Deleted"))
        .catch((e) => {
            toast.error("Error Deleting Key Packages");
            console.error(e);
        });
}

function publishKeyPackage() {
    invoke("publish_new_key_package", {})
        .then(() => toast.success("Key Package Published"))
        .catch((e) => {
            toast.error("Error Publishing Key Package");
            console.error(e);
        });
}
</script>

<Header backLocation="/chats" title="Settings" />

<main class="px-4 py-6 flex flex-col gap-4">
    <Accordion.Root type="single" bind:value={accordionOpenSection} class="px-2">
        <Accordion.Item value="profile">
            <Accordion.Trigger class="overflow-visible">
                <h2 class="text-3xl font-normal text-primary leading-none">Profile</h2>
                <LoginSheet title="Add new profile" loading={addProfileLoading} bind:sheetVisible={showLoginSheet} showCreateAccount={true}>
                    <Button variant="ghost" size="icon" class="p-2 shrink-0 -mr-2">
                        <AddLarge size={24} class="shrink-0 !h-6 !w-6" />
                    </Button>
                </LoginSheet>
            </Accordion.Trigger>
            <Accordion.Content class="overflow-visible">
                <div class="overflow-visible p-0 m-0">
                    <div class="flex flex-row gap-3 items-center min-w-0 w-full mb-4 overflow-visible">
                        <Avatar
                            pubkey={$activeAccount!.pubkey}
                            picture={$activeAccount!.metadata?.picture}
                            pxSize={56}
                        />
                        <div class="flex flex-col gap-0 min-w-0 justify-start text-left truncate w-full">
                            <div class="truncate text-lg font-medium">
                                {nameFromMetadata($activeAccount!.metadata, $activeAccount!.pubkey)}
                            </div>
                            <div class="flex gap-4 items-center">
                                <FormattedNpub npub={npubFromPubkey($activeAccount!.pubkey)} showCopy={true} />
                            </div>
                        </div>
                        {#if $accounts.length > 1}
                            <Sheet.Root bind:open={showSwitchAccountSheet}>
                                <Sheet.Trigger>
                                    <Button variant="ghost" size="icon" class="p-2 shrink-0 -mr-2">
                                        <ChevronSort size={24} class="text-muted-foreground shrink-0 !w-6 !h-6" />
                                    </Button>
                                </Sheet.Trigger>
                                <Sheet.Content side="bottom" class="pb-safe-bottom px-0 max-h-[90%]">
                                    <div class="flex flex-col h-full relative">
                                        <Sheet.Header class="text-left mb-4 px-6 sticky top-0">
                                            <Sheet.Title>Switch profile</Sheet.Title>
                                        </Sheet.Header>
                                        <div class="max-h-[500px] flex flex-col gap-0.5 overflow-y-auto pb-6">
                                            {#each $accounts as account (account.pubkey)}
                                                <Button variant="ghost" size="lg" class="w-full h-fit flex flex-row gap-3 items-center min-w-0 w-full py-2 focus-visible:outline-none focus-visible:ring-0" onclick={() => setActiveAccount(account.pubkey)}>
                                                    <Avatar
                                                        pubkey={account.pubkey}
                                                        picture={account.metadata?.picture}
                                                        pxSize={56}
                                                    />
                                                    <div class="flex flex-col gap-0 min-w-0 justify-start text-left truncate w-full">
                                                        <div class="truncate text-lg font-medium">
                                                            {nameFromMetadata(account.metadata, account.pubkey)}
                                                        </div>
                                                        <div class="flex gap-4 items-center">
                                                            <FormattedNpub npub={npubFromPubkey(account.pubkey)} showCopy={false} />
                                                        </div>
                                                    </div>
                                                </Button>
                                            {/each}
                                        </div>
                                    </div>
                                </Sheet.Content>
                            </Sheet.Root>
                        {/if}
                    </div>

                    <ul class="list-none p-0 m-0 overflow-hidden">
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <a href="/settings/profile/" class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                <div class="flex flex-row gap-3 items-center">
                                    <User size={24} class="shrink-0"/>
                                    <span>Edit profile</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <a href="/settings/nostr-keys/" class="flex flex-row justify-between items-center py-4 w-full no-underlinerow-button">
                                <div class="flex flex-row gap-3 items-center">
                                    <Password size={24} class="shrink-0"/>
                                    <span>Nostr keys</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <a href="/settings/network/" class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                <div class="flex flex-row gap-3 items-center">
                                    <Satellite size={24} class="shrink-0"/>
                                    <span>Network</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <a href="/settings/wallet/" class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                <div class="flex flex-row gap-3 items-center">
                                    <Wallet size={24} class="shrink-0"/>
                                    <span>Wallet</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <ConfirmSheet title="Sign out?" description="Are you sure you want to sign out of this account? If you haven't backed up your keys, you won't be able to recover them." acceptText="Sign out" cancelText="Cancel" acceptFn={() => handleLogout($activeAccount!.pubkey)}>
                                <button class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                    <div class="flex flex-row gap-3 items-center">
                                        <Logout size={24} class="shrink-0"/>
                                    <span>Sign out</span>
                                    </div>
                                </button>
                            </ConfirmSheet>
                        </li>
                    </ul>
                </div>
            </Accordion.Content>
        </Accordion.Item>
        <Accordion.Item value="privacy">
            <Accordion.Trigger>
                <h2 class="text-3xl font-normal text-primary leading-none">Privacy & Security</h2>
            </Accordion.Trigger>
            <Accordion.Content>
                <div class="overflow-hidden p-0 m-0">
                    <ul class="list-none p-0 m-0 overflow-hidden">
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <ConfirmSheet title="Delete everything?" description="This will delete all group and message data, and sign you out of all accounts but will not delete your nostr keys or any other events you've published to relays.<br><br>Are you sure you want to delete all data from White Noise? This cannot be undone." acceptText="Delete all data" cancelText="Cancel" acceptFn={deleteAll}>
                                <button class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                    <div class="flex flex-row gap-3 items-center">
                                <TrashCan size={24} class="shrink-0"/>
                                <span>Delete all data</span>
                                    </div>
                                </button>
                            </ConfirmSheet>
                        </li>
                    </ul>
                </div>
            </Accordion.Content>
        </Accordion.Item>
        <Accordion.Item value="developer">
            <Accordion.Trigger>
                <h2 class="text-3xl font-normal text-primary leading-none">Developer Settings</h2>
            </Accordion.Trigger>
            <Accordion.Content>
                <div class="overflow-hidden p-0 m-0">
                    <ul class="list-none p-0 m-0 overflow-hidden">
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <ConfirmSheet title="Publish a key package" description="Are you sure you want to publish a new Key Package event to relays?" acceptText="Publish key package" cancelText="Cancel" acceptFn={publishKeyPackage}>
                                <button class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                    <div class="flex flex-row gap-3 items-center">
                                        <Password size={24} class="shrink-0"/>
                                        <span>Publish a key package</span>
                                    </div>
                                </button>
                            </ConfirmSheet>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <ConfirmSheet title="Delete all key packages" description="Are you sure you want to delete all key packages?" acceptText="Delete all key packages" cancelText="Cancel" acceptFn={deleteAllKeyPackages}>
                                <button class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                    <div class="flex flex-row gap-3 items-center">
                                        <TrashCan size={24} class="shrink-0"/>
                                        <span>Delete all key packages</span>
                                    </div>
                                </button>
                            </ConfirmSheet>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <button onclick={testNotification} class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                <div class="flex flex-row gap-3 items-center">
                                    <Notification size={24} class="shrink-0"/>
                                    <span>Test notifications</span>
                                </div>
                            </button>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <button onclick={() => toast.success("Toast success")} class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                <div class="flex flex-row gap-3 items-center">
                                    <Notification size={24} class="shrink-0"/>
                                    <span>Test toast success</span>
                                </div>
                            </button>
                        </li>
                        <li class="p-0 m-0 leading-none text-2xl text-muted-foreground">
                            <button onclick={() => toast.error("Toast error")} class="flex flex-row justify-between items-center py-4 w-full no-underline">
                                <div class="flex flex-row gap-3 items-center">
                                    <Notification size={24} class="shrink-0"/>
                                    <span>Test toast error</span>
                                </div>
                            </button>
                        </li>
                    </ul>
                </div>
            </Accordion.Content>
        </Accordion.Item>
    </Accordion.Root>
</main>
