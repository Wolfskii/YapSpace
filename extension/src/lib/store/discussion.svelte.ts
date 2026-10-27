// The state of the discussion shown in the panel (docs/architecture/extension.md §I.3, §I.4).
//
// Messages are keyed by seq, so a message that arrives twice (once from REST, once from the socket) is stored once.
// Every load carries a generation number. A late answer for an older page is dropped.

import { SvelteMap } from 'svelte/reactivity';
import { ApiError, type Discussion, type Identity, type Message, type YapSpaceApi } from '../api/client';
import type { PageContext } from '../messages';

export type Phase = 'idle' | 'resolving' | 'unsupported' | 'none' | 'loading' | 'ready' | 'error';

export interface PendingMessage {
  clientMsgId: string;
  body: string;
  status: 'sending' | 'failed';
}

export const PAGE_SIZE = 50;
/** The panel keeps at most this many messages in memory (extension.md §I.4). */
export const MAX_IN_MEMORY = 1000;
const SEND_ATTEMPTS = 3;

export class DiscussionState {
  phase = $state<Phase>('idle');
  /** Why the page is unsupported, or the error code. */
  reason = $state<string | null>(null);
  identity = $state<Identity | null>(null);
  discussion = $state<Discussion | null>(null);
  hasOlder = $state(false);
  loadingOlder = $state(false);
  private readonly messagesBySeq = new SvelteMap<number, Message>();
  readonly pending = new SvelteMap<string, PendingMessage>();
  private generation = 0;
  private context: PageContext | null = null;

  /** Messages in ascending seq order. */
  readonly ordered = $derived(
    [...this.messagesBySeq.values()].sort((a, b) => a.seq - b.seq),
  );

  /** The highest seq the panel has. Used as the resume cursor after a reconnect. */
  readonly lastSeq = $derived(this.ordered.at(-1)?.seq ?? 0);

  /** The oldest seq loaded so far, for paging backwards. */
  private nextBeforeSeq: number | null = null;

  /** Resolves the page and loads the newest messages. Returns the discussion id to subscribe to, or null. */
  async load(api: YapSpaceApi, context: PageContext): Promise<string | null> {
    const generation = ++this.generation;
    this.context = context;
    this.reset();

    if (context.url === null) {
      this.phase = 'unsupported';
      this.reason = 'page';
      return null;
    }

    this.phase = 'resolving';
    try {
      const resolved = await api.resolve(context.url, context.canonical);
      if (generation !== this.generation) return null;
      this.identity = resolved.identity;
      if (!resolved.discussion) {
        this.phase = 'none';
        return null;
      }
      this.discussion = resolved.discussion;
      await this.loadNewest(api, resolved.discussion.id, generation);
      return resolved.discussion.id;
    } catch (error) {
      if (generation !== this.generation) return null;
      this.fail(error);
      return null;
    }
  }

  /** Creates the discussion for the current page on the first post. */
  async ensureDiscussion(api: YapSpaceApi): Promise<string> {
    if (this.discussion) return this.discussion.id;
    if (!this.context?.url) throw new ApiError({ status: 422, code: 'url_unsupported', message: 'unsupported page' });
    const created = await api.createDiscussion(this.context.url, this.context.canonical, this.context.title);
    this.discussion = created;
    this.identity = {
      kind: 'url',
      display_url: created.display_url,
      site: created.site,
      ruleset: 1,
    };
    this.phase = 'ready';
    return created.id;
  }

  async loadOlder(api: YapSpaceApi): Promise<void> {
    if (!this.discussion || this.nextBeforeSeq === null || this.loadingOlder) return;
    const generation = this.generation;
    this.loadingOlder = true;
    try {
      const page = await api.listMessages(this.discussion.id, { beforeSeq: this.nextBeforeSeq, limit: PAGE_SIZE });
      if (generation !== this.generation) return;
      for (const message of page.items) this.messagesBySeq.set(message.seq, message);
      this.hasOlder = page.has_more;
      this.nextBeforeSeq = page.next_before_seq ?? null;
      this.trim();
    } finally {
      this.loadingOlder = false;
    }
  }

  /** Posts a message. The client id is reused on retries, so the server stores the message once. */
  async post(api: YapSpaceApi, body: string, parentId: string | null = null): Promise<void> {
    const clientMsgId = crypto.randomUUID();
    this.pending.set(clientMsgId, { clientMsgId, body, status: 'sending' });
    try {
      const discussionId = await this.ensureDiscussion(api);
      let lastError: unknown = null;
      for (let attempt = 0; attempt < SEND_ATTEMPTS; attempt += 1) {
        try {
          const message = await api.postMessage(discussionId, body, clientMsgId, parentId ?? undefined);
          this.applyMessage(message);
          this.pending.delete(clientMsgId);
          return;
        } catch (error) {
          lastError = error;
          // Client errors will not change on retry. Network errors and server errors might.
          if (error instanceof ApiError && error.status !== 0 && error.status < 500) break;
        }
      }
      throw lastError;
    } catch (error) {
      this.pending.set(clientMsgId, { clientMsgId, body, status: 'failed' });
      throw error;
    }
  }

  /** Removes a message the user wrote. The server keeps a tombstone, so the local copy becomes one too. */
  async remove(api: YapSpaceApi, messageId: string): Promise<void> {
    await api.deleteMessage(messageId);
    for (const message of this.messagesBySeq.values()) {
      if (message.id === messageId) {
        this.messagesBySeq.set(message.seq, { ...message, status: 'deleted', body: null });
      }
    }
  }

  /** Stores a message from the server. Stores by seq, so a duplicate never adds a second copy. */
  applyMessage(message: Message): void {
    this.messagesBySeq.set(message.seq, message);
    this.trim();
  }

  /** Forgets everything. The next load starts clean. */
  reset(): void {
    this.messagesBySeq.clear();
    this.pending.clear();
    this.discussion = null;
    this.identity = null;
    this.hasOlder = false;
    this.nextBeforeSeq = null;
    this.reason = null;
    this.phase = 'idle';
  }

  private async loadNewest(api: YapSpaceApi, discussionId: string, generation: number): Promise<void> {
    this.phase = 'loading';
    const page = await api.listMessages(discussionId, { limit: PAGE_SIZE });
    if (generation !== this.generation) return;
    this.messagesBySeq.clear();
    for (const message of page.items) this.messagesBySeq.set(message.seq, message);
    this.hasOlder = page.has_more;
    this.nextBeforeSeq = page.next_before_seq ?? null;
    this.phase = 'ready';
  }

  private fail(error: unknown): void {
    this.phase = error instanceof ApiError && error.code === 'url_unsupported' ? 'unsupported' : 'error';
    this.reason = error instanceof ApiError ? (error.reason ?? error.code) : 'unknown';
  }

  /** Keeps memory bounded by dropping the oldest messages above the cap. They can be loaded again. */
  private trim(): void {
    if (this.messagesBySeq.size <= MAX_IN_MEMORY) return;
    const seqs = [...this.messagesBySeq.keys()].sort((a, b) => a - b);
    const excess = seqs.length - MAX_IN_MEMORY;
    for (const seq of seqs.slice(0, excess)) this.messagesBySeq.delete(seq);
    this.hasOlder = true;
    this.nextBeforeSeq = seqs[excess] ?? null;
  }
}
