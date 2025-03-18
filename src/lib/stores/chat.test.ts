import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import type { ChatState, Message, ReactionSummary } from '$lib/types/chat';
import type { NEvent, NostrMlsGroup, NostrMlsGroupWithRelays } from '$lib/types/nostr';
import { NostrMlsGroupType } from '$lib/types/nostr';
import * as tauri from '@tauri-apps/api/core';
import { activeAccount, type Account } from './accounts';
import { createChatStore } from './chat';

vi.spyOn(tauri, 'invoke').mockImplementation(async () => null);

const userAccount: Account = {
  pubkey: 'user-pubkey',
  metadata: {},
  nostr_relays: [],
  inbox_relays: [],
  key_package_relays: [],
  mls_group_ids: [],
  settings: {
    darkTheme: false,
    devMode: false,
    lockdownMode: false
  },
  onboarding: {
    inbox_relays: false,
    key_package_relays: false,
    publish_key_package: false
  },
  last_used: Date.now(),
  active: true
};

const createMessageEvent = (id: string, content: string, createdAt: number, replyToId?: string): NEvent => {
  const tags: string[][] = [];
  if (replyToId) {
    tags.push(['q', replyToId]);
  }
  
  return {
    id,
    pubkey: 'user-pubkey',
    kind: 9,
    content,
    created_at: createdAt,
    tags,
    sig: 'test-sig'
  };
};

const createReactionEvent = (id: string, content: string, createdAt: number, targetId: string): NEvent => {
  return {
    id,
    pubkey: 'other-pubkey',
    kind: 7,
    content,
    created_at: createdAt,
    tags: [['e', targetId], ['p', 'user-pubkey']],
    sig: 'test-sig'
  };
};

const createDeletionEvent = (id: string, targetId: string, createdAt: number): NEvent => {
  return {
    id,
    pubkey: 'user-pubkey',
    kind: 5,
    content: '',
    created_at: createdAt,
    tags: [['e', targetId]],
    sig: 'test-sig'
  };
};

const createTestGroup = (): NostrMlsGroup => {
  return {
    mls_group_id: new Uint8Array([1, 2, 3, 4]),
    nostr_group_id: 'test-group-id',
    name: 'Test Group',
    description: 'A test group',
    admin_pubkeys: ['user-pubkey'],
    last_message_at: Date.now(),
    last_message_id: 'last-message-id',
    group_type: NostrMlsGroupType.Group
  };
};

