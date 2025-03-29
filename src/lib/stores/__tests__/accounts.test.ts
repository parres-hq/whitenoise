import { invoke } from "@tauri-apps/api/core";
import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
    LoginError,
    LogoutError,
    accounts,
    activeAccount,
    colorForRelayStatus,
    createAccount,
    fetchRelays,
    hasNostrWalletConnectUri,
    hexPattern,
    login,
    logout,
    nsecPattern,
    relays,
    removeNostrWalletConnectUri,
    setActiveAccount,
    setNostrWalletConnectUri,
} from "../accounts";

// Mock Tauri API
const mockInvoke = vi.hoisted(() => vi.fn());
const mockEmit = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
    invoke: mockInvoke,
}));

vi.mock("@tauri-apps/api/event", () => ({
    emit: mockEmit,
}));

describe("accounts store", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        accounts.set([]);
        activeAccount.set(null);
        relays.set({});
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    describe("account management", () => {
        const mockAccount = {
            pubkey: "1234567890abcdef",
            metadata: { name: "Test User" },
            nostr_relays: ["wss://relay1.com"],
            inbox_relays: ["wss://inbox1.com"],
            key_package_relays: ["wss://key1.com"],
            mls_group_ids: [new Uint8Array()],
            settings: { darkTheme: false, devMode: false, lockdownMode: false },
            onboarding: { inbox_relays: true, key_package_relays: true, publish_key_package: true },
            last_used: Date.now(),
            active: true,
        };

        it("should set active account", async () => {
            mockInvoke.mockResolvedValue(mockAccount);
            accounts.set([mockAccount]);
            await setActiveAccount(mockAccount.pubkey);
            expect(mockEmit).toHaveBeenCalledWith("account_changing", mockAccount.pubkey);
            expect(get(activeAccount)).toEqual(mockAccount);
        });

        it("should create new account", async () => {
            mockInvoke.mockResolvedValue(mockAccount);
            await createAccount();
            expect(get(activeAccount)).toEqual(mockAccount);
        });

        it("should handle logout", async () => {
            mockInvoke.mockResolvedValue(undefined);
            await expect(logout(mockAccount.pubkey)).resolves.not.toThrow();
        });

        it("should throw LogoutError when account not found", async () => {
            mockInvoke.mockRejectedValue("No account found");
            await expect(logout(mockAccount.pubkey)).rejects.toThrow(LogoutError);
        });

        it("should handle login with valid hex key", async () => {
            const validHex = "1234567890abcdef".repeat(4);
            mockInvoke.mockResolvedValue(undefined);
            await expect(login(validHex)).resolves.not.toThrow();
        });

        it("should handle login with valid nsec key", async () => {
            const validNsec = `nsec1${"1234567890abcdef1234567890abcdef1234567890abcdef1234567890"}`;
            mockInvoke.mockResolvedValue(undefined);
            await expect(login(validNsec)).resolves.not.toThrow();
        });

        it("should throw LoginError with invalid key", async () => {
            await expect(login("invalid-key")).rejects.toThrow(LoginError);
        });
    });

    describe("relay management", () => {
        it("should fetch and update relays", async () => {
            const mockRelays = { "wss://relay1.com": "Connected" };
            mockInvoke.mockResolvedValue(mockRelays);
            await fetchRelays();
            expect(get(relays)).toEqual(mockRelays);
        });

        it("should return correct color for relay status", () => {
            expect(colorForRelayStatus("Connected")).toBe("text-green-500");
            expect(colorForRelayStatus("Connecting")).toBe("text-yellow-500");
            expect(colorForRelayStatus("Disconnected")).toBe("text-red-500");
            expect(colorForRelayStatus("Unknown")).toBe("");
        });
    });

    describe("Nostr Wallet Connect", () => {
        it("should check for NWC URI", async () => {
            mockInvoke.mockResolvedValue(true);
            await expect(hasNostrWalletConnectUri()).resolves.toBe(true);
        });

        it("should set NWC URI", async () => {
            const validUri = "nostr+walletconnect://relay1.com?secret=123&relay=wss://relay1.com";
            mockInvoke.mockResolvedValue(undefined);
            await expect(setNostrWalletConnectUri(validUri)).resolves.not.toThrow();
        });

        it("should remove NWC URI", async () => {
            mockInvoke.mockResolvedValue(undefined);
            await expect(removeNostrWalletConnectUri()).resolves.not.toThrow();
        });

        it("should fetch NWC balance", async () => {
            const mockBalance = 1000; // 1000 sats
            mockInvoke.mockResolvedValue(mockBalance);
            const balance = await invoke("get_nostr_wallet_connect_balance");
            expect(balance).toBe(mockBalance);
        });

        it("should throw NostrWalletConnectError when fetching balance fails", async () => {
            mockInvoke.mockRejectedValue("Failed to get balance");
            await expect(invoke("get_nostr_wallet_connect_balance")).rejects.toThrow(
                "Failed to get balance"
            );
        });
    });

    describe("validation patterns", () => {
        it("should validate hex pattern", () => {
            expect(hexPattern.test("1234567890abcdef".repeat(4))).toBe(true);
            expect(hexPattern.test("invalid")).toBe(false);
        });

        it("should validate nsec pattern", () => {
            const validNsec = `nsec1${"1234567890abcdef1234567890abcdef1234567890abcdef1234567890"}`;
            expect(nsecPattern.test(validNsec)).toBe(true);
            expect(nsecPattern.test("invalid")).toBe(false);
        });
    });
});
