import type { ChatState, Message } from "$lib/types/chat";
import { NostrMlsGroupState, NostrMlsGroupType } from "$lib/types/nostr";
import { NMessageState } from "$lib/types/nostr";
import type { NGroup } from "$lib/types/nostr";
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

// Helper to create a dummy MlsGroupId for tests
function createTestMlsGroupId(): import("$lib/types/nostr").MlsGroupId {
    return { value: { vec: new Uint8Array([1, 2, 3, 4]) } };
}

// Updated helpers to return MessageWithTokens
const createMessageEvent = (
    id: string,
    content: string,
    createdAt: number,
    replyToId?: string
): import("$lib/types/nostr").MessageWithTokens => {
    const tags: string[][] = [];
    if (replyToId) {
        // For tests, use dummy relay and pubkey values
        tags.push(["q", replyToId, "test-relay", "user-pubkey"]);
    }

    const message: Message = {
        event_id: id,
        kind: 9,
        content,
        created_at: createdAt,
        mls_group_id: createTestMlsGroupId(),
        pubkey: "user-pubkey",
        tags,
        wrapper_event_id: "test-wrapper-id",
        state: NMessageState.Created,
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
    return {
        message,
        tokens: [{ Text: content }],
    };
};

const createMessageWithMedia = (
    id: string,
    content: string,
    createdAt: number,
    mediaUrl: string,
    mimeType: string,
    blurhash?: string
): import("$lib/types/nostr").MessageWithTokens => {
    const imetaValues = [
        "imeta",
        `url ${mediaUrl}`,
        `mime ${mimeType}`,
        "size 1234",
        "dim 800x600",
        blurhash ? `blurhash ${blurhash}` : "",
    ].filter(Boolean);

    const tags: string[][] = [imetaValues];

    const message: Message = {
        event_id: id,
        kind: 9,
        content,
        created_at: createdAt,
        mls_group_id: createTestMlsGroupId(),
        pubkey: "user-pubkey",
        tags,
        wrapper_event_id: "test-wrapper-id",
        state: NMessageState.Created,
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
    return {
        message,
        tokens: [{ Text: content }, { Url: mediaUrl }],
    };
};

const createReactionEvent = (
    id: string,
    content: string,
    createdAt: number,
    targetId: string,
    pubkey = "user-pubkey"
): import("$lib/types/nostr").MessageWithTokens => {
    const message: Message = {
        event_id: id,
        kind: 7,
        content,
        created_at: createdAt,
        mls_group_id: createTestMlsGroupId(),
        pubkey,
        tags: [
            ["e", targetId],
            ["p", "user-pubkey"],
        ],
        wrapper_event_id: "test-wrapper-id",
        state: NMessageState.Created,
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
    return {
        message,
        tokens: [{ Text: content }],
    };
};

const createDeletionMessage = (
    id: string,
    targetId: string,
    createdAt: number
): import("$lib/types/nostr").MessageWithTokens => {
    const message: Message = {
        event_id: id,
        kind: 5,
        content: "",
        created_at: createdAt,
        mls_group_id: createTestMlsGroupId(),
        pubkey: "user-pubkey",
        tags: [["e", targetId]],
        wrapper_event_id: "test-wrapper-id",
        state: NMessageState.Created,
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
    return {
        message,
        tokens: [{ Text: "" }],
    };
};

const createTestGroup = (): NGroup => {
    return {
        mls_group_id: createTestMlsGroupId(),
        nostr_group_id: new Uint8Array([5, 6, 7, 8]),
        name: "Test Group",
        description: "A test group",
        admin_pubkeys: ["user-pubkey"],
        last_message_at: Date.now(),
        last_message_id: "last-message-id",
        group_type: NostrMlsGroupType.Group,
        epoch: 0,
        state: NostrMlsGroupState.Active,
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

    describe("handleMessage", () => {
        describe("with message event", () => {
            it("saves the message", () => {
                const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);

                chatStore.handleMessage(messageEvent);

                const state = get(chatStore) as ChatState;
                expect(state.chatMessages).toEqual([
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
                        event: messageEvent.message.event,
                        tokens: [{ Text: "Hello world" }],
                        mediaAttachments: [],
                    },
                ]);
            });

            describe("when event has bolt 11 tag", () => {
                it("saves message with lightning invoice", () => {
                    const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                    messageEvent.message.tags.push([
                        "bolt11",
                        "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                        "21000",
                        "Bitdevs pizza",
                    ]);
                    chatStore.handleMessage(messageEvent);
                    const state = get(chatStore) as ChatState;
                    expect(state.chatMessages).toEqual([
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
                            event: messageEvent.message.event,
                            tokens: [{ Text: "Hello world" }],
                            mediaAttachments: [],
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
                        invoiceMessageEvent.message.tags.push([
                            "bolt11",
                            "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                            "21000",
                            "Bitdevs pizza",
                        ]);
                        chatStore.handleMessage(invoiceMessageEvent);
                        const paymentMessageEvent = createMessageEvent("msg-2", "", 2000, "msg-1");
                        paymentMessageEvent.message.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleMessage(paymentMessageEvent);
                        const paymentMessage = chatStore.findChatMessage("msg-2");

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
                            event: paymentMessageEvent.message.event,
                            tokens: [{ Text: "" }],
                            mediaAttachments: [],
                        });
                    });
                    it("updates the lightning invoice to paid", () => {
                        const invoiceMessageEvent = createMessageEvent(
                            "msg-1",
                            "Hello world",
                            1000
                        );
                        invoiceMessageEvent.message.tags.push([
                            "bolt11",
                            "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                            "21000",
                            "Bitdevs pizza",
                        ]);
                        chatStore.handleMessage(invoiceMessageEvent);
                        const paymentMessageEvent = createMessageEvent("msg-2", "", 2000, "msg-1");
                        paymentMessageEvent.message.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleMessage(paymentMessageEvent);
                        const invoiceMessage = chatStore.findChatMessage("msg-1");
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
                            event: invoiceMessageEvent.message.event,
                            tokens: [{ Text: "Hello world" }],
                            mediaAttachments: [],
                        });
                    });

                    it("handles payment when reply-to message doesn't exist", () => {
                        const paymentMessageEvent = createMessageEvent(
                            "msg-2",
                            "",
                            2000,
                            "non-existent"
                        );
                        paymentMessageEvent.message.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleMessage(paymentMessageEvent);
                        const paymentMessage = chatStore.findChatMessage("msg-2");

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
                            event: paymentMessageEvent.message.event,
                            tokens: [{ Text: "" }],
                            mediaAttachments: [],
                        });
                    });
                });
                describe("without reply to lightning invoice", () => {
                    it("saves message with lightning payment but not paid", () => {
                        const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                        messageEvent.message.tags.push(["preimage", "preimage-1"]);
                        chatStore.handleMessage(messageEvent);
                        const state = get(chatStore) as ChatState;
                        expect(state.chatMessages).toEqual([
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
                                event: messageEvent.message.event,
                                tokens: [{ Text: "Hello world" }],
                                mediaAttachments: [],
                            },
                        ]);
                    });
                });
            });

            describe("with media attachments", () => {
                it("saves message with image attachment", () => {
                    const messageEvent = createMessageWithMedia(
                        "msg-1",
                        "Check out this image",
                        1000,
                        "https://example.com/image.jpg",
                        "image/jpeg",
                        "LEHV6nWB2yk8pyo0adR*.7kCMdnj"
                    );

                    chatStore.handleMessage(messageEvent);

                    const state = get(chatStore) as ChatState;
                    expect(state.chatMessages).toEqual([
                        {
                            id: "msg-1",
                            pubkey: "user-pubkey",
                            replyToId: undefined,
                            content: "Check out this image",
                            createdAt: 1000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: undefined,
                            lightningPayment: undefined,
                            event: messageEvent.message.event,
                            tokens: [{ Text: "Check out this image" }],
                            mediaAttachments: [
                                {
                                    url: "https://example.com/image.jpg",
                                    type: "image",
                                    blurhashSvg: expect.any(String),
                                },
                            ],
                        },
                    ]);
                });

                it("saves message with video attachment", () => {
                    const messageEvent = createMessageWithMedia(
                        "msg-1",
                        "Check out this video",
                        1000,
                        "https://example.com/video.mp4",
                        "video/mp4"
                    );

                    chatStore.handleMessage(messageEvent);

                    const state = get(chatStore) as ChatState;
                    expect(state.chatMessages).toEqual([
                        {
                            id: "msg-1",
                            pubkey: "user-pubkey",
                            replyToId: undefined,
                            content: "Check out this video",
                            createdAt: 1000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: undefined,
                            lightningPayment: undefined,
                            event: messageEvent.message.event,
                            tokens: [{ Text: "Check out this video" }],
                            mediaAttachments: [
                                {
                                    url: "https://example.com/video.mp4",
                                    type: "video",
                                    blurhashSvg: undefined,
                                },
                            ],
                        },
                    ]);
                });

                it("saves message with multiple media attachments", () => {
                    const messageEvent = createMessageWithMedia(
                        "msg-1",
                        "Multiple attachments",
                        1000,
                        "https://example.com/image.jpg",
                        "image/jpeg",
                        "LEHV6nWB2yk8pyo0adR*.7kCMdnj"
                    );
                    messageEvent.message.tags.push([
                        "imeta",
                        "url https://example.com/video.mp4",
                        "mime video/mp4",
                    ]);
                    messageEvent.tokens.push({ Url: "https://example.com/video.mp4" });

                    chatStore.handleMessage(messageEvent);

                    const state = get(chatStore) as ChatState;
                    expect(state.chatMessages).toEqual([
                        {
                            id: "msg-1",
                            pubkey: "user-pubkey",
                            replyToId: undefined,
                            content: "Multiple attachments",
                            createdAt: 1000,
                            reactions: [],
                            isMine: true,
                            isSingleEmoji: false,
                            lightningInvoice: undefined,
                            lightningPayment: undefined,
                            event: messageEvent.message.event,
                            tokens: [{ Text: "Multiple attachments" }],
                            mediaAttachments: [
                                {
                                    url: "https://example.com/image.jpg",
                                    type: "image",
                                    blurhashSvg: expect.any(String),
                                },
                                {
                                    url: "https://example.com/video.mp4",
                                    type: "video",
                                    blurhashSvg: undefined,
                                },
                            ],
                        },
                    ]);
                });

                it("handles message with media attachment and lightning invoice", () => {
                    const messageEvent = createMessageWithMedia(
                        "msg-1",
                        "Image with invoice",
                        1000,
                        "https://example.com/image.jpg",
                        "image/jpeg"
                    );
                    messageEvent.message.tags.push([
                        "bolt11",
                        "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                        "21000",
                        "Bitdevs pizza",
                    ]);

                    chatStore.handleMessage(messageEvent);

                    const state = get(chatStore) as ChatState;
                    expect(state.chatMessages).toEqual([
                        {
                            id: "msg-1",
                            pubkey: "user-pubkey",
                            replyToId: undefined,
                            content: "Image with invoice",
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
                            event: messageEvent.message.event,
                            tokens: [{ Text: "Image with invoice" }],
                            mediaAttachments: [
                                {
                                    url: "https://example.com/image.jpg",
                                    type: "image",
                                    blurhashSvg: undefined,
                                },
                            ],
                        },
                    ]);
                });
            });
        });

        describe("with reaction event", () => {
            it("saves the reaction in target message", () => {
                const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleMessage(messageEvent);
                const reactionEvent = createReactionEvent(
                    "reaction-1",
                    "👍",
                    1000,
                    "msg-1",
                    "other-pubkey"
                );
                chatStore.handleMessage(reactionEvent);
                const message = chatStore.findChatMessage("msg-1");
                expect(message?.reactions).toEqual([
                    {
                        id: "reaction-1",
                        pubkey: "other-pubkey",
                        content: "👍",
                        targetId: "msg-1",
                        createdAt: 1000,
                        isMine: false,
                        event: reactionEvent.message.event,
                    },
                ]);
            });
        });

        describe("with deletion message", () => {
            describe("when deleting a message", () => {
                it("saves deletion message", () => {
                    const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                    chatStore.handleMessage(messageEvent);
                    const deletionMessage = createDeletionMessage("deletion-1", "msg-1", 2000);
                    chatStore.handleMessage(deletionMessage);
                    expect(chatStore.isDeleted("msg-1")).toBe(true);
                });
            });

            describe("when deleting a reaction", () => {
                it("saves deletion", () => {
                    const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                    chatStore.handleMessage(messageEvent);
                    const reactionEvent = createReactionEvent(
                        "reaction-1",
                        "👍",
                        2000,
                        "msg-1",
                        "other-pubkey"
                    );
                    chatStore.handleMessage(reactionEvent);
                    const message = createDeletionMessage("deletion-2", "reaction-1", 3000);
                    chatStore.handleMessage(message);
                    expect(chatStore.isDeleted("reaction-1")).toBe(true);
                });
            });
        });
    });

    describe("handleMessages", () => {
        it("handles multiple events in the correct order", () => {
            const firstMessageEvent = createMessageEvent("msg-1", "First message", 1000);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                1500,
                "msg-1",
                "other-pubkey"
            );
            const message = createDeletionMessage("deletion-1", "msg-1", 2000);
            const secondMessageEvent = createMessageEvent("msg-2", "Second message", 2500);
            const events: import("$lib/types/nostr").MessageWithTokens[] = [
                firstMessageEvent,
                reactionEvent,
                message,
                secondMessageEvent,
            ];
            chatStore.handleMessages(events);

            const state = get(chatStore) as ChatState;
            expect(state.chatMessages).toEqual([
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
                            event: reactionEvent.message.event,
                        },
                    ],
                    isMine: true,
                    tokens: [{ Text: "First message" }],
                    isSingleEmoji: false,
                    lightningInvoice: undefined,
                    lightningPayment: undefined,
                    event: firstMessageEvent.message.event,
                    mediaAttachments: [],
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
                    event: secondMessageEvent.message.event,
                    tokens: [{ Text: "Second message" }],
                    mediaAttachments: [],
                },
            ]);
            expect(chatStore.isDeleted("msg-1")).toBe(true);
        });
    });

    describe("clear", () => {
        it("clears messages", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            expect(get(chatStore).chatMessages).toHaveLength(1);
            chatStore.clear();
            expect(get(chatStore).chatMessages).toHaveLength(0);
        });

        it("clears messages reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                1500,
                "msg-1",
                "other-pubkey"
            );
            chatStore.handleMessage(reactionEvent);
            const oldMessage = chatStore.findChatMessage("msg-1");
            expect(oldMessage?.reactions).toHaveLength(1);
            chatStore.clear();
            chatStore.handleMessage(messageEvent);
            const newMessage = chatStore.findChatMessage("msg-1");
            expect(newMessage?.reactions).toEqual([]);
        });

        it("clears reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                1500,
                "msg-1",
                "other-pubkey"
            );
            chatStore.handleMessage(reactionEvent);
            const oldMessage = chatStore.findChatMessage("msg-1");
            expect(oldMessage?.reactions).toHaveLength(1);
            chatStore.clear();
            chatStore.handleMessage(messageEvent);
            expect(chatStore.findReactionMessage("reaction-1")).toBeUndefined();
        });

        it("clears deletions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const message = createDeletionMessage("deletion-1", "msg-1", 2000);
            chatStore.handleMessage(message);
            expect(chatStore.isDeleted("msg-1")).toBe(true);
            chatStore.clear();
            chatStore.handleMessage(messageEvent);
            expect(chatStore.isDeleted("msg-1")).toBe(false);
        });
    });

    describe("findChatMessage", () => {
        it("finds a message by id", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const message = chatStore.findChatMessage("msg-1");
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
                event: messageEvent.message.event,
                reactions: [],
                tokens: [{ Text: "Hello world" }],
                mediaAttachments: [],
            });
        });

        it("returns undefined for a non-existent message", () => {
            const message = chatStore.findChatMessage("non-existent");

            expect(message).toBeUndefined();
        });
    });

    describe("findReactionMessage", () => {
        it("finds a reaction by its ID", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const reactionEvent = createReactionEvent("reaction-1", "👍", 1001, "msg-1");
            chatStore.handleMessage(reactionEvent);

            const reaction = chatStore.findReactionMessage("reaction-1");

            expect(reaction).toEqual({
                id: "reaction-1",
                pubkey: "user-pubkey",
                targetId: "msg-1",
                content: "👍",
                createdAt: 1001,
                isMine: true,
                event: reactionEvent.message.event,
            });
        });

        it("returns undefined for a non-existent reaction", () => {
            const reaction = chatStore.findReactionMessage("non-existent");

            expect(reaction).toBeUndefined();
        });
    });

    describe("findReplyToChatMessage", () => {
        it("finds the parent message of a reply", () => {
            const parentMessageEvent = createMessageEvent("parent-msg", "Parent message", 1000);
            chatStore.handleMessage(parentMessageEvent);
            const replyMessageEvent = createMessageEvent(
                "reply-msg",
                "Reply message",
                1100,
                "parent-msg"
            );
            chatStore.handleMessage(replyMessageEvent);
            const replyMessage = chatStore.findChatMessage("reply-msg");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const parentMessage = chatStore.findReplyToChatMessage(replyMessage!);

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
                event: parentMessageEvent.message.event,
                reactions: [],
                tokens: [{ Text: "Parent message" }],
                mediaAttachments: [],
            });
        });

        it("returns undefined if the message has no reply-to", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const message = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const replyToMessage = chatStore.findReplyToChatMessage(message!);

            expect(replyToMessage).toBeUndefined();
        });

        it("returns undefined if the parent message does not exist", () => {
            const replyMessageEvent = createMessageEvent(
                "reply-msg",
                "Reply message",
                1100,
                "non-existent-parent"
            );
            chatStore.handleMessage(replyMessageEvent);
            const replyMessage = chatStore.findChatMessage("reply-msg");

            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.findReplyToChatMessage(replyMessage!)).toBeUndefined();
        });
    });

    describe("getMessageReactionsSummary", () => {
        it("returns a summary of reactions for a message", () => {
            chatStore.handleMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
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
            chatStore.handleMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 1000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-3", "❤️", 3000, "msg-1", "other-pubkey"),
                createDeletionMessage("deletion-1", "reaction-1", 3000),
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
            chatStore.handleMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 1000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-3", "❤️", 3000, "msg-1", "other-pubkey"),
                createDeletionMessage("deletion-1", "reaction-1", 3000),
                createDeletionMessage("deletion-2", "reaction-2", 3500),
                createDeletionMessage("deletion-3", "reaction-3", 4000),
            ]);
            const summary = chatStore.getMessageReactionsSummary("msg-1");
            expect(summary).toEqual([]);
        });
    });

    describe("hasReactions", () => {
        it("returns true for a message with active reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const reactionEvent = createReactionEvent(
                "reaction-1",
                "👍",
                2000,
                "msg-1",
                "other-pubkey"
            );
            chatStore.handleMessage(reactionEvent);

            const message = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(true);
        });

        it("returns false for a message with no reactions", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);

            const message = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(false);
        });

        it("returns false when all reactions are deleted", () => {
            chatStore.handleMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "👍", 3000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-3", "❤️", 4000, "msg-1", "other-pubkey"),
                createDeletionMessage("deletion-1", "reaction-1", 5000),
                createDeletionMessage("deletion-2", "reaction-2", 6000),
                createDeletionMessage("deletion-3", "reaction-3", 7000),
            ]);

            const message = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            expect(chatStore.hasReactions(message!)).toBe(false);
        });

        it("returns true when some reactions remain after deletions", () => {
            chatStore.handleMessages([
                createMessageEvent("msg-1", "Hello world", 1000),
                createReactionEvent("reaction-1", "👍", 2000, "msg-1", "other-pubkey"),
                createReactionEvent("reaction-2", "❤️", 3000, "msg-1", "other-pubkey"),
                createDeletionMessage("deletion-1", "reaction-1", 4000),
            ]);

            const message = chatStore.findChatMessage("msg-1");
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
                chatStore.handleMessage(messageEvent);
                const reactionResponse: import("$lib/types/nostr").MessageWithTokens = {
                    message: {
                        event_id: "reaction-1",
                        kind: 7,
                        content: "👍",
                        created_at: 1001,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
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
                    message: {
                        event_id: "reaction-1",
                        kind: 7,
                        content: "👍",
                        created_at: 1001,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
                });
            });
        });

        describe("with different user reaction with same content", () => {
            let group: NGroup;
            let messageEvent: import("$lib/types/nostr").MessageWithTokens;
            let firstReactionResponse: import("$lib/types/nostr").MessageWithTokens;
            const otherUserAccount: Account = { ...userAccount, pubkey: "other-pubkey" };
            let otherUserChatStore: ReturnType<typeof createChatStore>;

            beforeEach(async () => {
                group = createTestGroup();
                messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleMessage(messageEvent);
                firstReactionResponse = {
                    message: {
                        event_id: "reaction-1",
                        kind: 7,
                        content: "👍",
                        created_at: 1001,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
                };

                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(firstReactionResponse);
                await chatStore.clickReaction(group, "👍", "msg-1");
                activeAccount.set(otherUserAccount);
                otherUserChatStore = createChatStore();
                otherUserChatStore.handleMessage(messageEvent);
                otherUserChatStore.handleMessage(firstReactionResponse);

                const secondReactionResponse: import("$lib/types/nostr").MessageWithTokens = {
                    message: {
                        event_id: "reaction-2",
                        kind: 7,
                        content: "👍",
                        created_at: 1002,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "other-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
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
                    message: {
                        event_id: "reaction-2",
                        kind: 7,
                        content: "👍",
                        created_at: 1002,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "other-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
                });
            });
        });

        describe("with deleted user reaction with same content", () => {
            let group: NGroup;
            let messageEvent: import("$lib/types/nostr").MessageWithTokens;

            beforeEach(async () => {
                group = createTestGroup();
                messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
                chatStore.handleMessage(messageEvent);
                const firstReactionResponse: import("$lib/types/nostr").MessageWithTokens = {
                    message: {
                        event_id: "reaction-1",
                        kind: 7,
                        content: "👍",
                        created_at: 1001,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
                };
                vi.spyOn(tauri, "invoke").mockResolvedValueOnce(firstReactionResponse);
                await chatStore.clickReaction(group, "👍", "msg-1");
                const deletionResponse: import("$lib/types/nostr").MessageWithTokens = {
                    message: {
                        event_id: "deletion-1",
                        kind: 5,
                        content: "",
                        created_at: 1002,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [["e", "reaction-1"]],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
                        event: {
                            id: "deletion-1",
                            kind: 5,
                            pubkey: "user-pubkey",
                            content: "",
                            created_at: 1002,
                            tags: [["e", "reaction-1"]],
                            sig: "test-sig",
                        },
                    },
                    tokens: [{ Text: "" }],
                };

                vi.spyOn(tauri, "invoke").mockImplementation(async () => deletionResponse);
                await chatStore.clickReaction(group, "👍", "msg-1");
                const secondReactionResponse: import("$lib/types/nostr").MessageWithTokens = {
                    message: {
                        event_id: "reaction-2",
                        kind: 7,
                        content: "👍",
                        created_at: 1003,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
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
                    message: {
                        event_id: "reaction-2",
                        kind: 7,
                        content: "👍",
                        created_at: 1003,
                        mls_group_id: createTestMlsGroupId(),
                        pubkey: "user-pubkey",
                        tags: [],
                        wrapper_event_id: "test-wrapper-id",
                        state: NMessageState.Created,
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
                    },
                    tokens: [{ Text: "👍" }],
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
            chatStore.handleMessage(messageEvent);
            const deletionResponse: import("$lib/types/nostr").MessageWithTokens = {
                message: {
                    event_id: "deletion-1",
                    kind: 5,
                    content: "",
                    created_at: 1000,
                    mls_group_id: createTestMlsGroupId(),
                    pubkey: "user-pubkey",
                    tags: [["e", "msg-1"]],
                    wrapper_event_id: "test-wrapper-id",
                    state: NMessageState.Created,
                    event: {
                        id: "deletion-1",
                        kind: 5,
                        pubkey: "user-pubkey",
                        content: "",
                        created_at: 1000,
                        tags: [["e", "msg-1"]],
                        sig: "test-sig",
                    },
                },
                tokens: [{ Text: "" }],
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
            const messageEvent: import("$lib/types/nostr").MessageWithTokens = {
                message: {
                    event_id: "msg-1",
                    kind: 9,
                    content: "Hello world",
                    created_at: 1000,
                    mls_group_id: createTestMlsGroupId(),
                    pubkey: "other-pubkey",
                    tags: [],
                    wrapper_event_id: "test-wrapper-id",
                    state: NMessageState.Created,
                    event: {
                        id: "msg-1",
                        kind: 9,
                        pubkey: "other-pubkey",
                        content: "Hello world",
                        created_at: 1000,
                        tags: [],
                        sig: "test-sig",
                    },
                },
                tokens: [{ Text: "Hello world" }],
            };
            chatStore.handleMessage(messageEvent);

            const group = createTestGroup();
            const result = await chatStore.deleteMessage(group, "msg-1");

            expect(result).toBeNull();
            expect(tauri.invoke).not.toHaveBeenCalled();
        });
    });

    describe("payLightningInvoice", () => {
        it("calls the expected tauri command and handles the payment response", async () => {
            const invoiceMessageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            invoiceMessageEvent.message.tags.push([
                "bolt11",
                "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                "21000",
                "Bitdevs pizza",
            ]);
            chatStore.handleMessage(invoiceMessageEvent);
            const invoiceMessage = chatStore.findChatMessage("msg-1");
            expect(invoiceMessage?.lightningInvoice).toEqual({
                invoice:
                    "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
                amount: 21,
                description: "Bitdevs pizza",
                isPaid: false,
            });
            const paymentResponse: import("$lib/types/nostr").MessageWithTokens = {
                message: {
                    event_id: "payment-1",
                    kind: 9,
                    content: "Payment sent",
                    created_at: 1000,
                    mls_group_id: createTestMlsGroupId(),
                    pubkey: "user-pubkey",
                    tags: [],
                    wrapper_event_id: "test-wrapper-id",
                    state: NMessageState.Created,
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
                },
                tokens: [{ Text: "Payment sent" }],
            };

            vi.spyOn(tauri, "invoke")
                .mockResolvedValueOnce(["test-relay"]) // for get_group_relays
                .mockResolvedValueOnce(paymentResponse); // for pay_invoice

            const group = createTestGroup();
            const chatMessage = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const result = await chatStore.payLightningInvoice(group, chatMessage!);

            expect(tauri.invoke).toHaveBeenCalledWith("pay_invoice", {
                group,
                tags: [["q", "msg-1", "test-relay", "user-pubkey"]],
                bolt11: "lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte",
            });

            expect(result).toEqual(paymentResponse);
        });

        it("returns null if message has no lightning invoice", async () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);

            const group = createTestGroup();
            const chatMessage = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            const result = await chatStore.payLightningInvoice(group, chatMessage!);

            expect(result).toBeNull();
            expect(tauri.invoke).not.toHaveBeenCalled();
        });

        it("updates lightning invoice to paid after successful payment", async () => {
            const messageEvent = createMessageEvent("msg-1", "Please pay me", 1000);
            messageEvent.message.tags.push(["bolt11", "lnbc123456789", "21000", "Test payment"]);
            chatStore.handleMessage(messageEvent);

            const paymentResponse: import("$lib/types/nostr").MessageWithTokens = {
                message: {
                    event_id: "payment-1",
                    kind: 9,
                    content: "Payment sent",
                    created_at: 1000,
                    mls_group_id: createTestMlsGroupId(),
                    pubkey: "user-pubkey",
                    tags: [],
                    wrapper_event_id: "test-wrapper-id",
                    state: NMessageState.Created,
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
                },
                tokens: [{ Text: "Payment sent" }],
            };

            vi.spyOn(tauri, "invoke")
                .mockResolvedValueOnce(["test-relay"]) // for get_group_relays
                .mockResolvedValueOnce(paymentResponse); // for pay_invoice

            const group = createTestGroup();
            const chatMessage = chatStore.findChatMessage("msg-1");
            // biome-ignore lint/style/noNonNullAssertion: This is a test file where we control the data
            await chatStore.payLightningInvoice(group, chatMessage!);
            const updatedChatMessage = chatStore.findChatMessage("msg-1");
            expect(updatedChatMessage?.lightningInvoice?.isPaid).toBe(true);
        });
    });

    describe("isMessageDeletable", () => {
        it("returns true for message that is mine and not deleted", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            expect(chatStore.isMessageDeletable("msg-1")).toBe(true);
        });

        it("returns false for a non-existent message", () => {
            expect(chatStore.isMessageDeletable("non-existent")).toBe(false);
        });

        it("returns false for a message that is already deleted", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const message = createDeletionMessage("deletion-1", "msg-1", 2000);
            chatStore.handleMessage(message);
            expect(chatStore.isMessageDeletable("msg-1")).toBe(false);
        });

        it("returns false for a message that is not mine", () => {
            const messageEvent: import("$lib/types/nostr").MessageWithTokens = {
                message: {
                    event_id: "msg-1",
                    kind: 9,
                    content: "Hello world",
                    created_at: 1000,
                    mls_group_id: createTestMlsGroupId(),
                    pubkey: "other-pubkey",
                    tags: [],
                    wrapper_event_id: "test-wrapper-id",
                    state: NMessageState.Created,
                    event: {
                        id: "msg-1",
                        kind: 9,
                        pubkey: "other-pubkey",
                        content: "Hello world",
                        created_at: 1000,
                        tags: [],
                        sig: "test-sig",
                    },
                },
                tokens: [{ Text: "Hello world" }],
            };

            chatStore.handleMessage(messageEvent);
            expect(chatStore.isMessageDeletable("msg-1")).toBe(false);
        });
    });

    describe("isMessageCopyable", () => {
        it("returns true for an existing message", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            expect(chatStore.isMessageCopyable("msg-1")).toBe(true);
        });

        it("returns false for a non-existent message", () => {
            expect(chatStore.isMessageCopyable("non-existent")).toBe(false);
        });

        it("returns false for a deleted message", () => {
            const messageEvent = createMessageEvent("msg-1", "Hello world", 1000);
            chatStore.handleMessage(messageEvent);
            const message = createDeletionMessage("deletion-1", "msg-1", 2000);
            chatStore.handleMessage(message);
            expect(chatStore.isMessageCopyable("msg-1")).toBe(false);
        });
    });
});
