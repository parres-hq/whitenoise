import type { NEvent } from "$lib/types/nostr";
import * as qrcode from "qrcode";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
    eventToLightningInvoice,
    eventToLightningPayment,
    lightningInvoiceToQRCode,
} from "../lightning";

// Mock QRCode
const mockToDataURL = vi.hoisted(() => vi.fn());
vi.mock("qrcode", () => ({
    toDataURL: mockToDataURL,
}));

// Mock console.error
const mockConsoleError = vi.hoisted(() => vi.fn());
vi.spyOn(console, "error").mockImplementation(mockConsoleError);

beforeEach(() => {
    mockToDataURL.mockClear();
    mockConsoleError.mockClear();
});

describe("eventToLightningInvoice", () => {
    describe("with valid bolt11 tag", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [
                ["p", "some-pubkey"],
                [
                    "bolt11",
                    "lnbc10m1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdpl2pkx2ctnv5sxxmmwwd5kgetjypeh2ursdae8g6twvus8g6rfwvs8qun0dfjkxaq8rkx3yf5tcsyz3d73gafnh3cax9rn449d9p5uxz9ezhhypd0elx87sjle52x86fux2ypatgddc6k63n7erqz25le42c4u4ecky03ylcqca784w",
                    "10000",
                    "Test invoice",
                ],
                ["other", "value"],
            ],
            content: "Test content",
            sig: "signature",
        };

        it("extracts the lightning invoice details", () => {
            const result = eventToLightningInvoice(event);
            expect(result).toEqual({
                invoice:
                    "lnbc10m1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdpl2pkx2ctnv5sxxmmwwd5kgetjypeh2ursdae8g6twvus8g6rfwvs8qun0dfjkxaq8rkx3yf5tcsyz3d73gafnh3cax9rn449d9p5uxz9ezhhypd0elx87sjle52x86fux2ypatgddc6k63n7erqz25le42c4u4ecky03ylcqca784w",
                amount: 10,
                description: "Test invoice",
                isPaid: false,
            });
        });
    });

    describe("without bolt11 tag", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [
                ["p", "some-pubkey"],
                ["other", "value"],
            ],
            content: "Test content",
            sig: "signature",
        };

        it("returns undefined", () => {
            expect(eventToLightningInvoice(event)).toBeUndefined();
        });
    });

    describe("with bolt11 tag but no value", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [["p", "some-pubkey"], ["bolt11"], ["other", "value"]],
            content: "Test content",
            sig: "signature",
        };

        it("returns undefined", () => {
            expect(eventToLightningInvoice(event)).toBeUndefined();
        });
    });

    describe("with amount and without description", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [
                ["p", "some-pubkey"],
                ["bolt11", "lnbc10m1invoice", "5000"],
                ["other", "value"],
            ],
            content: "Test content",
            sig: "signature",
        };

        it("extracts the lightning invoice with amount and no description", () => {
            const result = eventToLightningInvoice(event);
            expect(result).toEqual({
                invoice: "lnbc10m1invoice",
                amount: 5,
                description: undefined,
                isPaid: false,
            });
        });
    });

    describe("without amount but with description", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [
                ["p", "some-pubkey"],
                ["bolt11", "lnbc10m1invoice", "", "Just a description"],
                ["other", "value"],
            ],
            content: "Test content",
            sig: "signature",
        };

        it("extracts the lightning invoice with zero amount and description", () => {
            const result = eventToLightningInvoice(event);
            expect(result).toEqual({
                invoice: "lnbc10m1invoice",
                amount: 0,
                description: "Just a description",
                isPaid: false,
            });
        });
    });
});

describe("eventToLightningPayment", () => {
    describe("with preimage tag", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [
                ["p", "some-pubkey"],
                ["preimage", "my-preimage-hash"],
                ["other", "value"],
            ],
            content: "Test content",
            sig: "signature",
        };

        it("extracts the lightning payment details", () => {
            const result = eventToLightningPayment(event);
            expect(result).toEqual({
                preimage: "my-preimage-hash",
                isPaid: false,
            });
        });
    });

    describe("without preimage tag", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [
                ["p", "some-pubkey"],
                ["other", "value"],
            ],
            content: "Test content",
            sig: "signature",
        };

        it("returns undefined", () => {
            expect(eventToLightningPayment(event)).toBeUndefined();
        });
    });

    describe("with preimage tag but no value", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 1,
            tags: [["p", "some-pubkey"], ["preimage"], ["other", "value"]],
            content: "Test content",
            sig: "signature",
        };

        it("returns undefined", () => {
            expect(eventToLightningPayment(event)).toBeUndefined();
        });
    });
});

describe("lightningInvoiceToQRCode", () => {
    it("converts a lightning invoice to a QR code data URL", async () => {
        const mockDataUrl = "data:image/png;base64,test";
        mockToDataURL.mockResolvedValue(mockDataUrl);

        const invoice = "lnbc10m1invoice";
        const result = await lightningInvoiceToQRCode(invoice);

        // Check if toDataURL was called with the correct lightning: prefix
        expect(qrcode.toDataURL).toHaveBeenCalledWith(`lightning:${invoice}`);
        expect(result).toBe(mockDataUrl);
    });

    it("handles errors by returning an empty string", async () => {
        // Make the mock reject once
        mockToDataURL.mockRejectedValueOnce(new Error("QR code generation failed"));

        const invoice = "invalid-invoice";
        const result = await lightningInvoiceToQRCode(invoice);

        expect(result).toBe("");
        expect(mockConsoleError).toHaveBeenCalledWith(
            "Error generating QR code:",
            expect.any(Error)
        );
    });
});
