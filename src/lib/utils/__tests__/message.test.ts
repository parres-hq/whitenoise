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
            mediaAttachments: [],
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

    describe("token processing", () => {
        it("removes trailing whitespace and linebreak tokens at the end of the message", () => {
            const tokens = [
                { Text: "Hello" },
                { Whitespace: null },
                { Text: "world" },
                { LineBreak: null },
                { Whitespace: null },
            ];
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
            expect(chatMessage.tokens).toEqual([
                { Text: "Hello" },
                { Whitespace: null },
                { Text: "world" },
            ]);
        });

        describe("with media attachments", () => {
            const tokens = [
                { Text: "Hello" },
                { Whitespace: null },
                { Text: "world" },
                { Whitespace: null },
                { Url: "https://example.com/not-media" },
                { Whitespace: null },
                { Url: "https://example.com/image.jpg" },
            ];
            const eventWithMedia = {
                ...defaultEvent,
                tags: [
                    [
                        "imeta",
                        "url https://example.com/image.jpg",
                        "m image/jpeg",
                        "filename image.jpg",
                        "dim 100x100",
                        "blurhash LGI4eB~C~BR5W7I9x[-;RQyDM{Rj",
                    ],
                ],
            };
            const message: Message = {
                event: eventWithMedia,
                event_id: defaultEvent.id,
                mls_group_id: testMlsGroupId,
                created_at: defaultEvent.created_at,
                content: defaultEvent.content,
                pubkey: defaultEvent.pubkey,
                kind: defaultEvent.kind,
                wrapper_event_id: "test-wrapper-id",
                tags: eventWithMedia.tags,
                state: NMessageStateEnum.Created,
            };

            it("removes media attachment URLs from tokens", () => {
                const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");

                expect(chatMessage.tokens).toEqual([
                    { Text: "Hello" },
                    { Whitespace: null },
                    { Text: "world" },
                    { Whitespace: null },
                    { Url: "https://example.com/not-media" },
                ]);
            });
            it("adds expected number of media attachments", () => {
                const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");

                expect(chatMessage.mediaAttachments.length).toEqual(1);
            });
            it("adds expected media attachment url", () => {
                const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
                const mediaAttachment = chatMessage.mediaAttachments[0];

                expect(mediaAttachment.url).toEqual("https://example.com/image.jpg");
            });

            it("adds expected media attachment type", () => {
                const chatMessage = messageToChatMessage({ message, tokens }, "some-pubkey");
                const mediaAttachment = chatMessage.mediaAttachments[0];

                expect(mediaAttachment.type).toEqual("image");
            });
        });
    });
});
