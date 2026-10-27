<script module lang="ts">
  export const MAX_BODY = 2000;
  // Warn before the server limit so the user can shorten the message.
  const WARN_AT = 1900;
</script>

<script lang="ts">

  interface Props {
    disabled: boolean;
    replyingTo: string | null;
    onSend: (body: string) => void;
    onCancelReply: () => void;
  }

  let { disabled, replyingTo, onSend, onCancelReply }: Props = $props();

  let text = $state('');
  const length = $derived([...text].length);
  const tooLong = $derived(length > MAX_BODY);

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const body = text.trim();
    if (body.length === 0 || tooLong || disabled) return;
    onSend(body);
    text = '';
  }

  function onKeydown(event: KeyboardEvent) {
    // Enter sends. Shift+Enter adds a line break.
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      (event.currentTarget as HTMLTextAreaElement).form?.requestSubmit();
    }
  }
</script>

<form class="composer" onsubmit={submit}>
  {#if replyingTo}
    <p class="reply-chip">
      Replying to {replyingTo}
      <button type="button" onclick={onCancelReply} aria-label="Cancel reply">×</button>
    </p>
  {/if}
  <label class="sr-only" for="yapspace-body">Message</label>
  <textarea
    id="yapspace-body"
    rows="2"
    placeholder={disabled ? 'Sign in to join' : 'Write a message'}
    disabled={disabled}
    bind:value={text}
    onkeydown={onKeydown}
  ></textarea>
  <div class="row">
    <span class="count" class:warn={length >= WARN_AT} class:over={tooLong}>{length}/{MAX_BODY}</span>
    <button type="submit" disabled={disabled || text.trim().length === 0 || tooLong}>Send</button>
  </div>
</form>

<style>
  .composer {
    display: grid;
    gap: 6px;
    padding: 12px 16px;
    border-top: 1px solid #8884;
  }
  textarea {
    font: inherit;
    resize: vertical;
    padding: 8px;
    border: 1px solid #8884;
    border-radius: 6px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .count {
    font-size: 12px;
    color: #777;
  }
  .warn {
    color: #b36b00;
  }
  .over {
    color: #b00020;
  }
  .reply-chip {
    margin: 0;
    font-size: 12px;
    display: flex;
    justify-content: space-between;
  }
  button {
    font: inherit;
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid #8884;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
