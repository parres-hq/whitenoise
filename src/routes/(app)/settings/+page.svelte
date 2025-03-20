<script lang="ts">
import { goto } from "$app/navigation";
import Alert from "$lib/components/Alert.svelte";
import Avatar from "$lib/components/Avatar.svelte";
import Header from "$lib/components/Header.svelte";
import HeaderToolbar from "$lib/components/HeaderToolbar.svelte";
import {
    type Account,
    LogoutError,
    accounts,
    activeAccount,
    createAccount,
    fetchRelays,
    login,
    logout,
    setActiveAccount,
    updateAccountsStore,
} from "$lib/stores/accounts";
import { getToastState } from "$lib/stores/toast-state.svelte";
import { isValidHexPubkey, isValidNsec } from "$lib/types/nostr";
import { copyToClipboard } from "$lib/utils/clipboard";
import { nameFromMetadata, npubFromPubkey } from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
    isPermissionGranted,
    requestPermission,
    sendNotification,
} from "@tauri-apps/plugin-notification";
import {
    Bell,
    CaretRight,
    CopySimple,
    HardDrives,
    Key,
    Lightning,
    PlusCircle,
    SignIn,
    Skull,
    Trash,
    UserPlus,
} from "phosphor-svelte";
import { onDestroy, onMount } from "svelte";

