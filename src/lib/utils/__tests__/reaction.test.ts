import type { Message } from "$lib/types/chat";
import type { NEvent, NMessageState } from "$lib/types/nostr";
import { NMessageState as NMessageStateEnum } from "$lib/types/nostr";
import { describe, expect, it } from "vitest";
import { messageToReactionMessage } from "../reaction";

describe("messageToReactionMessage", () => {
    describe("with valid target id", () => {
        const message = {
            event_id: "test-id",
            account_pubkey: "test-pubkey",
            author_pubkey: "test-pubkey",
            mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
            created_at: 1234567890,
            event_kind: 7,
            content: "👍",
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "👍" }],
            event: {
                id: "test-id",
                pubkey: "test-pubkey",
                created_at: 1234567890,
                kind: 7,
                tags: [
                    ["p", "author-pubkey"],
                    ["e", "target-event-id"],
                    ["other", "value"],
                ],
                content: "👍",
                sig: "signature",
            },
            pubkey: "test-pubkey",
            kind: 7,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };

        it("returns a Reaction object", () => {
            const result = messageToReactionMessage(
                { message, tokens: message.tokens },
                "test-pubkey"
            );
            expect(result).toEqual({
                id: "test-id",
                pubkey: "test-pubkey",
                content: "👍",
                createdAt: 1234567890,
                targetId: "target-event-id",
                isMine: true,
                event: {
                    id: "test-id",
                    pubkey: "test-pubkey",
                    created_at: 1234567890,
                    kind: 7,
                    tags: [
                        ["p", "author-pubkey"],
                        ["e", "target-event-id"],
                        ["other", "value"],
                    ],
                    content: "👍",
                    sig: "signature",
                },
            });
        });

        describe("with a different pubkey", () => {
            it("isMine of reaction is false", () => {
                const result = messageToReactionMessage(
                    { message, tokens: message.tokens },
                    "other-pubkey"
                );
                expect(result).toEqual({
                    id: "test-id",
                    pubkey: "test-pubkey",
                    content: "👍",
                    createdAt: 1234567890,
                    targetId: "target-event-id",
                    isMine: false,
                    event: {
                        id: "test-id",
                        pubkey: "test-pubkey",
                        created_at: 1234567890,
                        kind: 7,
                        tags: [
                            ["p", "author-pubkey"],
                            ["e", "target-event-id"],
                            ["other", "value"],
                        ],
                        content: "👍",
                        sig: "signature",
                    },
                });
            });
        });
    });

    describe("without a target id", () => {
        const event: NEvent = {
            id: "test-id",
            pubkey: "test-pubkey",
            created_at: 1234567890,
            kind: 7,
            tags: [
                ["p", "author-pubkey"],
                ["other", "value"],
            ],
            content: "👍",
            sig: "signature",
        };
        const message = {
            event_id: "test-id",
            account_pubkey: "test-pubkey",
            author_pubkey: "test-pubkey",
            mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
            created_at: 1234567890,
            event_kind: 7,
            content: "👍",
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "👍" }],
            event: {
                id: "test-id",
                pubkey: "test-pubkey",
                created_at: 1234567890,
                kind: 7,
                tags: [
                    ["p", "author-pubkey"],
                    ["other", "value"],
                ],
                content: "👍",
                sig: "signature",
            },
            pubkey: "test-pubkey",
            kind: 7,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };
        it("returns null", () => {
            expect(
                messageToReactionMessage({ message, tokens: message.tokens }, "test-pubkey")
            ).toBeNull();
        });
    });

    describe("with empty e tag", () => {
        const message = {
            event_id: "test-id",
            account_pubkey: "test-pubkey",
            author_pubkey: "test-pubkey",
            mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
            created_at: 1234567890,
            event_kind: 7,
            content: "👍",
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "👍" }],
            event: {
                id: "test-id",
                pubkey: "test-pubkey",
                created_at: 1234567890,
                kind: 7,
                tags: [["p", "author-pubkey"], ["e"], ["other", "value"]],
                content: "👍",
                sig: "signature",
            },
            pubkey: "test-pubkey",
            kind: 7,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };

        it("returns null", () => {
            expect(
                messageToReactionMessage({ message, tokens: message.tokens }, "test-pubkey")
            ).toBeNull();
        });
    });
});
