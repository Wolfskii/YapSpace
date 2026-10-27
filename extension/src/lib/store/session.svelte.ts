// The signed-in user. The token lives in extension storage (security.md §H.3). Only extension pages read it, and the
// content script never touches it.

import type { User } from '../api/client';

export interface KeyValueStorage {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  remove(key: string): Promise<void>;
}

const STORAGE_KEY = 'ys.session';

export class SessionState {
  token = $state<string | null>(null);
  user = $state<User | null>(null);
  private readonly storage: KeyValueStorage;

  constructor(storage: KeyValueStorage) {
    this.storage = storage;
  }

  get signedIn(): boolean {
    return this.token !== null && this.user !== null;
  }

  /** Loads a saved session. Corrupt data is ignored, so the user simply appears signed out. */
  async load(): Promise<void> {
    try {
      const raw = await this.storage.get(STORAGE_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw) as { token?: unknown; user?: unknown };
      if (typeof parsed.token === 'string' && typeof parsed.user === 'object' && parsed.user !== null) {
        this.token = parsed.token;
        this.user = parsed.user as User;
      }
    } catch {
      this.token = null;
      this.user = null;
    }
  }

  async save(token: string, user: User): Promise<void> {
    this.token = token;
    this.user = user;
    await this.storage.set(STORAGE_KEY, JSON.stringify({ token, user }));
  }

  async clear(): Promise<void> {
    this.token = null;
    this.user = null;
    await this.storage.remove(STORAGE_KEY);
  }
}
