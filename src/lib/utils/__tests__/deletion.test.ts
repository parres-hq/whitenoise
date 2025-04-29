import type { Message } from "$lib/types/chat";
import { NMessageState as NMessageStateEnum } from "$lib/types/nostr";
import { describe, expect, it } from "vitest";
import { messageToDeletionMessage } from "../deletion";

describe("messageToDeletionMessage", () => {
    describe("with valid target id in e tag", () => {
        const message: Message = {
            event_id: "deletion-event-id",
            mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
            created_at: 1234567890,
            content: "Delete this event",
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
            pubkey: "author-pubkey",
            kind: 5,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };
        const tokens = [{ Text: "Delete this event" }];
        it("returns a valid Deletion object", () => {
            const deletion = messageToDeletionMessage({ message, tokens });
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
            mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
            created_at: 1234567890,
            content: "Delete this event",
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
            pubkey: "author-pubkey",
            kind: 5,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };
        const tokens = [{ Text: "Delete this event" }];
        it("returns null", () => {
            const deletion = messageToDeletionMessage({ message, tokens });
            expect(deletion).toBeNull();
        });
    });

    describe("with empty e tag", () => {
        const message: Message = {
            event_id: "deletion-event-id",
            mls_group_id: { value: { vec: new Uint8Array([1, 2, 3, 4]) } },
            created_at: 1234567890,
            content: "Delete this event",
            event: {
                id: "deletion-event-id",
                pubkey: "author-pubkey",
                created_at: 1234567890,
                kind: 5,
                tags: [["p", "some-pubkey"], ["e"], ["other", "value"]],
                content: "Delete this event",
                sig: "signature",
            },
            pubkey: "author-pubkey",
            kind: 5,
            tags: [],
            wrapper_event_id: "test-wrapper-id",
            state: NMessageStateEnum.Created,
        };
        const tokens = [{ Text: "Delete this event" }];
        it("returns null", () => {
            const deletion = messageToDeletionMessage({ message, tokens });
            expect(deletion).toBeNull();
        });
    });
});
