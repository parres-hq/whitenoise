<script lang="ts">
import { goto } from "$app/navigation";
import Alert from "$lib/components/Alert.svelte";
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import Header from "$lib/components/Header.svelte";
import LoginSheet from "$lib/components/LoginSheet.svelte";
import * as Accordion from "$lib/components/ui/accordion";
import Button from "$lib/components/ui/button/button.svelte";
import * as Sheet from "$lib/components/ui/sheet";

import * as AlertDialog from "$lib/components/ui/alert-dialog";
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
let showDeleteAlert = $state(false);
let showKeyPackageAlert = $state(false);
let showDeleteKeyPackagesAlert = $state(false);
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
        .then(() => {
            toast.success("Successfully logged out");
        })
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
    showDeleteAlert = true;
}

function launchKeyPackage() {
    showKeyPackageAlert = true;
}

function deleteAllKeyPackages() {
    showDeleteKeyPackagesAlert = true;
}

function publishKeyPackage() {
    invoke("publish_new_key_package", {})
        .then(() => {
            toast.success("Key Package Published");
            showKeyPackageAlert = false;
        })
        .catch((e) => {
            toast.error("Error Publishing Key Package");
            console.error(e);
        });
}
</script>

{#if showDeleteAlert}
    <Alert
        title="Delete everything?"
        body="This will delete all group and message data, and sign you out of all accounts. This will not delete your nostr keys or any other events you've published to relays. Are you sure you want to delete all data from White Noise? This cannot be undone."
        acceptFn={async () => {
            invoke("delete_all_data")
                .then(() => {
                    toast.info("All accounts, groups, and messages have been deleted");
                    showDeleteAlert = false;
                    goto("/login");
                })
                .catch((e) => {
                    toast.error("Error deleting data");
                    console.error(e);
                });
        }}
        acceptText="Yes, delete everything"
        acceptStyle="warning"
        cancelText="Cancel"
        bind:showAlert={showDeleteAlert}
    />
{/if}

{#if showKeyPackageAlert}
    <Alert
        title="Publish Key Package?"
        body="Are you sure you want to publish a new Key Package event to relays?"
        acceptFn={publishKeyPackage}
        acceptText="Publish Key Package"
        acceptStyle="primary"
        cancelText="Cancel"
        bind:showAlert={showKeyPackageAlert}
    />
{/if}

{#if showDeleteKeyPackagesAlert}
    <Alert
        title="Delete All Key Packages?"
        body="Are you sure you want to send delete requests to all relays where your key packages are found?"
        acceptFn={async () => {
            invoke("delete_all_key_packages")
                .then(() => {
                    toast.success("Key Packages Deleted");
                    showDeleteKeyPackagesAlert = false;
                })
                .catch((e) => {
                    toast.error("Error Deleting Key Packages");
                    console.error(e);
                });
        }}
        acceptText="Yes, delete all key packages"
        acceptStyle="warning"
        cancelText="Cancel"
        bind:showAlert={showDeleteKeyPackagesAlert}
    />
{/if}

<Header backLocation="/chats" title="Settings" />

<main class="px-4 py-6 flex flex-col gap-4">
    <Accordion.Root bind:value={accordionOpenSection} class="px-2">
        <Accordion.Item value="profile">
            <Accordion.Trigger class="overflow-visible">
                <h2 class="section-title">Profile</h2>
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

                    <ul class="section-list">
                        <li class="section-list-item">
                            <a href="/settings/profile/" class="row-button">
                                <div class="row-button-content">
                                    <User size={24} class="shrink-0"/>
                                    <span>Edit profile</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="section-list-item">
                            <a href="/settings/nostr-keys/" class="row-button">
                                <div class="row-button-content">
                                    <Password size={24} class="shrink-0"/>
                                    <span>Nostr keys</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="section-list-item">
                            <a href="/settings/network/" class="row-button">
                                <div class="row-button-content">
                                    <Satellite size={24} class="shrink-0"/>
                                    <span>Network</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="section-list-item">
                            <a href="/settings/wallet/" class="row-button">
                                <div class="row-button-content">
                                    <Wallet size={24} class="shrink-0"/>
                                    <span>Wallet</span>
                                </div>
                                <ChevronRight size={24} class="icon-right"/>
                            </a>
                        </li>
                        <li class="section-list-item">
                            <button onclick={() => handleLogout($activeAccount!.pubkey)} class="row-button">
                                <div class="row-button-content">
                                    <Logout size={24} class="shrink-0"/>
                                    <span>Sign out</span>
                                </div>
                            </button>
                        </li>
                    </ul>
                </div>
            </Accordion.Content>
        </Accordion.Item>
        <Accordion.Item value="privacy">
            <Accordion.Trigger>
                <h2 class="section-title">Privacy & Security</h2>
            </Accordion.Trigger>
            <Accordion.Content>
                <div class="overflow-hidden p-0 m-0">
                    <ul class="section-list">
                        <li class="section-list-item">
                            <button onclick={deleteAll} class="row-button">
                            <div class="row-button-content">
                                <TrashCan size={24} class="shrink-0"/>
                                <span>Delete all data</span>
                            </div>
                            </button>
                        </li>
                    </ul>
                </div>
            </Accordion.Content>
        </Accordion.Item>
        <Accordion.Item value="developer">
            <Accordion.Trigger>
                <h2 class="section-title">Developer Settings</h2>
            </Accordion.Trigger>
            <Accordion.Content>
                <div class="overflow-hidden p-0 m-0">
                    <ul class="section-list">
                        <li class="section-list-item">
                            <button onclick={launchKeyPackage} class="row-button">
                                <div class="row-button-content">
                                    <Password size={24} class="shrink-0"/>
                                    <span>Publish a key package</span>
                                </div>
                            </button>
                        </li>
                        <li class="section-list-item">
                            <AlertDialog.Root>
                                <AlertDialog.Trigger>
                                    <Button variant="ghost" size="lg" class="row-button">
                                        <div class="row-button-content">
                                            <TrashCan size={24} class="shrink-0"/>
                                            <span>Delete all key packages</span>
                                        </div>
                                    </Button>
                                </AlertDialog.Trigger>
                                <AlertDialog.Content>
                                  <AlertDialog.Header>
                                    <AlertDialog.Title>Are you absolutely sure?</AlertDialog.Title>
                                    <AlertDialog.Description>
                                      Well send delete requests to all relays where your key packages are found.
                                    </AlertDialog.Description>
                                  </AlertDialog.Header>
                                  <AlertDialog.Footer>
                                    <AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
                                    <AlertDialog.Action>Continue</AlertDialog.Action>
                                  </AlertDialog.Footer>
                                </AlertDialog.Content>
                              </AlertDialog.Root>

                        </li>
                        <li class="section-list-item">
                            <button onclick={testNotification} class="row-button">
                                <div class="row-button-content">
                                    <Notification size={24} class="shrink-0"/>
                                    <span>Test notifications</span>
                                </div>
                            </button>
                        </li>
                        <li class="section-list-item">
                            <button onclick={() => toast.success("Toast success")} class="row-button">
                                <div class="row-button-content">
                                    <Notification size={24} class="shrink-0"/>
                                    <span>Test toast success</span>
                                </div>
                            </button>
                        </li>
                        <li class="section-list-item">
                            <button onclick={() => toast.error("Toast error")} class="row-button">
                                <div class="row-button-content">
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

<style lang="postcss">
    .section-title {
        @apply text-3xl font-normal text-primary leading-none;
    }

    .section-list {
        @apply list-none p-0 m-0 overflow-hidden;
    }

    .section-list-item {
        @apply p-0 m-0 leading-none text-2xl text-muted-foreground;
    }

    .section-list-item > button,
    .section-list-item > a {
        @apply flex flex-row justify-between items-center py-4 w-full no-underline;
    }

    .row-button-content {
        @apply flex flex-row gap-3 items-center;
    }
</style>
