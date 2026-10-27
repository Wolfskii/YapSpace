<script lang="ts">
  import type { Message } from '../lib/api/client';
  import { linkify } from '../lib/linkify';

  interface Props {
    messages: Message[];
    hasOlder: boolean;
    loadingOlder: boolean;
    onLoadOlder: () => void;
    onReply?: (message: Message) => void;
  }

  let { messages, hasOlder, loadingOlder, onLoadOlder, onReply }: Props = $props();

  function when(iso: string): string {
    return new Date(iso).toLocaleString();
  }
</script>

<div class="list" role="log" aria-live="polite" aria-label="Messages">
  {#if hasOlder}
    <button type="button" class="older" disabled={loadingOlder} onclick={onLoadOlder}>
      {loadingOlder ? 'Loading…' : 'Load older messages'}
    </button>
  {/if}

  {#each messages as message (message.seq)}
    <article class="message" data-status={message.status}>
      <header>
        {#if message.author}
          <strong>{message.author.username}</strong>
        {:else}
          <strong class="muted">deleted account</strong>
        {/if}
        <time datetime={message.created_at} title={when(message.created_at)}>
          {when(message.created_at)}
        </time>
      </header>

      {#if message.parent}
        <p class="parent">
          Replying to {message.parent.author_username ?? 'a message'}:
          <span>{message.parent.excerpt ?? 'message removed'}</span>
        </p>
      {/if}

      {#if message.body === null}
        <p class="muted">
          {message.status === 'removed' ? 'Removed by moderators.' : 'Message deleted.'}
        </p>
      {:else}
        <p class="body">
          {#each linkify(message.body ?? '') as token, index (index)}
            {#if token.type === 'link'}
              <a href={token.href} target="_blank" rel="noopener noreferrer nofollow ugc">{token.value}</a>
            {:else}
              {token.value}
            {/if}
          {/each}
        </p>
      {/if}

      {#if onReply && message.body !== null}
        <button type="button" class="reply" onclick={() => onReply?.(message)}>Reply</button>
      {/if}
    </article>
  {:else}
    <p class="empty">Be the first to start the conversation.</p>
  {/each}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 16px;
    overflow-y: auto;
  }
  .message {
    border-bottom: 1px solid #8883;
    padding-bottom: 8px;
  }
  header {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
  }
  time {
    color: #888;
    font-size: 12px;
  }
  .body {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    margin: 4px 0;
  }
  .parent {
    font-size: 12px;
    color: #666;
    margin: 4px 0;
  }
  .parent span {
    font-style: italic;
  }
  .muted,
  .empty {
    color: #777;
  }
  .older,
  .reply {
    font: inherit;
    font-size: 12px;
    background: none;
    border: none;
    color: #2a6fdb;
    cursor: pointer;
    padding: 0;
    justify-self: start;
  }
  a {
    color: #2a6fdb;
    overflow-wrap: anywhere;
  }
  @media (prefers-color-scheme: dark) {
    a,
    .older,
    .reply {
      color: #7aa7ff;
    }
  }
</style>
