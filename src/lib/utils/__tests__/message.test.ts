import type { Message } from "$lib/types/chat";
import type { NEvent } from "$lib/types/nostr";
import { describe, expect, it } from "vitest";
import { messageToChatMessage } from "../message";

describe("messageToChatMessage", () => {
    const defaultEvent: NEvent = {
        id: "event123",
        pubkey: "pubkey456",
        created_at: 1622548800,
        kind: 9,
        tags: [],
        content: "Hello world",
        sig: "signature",
    };
    it("converts message to chat message", () => {
        const message: Message = {
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
        const chatMessage = messageToChatMessage(message, "some-pubkey");

        expect(chatMessage).toEqual({
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

    describe("with emojis", () => {
        it("returns isSingleEmoji true for a single basic emoji", () => {
            const event = { ...defaultEvent, content: "😊" };
            const message: Message = {
                event,
                event_id: event.id,
                account_pubkey: event.pubkey,
                author_pubkey: event.pubkey,
                mls_group_id: "mls_group_id",
                created_at: event.created_at,
                event_kind: event.kind,
                content: event.content,
                outer_event_id: "outer_event_id",
                tokens: [{ Text: "😊" }],
            };
            const chatMessage = messageToChatMessage(message, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for a compound emoji", () => {
            const event = { ...defaultEvent, content: "👨‍👩‍👧‍👦" };
            const message: Message = {
                event,
                event_id: event.id,
                account_pubkey: event.pubkey,
                author_pubkey: event.pubkey,
                mls_group_id: "mls_group_id",
                created_at: event.created_at,
                event_kind: event.kind,
                content: event.content,
                outer_event_id: "outer_event_id",
                tokens: [{ Text: "👨‍👩‍👧‍👦" }],
            };
            const chatMessage = messageToChatMessage(message, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for an emoji with skin tone modifier", () => {
            const event = { ...defaultEvent, content: "👍🏽" };
            const message: Message = {
                event,
                event_id: event.id,
                account_pubkey: event.pubkey,
                author_pubkey: event.pubkey,
                mls_group_id: "mls_group_id",
                created_at: event.created_at,
                event_kind: event.kind,
                content: event.content,
                outer_event_id: "outer_event_id",
                tokens: [{ Text: "👍🏽" }],
            };
            const chatMessage = messageToChatMessage(message, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for emoji with whitespace", () => {
            const event = { ...defaultEvent, content: " 🎉 " };
            const message: Message = {
                event,
                event_id: event.id,
                account_pubkey: event.pubkey,
                author_pubkey: event.pubkey,
                mls_group_id: "mls_group_id",
                created_at: event.created_at,
                event_kind: event.kind,
                content: event.content,
                outer_event_id: "outer_event_id",
                tokens: [{ Text: " 🎉 " }],
            };
            const chatMessage = messageToChatMessage(message, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji false for text with emoji", () => {
            const event = { ...defaultEvent, content: "Hello 👋" };
            const message: Message = {
                event,
                event_id: event.id,
                account_pubkey: event.pubkey,
                author_pubkey: event.pubkey,
                mls_group_id: "mls_group_id",
                created_at: event.created_at,
                event_kind: event.kind,
                content: event.content,
                outer_event_id: "outer_event_id",
                tokens: [{ Text: "Hello 👋" }],
            };
            const chatMessage = messageToChatMessage(message, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(false);
        });

        it("returns isSingleEmoji false for multiple emojis", () => {
            const event = { ...defaultEvent, content: "😊😎" };
            const message: Message = {
                event,
                event_id: event.id,
                account_pubkey: event.pubkey,
                author_pubkey: event.pubkey,
                mls_group_id: "mls_group_id",
                created_at: event.created_at,
                event_kind: event.kind,
                content: event.content,
                outer_event_id: "outer_event_id",
                tokens: [{ Text: "😊😎" }],
            };
            const chatMessage = messageToChatMessage(message, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(false);
        });
    });
});
