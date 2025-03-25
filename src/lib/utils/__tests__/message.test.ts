import type { CachedMessage } from "$lib/types/chat";
import type { NEvent } from "$lib/types/nostr";
import { describe, expect, it } from "vitest";
import { cachedMessageToMessage, eventToMessage } from "../message";

describe("eventToMessage", () => {
    const defaultEvent: NEvent = {
        id: "event123",
        pubkey: "pubkey456",
        created_at: 1622548800,
        kind: 1,
        tags: [],
        content: "Hello world",
        sig: "signature",
    };

    it("converts event to message", () => {
        const result = eventToMessage(defaultEvent, "some-other-pubkey");
        expect(result).toEqual({
            id: "event123",
            pubkey: "pubkey456",
            content: "Hello world",
            createdAt: 1622548800,
            replyToId: undefined,
            reactions: [],
            lightningInvoice: undefined,
            lightningPayment: undefined,
            isSingleEmoji: false,
            isMine: false,
            event: defaultEvent,
            tokens: [],
        });
    });

    describe("with emojis", () => {
        it("returns isSingleEmoji true for a single basic emoji", () => {
            const event = { ...defaultEvent, content: "😊" };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for a compound emoji", () => {
            const event = { ...defaultEvent, content: "👨‍👩‍👧‍👦" };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for an emoji with skin tone modifier", () => {
            const event = { ...defaultEvent, content: "👍🏽" };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for emoji with whitespace", () => {
            const event = { ...defaultEvent, content: " 🎉 " };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji false for text with emoji", () => {
            const event = { ...defaultEvent, content: "Hello 👋" };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.isSingleEmoji).toEqual(false);
        });

        it("returns isSingleEmoji false for multiple emojis", () => {
            const event = { ...defaultEvent, content: "😊😎" };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.isSingleEmoji).toEqual(false);
        });
    });

    describe("with same pubkey", () => {
        it("returns isMine true", () => {
            const event = { ...defaultEvent, pubkey: "pubkey456" };
            const message = eventToMessage(event, "pubkey456");
            expect(message.isMine).toEqual(true);
        });
    });

    describe("with reply q tag", () => {
        it("returns replyToId from q tag", () => {
            const event = { ...defaultEvent, tags: [["q", "original-event-id"]] };
            const message = eventToMessage(event, "some-pubkey");
            expect(message.replyToId).toEqual("original-event-id");
        });
    });

    describe("with bolt11 tag", () => {
        const invoice =
            "lntbs330n1pnuu4msdqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5hhsamdzupvqvygycgk5a37fx94m5qctzhz37sf0tensuje9phxgssp50qpmchgkh5z94gffsq3u9sgyr4l778wzj7x4g2wvwtyghdxmt23s9qyysgqcqpcxqyz5vqfuj3u2u2lcs7wdu6k8jh2vur9l3zmffwfup2k8ea7fgeg2puc6xs9cssqcl0xhzngg8z5ye62h3vcgfve56zd9rum2sygndh66qdehgqm4ajkej";
        const event: NEvent = {
            id: "event123",
            pubkey: "pubkey456",
            created_at: 1622548800,
            kind: 9734,
            tags: [["bolt11", invoice, "1000000", "Invoice description"]],
            content: `Please pay this invoice: ${invoice}`,
            sig: "signature",
        };

        it("sets lightning invoice", () => {
            const message = eventToMessage(event, "some-pubkey");
            expect(message.lightningInvoice).toMatchObject({
                invoice:
                    "lntbs330n1pnuu4msdqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5hhsamdzupvqvygycgk5a37fx94m5qctzhz37sf0tensuje9phxgssp50qpmchgkh5z94gffsq3u9sgyr4l778wzj7x4g2wvwtyghdxmt23s9qyysgqcqpcxqyz5vqfuj3u2u2lcs7wdu6k8jh2vur9l3zmffwfup2k8ea7fgeg2puc6xs9cssqcl0xhzngg8z5ye62h3vcgfve56zd9rum2sygndh66qdehgqm4ajkej",
                amount: 1000,
                description: "Invoice description",
                isPaid: false,
            });
        });

        it("shortens lightning invoice in content", () => {
            const message = eventToMessage(event, "some-pubkey");
            expect(message.content).toEqual(
                "Please pay this invoice: lntbs330n1pnuu4...66qdehgqm4ajkej"
            );
        });
    });

    describe("with preimage tag", () => {
        const event: NEvent = {
            id: "event123",
            pubkey: "pubkey456",
            created_at: 1622548800,
            kind: 9,
            tags: [["preimage", "preimage123"]],
            content: "Payment sent",
            sig: "signature",
        };
        it("saves lightning payment", () => {
            const message = eventToMessage(event, "some-pubkey");

            expect(message.lightningPayment).toEqual({
                preimage: "preimage123",
                isPaid: false,
            });
        });
    });
});

describe("cachedMessageToMessage", () => {
    const defaultEvent: NEvent = {
        id: "event123",
        pubkey: "pubkey456",
        created_at: 1622548800,
        kind: 9,
        tags: [],
        content: "Hello world",
        sig: "signature",
    };

    it("converts cached message to message", () => {
        const cachedMessage: CachedMessage = {
            event: defaultEvent,
            event_id: defaultEvent.id,
            account_pubkey: defaultEvent.pubkey,
            author_pubkey: defaultEvent.pubkey,
            mls_group_id: "mls_group_id",
            created_at: defaultEvent.created_at,
            event_kind: defaultEvent.kind,
            content: defaultEvent.content,
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "Hello world" }],
        };
        const message = cachedMessageToMessage(cachedMessage, "some-pubkey");

        expect(message).toEqual({
            id: "event123",
            pubkey: "pubkey456",
            content: "Hello world",
            createdAt: 1622548800,
            replyToId: undefined,
            reactions: [],
            lightningInvoice: undefined,
            lightningPayment: undefined,
            isSingleEmoji: false,
            isMine: false,
            event: defaultEvent,
            tokens: [{ Text: "Hello world" }],
        });
    });
});
