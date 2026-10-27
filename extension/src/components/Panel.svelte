<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { browser } from 'wxt/browser';
  import { ApiError, type AuthResponse, type Message, createYapSpaceApi } from '../lib/api/client';
  import { type PageContext, isPortMessage } from '../lib/messages';
  import { RealtimeClient, type RealtimeStatus } from '../lib/realtime';
  import { DiscussionState } from '../lib/store/discussion.svelte';
  import { type KeyValueStorage, SessionState } from '../lib/store/session.svelte';
  import AuthForm from './AuthForm.svelte';
  import Composer from './Composer.svelte';
  import MessageList from './MessageList.svelte';

  const storage: KeyValueStorage = {
    async get(key) {
      const found = await browser.storage.local.get(key);
      const value = found[key];
      return typeof value === 'string' ? value : null;
    },
    async set(key, value) {
      await browser.storage.local.set({ [key]: value });
    },
    async remove(key) {
      await browser.storage.local.remove(key);
    },
  };

  const session = new SessionState(storage);
  const api = createYapSpaceApi(__API_BASE__, () => session.token);
  const discussion = new DiscussionState();

  let context = $state<PageContext | null>(null);
  let status = $state<RealtimeStatus>('connecting');
  let showAuth = $state(false);
  let replyTo = $state<Message | null>(null);
  let sendError = $state<string | null>(null);
  let subscribedId: string | null = null;

  const realtime = new RealtimeClient({
    url: __WS_URL__,
    token: () => session.token,
    onStatus: (next) => (status = next),
    onFrame: handleFrame,
  });

  let port: ReturnType<typeof browser.runtime.connect> | null = null;

  function handleFrame(frame: Record<string, unknown>) {
    if (frame.type === 'message.created' || frame.type === 'message.updated') {
      const message = frame.message as Message;
      const discussionId = frame.discussion_id as string;
      const seq = frame.seq as number;
      if (discussionId !== discussion.discussion?.id) return;
      discussion.applyMessage(message);
      realtime.advance(discussionId, seq);
    } else if (frame.type === 'resync_required') {
      void reload();
    } else if (frame.type === 'error' && frame.code === 'unauthorized') {
      // The token is no longer valid. Continue anonymously and ask the user to sign in again.
      void session.clear();
    }
  }

  /** Loads the discussion for the current page and subscribes to its live events. */
  async function reload() {
    if (!context) return;
    const id = await discussion.load(api, context);
    if (subscribedId && subscribedId !== id) realtime.unsubscribe(subscribedId);
    if (id) {
      subscribedId = id;
      realtime.subscribe(id, discussion.lastSeq);
    } else {
      subscribedId = null;
    }
  }

  async function send(body: string) {
    sendError = null;
    const parentId = replyTo?.id ?? null;
    try {
      await discussion.post(api, body, parentId);
      replyTo = null;
      const id = discussion.discussion?.id ?? null;
      if (id && subscribedId !== id) {
        subscribedId = id;
        realtime.subscribe(id, discussion.lastSeq);
      }
    } catch (error) {
      sendError = error instanceof ApiError ? sendErrorText(error) : 'Your message was not sent.';
    }
  }

  function sendErrorText(error: ApiError): string {
    if (error.code === 'rate_limited') return `Slow down. You can post again in ${error.retryAfterS ?? 30} seconds.`;
    if (error.code === 'discussion_locked') return 'This discussion is locked.';
    if (error.code === 'account_suspended') return 'Your account is suspended. You can still read.';
    if (error.code === 'duplicate_message') return 'You just posted that message.';
    if (error.code === 'network') return 'You are offline. The message is kept here, so you can try again.';
    return error.message;
  }

  async function onSignedIn(auth: AuthResponse) {
    await session.save(auth.session_token, auth.user);
    showAuth = false;
    realtime.close();
    realtime.connect();
    if (subscribedId) realtime.subscribe(subscribedId, discussion.lastSeq);
  }

  function closePanel() {
    void browser.runtime.sendMessage({ type: "panel-close" }).catch(() => undefined);
  }

  async function signOut() {
    try {
      await api.logout();
    } catch {
      // Signing out locally still works if the server cannot be reached.
    }
    await session.clear();
  }

  onMount(async () => {
    await session.load();
    realtime.connect();
    port = browser.runtime.connect({ name: 'panel' });
    port.onMessage.addListener(async (message: unknown) => {
      if (!isPortMessage(message)) return;
      if (message.type === 'context') {
        context = message.context;
        await reload();
      }
    });
  });

  onDestroy(() => {
    port?.disconnect();
    realtime.close();
  });

  const statusLabel = $derived(
    status === 'open'
      ? 'Live'
      : status === 'connecting'
        ? 'Connecting…'
        : status === 'reconnecting'
          ? 'Reconnecting…'
          : 'Live updates unavailable',
  );
