import type { CachedMessage, ChatState } from "$lib/types/chat";
import type { NEvent, NostrMlsGroup, NostrMlsGroupWithRelays } from "$lib/types/nostr";
import { NostrMlsGroupType } from "$lib/types/nostr";
import * as tauri from "@tauri-apps/api/core";
import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type Account, activeAccount } from "../accounts";
import { createChatStore } from "../chat";

// Mock Tauri API
const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
    invoke: mockInvoke,
}));

const userAccount: Account = {
    pubkey: "user-pubkey",
    metadata: {},
    nostr_relays: [],
    inbox_relays: [],
    key_package_relays: [],
    mls_group_ids: [],
    settings: {
        darkTheme: false,
        devMode: false,
        lockdownMode: false,
    },
    onboarding: {
        inbox_relays: false,
        key_package_relays: false,
        publish_key_package: false,
    },
    last_used: Date.now(),
    active: true,
};

const createMessageEvent = (
    id: string,
    content: string,
    createdAt: number,
    replyToId?: string
): CachedMessage => {
    const tags: string[][] = [];
    if (replyToId) {
        tags.push(["q", replyToId]);
    }

    return {
        event_id: id,
        account_pubkey: "user-pubkey",
        event_kind: 9,
        content,
        created_at: createdAt,
        outer_event_id: "random-outer-event-id",
        tokens: [{ Text: content }],
        author_pubkey: "user-pubkey",
        mls_group_id: "test-group-id",
        event: {
            id,
            kind: 9,
            pubkey: "user-pubkey",
            created_at: createdAt,
            tags,
            content,
            sig: "test-sig",
        },
    };
};

const createReactionEvent = (
    id: string,
    content: string,
    createdAt: number,
    targetId: string,
    pubkey = "user-pubkey"
): CachedMessage => {
    return {
        event_id: id,
        account_pubkey: pubkey,
        event_kind: 7,
        content,
        created_at: createdAt,
        outer_event_id: "random-outer-event-id",
        tokens: [],
        author_pubkey: pubkey,
        mls_group_id: "test-group-id",
        event: {
            id,
            kind: 7,
            pubkey: pubkey,
            content,
            created_at: createdAt,
            tags: [
                ["e", targetId],
                ["p", "user-pubkey"],
            ],
            sig: "test-sig",
        },
    };
};

const createDeletionEvent = (id: string, targetId: string, createdAt: number): CachedMessage => {
    return {
        event_id: id,
        account_pubkey: "user-pubkey",
        event_kind: 5,
        content: "",
        created_at: createdAt,
        outer_event_id: "random-outer-event-id",
        tokens: [],
        author_pubkey: "user-pubkey",
        mls_group_id: "test-group-id",
        event: {
            id,
            kind: 5,
            pubkey: "user-pubkey",
            created_at: createdAt,
            tags: [["e", targetId]],
            content: "",
            sig: "test-sig",
        },
    };
};

const createTestGroup = (): NostrMlsGroup => {
    return {
        mls_group_id: new Uint8Array([1, 2, 3, 4]),
        nostr_group_id: "test-group-id",
        name: "Test Group",
        description: "A test group",
        admin_pubkeys: ["user-pubkey"],
        last_message_at: Date.now(),
        last_message_id: "last-message-id",
        group_type: NostrMlsGroupType.Group,
    };
};