describe('Chat Store', () => {
  let chatStore: ReturnType<typeof createChatStore>;
  let originalAccount: Account | null;


  beforeEach(() => {
    originalAccount = get(activeAccount);
    activeAccount.set(userAccount);
    vi.clearAllMocks();
    vi.spyOn(tauri, 'invoke').mockImplementation(async () => null);
    chatStore = createChatStore();
  });

  afterEach(() => {
    activeAccount.set(originalAccount);
    vi.restoreAllMocks();
  });

  describe('handleEvent', () => {
    describe('with message event', () => {
      it('saves the message', () => {
        const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
        
        chatStore.handleEvent(messageEvent);
        
        const state = get(chatStore) as ChatState;
        expect(state.messages).toEqual([{
          id: 'msg-1',
          pubkey: 'user-pubkey',
          replyToId: undefined,
          content: 'Hello world',
          createdAt: 1000,
          reactions: [],
          isMine: true,
          isSingleEmoji: false,
          lightningInvoice: undefined,
          lightningPayment: undefined,
          event: messageEvent
        }])
      });

      describe('when event has bolt 11 tag', () => {
        it('saves message with lightning invoice', () => {
          const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
          messageEvent.tags.push(['bolt11', 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte', '21000', 'Bitdevs pizza']);
          chatStore.handleEvent(messageEvent);
          const state = get(chatStore) as ChatState;
          expect(state.messages).toEqual([{
            id: 'msg-1',
            pubkey: 'user-pubkey',
            replyToId: undefined,
            content: 'Hello world',
            createdAt: 1000,
            reactions: [],
            isMine: true,
            isSingleEmoji: false,
            lightningInvoice: {
              amount: 21,
              description: 'Bitdevs pizza',
              invoice: 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte',
              isPaid: false,
            },
            lightningPayment: undefined,
            event: messageEvent
          }])
        })
      })

      describe('when event has preimage tag', () => {
        describe('when replying to a lightning invoice', () => {
          it('saves message with lightning payment paid', () => {
            const invoiceMessageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
            invoiceMessageEvent.tags.push(['bolt11', 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte', '21000', 'Bitdevs pizza']);
            chatStore.handleEvent(invoiceMessageEvent);
            const paymentMessageEvent = createMessageEvent('msg-2', '', 2000, 'msg-1');
            paymentMessageEvent.tags.push(['preimage', 'preimage-1']);
            chatStore.handleEvent(paymentMessageEvent);
            const paymentMessage = chatStore.findMessage('msg-2');

            expect(paymentMessage).toEqual(
              {
                id: 'msg-2',
                pubkey: 'user-pubkey',
                replyToId: 'msg-1',
                content: '',
                createdAt: 2000,
                reactions: [],
                isMine: true,
                isSingleEmoji: false,
                lightningInvoice: undefined,
                lightningPayment: {
                  preimage: 'preimage-1',
                  isPaid: true,
                },
                event: paymentMessageEvent
              }
            );
          })
          it('updates the lightning invoice to paid', () => {
            const invoiceMessageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
            invoiceMessageEvent.tags.push(['bolt11', 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte', '21000', 'Bitdevs pizza']);
            chatStore.handleEvent(invoiceMessageEvent);
            const paymentMessageEvent = createMessageEvent('msg-2', '', 2000, 'msg-1');
            paymentMessageEvent.tags.push(['preimage', 'preimage-1']);
            chatStore.handleEvent(paymentMessageEvent);
            const invoiceMessage = chatStore.findMessage('msg-1');
            expect(invoiceMessage).toEqual(
              {
                id: 'msg-1',
                pubkey: 'user-pubkey',
                replyToId: undefined,
                content: 'Hello world',
                createdAt: 1000,
                reactions: [],
                isMine: true,
                isSingleEmoji: false,
                lightningInvoice: {
                  amount: 21,
                  description: 'Bitdevs pizza',
                  invoice: 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte',
                  isPaid: true,
                },
                lightningPayment: undefined,
                event: invoiceMessageEvent
              }
            )
          })
        })
        describe('without reply to lightning invoice', () => {
          it('saves message with lightning payment but not paid', () => {
            const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
            messageEvent.tags.push(['preimage', 'preimage-1']);
            chatStore.handleEvent(messageEvent);
            const state = get(chatStore) as ChatState;
            expect(state.messages).toEqual([{
              id: 'msg-1',
              pubkey: 'user-pubkey',
              replyToId: undefined,
              content: 'Hello world',
              createdAt: 1000,
              reactions: [],
              isMine: true,
              isSingleEmoji: false,
              lightningInvoice: undefined,
              lightningPayment: {
                preimage: 'preimage-1',
                isPaid: false,
              },
              event: messageEvent
            }])
          })
        })
      })
    });

    describe('with reaction event', () => {
      it('saves the reaction in target message', () => {
        const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
        chatStore.handleEvent(messageEvent);
        const reactionEvent = createReactionEvent('reaction-1', '👍', 1000, 'msg-1');
        chatStore.handleEvent(reactionEvent);
        const message = chatStore.findMessage('msg-1');
        expect(message!.reactions).toEqual([{
          id: 'reaction-1',
          pubkey: 'other-pubkey',
          content: '👍',
          targetId: 'msg-1',
          createdAt: 1000,
          isMine: false,
          event: reactionEvent,
        }]);
      });
    });

    describe('with deletion event', () => {
      describe('when deleting a message', () => {
        it('saves deletion', () => {
          const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
          chatStore.handleEvent(messageEvent);
          const deletionEvent = createDeletionEvent('deletion-1', 'msg-1', 2000);
          chatStore.handleEvent(deletionEvent);
          expect(chatStore.isDeleted('msg-1')).toBe(true);
        });
      });

      describe('when deleting a reaction', () => {
        it('saves deletion', () => {
          const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
          chatStore.handleEvent(messageEvent);
          const reactionEvent = createReactionEvent('reaction-1', '👍', 2000, 'msg-1');
          chatStore.handleEvent(reactionEvent);
          const deletionEvent = createDeletionEvent('deletion-2', 'reaction-1', 3000);
          chatStore.handleEvent(deletionEvent);
          expect(chatStore.isDeleted('reaction-1')).toBe(true);
        })
      })
    });
  });

  describe('handleEvents', () => {
    it('handles multiple events in the correct order', () => {
      const firstMessageEvent = createMessageEvent('msg-1', 'First message', 1000);
      const reactionEvent = createReactionEvent('reaction-1', '👍', 1500, 'msg-1');
      const deletionEvent = createDeletionEvent('deletion-1', 'msg-1', 2000);
      const secondMessageEvent = createMessageEvent('msg-2', 'Second message', 2500);
      const events: NEvent[] = [reactionEvent, deletionEvent, secondMessageEvent, firstMessageEvent]
      chatStore.handleEvents(events);
      
      const state = get(chatStore) as ChatState;
      expect(state.messages).toEqual([{
        id: 'msg-1',
        pubkey: 'user-pubkey',
        replyToId: undefined,
        content: 'First message',
        createdAt: 1000,
        reactions: [
          {
            id: 'reaction-1',
            pubkey: 'other-pubkey',
            content: '👍',
            targetId: 'msg-1',
            createdAt: 1500,
            isMine: false,
            event: reactionEvent
          }
        ],
        isMine: true,
        isSingleEmoji: false,
        lightningInvoice: undefined,
        lightningPayment: undefined,
        event: firstMessageEvent
      }, {
        id: 'msg-2',
        pubkey: 'user-pubkey',
        replyToId: undefined,
        content: 'Second message',
        createdAt: 2500,
        reactions: [],
        isMine: true,
        isSingleEmoji: false,
        lightningInvoice: undefined,
        lightningPayment: undefined,
        event: secondMessageEvent
      }])
      expect(chatStore.isDeleted('msg-1')).toBe(true);
    });
  });

  describe('clear', () => {
    it('clears messages', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      expect(get(chatStore).messages).toHaveLength(1);
      chatStore.clear();
      expect(get(chatStore).messages).toHaveLength(0);
    });

    it('clears reactions', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const reactionEvent = createReactionEvent('reaction-1', '👍', 1500, 'msg-1');
      chatStore.handleEvent(reactionEvent);
      const oldMessage = chatStore.findMessage('msg-1');
      expect(oldMessage?.reactions).toHaveLength(1);
      chatStore.clear();
      chatStore.handleEvent(messageEvent);
      const newMessage = chatStore.findMessage('msg-1');
      expect(newMessage?.reactions).toEqual([]);
    });

    it('clears deletions', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const deletionEvent = createDeletionEvent('deletion-1', 'msg-1', 2000);
      chatStore.handleEvent(deletionEvent);
      expect(chatStore.isDeleted('msg-1')).toBe(true);
      chatStore.clear();
      chatStore.handleEvent(messageEvent);
      expect(chatStore.isDeleted('msg-1')).toBe(false);
    });


  });

  describe('findMessage', () => {
    it('finds a message by id', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const message = chatStore.findMessage('msg-1');
      expect(message).toEqual({
        id: 'msg-1',
        pubkey: 'user-pubkey',
        replyToId: undefined,
        content: 'Hello world',
        createdAt: 1000,
        isMine: true,
        isSingleEmoji: false,
        lightningInvoice: undefined,
        lightningPayment: undefined,
        event: messageEvent,
        reactions: []
      });
    });

    it('returns undefined for a non-existent message', () => {
      const message = chatStore.findMessage('non-existent');
      
      expect(message).toBeUndefined();
    });
  });

  describe('findReplyToMessage', () => {
    it('finds the parent message of a reply', () => {
      const parentMessageEvent = createMessageEvent('parent-msg', 'Parent message', 1000);
      chatStore.handleEvent(parentMessageEvent);
      const replyMessageEvent = createMessageEvent('reply-msg', 'Reply message', 1100, 'parent-msg');
      chatStore.handleEvent(replyMessageEvent);
      const replyMessage = chatStore.findMessage('reply-msg');
      const parentMessage = chatStore.findReplyToMessage(replyMessage!);
      
      expect(parentMessage).toEqual({
        id: 'parent-msg',
        pubkey: 'user-pubkey',
        replyToId: undefined,
        content: 'Parent message',
        createdAt: 1000,
        isMine: true,
        isSingleEmoji: false,
        lightningInvoice: undefined,
        lightningPayment: undefined,
        event: parentMessageEvent,
        reactions: []
      });
    });

    it('returns undefined if the message has no reply-to', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const message = chatStore.findMessage('msg-1');
      const replyToMessage = chatStore.findReplyToMessage(message!);
      
      expect(replyToMessage).toBeUndefined();
    });

    it('returns undefined if the parent message does not exist', () => {
      const replyMessageEvent = createMessageEvent('reply-msg', 'Reply message', 1100, 'non-existent-parent');
      chatStore.handleEvent(replyMessageEvent);
      const replyMessage = chatStore.findMessage('reply-msg');
      
      expect(chatStore.findReplyToMessage(replyMessage!)).toBeUndefined();
    });
  });

  describe('getMessageReactionsSummary', () => {
    it('returns a summary of reactions for a message', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      
      chatStore.handleEvents(
        [
          createReactionEvent('reaction-1', '👍', 1000, 'msg-1'),
          createReactionEvent('reaction-2', '👍', 2000, 'msg-1'),
          createReactionEvent('reaction-3', '❤️', 3000, 'msg-1')
        ]
      );
      const summary = chatStore.getMessageReactionsSummary('msg-1');
      
      expect(summary).toEqual([
        {
          emoji: '👍',
          count: 2
        },
        {
          emoji: '❤️',
          count: 1
        }
      ])
    });

    it('excludes deleted reactions from the summary', () => {
      chatStore.handleEvents(
        [
          createMessageEvent('msg-1', 'Hello world', 1000),
          createReactionEvent('reaction-1', '👍', 1000, 'msg-1'),
          createReactionEvent('reaction-2', '👍', 2000, 'msg-1'),
          createReactionEvent('reaction-3', '❤️', 3000, 'msg-1'),
          createDeletionEvent('deletion-1', 'reaction-1', 3000),
        ]
      );
      const summary = chatStore.getMessageReactionsSummary('msg-1');
      
      expect(summary).toEqual([
        {
          emoji: '👍',
          count: 1
        },
        {
          emoji: '❤️',
          count: 1
        }
      ])
    });

    it('when all reactions are deleted, returns an empty array', () => {
      chatStore.handleEvents(
        [
          createMessageEvent('msg-1', 'Hello world', 1000),
          createReactionEvent('reaction-1', '👍', 1000, 'msg-1'),
          createReactionEvent('reaction-2', '👍', 2000, 'msg-1'),
          createReactionEvent('reaction-3', '❤️', 3000, 'msg-1'),
          createDeletionEvent('deletion-1', 'reaction-1', 3000),
          createDeletionEvent('deletion-2', 'reaction-2', 3500),
          createDeletionEvent('deletion-3', 'reaction-3', 4000),
        ]
      );
      const summary = chatStore.getMessageReactionsSummary('msg-1');
      expect(summary).toEqual([]);
    })
  });

  describe('hasReactions', () => {
    it('returns true for a message with active reactions', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const reactionEvent = createReactionEvent('reaction-1', '👍', 2000, 'msg-1');
      chatStore.handleEvent(reactionEvent);
      
      const message = chatStore.findMessage('msg-1')!;
      expect(chatStore.hasReactions(message)).toBe(true);
    });

    it('returns false for a message with no reactions', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      
      const message = chatStore.findMessage('msg-1')!;
      expect(chatStore.hasReactions(message)).toBe(false);
    });

    it('returns false when all reactions are deleted', () => {
      chatStore.handleEvents([
        createMessageEvent('msg-1', 'Hello world', 1000),
        createReactionEvent('reaction-1', '👍', 2000, 'msg-1'),
        createDeletionEvent('deletion-1', 'reaction-1', 3000)
      ]);
      
      const message = chatStore.findMessage('msg-1')!;
      expect(chatStore.hasReactions(message)).toBe(false);
    });

    it('returns true when some reactions remain after deletions', () => {
      chatStore.handleEvents([
        createMessageEvent('msg-1', 'Hello world', 1000),
        createReactionEvent('reaction-1', '👍', 2000, 'msg-1'),
        createReactionEvent('reaction-2', '❤️', 3000, 'msg-1'),
        createDeletionEvent('deletion-1', 'reaction-1', 4000)
      ]);
      
      const message = chatStore.findMessage('msg-1')!;
      expect(chatStore.hasReactions(message)).toBe(true);
    });
  });

  describe('clickReaction', () => {
    it('returns null if message is not found', async () => {
      const group = createTestGroup();
      const result = await chatStore.clickReaction(group, '👍', 'non-existent');
      
      expect(result).toBeNull();
      expect(tauri.invoke).not.toHaveBeenCalled();
    });

    describe('without user reaction', () => {
      beforeEach(() => {
        const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
        chatStore.handleEvent(messageEvent);
        const reactionResponse: NEvent = {
          id: 'reaction-1', 
          pubkey: 'user-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1001, 
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        };
        
        vi.spyOn(tauri, 'invoke').mockResolvedValueOnce(reactionResponse);
      })
      it('calls the expected tauri command to add reaction', async () => {
        const group = createTestGroup();
        await chatStore.clickReaction(group, '👍', 'msg-1');
        expect(tauri.invoke).toHaveBeenCalledWith('send_mls_message', {
          group,
          message: '👍',
          kind: 7,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']]
        });
      });
      it('returns the new reaction response', async () => {
        const group = createTestGroup();
        const result = await chatStore.clickReaction(group, '👍', 'msg-1');
        expect(result).toEqual({
          id: 'reaction-1', 
          pubkey: 'user-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1001,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        });
      });
    });
    
    describe('with different user reaction with same content', () => {
      let group: NostrMlsGroup;
      let messageEvent: NEvent;
      let firstReactionResponse: NEvent;
      const otherUserAccount: Account = { ...userAccount, pubkey: 'other-pubkey' };
      let otherUserChatStore: ReturnType<typeof createChatStore>;
      
      beforeEach(async () => {
        group = createTestGroup();
        messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
        chatStore.handleEvent(messageEvent);
        firstReactionResponse = {
          id: 'reaction-1', 
          pubkey: 'user-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1001, 
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        };
        
        vi.spyOn(tauri, 'invoke').mockResolvedValueOnce(firstReactionResponse);
        await chatStore.clickReaction(group, '👍', 'msg-1');
        activeAccount.set(otherUserAccount);
        otherUserChatStore = createChatStore();
        otherUserChatStore.handleEvent(messageEvent);
        otherUserChatStore.handleEvent(firstReactionResponse);
        
        const secondReactionResponse: NEvent = {
          id: 'reaction-2', 
          pubkey: 'other-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1002, 
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        };
        vi.spyOn(tauri, 'invoke').mockResolvedValueOnce(secondReactionResponse);
      });
      
      it('calls the expected tauri command to add reaction', async () => {
        await otherUserChatStore.clickReaction(group, '👍', 'msg-1');
        expect(tauri.invoke).toHaveBeenCalledWith('send_mls_message', {
          group,
          message: '👍',
          kind: 7,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']]
        });
      });
      
      it('returns the new reaction response', async () => {        
        const result = await otherUserChatStore.clickReaction(group, '👍', 'msg-1');
        expect(result).toEqual({
          id: 'reaction-2', 
          pubkey: 'other-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1002,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        });
      });
    });

    describe('with deleted user reaction with same content', () => {
      let group: NostrMlsGroup;
      let messageEvent: NEvent;
      
      beforeEach(async () => {
        group = createTestGroup();
        messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
        chatStore.handleEvent(messageEvent);
        const firstReactionResponse: NEvent = {
          id: 'reaction-1', 
          pubkey: 'user-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1001, 
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        };        
        vi.spyOn(tauri, 'invoke').mockResolvedValueOnce(firstReactionResponse);
        await chatStore.clickReaction(group, '👍', 'msg-1');
        const deletionResponse: NEvent = {
          id: 'deletion-1', 
          pubkey: 'user-pubkey', 
          kind: 5, 
          content: '', 
          created_at: 1002, 
          tags: [['e', 'reaction-1']],
          sig: 'test-sig'
        };
        
        vi.spyOn(tauri, 'invoke').mockImplementation(async () => deletionResponse);
        await chatStore.clickReaction(group, '👍', 'msg-1');
        const secondReactionResponse: NEvent = {
          id: 'reaction-2', 
          pubkey: 'user-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1003,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        };
        vi.spyOn(tauri, 'invoke').mockResolvedValueOnce(secondReactionResponse);
      });
      
      it('calls the expected tauri command to add reaction', async () => {
        await chatStore.clickReaction(group, '👍', 'msg-1');
        expect(tauri.invoke).toHaveBeenCalledWith('send_mls_message', {
          group,
          message: '👍',
          kind: 7,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']]
        });
      });
      
      it('returns the new reaction response', async () => {        
        const result = await chatStore.clickReaction(group, '👍', 'msg-1');
        expect(result).toEqual({
          id: 'reaction-2', 
          pubkey: 'user-pubkey', 
          kind: 7, 
          content: '👍', 
          created_at: 1003,
          tags: [['e', 'msg-1'], ['p', 'user-pubkey']],
          sig: 'test-sig'
        });
      });

      it('keeps old reaction as deleted', async () => {        
        await chatStore.clickReaction(group, '👍', 'msg-1');
        expect(chatStore.isDeleted('reaction-1')).toBe(true);
      });

      it('does not delete new reaction', async () => {        
        await chatStore.clickReaction(group, '👍', 'msg-1');
        expect(chatStore.isDeleted('reaction-2')).toBe(false);
      });
    });
  });

  describe('deleteMessage', () => {
    it('calls the expected tauri command and handles the response', async () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const deletionResponse: NEvent = {
        id: 'deletion-1', 
        pubkey: 'user-pubkey', 
        kind: 5, 
        content: '', 
        created_at: 1002, 
        tags: [['e', 'reaction-1']],
        sig: 'test-sig'
      };
      
      vi.spyOn(tauri, 'invoke').mockImplementation(async () => deletionResponse);
      
      const group = createTestGroup();
      const result = await chatStore.deleteMessage(group, 'msg-1');
      expect(tauri.invoke).toHaveBeenCalledWith('delete_message', {
        group,
        messageId: 'msg-1',
      });
      expect(result).toEqual(deletionResponse);
    });

    it('returns null if message is not found', async () => {
      const group = createTestGroup();
      const result = await chatStore.deleteMessage(group, 'non-existent');
      
      expect(result).toBeNull();
      expect(tauri.invoke).not.toHaveBeenCalled();
    });
  });

  describe('payLightningInvoice', () => {
    it('calls the expected tauri command and handles the payment response', async () => {
      const invoiceMessageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      invoiceMessageEvent.tags.push(['bolt11', 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte', '21000', 'Bitdevs pizza']);
      chatStore.handleEvent(invoiceMessageEvent);
      const invoiceMessage = chatStore.findMessage('msg-1')!;
      expect(invoiceMessage.lightningInvoice).toEqual({
        invoice: 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte',
        amount: 21,
        description: 'Bitdevs pizza',
        isPaid: false
      });
      const paymentResponse: NEvent = {
        id: 'payment-1', 
        pubkey: 'user-pubkey', 
        kind: 9, 
        content: 'Payment sent', 
        created_at: 1002, 
        tags: [['q', 'msg-1', 'test-relay', 'user-pubkey'], ['preimage', 'test-preimage']],
        sig: 'test-sig'
      };
      
      vi.spyOn(tauri, 'invoke').mockImplementation(async () => paymentResponse);
      
      const groupWithRelays : NostrMlsGroupWithRelays = {
        group: createTestGroup(),
        relays: ['test-relay']
      };
      
      const result = await chatStore.payLightningInvoice(groupWithRelays, invoiceMessage);
      
      expect(tauri.invoke).toHaveBeenCalledWith('pay_invoice', {
        group: groupWithRelays.group,
        tags: [['q', 'msg-1', 'test-relay', 'user-pubkey']],
        bolt11: 'lntbs210n1pnu7rc4dqqnp4qg094pqgshvyfsltrck5lkdw5negkn3zwe36ukdf8zhwfc2h5ay6spp5rfrpyaypdh8jpw2vptz5zrna7k68zz4npl7nrjdxqav2zfeu02cqsp5qw2sue0k56dytxvn7fnyl3jn044u6xawc7gzkxh65ftfnkyf5tds9qyysgqcqpcxqyz5vqs24aglvyr5k79da9aparklu7dr767krnapz7f9zp85mjd29m747quzpkg6x5hk42xt6z5eell769emk9mvr4wt8ftwz08nenx2fnl7cpfv0cte',
      });
      
      expect(result).toEqual(paymentResponse);
    });

    it('returns null if message has no lightning invoice', async () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      
      const groupWithRelays = {
        group: createTestGroup(),
        relays: ['test-relay']
      };
      
      const message = chatStore.findMessage('msg-1')!;
      
      const result = await chatStore.payLightningInvoice(groupWithRelays, message);
      
      expect(result).toBeNull();
      expect(tauri.invoke).not.toHaveBeenCalled();
    });

    it('updates lightning invoice to paid after successful payment', async () => {
      const messageEvent = createMessageEvent('msg-1', 'Please pay me', 1000);
      messageEvent.tags.push(['bolt11', 'lnbc123456789', '21000', 'Test payment']);
      chatStore.handleEvent(messageEvent);
      
      const paymentResponse: NEvent = {
        id: 'payment-1', 
        pubkey: 'user-pubkey', 
        kind: 9, 
        content: 'Payment sent', 
        created_at: 1002, 
        tags: [['q', 'msg-1', 'test-relay', 'user-pubkey'], ['preimage', 'test-preimage']],
        sig: 'test-sig'
      };
      
      vi.spyOn(tauri, 'invoke').mockImplementation(async () => paymentResponse);
      
      const groupWithRelays = {
        group: createTestGroup(),
        relays: ['test-relay']
      };
      
      const message = chatStore.findMessage('msg-1');
      await chatStore.payLightningInvoice(groupWithRelays, message!);
      const updatedMessage = chatStore.findMessage('msg-1')!;
      
      expect(updatedMessage.lightningInvoice!.isPaid).toBe(true);
    });
  });

  describe('isMessageDeletable', () => {
    it('returns true for message that is mine and not deleted', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      expect(chatStore.isMessageDeletable('msg-1')).toBe(true);
    });

    it('returns false for a non-existent message', () => {
      expect(chatStore.isMessageDeletable('non-existent')).toBe(false);
    });

    it('returns false for a message that is already deleted', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const deletionEvent = createDeletionEvent('deletion-1', 'msg-1', 2000);
      chatStore.handleEvent(deletionEvent);
      expect(chatStore.isMessageDeletable('msg-1')).toBe(false);
    });

    it('returns false for a message that is not mine', () => {
      const messageEvent: NEvent = {
        id: 'msg-1',
        pubkey: 'other-pubkey',
        kind: 9,
        content: 'Hello world',
        created_at: 1000,
        tags: [],
        sig: 'test-sig'
      };
      
      chatStore.handleEvent(messageEvent);
      expect(chatStore.isMessageDeletable('msg-1')).toBe(false);
    });
  });

  describe('isMessageCopyable', () => {
    it('returns true for an existing message', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      expect(chatStore.isMessageCopyable('msg-1')).toBe(true);
    });

    it('returns false for a non-existent message', () => {
      expect(chatStore.isMessageCopyable('non-existent')).toBe(false);
    });

    it('returns false for a deleted message', () => {
      const messageEvent = createMessageEvent('msg-1', 'Hello world', 1000);
      chatStore.handleEvent(messageEvent);
      const deletionEvent = createDeletionEvent('deletion-1', 'msg-1', 2000);
      chatStore.handleEvent(deletionEvent);
      expect(chatStore.isMessageCopyable('msg-1')).toBe(false);
    });
  });
}); 