</script>

<main class="panel">
  <header>
    <div class="title">
      <strong>{discussion.discussion?.title ?? context?.title ?? 'YapSpace'}</strong>
      {#if discussion.identity}
        <span class="site">{discussion.identity.display_url}</span>
      {/if}
    </div>
    <div class="status" aria-live="polite">{statusLabel}</div>
    {#if session.signedIn}
      <button type="button" class="link" onclick={signOut}>Sign out</button>
    {/if}
    <button type="button" class="link" aria-label="Close YapSpace" onclick={closePanel}>×</button>
  </header>

  {#if discussion.phase === 'unsupported'}
    <p class="notice">YapSpace cannot discuss this page.</p>
  {:else if discussion.phase === 'error'}
    <p class="notice" role="alert">
      Could not load this discussion. <button type="button" class="link" onclick={reload}>Try again</button>
    </p>
  {:else if discussion.phase === 'resolving' || discussion.phase === 'loading' || discussion.phase === 'idle'}
    <p class="notice">Loading…</p>
  {:else}
    <MessageList
      messages={discussion.ordered}
      hasOlder={discussion.hasOlder}
      loadingOlder={discussion.loadingOlder}
      onLoadOlder={() => discussion.loadOlder(api)}
      onReply={(message) => (replyTo = message)}
    />
  {/if}

  {#if !session.signedIn && showAuth}
    <AuthForm {api} {onSignedIn} />
  {:else if !session.signedIn}
    <div class="join">
      <button type="button" onclick={() => (showAuth = true)}>Sign in to join the conversation</button>
    </div>
  {/if}

  {#if discussion.phase === 'ready' || discussion.phase === 'none'}
    {#if sendError}
      <p class="notice" role="alert">{sendError}</p>
    {/if}
    <Composer
      disabled={!session.signedIn}
      replyingTo={replyTo ? replyTo.author?.username ?? 'a deleted account' : null}
      onSend={send}
      onCancelReply={() => (replyTo = null)}
    />
  {/if}
</main>

<style>
  :global(html, body) {
    margin: 0;
    height: 100%;
    font:
      14px/1.45 system-ui,
      sans-serif;
    background: #fff;
    color: #1d1d1f;
  }
  @media (prefers-color-scheme: dark) {
    :global(html, body) {
      background: #1c1c1e;
      color: #f2f2f7;
    }
  }
  .panel {
    display: flex;
    flex-direction: column;
    height: 100vh;
    border-radius: 12px;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid #8884;
  }
  .title {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .title strong,
  .site {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .site {
    font-size: 12px;
    color: #888;
  }
  .status {
    font-size: 12px;
    color: #888;
  }
  .notice,
  .join {
    margin: 0;
    padding: 12px 16px;
    color: #666;
  }
  .join button,
  .link {
    font: inherit;
    background: none;
    border: none;
    color: #2a6fdb;
    cursor: pointer;
    padding: 0;
  }
  @media (prefers-color-scheme: dark) {
    .join button,
    .link {
      color: #7aa7ff;
    }
  }
</style>
