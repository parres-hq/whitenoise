import type { Message } from "$lib/types/chat";
import { describe, expect, it } from "vitest";
import { messageToDeletionMessage } from "../deletion";

describe("messageToDeletionMessage", () => {
    describe("with valid target id in e tag", () => {
        const message: Message = {
            event_id: "deletion-event-id",
            account_pubkey: "author-pubkey",
            author_pubkey: "author-pubkey",
            mls_group_id: "mls_group_id",
            created_at: 1234567890,
            event_kind: 5,
            content: "Delete this event",
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "Delete this event" }],
            event: {
                id: "deletion-event-id",
                pubkey: "author-pubkey",
                created_at: 1234567890,
                kind: 5,
                tags: [
                    ["p", "some-pubkey"],
                    ["e", "target-event-id"],
                    ["other", "value"],
                ],
                content: "Delete this event",
                sig: "signature",
            },
        };
        it("returns a valid Deletion object", () => {
            const deletion = messageToDeletionMessage(message);
            expect(deletion).toEqual({
                id: "deletion-event-id",
                pubkey: "author-pubkey",
                targetId: "target-event-id",
                event: {
                    id: "deletion-event-id",
                    pubkey: "author-pubkey",
                    created_at: 1234567890,
                    kind: 5,
                    tags: [
                        ["p", "some-pubkey"],
                        ["e", "target-event-id"],
                        ["other", "value"],
                    ],
                    content: "Delete this event",
                    sig: "signature",
                },
            });
        });
    });

    describe("without e tag", () => {
        const message: Message = {
            event_id: "deletion-event-id",
            account_pubkey: "author-pubkey",
            author_pubkey: "author-pubkey",
            mls_group_id: "mls_group_id",
            created_at: 1234567890,
            event_kind: 5,
            content: "Delete this event",
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "Delete this event" }],
            event: {
                id: "deletion-event-id",
                pubkey: "author-pubkey",
                created_at: 1234567890,
                kind: 5,
                tags: [
                    ["p", "some-pubkey"],
                    ["other", "value"],
                ],
                content: "Delete this event",
                sig: "signature",
            },
        };

        it("returns null", () => {
            const deletion = messageToDeletionMessage(message);
            expect(deletion).toBeNull();
        });
    });

    describe("with empty e tag", () => {
        const message: Message = {
            event_id: "deletion-event-id",
            account_pubkey: "author-pubkey",
            author_pubkey: "author-pubkey",
            mls_group_id: "mls_group_id",
            created_at: 1234567890,
            event_kind: 5,
            content: "Delete this event",
            outer_event_id: "outer_event_id",
            tokens: [{ Text: "Delete this event" }],
            event: {
                id: "deletion-event-id",
                pubkey: "author-pubkey",
                created_at: 1234567890,
                kind: 5,
                tags: [["p", "some-pubkey"], ["e"], ["other", "value"]],
                content: "Delete this event",
                sig: "signature",
            },
        };

        it("returns null", () => {
            const deletion = messageToDeletionMessage(message);
            expect(deletion).toBeNull();
        });
    });
});