describe("Chat Store", () => {
    let chatStore: ReturnType<typeof createChatStore>;
    let originalAccount: Account | null;

    beforeEach(() => {
        originalAccount = get(activeAccount);
        activeAccount.set(userAccount);
        vi.clearAllMocks();
        vi.spyOn(tauri, "invoke").mockImplementation(async () => null);
        chatStore = createChatStore();
    });

    afterEach(() => {
        activeAccount.set(originalAccount);
        vi.restoreAllMocks();
    });

    describe("handleCachedMessage", () => {
        describe("with message event", () => {
            it("saves the message", () => {
                const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);

                chatStore.handleCachedMessage(messageEvent);

                const state = get(chatStore) as ChatState;
                expect(state.messages).toEqual([
                    {
                        id: "msg-1",
                        pubkey: "user-pubkey",
                        replyToId: undefined,
                        content: "Hello world",
                        createdAt: 1000,
                        reactions: [],
                        isMine: true,
                        isSingleEmoji: false,
                        lightningInvoice: undefined,
                        lightningPayment: undefined,
                        event: messageEvent.event,
                        tokens: [{ Text: "Hello world" }],
                    },
                ]);
            });

            describe("when event has bolt 11 tag", () => {
                it("saves message with lightning invoice", () => {
                    const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                    messageEvent.event.tags.push([
                        "bolt11",
                        "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                        "21000",
                        "Bitdevs pizza",
                    ]);
                    chatStore.handleCachedMessage(messageEvent);
                    const state = get(chatStore) as ChatState;
                    expect(state.messages).toEqual([
                        {
                            id: "msg-1",
                            pubkey: "user-pubkey",
                            replyToId: undefined,
                            content: "Hello world",
                            createdAt: 1000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: {
                                amount: 21,
                                description: "Bitdevs pizza",
                                invoice:
                                    "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                                isPaid: false,
                            },
                            lightningPayment: undefined,
                            event: messageEvent.event,
                            tokens: [{ Text: "Hello world" }],
                        },
                    ]);
                });
            });

            describe("when event has preimage tag", () => {
                describe("when replying to a lightning invoice", () => {
                    it("saves message with lightning payment paid", () => {
                        const invoiceMessageEvent = createMessageEvent(
                            "msg-1",
                            "Hello world",
                            1000
                        );
                        invoiceMessageEvent.event.tags.push([
                            "bolt11",
                            "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                            "21000",
                            "Bitdevs pizza",
                        ]);
                        chatStore.handleCachedMessage(invoiceMessageEvent);
                        const paymentMessageEvent = createMessageEvent("msg-2", "", 2000, "msg-1");
                        paymentMessageEvent.event.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleCachedMessage(paymentMessageEvent);
                        const paymentMessage = chatStore.findMessage("msg-2");

                        expect(paymentMessage).toEqual({
                            id: "msg-2",
                            pubkey: "user-pubkey",
                            replyToId: "msg-1",
                            content: "",
                            createdAt: 2000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: undefined,
                            lightningPayment: {
                                preimage: "preimage-1",
                                isPaid: true,
                            },
                            event: paymentMessageEvent.event,
                            tokens: [{ Text: "" }],
                        });
                    });
                    it("updates the lightning invoice to paid", () => {
                        const invoiceMessageEvent = createMessageEvent(
                            "msg-1",
                            "Hello world",
                            1000
                        );
                        invoiceMessageEvent.event.tags.push([
                            "bolt11",
                            "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                            "21000",
                            "Bitdevs pizza",
                        ]);
                        chatStore.handleCachedMessage(invoiceMessageEvent);
                        const paymentMessageEvent = createMessageEvent("msg-2", "", 2000, "msg-1");
                        paymentMessageEvent.event.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleCachedMessage(paymentMessageEvent);
                        const invoiceMessage = chatStore.findMessage("msg-1");
                        expect(invoiceMessage).toEqual({
                            id: "msg-1",
                            pubkey: "user-pubkey",
                            replyToId: undefined,
                            content: "Hello world",
                            createdAt: 1000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: {
                                amount: 21,
                                description: "Bitdevs pizza",
                                invoice:
                                    "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                                isPaid: true,
                            },
                            lightningPayment: undefined,
                            event: invoiceMessageEvent.event,
                            tokens: [{ Text: "Hello world" }],
                        });
                    });

                    it("handles payment when reply-to message doesn't exist", () => {
                        const paymentMessageEvent = createMessageEvent(
                            "msg-2",
                            "",
                            2000,
                            "non-existent"
                        );
                        paymentMessageEvent.event.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleCachedMessage(paymentMessageEvent);
                        const paymentMessage = chatStore.findMessage("msg-2");

                        expect(paymentMessage).toEqual({
                            id: "msg-2",
                            pubkey: "user-pubkey",
                            replyToId: "non-existent",
                            content: "",
                            createdAt: 2000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: undefined,
                            lightningPayment: {
                                preimage: "preimage-1",
                                isPaid: false,
                            },
                            event: paymentMessageEvent.event,
                            tokens: [{ Text: "" }],
                        });
                    });
                });
                describe("without reply to lightning invoice", () => {
                    it("saves message with lightning payment but not paid", () => {
                        const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                        messageEvent.event.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleCachedMessage(messageEvent);
                        const state = get(chatStore) as ChatState;
                        expect(state.messages).toEqual([
                            {
                                id: "msg-1",
                                pubkey: "user-pubkey",
                                replyToId: undefined,
                                content: "Hello world",
                                createdAt: 1000,
                                reactions: [],
                                isMine: true,
                                isSingleEmoji: false,
                                lightningInvoice: undefined,
                                lightningPayment: {
                                    preimage: "preimage-1",
                                    isPaid: false,
                                },
                                event: messageEvent.event,
                                tokens: [{ Text: "Hello world" }],
                            },
                        ]);
                    });
                });
            });
        });

        describe("with reaction event", () => {
            it("saves the reaction in target message", () => {
                const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleCachedMessage(messageEvent);
                const reactionEvent = createReactionEvent(
                    "reaction-1",
                    "👍",
                    1000,
                    "msg-1",
                    "other-pubkey"
                );
                chatStore.handleCachedMessage(reactionEvent);
                const message = chatStore.findMessage("msg-1");
                expect(message?.reactions).toEqual([
                    {
                        id: "reaction-1",
                        pubkey: "other-pubkey",
                        content: "👍",
                        targetId: "msg-1",
                        createdAt: 1000,
                        isMine: false,
                        event: reactionEvent.event,
                    },
                ]);
            });
        });

        describe("with deletion event", () => {
            describe("when deleting a message", () => {
                it("saves deletion", () => {
                    const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                    chatStore.handleCachedMessage(messageEvent);
                    const deletionEvent = createDeletionEvent("deletion-1", "msg-1", 2000);
                    chatStore.handleCachedMessage(deletionEvent);
                    expect(chatStore.isDeleted("msg-1")).toBe(true);
                });
            });

            describe("when deleting a reaction", () => {
                it("saves deletion", () => {
                    const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                    chatStore.handleCachedMessage(messageEvent);
                    const reactionEvent = createReactionEvent(
                        "reaction-1",
                        "👍",
                        2000,
                        "msg-1",
                        "other-pubkey"
                    );
                    chatStore.handleCachedMessage(reactionEvent);
                    const deletionEvent = createDeletionEvent("deletion-2", "reaction-1", 3000);
                    chatStore.handleCachedMessage(deletionEvent);
                    expect(chatStore.isDeleted("reaction-1")).toBe(true);
                });
            });
        });
    });

    describe("handleCachedMessages", () => {
        it("handles multiple events in the correct order", () => {
            const firstMessageEvent = createMessageEvent("msg-1", "First message", 1000);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                1500,
                "msg-1",
                "other-pubkey"
            );
            const deletionEvent = createDeletionEvent("deletion-1", "msg-1", 2000);
            const secondMessageEvent = createMessageEvent("msg-2", "Second message", 2500);
            const events: CachedMessage[] = [
                firstMessageEvent,
                reactionEvent,
                deletionEvent,
                secondMessageEvent,
            ];
            chatStore.handleCachedMessages(events);

            const state = get(chatStore) as ChatState;
            expect(state.messages).toEqual([
                {
                    id: "msg-1",
                    pubkey: "user-pubkey",
                    replyToId: undefined,
                    content: "First message",
                    createdAt: 1000,
                    reactions: [
                        {
                            id: "reaction-1",
                            pubkey: "other-pubkey",
                            content: "👍",
                            targetId: "msg-1",
                            createdAt: 1500,
                            isMine: false,
                            event: reactionEvent.event,
                        },
                    ],
                    isMine: true,
                    tokens: [{ Text: "First message" }],
                    isSingleEmoji: false,
                    lightningInvoice: undefined,
                    lightningPayment: undefined,
                    event: firstMessageEvent.event,
                },
                {
                    id: "msg-2",
                    pubkey: "user-pubkey",
                    replyToId: undefined,
                    content: "Second message",
                    createdAt: 2500,
                    reactions: [],
                    isMine: true,
                    isSingleEmoji: false,
                    lightningInvoice: undefined,
                    lightningPayment: undefined,
                    event: secondMessageEvent.event,
                    tokens: [{ Text: "Second message" }],
                },
            ]);
            expect(chatStore.isDeleted("msg-1")).toBe(true);
        });
    });

    describe("clear", () => {
        it("clears messages", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            expect(get(chatStore).messages).toHaveLength(1);
            chatStore.clear();
            expect(get(chatStore).messages).toHaveLength(0);
        });

        it("clears messages reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                1500,
                "msg-1",
                "other-pubkey"
            );
            chatStore.handleCachedMessage(reactionEvent);
            const oldMessage = chatStore.findMessage("msg-1");
            expect(oldMessage?.reactions).toHaveLength(1);
            chatStore.clear();
            chatStore.handleCachedMessage(messageEvent);
            const newMessage = chatStore.findMessage("msg-1");
            expect(newMessage?.reactions).toEqual([]);
        });

        it("clears reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                1500,
                "msg-1",
                "other-pubkey"
            );
            chatStore.handleCachedMessage(reactionEvent);
            const oldMessage = chatStore.findMessage("msg-1");
            expect(oldMessage?.reactions).toHaveLength(1);
            chatStore.clear();
            chatStore.handleCachedMessage(messageEvent);
            expect(chatStore.findReaction("reaction-1")).toBeUndefined();
        });

        it("clears deletions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const deletionEvent = createDeletionEvent("deletion-1", "msg-1", 2000);
            chatStore.handleCachedMessage(deletionEvent);
            expect(chatStore.isDeleted("msg-1")).toBe(true);
            chatStore.clear();
            chatStore.handleCachedMessage(messageEvent);
            expect(chatStore.isDeleted("msg-1")).toBe(false);
        });
    });

    describe("findMessage", () => {
        it("finds a message by id", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const message = chatStore.findMessage("msg-1");
            expect(message).toEqual({
                id: "msg-1",
                pubkey: "user-pubkey",
                replyToId: undefined,
                content: "Hello world",
                createdAt: 1000,
                isMine: true,
                isSingleEmoji: false,
                lightningInvoice: undefined,
                lightningPayment: undefined,
                event: messageEvent.event,
                reactions: [],
                tokens: [{ Text: "Hello world" }],
            });
        });

        it("returns undefined for a non-existent message", () => {
            const message = chatStore.findMessage("non-existent");

            expect(message).toBeUndefined();
        });
    });

    describe("findReaction", () => {
        it("finds a reaction by its ID", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const reactionEvent = createReactionEvent("reaction-1", "👍", 1001, "msg-1");
            chatStore.handleCachedMessage(reactionEvent);

            const reaction = chatStore.findReaction("reaction-1");

            expect(reaction).toEqual({
                id: "reaction-1",
                pubkey: "user-pubkey",
                targetId: "msg-1",
                content: "👍",
                createdAt: 1001,
                isMine: true,
                event: reactionEvent.event,
            });
        });

        it("returns undefined for a non-existent reaction", () => {
            const reaction = chatStore.findReaction("non-existent");

            expect(reaction).toBeUndefined();
        });
    });

    describe("findReplyToMessage", () => {
        it("finds the parent message of a reply", () => {
            const parentMessageEvent = createMessageEvent("parent-msg", "Parent message", 1000);
            chatStore.handleCachedMessage(parentMessageEvent);
            const replyMessageEvent = createMessageEvent(
                "reply-msg",
                "Reply message",
                1100,
                "parent-msg"
            );
            chatStore.handleCachedMessage(replyMessageEvent);
            const replyMessage = chatStore.findMessage("reply-msg");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const parentMessage = chatStore.findReplyToMessage(replyMessage!);

            expect(parentMessage).toEqual({
                id: "parent-msg",
                pubkey: "user-pubkey",
                replyToId: undefined,
                content: "Parent message",
                createdAt: 1000,
                isMine: true,
                isSingleEmoji: false,
                lightningInvoice: undefined,
                lightningPayment: undefined,
                event: parentMessageEvent.event,
                reactions: [],
                tokens: [{ Text: "Parent message" }],
            });
        });

        it("returns undefined if the message has no reply-to", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const replyToMessage = chatStore.findReplyToMessage(message!);

            expect(replyToMessage).toBeUndefined();
        });

        it("returns undefined if the parent message does not exist", () => {
            const replyMessageEvent = createMessageEvent(
                "reply-msg",
                "Reply message",
                1100,
                "non-existent-parent"
            );
            chatStore.handleCachedMessage(replyMessageEvent);
            const replyMessage = chatStore.findMessage("reply-msg");

            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.findReplyToMessage(replyMessage!)).toBeUndefined();
        });
    });

    describe("getMessageReactionsSummary", () => {
        it("returns a summary of reactions for a message", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);

            chatStore.handleCachedMessages([
                createReactionEvent("reaction-1", "👍", 1000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-3", "❤️", 3000, "msg-1", "other-pubkey"),
            ]);
            const summary = chatStore.getMessageReactionsSummary("msg-1");

            expect(summary).toEqual([
                {
                    emoji: "👍",
                    count: 2,
                },
                {
                    emoji: "❤️",
                    count: 1,
                },
            ]);
        });

        it("excludes deleted reactions from the summary", () => {
            chatStore.handleCachedMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 1000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-3", "❤️", 3000, "msg-1", "other-pubkey"),
                createDeletionEvent("deletion-1", "reaction-1", 3000),
            ]);
            const summary = chatStore.getMessageReactionsSummary("msg-1");

            expect(summary).toEqual([
                {
                    emoji: "👍",
                    count: 1,
                },
                {
                    emoji: "❤️",
                    count: 1,
                },
            ]);
        });

        it("when all reactions are deleted, returns an empty array", () => {
            chatStore.handleCachedMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 1000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-3", "❤️", 3000, "msg-1", "other-pubkey"),
                createDeletionEvent("deletion-1", "reaction-1", 3000),
                createDeletionEvent("deletion-2", "reaction-2", 3500),
                createDeletionEvent("deletion-3", "reaction-3", 4000),
            ]);
            const summary = chatStore.getMessageReactionsSummary("msg-1");
            expect(summary).toEqual([]);
        });
    });

    describe("hasReactions", () => {
        it("returns true for a message with active reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                2000,
                "msg-1",
                "other-pubkey"
            );
            chatStore.handleCachedMessage(reactionEvent);

            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(true);
        });

        it("returns false for a message with no reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);

            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(false);
        });

        it("returns false when all reactions are deleted", () => {
            chatStore.handleCachedMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 2000, "msg-1", "other-pubkey"),
                createDeletionEvent("deletion-1", "reaction-1", 3000),
            ]);

            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(false);
        });

        it("returns true when some reactions remain after deletions", () => {
            chatStore.handleCachedMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "❤️", 3000, "msg-1", "other-pubkey"),
                createDeletionEvent("deletion-1", "reaction-1", 4000),
            ]);

            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(true);
        });
    });

    describe("clickReaction", () => {
        it("returns null if message is not found", async () => {
            const group = createTestGroup();
            const result = await chatStore.clickReaction(group, "👍", "non-existent");

            expect(result).toBeNull();
            expect(tauri.invoke).not.toHaveBeenCalled();
        });

        describe("without user reaction", () => {
            beforeEach(() => {
                const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleCachedMessage(messageEvent);
                const reactionResponse: CachedMessage = {
                    event_id: "reaction-1",
                    account_pubkey: "user-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1001,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-1",
                        kind: 7,
                        pubkey: "user-pubkey",
                        content: "👍",
                        created_at: 1001,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                };

                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(reactionResponse);
            });
            it("calls the expected tauri command to add reaction", async () => {
                const group = createTestGroup();
                await chatStore.clickReaction(group, "👍", "msg-1");
                expect(tauri.invoke).toHaveBeenCalledWith("send_mls_message", {
                    group,
                    message: "👍",
                    kind: 7,
                    tags: [
                        ["e", "msg-1"],
                        ["p", "user-pubkey"],
                    ],
                });
            });
            it("returns the new reaction response", async () => {
                const group = createTestGroup();
                const result = await chatStore.clickReaction(group, "👍", "msg-1");
                expect(result).toEqual({
                    event_id: "reaction-1",
                    account_pubkey: "user-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1001,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-1",
                        kind: 7,
                        pubkey: "user-pubkey",
                        content: "👍",
                        created_at: 1001,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                });
            });
        });

        describe("with different user reaction with same content", () => {
            let group: NostrMlsGroup;
            let messageEvent: CachedMessage;
            let firstReactionResponse: CachedMessage;
            const otherUserAccount: Account = { ...userAccount, pubkey: "other-pubkey" };
            let otherUserChatStore: ReturnType<typeof createChatStore>;

            beforeEach(async () => {
                group = createTestGroup();
                messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleCachedMessage(messageEvent);
                firstReactionResponse = {
                    event_id: "reaction-1",
                    account_pubkey: "user-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1001,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-1",
                        kind: 7,
                        pubkey: "user-pubkey",
                        content: "👍",
                        created_at: 1000,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                };

                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(firstReactionResponse);
                await chatStore.clickReaction(group, "👍", "msg-1");
                activeAccount.set(otherUserAccount);
                otherUserChatStore = createChatStore();
                otherUserChatStore.handleCachedMessage(messageEvent);
                otherUserChatStore.handleCachedMessage(firstReactionResponse);

                const secondReactionResponse: CachedMessage = {
                    event_id: "reaction-2",
                    account_pubkey: "other-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1002,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "other-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-2",
                        kind: 7,
                        pubkey: "other-pubkey",
                        content: "👍",
                        created_at: 1002,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                };
                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(secondReactionResponse);
            });

            it("calls the expected tauri command to add reaction", async () => {
                await otherUserChatStore.clickReaction(group, "👍", "msg-1");
                expect(tauri.invoke).toHaveBeenCalledWith("send_mls_message", {
                    group,
                    message: "👍",
                    kind: 7,
                    tags: [
                        ["e", "msg-1"],
                        ["p", "user-pubkey"],
                    ],
                });
            });

            it("returns the new reaction response", async () => {
                const result = await otherUserChatStore.clickReaction(group, "👍", "msg-1");
                expect(result).toEqual({
                    event_id: "reaction-2",
                    account_pubkey: "other-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1002,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "other-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-2",
                        kind: 7,
                        pubkey: "other-pubkey",
                        content: "👍",
                        created_at: 1002,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                });
            });
        });

        describe("with deleted user reaction with same content", () => {
            let group: NostrMlsGroup;
            let messageEvent: CachedMessage;

            beforeEach(async () => {
                group = createTestGroup();
                messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleCachedMessage(messageEvent);
                const firstReactionResponse: CachedMessage = {
                    event_id: "reaction-1",
                    account_pubkey: "user-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1001,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-1",
                        kind: 7,
                        pubkey: "user-pubkey",
                        content: "👍",
                        created_at: 1000,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                };
                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(firstReactionResponse);
                await chatStore.clickReaction(group, "👍", "msg-1");
                const deletionResponse: CachedMessage = {
                    event_id: "deletion-1",
                    account_pubkey: "user-pubkey",
                    event_kind: 5,
                    content: "",
                    created_at: 1002,
                    outer_event_id: "random-outer-event-id",
                    tokens: [],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "deletion-1",
                        kind: 5,
                        pubkey: "user-pubkey",
                        content: "",
                        created_at: 1002,
                        tags: [["e", "reaction-1"]],
                        sig: "test-sig",
                    },
                };

                vi.spyOn(tauri, "invoke").mockImplementation(async () => deletionResponse);
                await chatStore.clickReaction(group, "👍", "msg-1");
                const secondReactionResponse: CachedMessage = {
                    event_id: "reaction-2",
                    account_pubkey: "user-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1003,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-2",
                        kind: 7,
                        pubkey: "user-pubkey",
                        content: "👍",
                        created_at: 1003,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                };
                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(secondReactionResponse);
            });

            it("calls the expected tauri command to add reaction", async () => {
                await chatStore.clickReaction(group, "👍", "msg-1");
                expect(tauri.invoke).toHaveBeenCalledWith("send_mls_message", {
                    group,
                    message: "👍",
                    kind: 7,
                    tags: [
                        ["e", "msg-1"],
                        ["p", "user-pubkey"],
                    ],
                });
            });

            it("returns the new reaction response", async () => {
                const result = await chatStore.clickReaction(group, "👍", "msg-1");
                expect(result).toEqual({
                    event_id: "reaction-2",
                    account_pubkey: "user-pubkey",
                    event_kind: 7,
                    content: "👍",
                    created_at: 1003,
                    outer_event_id: "random-outer-event-id",
                    tokens: [{ Text: "👍" }],
                    author_pubkey: "user-pubkey",
                    mls_group_id: "test-group-id",
                    event: {
                        id: "reaction-2",
                        kind: 7,
                        pubkey: "user-pubkey",
                        content: "👍",
                        created_at: 1003,
                        tags: [
                            ["e", "msg-1"],
                            ["p", "user-pubkey"],
                        ],
                        sig: "test-sig",
                    },
                });
            });

            it("keeps old reaction as deleted", async () => {
                await chatStore.clickReaction(group, "👍", "msg-1");
                expect(chatStore.isDeleted("reaction-1")).toBe(true);
            });

            it("does not delete new reaction", async () => {
                await chatStore.clickReaction(group, "👍", "msg-1");
                expect(chatStore.isDeleted("reaction-2")).toBe(false);
            });
        });
    });

    describe("deleteMessage", () => {
        it("calls the expected tauri command and handles the response", async () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const deletionResponse: CachedMessage = {
                event_id: "deletion-1",
                account_pubkey: "user-pubkey",
                event_kind: 5,
                content: "",
                created_at: 1000,
                outer_event_id: "random-outer-event-id",
                tokens: [],
                author_pubkey: "user-pubkey",
                mls_group_id: "test-group-id",
                event: {
                    id: "deletion-1",
                    kind: 5,
                    pubkey: "user-pubkey",
                    content: "",
                    created_at: 1000,
                    tags: [["e", "msg-1"]],
                    sig: "test-sig",
                },
            };

            vi.spyOn(tauri, "invoke").mockImplementation(async () => deletionResponse);

            const group = createTestGroup();
            const result = await chatStore.deleteMessage(group, "msg-1");
            expect(tauri.invoke).toHaveBeenCalledWith("delete_message", {
                group,
                messageId: "msg-1",
            });
            expect(result).toEqual(deletionResponse);
        });

        it("returns null if message is not found", async () => {
            const group = createTestGroup();
            const result = await chatStore.deleteMessage(group, "non-existent");

            expect(result).toBeNull();
            expect(tauri.invoke).not.toHaveBeenCalled();
        });

        it("returns null if message is not mine", async () => {
            const messageEvent: CachedMessage = {
                event_id: "msg-1",
                account_pubkey: "other-pubkey",
                event_kind: 9,
                content: "Hello world",
                created_at: 1000,
                outer_event_id: "random-outer-event-id",
                tokens: [],
                author_pubkey: "other-pubkey",
                mls_group_id: "test-group-id",
                event: {
                    id: "msg-1",
                    kind: 9,
                    pubkey: "other-pubkey",
                    content: "Hello world",
                    created_at: 1000,
                    tags: [],
                    sig: "test-sig",
                },
            };
            chatStore.handleCachedMessage(messageEvent);

            const group = createTestGroup();
            const result = await chatStore.deleteMessage(group, "msg-1");

            expect(result).toBeNull();
            expect(tauri.invoke).not.toHaveBeenCalled();
        });
    });

    describe("payLightningInvoice", () => {
        it("calls the expected tauri command and handles the payment response", async () => {
            const invoiceMessageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            invoiceMessageEvent.event.tags.push([
                "bolt11",
                "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                "21000",
                "Bitdevs pizza",
            ]);
            chatStore.handleCachedMessage(invoiceMessageEvent);
            const invoiceMessage = chatStore.findMessage("msg-1");
            expect(invoiceMessage?.lightningInvoice).toEqual({
                invoice:
                    "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                amount: 21,
                description: "Bitdevs pizza",
                isPaid: false,
            });
            const paymentResponse: CachedMessage = {
                event_id: "payment-1",
                account_pubkey: "user-pubkey",
                event_kind: 9,
                content: "Payment sent",
                created_at: 1000,
                outer_event_id: "random-outer-event-id",
                tokens: [],
                author_pubkey: "user-pubkey",
                mls_group_id: "test-group-id",
                event: {
                    id: "payment-1",
                    kind: 9,
                    pubkey: "user-pubkey",
                    content: "Payment sent",
                    created_at: 1000,
                    tags: [
                        ["q", "msg-1", "test-relay", "user-pubkey"],
                        ["preimage", "test-preimage"],
                    ],
                    sig: "test-sig",
                },
            };

            vi.spyOn(tauri, "invoke").mockImplementation(async () => paymentResponse);

            const groupWithRelays: NostrMlsGroupWithRelays = {
                group: createTestGroup(),
                relays: ["test-relay"],
            };

            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const result = await chatStore.payLightningInvoice(groupWithRelays, invoiceMessage!);

            expect(tauri.invoke).toHaveBeenCalledWith("pay_invoice", {
                group: groupWithRelays.group,
                tags: [["q", "msg-1", "test-relay", "user-pubkey"]],
                bolt11: "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
            });

            expect(result).toEqual(paymentResponse);
        });

        it("returns null if message has no lightning invoice", async () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);

            const groupWithRelays = {
                group: createTestGroup(),
                relays: ["test-relay"],
            };

            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const result = await chatStore.payLightningInvoice(groupWithRelays, message!);

            expect(result).toBeNull();
            expect(tauri.invoke).not.toHaveBeenCalled();
        });

        it("updates lightning invoice to paid after successful payment", async () => {
            const messageEvent = createMessageEvent("msg-1", "Please pay me", 1000);
            messageEvent.event.tags.push(["bolt11", "lnbc123456789", "21000", "Test payment"]);
            chatStore.handleCachedMessage(messageEvent);

            const paymentResponse: CachedMessage = {
                event_id: "payment-1",
                account_pubkey: "user-pubkey",
                event_kind: 9,
                content: "Payment sent",
                created_at: 1000,
                outer_event_id: "random-outer-event-id",
                tokens: [],
                author_pubkey: "user-pubkey",
                mls_group_id: "test-group-id",
                event: {
                    id: "payment-1",
                    kind: 9,
                    pubkey: "user-pubkey",
                    content: "Payment sent",
                    created_at: 1000,
                    tags: [
                        ["q", "msg-1", "test-relay", "user-pubkey"],
                        ["preimage", "test-preimage"],
                    ],
                    sig: "test-sig",
                },
            };

            vi.spyOn(tauri, "invoke").mockImplementation(async () => paymentResponse);

            const groupWithRelays = {
                group: createTestGroup(),
                relays: ["test-relay"],
            };

            const message = chatStore.findMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            await chatStore.payLightningInvoice(groupWithRelays, message!);
            const updatedMessage = chatStore.findMessage("msg-1");
            expect(updatedMessage?.lightningInvoice?.isPaid).toBe(true);
        });
    });

    describe("isMessageDeletable", () => {
        it("returns true for message that is mine and not deleted", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            expect(chatStore.isMessageDeletable("msg-1")).toBe(true);
        });

        it("returns false for a non-existent message", () => {
            expect(chatStore.isMessageDeletable("non-existent")).toBe(false);
        });

        it("returns false for a message that is already deleted", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const deletionEvent = createDeletionEvent("deletion-1", "msg-1", 2000);
            chatStore.handleCachedMessage(deletionEvent);
            expect(chatStore.isMessageDeletable("msg-1")).toBe(false);
        });

        it("returns false for a message that is not mine", () => {
            const messageEvent: CachedMessage = {
                event_id: "msg-1",
                account_pubkey: "other-pubkey",
                event_kind: 9,
                content: "Hello world",
                created_at: 1000,
                outer_event_id: "random-outer-event-id",
                tokens: [],
                author_pubkey: "other-pubkey",
                mls_group_id: "test-group-id",
                event: {
                    id: "msg-1",
                    kind: 9,
                    pubkey: "other-pubkey",
                    content: "Hello world",
                    created_at: 1000,
                    tags: [],
                    sig: "test-sig",
                },
            };

            chatStore.handleCachedMessage(messageEvent);
            expect(chatStore.isMessageDeletable("msg-1")).toBe(false);
        });
    });

    describe("isMessageCopyable", () => {
        it("returns true for an existing message", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            expect(chatStore.isMessageCopyable("msg-1")).toBe(true);
        });

        it("returns false for a non-existent message", () => {
            expect(chatStore.isMessageCopyable("non-existent")).toBe(false);
        });

        it("returns false for a deleted message", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleCachedMessage(messageEvent);
            const deletionEvent = createDeletionEvent("deletion-1", "msg-1", 2000);
            chatStore.handleCachedMessage(deletionEvent);
            expect(chatStore.isMessageCopyable("msg-1")).toBe(false);
        });
    });
});
