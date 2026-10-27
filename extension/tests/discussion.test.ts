import { describe, expect, it } from 'vitest';
import {
  ApiError,
  type Discussion,
  type Identity,
  type Message,
  type MessagePage,
  type YapSpaceApi,
} from '../src/lib/api/client';
import { DiscussionState, MAX_IN_MEMORY } from '../src/lib/store/discussion.svelte';

const DISCUSSION: Discussion = {
  id: 'd-1',
  display_url: 'https://example.com/a',
  site: 'example.com',
  title: null,
  status: 'open',
  message_count: 0,
  last_seq: 0,
  created_at: '2026-10-27T10:00:00.000Z',
  last_message_at: null,
  merged_from: null,
};

const IDENTITY: Identity = { kind: 'url', display_url: 'https://example.com/a', site: 'example.com', ruleset: 1 };

function message(seq: number, body = `message ${seq}`): Message {
  return {
    id: `m-${seq}`,
    discussion_id: 'd-1',
    seq,
    author: { id: 'u-1', username: 'alice' },
    parent: null,
    body,
    status: 'visible',
    created_at: '2026-10-27T10:00:00.000Z',
    edited_at: null,
  };
}

function page(items: Message[], hasMore = false): MessagePage {
  return {
    items,
    has_more: hasMore,
    next_before_seq: hasMore ? (items[0]?.seq ?? null) : null,
    next_after_seq: null,
  };
}

/** A scripted API. Each test sets the answers it needs. */
function fakeApi(overrides: Partial<YapSpaceApi> = {}): YapSpaceApi {
  const base: YapSpaceApi = {
    resolve: async () => ({ identity: IDENTITY, discussion: DISCUSSION }),
    createDiscussion: async () => DISCUSSION,
    listMessages: async () => page([message(1), message(2)]),
    postMessage: async (_id, body, clientMsgId) => ({ ...message(3, body), id: `m-${clientMsgId}` }),
    deleteMessage: async () => undefined,
    register: async () => {
      throw new Error('not used');
    },
    login: async () => {
      throw new Error('not used');
    },
    logout: async () => undefined,
    me: async () => {
      throw new Error('not used');
    },
  };
  return { ...base, ...overrides };
}

const CONTEXT = { url: 'https://example.com/a', canonical: null, title: 'A' };

describe('DiscussionState.load', () => {
  it('shows the newest messages of an existing discussion', async () => {
    const state = new DiscussionState();
    const id = await state.load(fakeApi(), CONTEXT);
    expect(id).toBe('d-1');
    expect(state.phase).toBe('ready');
    expect(state.ordered.map((m) => m.seq)).toEqual([1, 2]);
    expect(state.lastSeq).toBe(2);
  });

  it('shows an empty state when nobody has started the discussion', async () => {
    const state = new DiscussionState();
    const id = await state.load(fakeApi({ resolve: async () => ({ identity: IDENTITY, discussion: null }) }), CONTEXT);
    expect(id).toBeNull();
    expect(state.phase).toBe('none');
  });

  it('marks unsupported pages from the server reason', async () => {
    const state = new DiscussionState();
    const api = fakeApi({
      resolve: async () => {
        throw new ApiError({ status: 422, code: 'url_unsupported', message: 'nope', reason: 'blocked_host' });
      },
    });
    await state.load(api, CONTEXT);
    expect(state.phase).toBe('unsupported');
    expect(state.reason).toBe('blocked_host');
  });

  it('does not show unsupported pages without calling the server', async () => {
    const state = new DiscussionState();
    let called = false;
    await state.load(
      fakeApi({
        resolve: async () => {
          called = true;
          return { identity: IDENTITY, discussion: null };
        },
      }),
      { url: null, canonical: null, title: '' },
    );
    expect(called).toBe(false);
    expect(state.phase).toBe('unsupported');
  });

  it('drops an answer for an older page when a newer load has started', async () => {
    const state = new DiscussionState();
    let releaseFirst: () => void = () => undefined;
    const slowFirst = new Promise<void>((resolve) => (releaseFirst = resolve));
    const api = fakeApi({
      resolve: async (url) => {
        if (url.endsWith('/old')) {
          await slowFirst;
          return { identity: IDENTITY, discussion: { ...DISCUSSION, id: 'old' } };
        }
        return { identity: IDENTITY, discussion: { ...DISCUSSION, id: 'new' } };
      },
      listMessages: async (id) => page([message(id === 'old' ? 9 : 1)]),
    });
    const first = state.load(api, { url: 'https://example.com/old', canonical: null, title: '' });
    await state.load(api, { url: 'https://example.com/new', canonical: null, title: '' });
    releaseFirst();
    await first;
    expect(state.discussion?.id).toBe('new');
    expect(state.ordered.map((m) => m.seq)).toEqual([1]);
  });
});

