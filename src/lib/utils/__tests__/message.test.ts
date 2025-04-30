import type { Message } from "$lib/types/chat";
import type { NEvent, NMessageState } from "$lib/types/nostr";
import { NMessageState as NMessageStateEnum } from "$lib/types/nostr";
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
    const testMlsGroupId = { value: { vec: new Uint8Array([1, 2, 3, 4]) } };
    it("converts message to chat message", () => {
        const tokens = [{ Text: "Hello world" }];
        const message: Message = {
            event: defaultEvent,
            event_id: defaultEvent.id,
            mls_group_id: testMlsGroupId,
            created_at: defaultEvent.created_at,
            content: defaultEvent.content,
            pubkey: defaultEvent.pubkey,
            kind: defaultEvent.kind,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };
        const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
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
            tokens,
        });
    });

    describe("with emojis", () => {
        it("returns isSingleEmoji true for a single basic emoji", () => {
            const event = { ...defaultEvent, content: "😊" };
            const tokens = [{ Text: "😊" }];
            const message: Message = {
                event,
                event_id: event.id,
                mls_group_id: testMlsGroupId,
                created_at: event.created_at,
                content: event.content,
                pubkey: event.pubkey,
                kind: event.kind,
                tags: [],
                wrapper_event_id: "test-wrapper-id",
                state: NMessageStateEnum.Created,
            };
            const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for a compound emoji", () => {
            const event = { ...defaultEvent, content: "👨‍👩‍👧‍👦" };
            const tokens = [{ Text: "👨‍👩‍👧‍👦" }];
            const message: Message = {
                event,
                event_id: event.id,
                mls_group_id: testMlsGroupId,
                created_at: event.created_at,
                content: event.content,
                pubkey: event.pubkey,
                kind: event.kind,
                tags: [],
                wrapper_event_id: "test-wrapper-id",
                state: NMessageStateEnum.Created,
            };
            const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for an emoji with skin tone modifier", () => {
            const event = { ...defaultEvent, content: "👍🏽" };
            const tokens = [{ Text: "👍🏽" }];
            const message: Message = {
                event,
                event_id: event.id,
                mls_group_id: testMlsGroupId,
                created_at: event.created_at,
                content: event.content,
                pubkey: event.pubkey,
                kind: event.kind,
                tags: [],
                wrapper_event_id: "test-wrapper-id",
                state: NMessageStateEnum.Created,
            };
            const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji true for emoji with whitespace", () => {
            const event = { ...defaultEvent, content: " 🎉 " };
            const tokens = [{ Text: " 🎉 " }];
            const message: Message = {
                event,
                event_id: event.id,
                mls_group_id: testMlsGroupId,
                created_at: event.created_at,
                content: event.content,
                pubkey: event.pubkey,
                kind: event.kind,
                tags: [],
                wrapper_event_id: "test-wrapper-id",
                state: NMessageStateEnum.Created,
            };
            const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(true);
        });

        it("returns isSingleEmoji false for text with emoji", () => {
            const event = { ...defaultEvent, content: "Hello 👋" };
            const tokens = [{ Text: "Hello 👋" }];
            const message: Message = {
                event,
                event_id: event.id,
                mls_group_id: testMlsGroupId,
                created_at: event.created_at,
                content: event.content,
                pubkey: event.pubkey,
                kind: event.kind,
                tags: [],
                wrapper_event_id: "test-wrapper-id",
                state: NMessageStateEnum.Created,
            };
            const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(false);
        });

        it("returns isSingleEmoji false for multiple emojis", () => {
            const event = { ...defaultEvent, content: "😊😎" };
            const tokens = [{ Text: "😊😎" }];
            const message: Message = {
                event,
                event_id: event.id,
                mls_group_id: testMlsGroupId,
                created_at: event.created_at,
                content: event.content,
                pubkey: event.pubkey,
                kind: event.kind,
                tags: [],
                wrapper_event_id: "test-wrapper-id",
                state: NMessageStateEnum.Created,
            };
            const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
            expect(chatMessage.isSingleEmoji).toEqual(false);
        });
    });
});
