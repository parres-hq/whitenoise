import type { NEvent } from "$lib/types/nostr";
import {
    hexKeyFromNpub,
    isInsecure,
    isValidHexKey,
    isValidNpub,
    isValidNsec,
    isValidWebSocketURL,
    latestMessagePreview,
    nameFromMetadata,
    npubFromPubkey,
    truncatedNpub,
} from "$lib/utils/nostr";
import { invoke } from "@tauri-apps/api/core";
import { decode as nip19Decode, npubEncode } from "nostr-tools/nip19";
import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock dependencies
vi.mock("@tauri-apps/api/core", () => ({
    invoke: vi.fn(),
}));

vi.mock("nostr-tools/nip19", () => ({
    decode: vi.fn(),
    npubEncode: vi.fn(),
}));

vi.mock("svelte/store", () => ({
    get: vi.fn(),
}));

vi.mock("$lib/stores/accounts", () => ({
    activeAccount: {},
}));

describe("Nostr Utils", () => {
    beforeEach(() => {
        vi.resetAllMocks();
    });

    describe("nameFromMetadata", () => {
        it("returns display_name when available", () => {
            const metadata = {
                display_name: "Display Name",
                name: "Name",
            };
            expect(nameFromMetadata(metadata)).toBe("Display Name");
        });

        it("returns name when display_name is not available", () => {
            const metadata = {
                name: "Name",
            };
            expect(nameFromMetadata(metadata)).toBe("Name");
        });

        it("returns truncated npub when neither display_name nor name is available", () => {
            const metadata = {};
            const pubkey = "testpubkey";
            vi.mocked(npubEncode).mockReturnValue(
                "npub1zuuajd7u3sx8xu92yav9jwxpr839cs0kc3q6t56vd5u9q033xmhsk6c2uc"
            );
            expect(nameFromMetadata(metadata, pubkey)).toBe("npub1zuuajd7u3sx8xu9...");
        });

        it("returns full npub when truncate is false", () => {
            const metadata = {};
            const pubkey = "testpubkey";
            vi.mocked(npubEncode).mockReturnValue(
                "npub1zuuajd7u3sx8xu92yav9jwxpr839cs0kc3q6t56vd5u9q033xmhsk6c2uc"
            );
            expect(nameFromMetadata(metadata, pubkey, false)).toBe(
                "npub1zuuajd7u3sx8xu92yav9jwxpr839cs0kc3q6t56vd5u9q033xmhsk6c2uc"
            );
        });

        it("returns empty string when no data is available", () => {
            const metadata = {};
            expect(nameFromMetadata(metadata)).toBe("");
        });

        it("trims whitespace from the name", () => {
            const metadata = {
                name: "  Name with spaces  ",
            };
            expect(nameFromMetadata(metadata)).toBe("Name with spaces");
        });
    });

    describe("npubFromPubkey", () => {
        it("converts pubkey to npub format", () => {
            const pubkey = "testpubkey";
            vi.mocked(npubEncode).mockReturnValue("npub1testpubkeylong");
            expect(npubFromPubkey(pubkey)).toBe("npub1testpubkeylong");
            expect(npubEncode).toHaveBeenCalledWith(pubkey);
        });
    });

    describe("truncatedNpub", () => {
        it("truncates npub to default length", () => {
            const pubkey = "testpubkey";
            vi.mocked(npubEncode).mockReturnValue("npub1testpubkeylong");
            expect(truncatedNpub(pubkey)).toBe("npub1testpubkeylong...");
        });

        it("truncates npub to specified length", () => {
            const pubkey = "testpubkey";
            vi.mocked(npubEncode).mockReturnValue(
                "npub1zuuajd7u3sx8xu92yav9jwxpr839cs0kc3q6t56vd5u9q033xmhsk6c2uc"
            );
            expect(truncatedNpub(pubkey, 10)).toBe("npub1zuuaj...");
        });
    });

    describe("isInsecure", () => {
        const testEvent: NEvent = {
            id: "testid",
            kind: 4,
            content: "test",
            pubkey: "testpubkey",
            created_at: 123,
            tags: [],
        };

        it("returns true for kind 4 events", () => {
            expect(isInsecure(testEvent)).toBe(true);
        });

        it("returns true for kind 14 events", () => {
            testEvent.kind = 14;
            expect(isInsecure(testEvent)).toBe(true);
        });

        it("returns false for other kinds of events", () => {
            testEvent.kind = 1;
            expect(isInsecure(testEvent)).toBe(false);
        });
    });

    describe("isValidWebSocketURL", () => {
        it("returns true for valid ws:// URL", () => {
            expect(isValidWebSocketURL("ws://example.com")).toBe(true);
        });

        it("returns true for valid wss:// URL", () => {
            expect(isValidWebSocketURL("wss://secure.example.com")).toBe(true);
        });

        it("returns false for http:// URL", () => {
            expect(isValidWebSocketURL("http://example.com")).toBe(false);
        });

        it("returns false for invalid URL format", () => {
            expect(isValidWebSocketURL("not a url")).toBe(false);
        });

        it("returns false for empty string", () => {
            expect(isValidWebSocketURL("")).toBe(false);
        });
    });

    describe("latestMessagePreview", () => {
        it('returns "New chat" when messageId is undefined', async () => {
            const result = await latestMessagePreview(undefined);
            expect(result).toBe("New chat");
        });

        it("returns empty string when no event is found", async () => {
            vi.mocked(invoke).mockResolvedValue(null);
            const result = await latestMessagePreview(123);
            expect(result).toBe("");
            expect(invoke).toHaveBeenCalledWith("query_message", { messageId: 123 });
        });

        it('returns "You: [content]" when event is from active account', async () => {
            const activeAccountMock = { pubkey: "userpubkey" };
            vi.mocked(get).mockReturnValue(activeAccountMock);
            vi.mocked(invoke).mockResolvedValueOnce({
                pubkey: "userpubkey",
                content: "Hello there",
            });

            const result = await latestMessagePreview(123);
            expect(result).toBe("You: Hello there");
        });

        it('returns "[name]: [content]" when event is from another user', async () => {
            vi.mocked(get).mockReturnValue({ pubkey: "userpubkey" });
            vi.mocked(invoke).mockResolvedValueOnce({
                pubkey: "otherpubkey",
                content: "Hi",
            });
            vi.mocked(invoke).mockResolvedValueOnce({
                metadata: { display_name: "Other User" },
            });
            vi.mocked(npubEncode).mockReturnValue("npub1otherpubkey");

            const result = await latestMessagePreview(123);
            expect(result).toBe("Other User: Hi");
            expect(invoke).toHaveBeenCalledWith("query_enriched_contact", {
                pubkey: "otherpubkey",
                updateAccount: false,
            });
        });
    });

    describe("isValidNpub", () => {
        it("returns true for valid npub", () => {
            expect(
                isValidNpub("npub17hm2l6z8hzsg38gdrn0my2xujs0x3eu6g6mnn5qmcxzjdus4n5nqw52esc")
            ).toBe(true);
        });

        it("returns false when not npub type", () => {
            expect(
                isValidNpub(
                    "nevent1qqsrpweecze62nn60py8p37n9f9zzqdesj0a2ah50xx0s9ra7slv4tqzypumuen7l8wthtz45p3ftn58pvrs9xlumvkuu2xet8egzkcklqtesqcyqqqqqqg5n8298"
                )
            ).toBe(false);
        });
    });

    describe("isValidHexKey", () => {
        it("returns true for valid 64-character hex string", () => {
            expect(
                isValidHexKey("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
            ).toBe(true);
        });

        it("returns true for valid 64-character hex string with uppercase", () => {
            expect(
                isValidHexKey("0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF")
            ).toBe(true);
        });

        it("returns false for string shorter than 64 characters", () => {
            expect(isValidHexKey("0123456789abcdef")).toBe(false);
        });

        it("returns false for string longer than 64 characters", () => {
            expect(
                isValidHexKey("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef00")
            ).toBe(false);
        });

        it("returns false for non-hex characters", () => {
            expect(
                isValidHexKey("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdez")
            ).toBe(false);
        });
    });

    describe("isValidNsec", () => {
        it("returns true for valid nsec", () => {
            expect(
                isValidNsec("nsec1en8rk0syat0nfr4kahhp6y784wv3rtct24gf3m75c25245v6fl9sdwdgqz")
            ).toBe(true);
        });

        it("returns false for invalid nsec", () => {
            expect(isValidNsec("invalid")).toBe(false);
        });

        it("returns false for non-nsec type", () => {
            expect(
                isValidNsec("npub1zuuajd7u3sx8xu92yav9jwxpr839cs0kc3q6t56vd5u9q033xmhsk6c2uc")
            ).toBe(false);
            expect(
                isValidNsec(
                    "nevent1qqs9hx6qnsg68jn5qgjeta2xk5l6se999yypvch7k3ll6xcd5xynz4gzypumuen7l8wthtz45p3ftn58pvrs9xlumvkuu2xet8egzkcklqtesqcyqqqqqqgwak2ut"
                )
            ).toBe(false);
        });
    });

    // TODO: This is broken cause nostr-tools sucks
    // describe("hexKeyFromNpub", () => {
    //     it("returns hex key from valid npub", () => {
    //         expect(
    //             hexKeyFromNpub("npub17hm2l6z8hzsg38gdrn0my2xujs0x3eu6g6mnn5qmcxzjdus4n5nqw52esc")
    //         ).toBe("f5f6afe847b8a0889d0d1cdfb228dc941e68e79a46b739d01bc18526f2159d26");
    //     });

    //     it("throws error for non-npub type", () => {
    //         expect(() =>
    //             hexKeyFromNpub(
    //                 "nevent1qqs9hx6qnsg68jn5qgjeta2xk5l6se999yypvch7k3ll6xcd5xynz4gzypumuen7l8wthtz45p3ftn58pvrs9xlumvkuu2xet8egzkcklqtesqcyqqqqqqgwak2ut"
    //             )
    //         ).toThrow("Invalid npub");
    //     });
    // });
});
