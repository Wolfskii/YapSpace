<script lang="ts">
  import { ApiError, type AuthResponse, type YapSpaceApi } from '../lib/api/client';

  interface Props {
    api: YapSpaceApi;
    onSignedIn: (auth: AuthResponse) => void;
  }

  let { api, onSignedIn }: Props = $props();

  let mode = $state<'signin' | 'register'>('signin');
  let username = $state('');
  let password = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  // Set after registration. The user must confirm they saved the code before the panel continues.
  let pendingCode = $state<string | null>(null);
  let pendingAuth = $state<AuthResponse | null>(null);
  let savedCode = $state(false);

  function explain(err: unknown): string {
    if (err instanceof ApiError) {
      if (err.code === 'network') return 'Could not reach YapSpace. Check your connection.';
      if (err.code === 'username_taken') return 'That username is taken.';
      if (err.code === 'invalid_credentials') return 'Wrong username or password.';
      if (err.code === 'rate_limited') return `Too many attempts. Try again in ${err.retryAfterS ?? 60} seconds.`;
      if (err.fields.length > 0) return err.fields.map((f) => f.message).join(' ');
      return err.message;
    }
    return 'Something went wrong.';
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = null;
    try {
      const auth =
        mode === 'register'
          ? await api.register(username, password, 'YapSpace extension')
          : await api.login(username, password, 'YapSpace extension');
      if (mode === 'register' && auth.recovery_code) {
        pendingCode = auth.recovery_code;
        pendingAuth = auth;
      } else {
        onSignedIn(auth);
      }
    } catch (err) {
      error = explain(err);
    } finally {
      busy = false;
    }
  }

  function finishRegistration() {
    if (pendingAuth) onSignedIn(pendingAuth);
  }
</script>

{#if pendingCode}
  <section class="recovery" aria-labelledby="recovery-title">
    <h2 id="recovery-title">Save your recovery code</h2>
    <p>
      This code is the only way back into your account if you forget your password. It is shown once.
    </p>
    <code class="code">{pendingCode}</code>
    <label class="check">
      <input type="checkbox" bind:checked={savedCode} />
      I have saved the recovery code somewhere safe.
    </label>
    <button type="button" disabled={!savedCode} onclick={finishRegistration}>Continue</button>
  </section>
{:else}
  <form class="auth" onsubmit={submit}>
    <div class="tabs" role="tablist">
      <button type="button" role="tab" aria-selected={mode === 'signin'} onclick={() => (mode = 'signin')}>
        Sign in
      </button>
      <button type="button" role="tab" aria-selected={mode === 'register'} onclick={() => (mode = 'register')}>
        Create account
      </button>
    </div>

    <label>
      Username
      <input
        name="username"
        autocomplete="username"
        required
        minlength="3"
        maxlength="24"
        pattern="[A-Za-z0-9_]+"
        bind:value={username}
      />
    </label>
    <label>
      Password
      <input
        type="password"
        name="password"
        autocomplete={mode === 'register' ? 'new-password' : 'current-password'}
        required
        minlength="10"
        maxlength="128"
        bind:value={password}
      />
    </label>

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    <button type="submit" disabled={busy}>
      {mode === 'register' ? 'Create account' : 'Sign in'}
    </button>
  </form>
{/if}

<style>
  .auth,
  .recovery {
    display: grid;
    gap: 10px;
    padding: 16px;
  }
  .tabs {
    display: flex;
    gap: 8px;
  }
  .tabs button[aria-selected='true'] {
    font-weight: 600;
    text-decoration: underline;
  }
  label {
    display: grid;
    gap: 4px;
    font-size: 14px;
  }
  input {
    font: inherit;
    padding: 8px;
    border: 1px solid #8884;
    border-radius: 6px;
  }
  button {
    font: inherit;
    padding: 8px 12px;
    border-radius: 6px;
    border: 1px solid #8884;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .error {
    color: #b00020;
    margin: 0;
  }
  .code {
    font-family: ui-monospace, monospace;
    font-size: 18px;
    padding: 12px;
    border: 1px dashed #8886;
    border-radius: 6px;
    user-select: all;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
  }
</style>