let showDeleteAlert = $state(false);
let showKeyPackageAlert = $state(false);
let showDeleteKeyPackagesAlert = $state(false);
let showLogin = $state(false);
let nsecOrHex = $state("");
let showLoginError = $state(false);
let loginError = $state("");

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
    if (isValidNsec(nsecOrHex) || isValidHexPubkey(nsecOrHex)) {
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

let showAccountsState = $state(false);
let accountsState = $state("");
async function toggleInspectAccounts() {
    showAccountsState = !showAccountsState;
    if (showAccountsState) {
        invoke("get_accounts_state").then((accounts) => {
            accountsState = JSON.stringify(accounts, null, 2);
        });
    }
}

let showGroupsState = $state(false);
let groupsState = $state("");
async function toggleInspectGroups() {
    showGroupsState = !showGroupsState;
    if (showGroupsState) {
        invoke("get_groups").then((groups) => {
            groupsState = JSON.stringify(groups, null, 2);
        });
    }
}

let showInvitesState = $state(false);
let invitesState = $state("");
async function toggleInspectInvites() {
    showInvitesState = !showInvitesState;
    if (showInvitesState) {
        invoke("get_invites").then((invites) => {
            invitesState = JSON.stringify(invites, null, 2);
        });
    }
}

async function copyNsec(account: Account) {
    try {
        const nsec = await invoke("export_nsec", { pubkey: account.pubkey });
        if (await copyToClipboard(nsec as string, "nsec")) {
            highlightButton(`[id="nsec-copy-${account.pubkey}"]`);
        } else {
            toastCopyErrorMessage("nsec");
        }
    } catch (e) {
        console.error(e);
        toastState.add(
            "Error exporting nsec",
            "There was an error exporting your nsec, please try again.",
            "error"
        );
    }
}

async function copyNpub(account: Account) {
    const npub = npubFromPubkey(account.pubkey);
    if (await copyToClipboard(npub, "npub")) {
        highlightButton(`[id="npub-copy-${account.pubkey}"]`);
    } else {
        toastCopyErrorMessage("npub");
    }
}

function highlightButton(selector: string) {
    const button = document.querySelector(selector);
    if (!button) return;

    button.classList.add("text-green-500");
    setTimeout(() => {
        button.classList.remove("text-green-500");
    }, 1000);
}

function toastCopyErrorMessage(errorMessage: string) {
    toastState.add(
        `Error copying ${errorMessage}`,
        `There was an error copying your ${errorMessage}, please try again.`,
        "error"
    );
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

<HeaderToolbar>
    {#snippet center()}
        <h1>Settings</h1>
    {/snippet}
</HeaderToolbar>

<Header title="Settings" />
<main class="px-4 flex flex-col pb-32">
    <h2 class="section-title">Accounts</h2>
    <div class="section w-full">
        {#each $accounts as account (account.pubkey)}
            <div class="flex flex-row gap-4 items-center border-b border-gray-700 py-3 min-w-0 w-full">
                <div class="flex flex-row items-center flex-1 min-w-0">
                    <button
                      onclick={() => setActiveAccount(account.pubkey)}
                    >
                      <Avatar
                          pubkey={account.pubkey}
                          picture={account.metadata?.picture}
                          pxSize={40}
                          showRing={$activeAccount?.pubkey === account.pubkey}
                      />
                    </button>
                    <div class="flex flex-col gap-1 min-w-0 justify-start text-left pl-4 truncate">
                        <div class="truncate">
                            {nameFromMetadata(account.metadata, account.pubkey)}
                        </div>
                        <div class="flex gap-2 items-center">
                          <p class="font-mono truncate">
                            {npubFromPubkey(account.pubkey)}
                          </p>
                          <button
                            class="transition-colors duration-200"
                            id={`npub-copy-${account.pubkey}`}
                            onclick={() => copyNpub(account)}
                          >
                            <CopySimple size={24} />
                          </button>
                        </div>
                    </div>
                </div>
                <button
                  class="export-nsec-butto min-w-fit text-sm button-outline shrink-0 transition-colors duration-200"
                  id={`nsec-copy-${account.pubkey}`}
                  onclick={() => copyNsec(account)}
                >
                  Copy nsec
                </button>
                <button class="min-w-fit text-sm button-outline shrink-0" onclick={() => handleLogout(account.pubkey)}>
                    Log out
                </button>
            </div>
        {/each}
        <div class="section-list-item !mt-6">
            <button onclick={() => (showLogin = !showLogin)} class="button-primary">
                <UserPlus size={24} />
                Add another account
            </button>
        </div>
        <div class="{showLogin ? 'flex' : 'hidden'} flex-col gap-8 items-start w-full mt-4 py-4">
            <div class="flex flex-col gap-4 items-start w-full">
                <label for="nsec" class="flex flex-col gap-2 text-lg items-start font-medium w-full">
                    Add another account?
                    <input
                        type="password"
                        id="nsec"
                        bind:value={nsecOrHex}
                        placeholder="nsec1&hellip;"
                        autocapitalize="off"
                        autocorrect="off"
                        class="w-full px-3 py-2 bg-transparent ring-1 ring-gray-700 rounded-md"
                    />
                </label>
                {#if showLoginError}
                    <span class="text-red-500 text-sm">
                        {loginError}
                    </span>
                {/if}
                <button type="submit" onclick={handleLogin} class="button-primary w-full !justify-start">
                    <SignIn size="20" />
                    Log In
                </button>
            </div>
            <button onclick={handleCreateAccount} class="button-outline w-full !justify-start">
                <PlusCircle size="20" />
                Create New Nostr Identity
            </button>
        </div>
    </div>
    <h2 class="section-title">Privacy & Security</h2>
    <div class="section">
        <ul class="section-list">
            <!-- <li class="section-list-item">
                <button onclick={() => goto("/settings/lockdown/")} class="row-button">
                    <Lock size={24} />
                    <span>Lockdown Mode</span>
                    <CaretRight size={24} class="ml-auto mr-0" />
                </button>
            </li> -->
            <li class="section-list-item">
                <button onclick={deleteAll} class="row-button">
                    <Skull size={24} />
                    <span>Delete all data</span>
                </button>
            </li>
        </ul>
    </div>

    <h2 class="section-title">Developer Settings</h2>
    <div class="section">
        <ul class="section-list">
            <li class="section-list-item">
                <button onclick={() => goto("/settings/network/")} class="row-button">
                    <HardDrives size={24} class="shrink-0" />
                    <span>Network</span>
                    <CaretRight size={24} class="ml-auto mr-0 shrink-0" />
                </button>
            </li>
            <li class="section-list-item">
                <button onclick={() => goto("/settings/lightning/")} class="row-button">
                    <Lightning size={24} class="shrink-0" />
                    <span>Lightning</span>
                    <CaretRight size={24} class="ml-auto mr-0 shrink-0" />
                </button>
            </li>
            <li class="section-list-item">
                <button onclick={launchKeyPackage} class="row-button">
                    <Key size={24} />
                    <span>Publish a Key Package</span>
                </button>
            </li>
            <li class="section-list-item">
                <button onclick={deleteAllKeyPackages} class="row-button">
                    <Trash size={24} class="shrink-0" />
                    <span class="truncate">Send delete requests for all key packages</span>
                </button>
            </li>

            <li class="section-list-item">
                <button onclick={testNotification} class="row-button">
                    <Bell size={24} class="shrink-0" />
                    <span class="truncate">Test Notification</span>
                </button>
            </li>

            <!-- <li class="section-list-item">
                <button onclick={toggleInspectAccounts} class="row-button">
                    <UserFocus size={24} />
                    <span>Inspect Accounts</span>
                    {#if showAccountsState}
                        <CaretDown size={24} class="ml-auto mr-0" />
                    {:else}
                        <CaretRight size={24} class="ml-auto mr-0" />
                    {/if}
                </button>
                {#if showAccountsState}
                    <div class="flex flex-col gap-4 items-start w-full mt-4 p-4">
                        <pre class="whitespace-pre overflow-x-auto w-full">{accountsState}</pre>
                    </div>
                {/if}
            </li>
            <li class="section-list-item">
                <button onclick={toggleInspectInvites} class="row-button">
                    <Envelope size={24} />
                    <span>Inspect Invites</span>
                    {#if showInvitesState}
                        <CaretDown size={24} class="ml-auto mr-0" />
                    {:else}
                        <CaretRight size={24} class="ml-auto mr-0" />
                    {/if}
                </button>
                {#if showInvitesState}
                    <div class="flex flex-col gap-4 items-start w-full mt-4 p-4">
                        <pre class="whitespace-pre overflow-x-auto w-full">{invitesState}</pre>
                    </div>
                {/if}
            </li>
            <li class="section-list-item">
                <button onclick={toggleInspectGroups} class="row-button">
                    <Users size={24} />
                    <span>Inspect Groups</span>
                    {#if showGroupsState}
                        <CaretDown size={24} class="ml-auto mr-0" />
                    {:else}
                        <CaretRight size={24} class="ml-auto mr-0" />
                    {/if}
                </button>
                {#if showGroupsState}
                    <div class="flex flex-col gap-4 items-start w-full mt-4 p-4">
                        <pre class="whitespace-pre overflow-x-auto w-full">{groupsState}</pre>
                    </div>
                {/if}
            </li> -->
        </ul>
    </div>
</main>
