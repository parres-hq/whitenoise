import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { type Writable, derived, get, writable } from "svelte/store";
import type { NMetadata } from "../types/nostr";

export type Account = {
    pubkey: string;
    metadata: NMetadata;
    nostr_relays: string[];
    inbox_relays: string[];
    key_package_relays: string[];
    mls_group_ids: Uint8Array[];
    settings: AccountSettings;
    onboarding: AccountOnboarding;
    last_used: number;
    active: boolean;
};

export type AccountSettings = {
    darkTheme: boolean;
    devMode: boolean;
    lockdownMode: boolean;
};

export type AccountOnboarding = {
    inbox_relays: boolean;
    key_package_relays: boolean;
    publish_key_package: boolean;
};

type RelaysData = Record<string, string>;

export const accounts: Writable<Account[]> = writable([]);
export const activeAccount: Writable<Account | null> = writable(null);
export const relays: Writable<RelaysData> = writable({} as RelaysData);

/** Basic matching patterns for hex and nsec keys */
export const hexPattern = /^[a-fA-F0-9]{64}$/;
export const nsecPattern = /^nsec1[a-zA-Z0-9]{58}$/;

/** Custom error class for login-related errors */
export class LoginError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "LoginError";
    }
}

/** Custom error class for logout-related errors */
export class LogoutError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "LogoutError";
    }
}

/** Custom error class for NWC-related errors */
export class NostrWalletConnectError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "NostrWalletConnectError";
    }
}

export async function setActiveAccount(pubkey: string): Promise<void> {
    if (
        !get(accounts)
            .map((account) => account.pubkey)
            .includes(pubkey) ||
        pubkey === get(activeAccount)?.pubkey
    )
        return;
    emit("account_changing", pubkey);
    return invoke("set_active_account", { hexPubkey: pubkey }).then(async (account) => {
        activeAccount.set(account as Account);
        await fetchRelays();
    });
}

export async function createAccount(): Promise<void> {
    return invoke("create_identity").then(async (account) => {
        activeAccount.set(account as Account);
        await fetchRelays();
    });
}

export async function logout(pubkey: string): Promise<void> {
    await invoke("logout", { hexPubkey: pubkey }).catch((e) => {
        if (e === "No account found") {
            throw new LogoutError("No account found");
        }
        throw new LogoutError(e);
    });
    await updateAccountsStore();
    await fetchRelays();
}

export async function login(nsecOrHex: string): Promise<void> {
    if (!nsecOrHex || (!hexPattern.test(nsecOrHex) && !nsecPattern.test(nsecOrHex))) {
        throw new LoginError("Invalid private key");
    }
    await invoke("login", { nsecOrHexPrivkey: nsecOrHex });
    await updateAccountsStore();
    await fetchRelays();
}

export async function updateAccountsStore(): Promise<void> {
    return invoke("get_accounts")
        .then((accountsResp) => {
            const sortedAccounts = (accountsResp as Account[]).sort((a, b) =>
                a.pubkey.localeCompare(b.pubkey)
            );
            accounts.set(sortedAccounts);
            const currentActiveAccount = sortedAccounts.find((account) => account.active) || null;
            activeAccount.set(currentActiveAccount);
        })
        .catch((_) => {
            accounts.set([]);
            activeAccount.set(null);
        });
}

export async function fetchRelays(): Promise<void> {
    const fetchedRelays: Record<string, string> = await invoke("fetch_relays");
    relays.set(fetchedRelays);
}

export function colorForRelayStatus(status: string): string {
    switch (status) {
        case "Pending":
        case "Initialized":
        case "Connecting":
            return "bg-yellow-500";
        case "Connected":
            return "bg-lime-500";
        case "Disconnected":
        case "Terminated":
            return "bg-red-500";
        default:
            return "";
    }
}

export async function hasNostrWalletConnectUri(): Promise<boolean> {
    try {
        return await invoke("has_nostr_wallet_connect_uri");
    } catch (error) {
        throw new NostrWalletConnectError(`Failed to check NWC URI: ${error}`);
    }
}

export async function getNostrWalletConnectBalance(): Promise<number> {
    try {
        const msats: number = await invoke("get_nostr_wallet_connect_balance");
        return msats / 1000;
    } catch (error) {
        throw new NostrWalletConnectError(`Failed to get NWC balance: ${error}`);
    }
}

export const hasLightningWallet = derived<Writable<Account | null>, boolean>(
    activeAccount,
    ($activeAccount, set) => {
        if (!$activeAccount) {
            set(false);
            return;
        }

        hasNostrWalletConnectUri()
            .then((result) => set(result))
            .catch((err) => {
                console.error("Error checking Lightning wallet status:", err);
                set(false);
            });
    },
    false
);

/** Validates a Nostr Wallet Connect URI and returns detailed error messages */
export function nostrWalletConnectUriError(uri: string): string | null {
    if (!validateNostrWalletConnectProtocol(uri)) {
        return "Invalid URI format: must start with 'nostr+walletconnect://'";
    }

    try {
        const url = new URL(uri);

        const relayError = relaysValidationError(url);
        if (relayError) return relayError;

        if (!url.searchParams.get("secret")) {
            return "Missing required 'secret' parameter";
        }

        return null;
    } catch {
        return "Missing required 'secret' parameter";
    }
}

function validateNostrWalletConnectProtocol(uri: string): boolean {
    return uri.startsWith("nostr+walletconnect://");
}

function relaysValidationError(url: URL): string | null {
    const relays = url.searchParams.getAll("relay");
    if (relays.length === 0) {
        return "Missing required 'relay' parameter";
    }

    for (const relay of relays) {
        const relayError = relayUrlValidationError(relay);
        if (relayError) {
            return relayError;
        }
    }
    return null;
}

function relayUrlValidationError(relay: string): string | null {
    try {
        const relayUrl = new URL(relay);
        if (relayUrl.protocol !== "wss:" && relayUrl.protocol !== "ws:") {
            return "Relay must use either WSS or WS protocol";
        }
        return null;
    } catch {
        return "Invalid relay URL format";
    }
}

export async function setNostrWalletConnectUri(uri: string): Promise<void> {
    const validationError = nostrWalletConnectUriError(uri);
    if (validationError) {
        throw new NostrWalletConnectError(validationError || "Invalid Nostr Wallet Connect URI");
    }

    try {
        await invoke("set_nostr_wallet_connect_uri", { nostrWalletConnectUri: uri });
    } catch (error) {
        throw new NostrWalletConnectError(`Failed to set NWC URI: ${error}`);
    }
}

export async function removeNostrWalletConnectUri(): Promise<void> {
    try {
        await invoke("remove_nostr_wallet_connect_uri");
    } catch (error) {
        throw new NostrWalletConnectError(`Failed to remove NWC URI: ${error}`);
    }
}
