<script lang="ts">
import { goto } from "$app/navigation";
import Alert from "$lib/components/Alert.svelte";
import Avatar from "$lib/components/Avatar.svelte";
import FormattedNpub from "$lib/components/FormattedNpub.svelte";
import Header from "$lib/components/Header.svelte";
import {
    LogoutError,
    activeAccount,
    createAccount,
    fetchRelays,
    login,
    logout,
    setActiveAccount,
    updateAccountsStore,
} from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import { isValidHexKey, isValidNsec, nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import {
    isPermissionGranted,
    requestPermission,
    sendNotification,
} from "@tauri-apps/plugin-notification";
import { ChevronRight } from "carbon-icons-svelte";
import AddLarge from "carbon-icons-svelte/lib/AddLarge.svelte";
import Logout from "carbon-icons-svelte/lib/Logout.svelte";
import Notification from "carbon-icons-svelte/lib/Notification.svelte";
import Password from "carbon-icons-svelte/lib/Password.svelte";
import Satellite from "carbon-icons-svelte/lib/Satellite.svelte";
import TrashCan from "carbon-icons-svelte/lib/TrashCan.svelte";
import User from "carbon-icons-svelte/lib/User.svelte";
import Wallet from "carbon-icons-svelte/lib/Wallet.svelte";

import { cubicInOut } from "svelte/easing";
import { slide } from "svelte/transition";

import { onDestroy, onMount } from "svelte";

let showDeleteAlert = $state(false);
let showKeyPackageAlert = $state(false);
let showDeleteKeyPackagesAlert = $state(false);
let nsecOrHex = $state("");
let showLoginError = $state(false);
let loginError = $state("");

let showProfileSection = $state(true);
let showPrivacySection = $state(false);
let showDeveloperSection = $state(false);

let slideParams = { duration: 300, easing: cubicInOut };

let unlisten: UnlistenFn;

let toastState = getToastState();

onMount(async () => {
    if (!unlisten) {
        unlisten = await listen<string>("account_changed", (_event) => {
            updateAccountsStore().then(() => {
                console.log("account_changed & updateAccountStore from settings page.");
                fetchRelays();
            });
        });
    }

    fetchRelays();
});

onDestroy(() => {
    unlisten?.();
    toastState.cleanup();
});

async function handleLogin() {
    if (isValidNsec(nsecOrHex) || isValidHexKey(nsecOrHex)) {
        showLoginError = false;
        login(nsecOrHex)
            .then(() => {
                toastState.add("Logged in", "Successfully logged in", "success");
                nsecOrHex = "";
            })
            .catch((e) => {
                console.error(e);
                showLoginError = true;
                loginError = "Failed to log in";
            });
    } else {
        showLoginError = true;
        loginError = "Invalid nsec or private key";
    }
}

async function handleCreateAccount() {
    showLoginError = false;
    createAccount()
        .then(() => {
            toastState.add("Created new account", "Successfully created new account", "success");
        })
        .catch((e) => {
            toastState.add(
                "Error creating account",
                `Failed to create a new account: ${e.message}`,
                "error"
            );
            console.error(e);
        });
}

async function handleLogout(pubkey: string): Promise<void> {
    showLoginError = false;
    logout(pubkey)
        .then(() => {
            toastState.add("Logged out", "Successfully logged out", "success");
        })
        .catch((e) => {
            if (e instanceof LogoutError) {
                goto("/");
            } else {
                toastState.add("Logout Error", `Failed to log out: ${e.message}`, "error");
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
            toastState.add(
                "Key Package Published",
                "Key Package published successfully",
                "success"
            );
            showKeyPackageAlert = false;
        })
        .catch((e) => {
            toastState.add(
                "Error Publishing Key Package",
                `Failed to publish key package: ${e.toString()}`,
                "error"
            );
            console.error(e);
        });
}

function toggleProfileSection() {
    if (showProfileSection) {
        return;
    }
    showProfileSection = !showProfileSection;
    if (showProfileSection) {
        showPrivacySection = false;
        showDeveloperSection = false;
    }
}

function togglePrivacySection() {
    if (showPrivacySection) {
        return;
    }
    showPrivacySection = !showPrivacySection;
    if (showPrivacySection) {
        showProfileSection = false;
        showDeveloperSection = false;
    }
}

function toggleDeveloperSection() {
    if (showDeveloperSection) {
        return;
    }
    showDeveloperSection = !showDeveloperSection;
    if (showDeveloperSection) {
        showProfileSection = false;
        showPrivacySection = false;
    }
}
</script>

{#if showDeleteAlert}
    <Alert
        title="Delete everything?"
        body="This will delete all group and message data, and sign you out of all accounts. This will not delete your nostr keys or any other events you've published to relays. Are you sure you want to delete all data from White Noise? This cannot be undone."
        acceptFn={async () => {
            invoke("delete_all_data")
                .then(() => {
                    toastState.add("Data deleted", "All accounts, groups, and messages have been deleted.", "info");
                    showDeleteAlert = false;
                    goto("/login");
                })
                .catch((e) => {
                    toastState.add("Error deleting data", `Failed to delete data: ${e.toString()}`, "error");
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
                    toastState.add("Key Packages Deleted", "All key packages have been deleted.", "success");
                    showDeleteKeyPackagesAlert = false;
                })
                .catch((e) => {
                    toastState.add("Error Deleting Key Packages", `Failed to delete key packages: ${e.toString()}`, "error");
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
    <div onclick={toggleProfileSection} onkeydown={(e) => {
        if (e.key === "Enter") toggleProfileSection()
    }} tabindex="0" role="button" class="section-title-button">
        <h2 class="section-title">Profile</h2>
        {#if showProfileSection}
            <button class="text-muted-foreground-light dark:text-muted-foreground-dark" onclick={() => console.log("open new profile sheet")} aria-label="Add a profile">
                <AddLarge size={24} />
            </button>
        {/if}
    </div>
    {#if showProfileSection}
        <div class="overflow-hidden p-0 m-0" transition:slide={slideParams}>
            <div class="flex flex-row gap-3 items-center min-w-0 w-full mb-4">
                <button
                    onclick={() => setActiveAccount($activeAccount!.pubkey)}
                >
                    <Avatar
                        pubkey={$activeAccount!.pubkey}
                        picture={$activeAccount!.metadata?.picture}
                        pxSize={56}
                    />
                </button>
                <div class="flex flex-col gap-0 min-w-0 justify-start text-left truncate w-full">
                    <div class="truncate text-lg font-medium">
                        {nameFromMetadata($activeAccount!.metadata, $activeAccount!.pubkey)}
                    </div>
                    <div class="flex gap-4 items-center w-[90%]">
                        <FormattedNpub npub={npubFromPubkey($activeAccount!.pubkey)} showCopy={true} />
                    </div>
                </div>
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
    {/if}

    <div onclick={togglePrivacySection} onkeydown={(e) => {
        if (e.key === "Enter") togglePrivacySection()
    }} tabindex="0" role="button" class="section-title-button">
        <h2 class="section-title">Privacy & Security</h2>
    </div>

    {#if showPrivacySection}
        <div class="overflow-hidden p-0 m-0" transition:slide={slideParams}>
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
    {/if}

    <div onclick={toggleDeveloperSection} onkeydown={(e) => {
        if (e.key === "Enter") toggleDeveloperSection()
    }} tabindex="0" role="button" class="section-title-button">
        <h2 class="section-title">Developer Settings</h2>
    </div>

    {#if showDeveloperSection}
        <div class="overflow-hidden p-0 m-0" transition:slide={slideParams}>
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
                <button onclick={deleteAllKeyPackages} class="row-button">
                    <div class="row-button-content">
                        <TrashCan size={24} class="shrink-0"/>
                        <span>Delete all key packages</span>
                    </div>
                </button>
            </li>
            <li class="section-list-item">
                <button onclick={testNotification} class="row-button">
                    <div class="row-button-content">
                        <Notification size={24} class="shrink-0"/>
                        <span>Test notifications</span>
                    </div>
                </button>
                </li>
            </ul>
        </div>
    {/if}
</main>

<style lang="postcss">
    .section-title-button {
        @apply flex flex-row justify-between items-center mb-2 cursor-pointer;
    }

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