describe('DiscussionState messages', () => {
  it('stores a message once, however many times it arrives', async () => {
    const state = new DiscussionState();
    await state.load(fakeApi(), CONTEXT);
    state.applyMessage(message(2));
    state.applyMessage(message(3));
    state.applyMessage(message(3));
    expect(state.ordered.map((m) => m.seq)).toEqual([1, 2, 3]);
  });

  it('posts with a client id, and the id stays the same across retries', async () => {
    const state = new DiscussionState();
    await state.load(fakeApi(), CONTEXT);
    const seen: string[] = [];
    let failures = 1;
    const api = fakeApi({
      postMessage: async (_id, body, clientMsgId) => {
        seen.push(clientMsgId);
        if (failures > 0) {
          failures -= 1;
          throw new ApiError({ status: 0, code: 'network', message: 'offline' });
        }
        return { ...message(3, body), id: 'm-3' };
      },
    });
    await state.post(api, 'hello');
    expect(seen).toHaveLength(2);
    expect(seen[0]).toBe(seen[1]);
    expect(state.pending.size).toBe(0);
    expect(state.ordered.at(-1)?.body).toBe('hello');
  });

  it('keeps a failed message as pending and does not retry a client error', async () => {
    const state = new DiscussionState();
    await state.load(fakeApi(), CONTEXT);
    let calls = 0;
    const api = fakeApi({
      postMessage: async () => {
        calls += 1;
        throw new ApiError({ status: 422, code: 'validation_failed', message: 'too long' });
      },
    });
    await expect(state.post(api, 'x')).rejects.toBeInstanceOf(ApiError);
    expect(calls).toBe(1);
    expect([...state.pending.values()][0]?.status).toBe('failed');
  });

  it('marks a deleted message as a tombstone without its body', async () => {
    const state = new DiscussionState();
    await state.load(fakeApi(), CONTEXT);
    await state.remove(fakeApi(), 'm-1');
    const removed = state.ordered.find((m) => m.seq === 1);
    expect(removed?.status).toBe('deleted');
    expect(removed?.body).toBeNull();
  });

  it('pages older messages without duplicates', async () => {
    const state = new DiscussionState();
    await state.load(
      fakeApi({
        listMessages: async (_id, query) =>
          query?.beforeSeq ? page([message(1), message(2)]) : page([message(3), message(4)], true),
      }),
      CONTEXT,
    );
    expect(state.hasOlder).toBe(true);
    await state.loadOlder(
      fakeApi({ listMessages: async () => page([message(1), message(2)]) }),
    );
    expect(state.ordered.map((m) => m.seq)).toEqual([1, 2, 3, 4]);
  });

  it('keeps memory bounded', async () => {
    const state = new DiscussionState();
    await state.load(fakeApi(), CONTEXT);
    for (let seq = 3; seq <= MAX_IN_MEMORY + 50; seq += 1) state.applyMessage(message(seq));
    expect(state.ordered.length).toBe(MAX_IN_MEMORY);
    expect(state.ordered[0]?.seq).toBe(51);
    expect(state.hasOlder).toBe(true);
  });
});
