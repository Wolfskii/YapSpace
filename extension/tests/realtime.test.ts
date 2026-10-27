import { describe, expect, it } from 'vitest';
import {
  MAX_BACKOFF_MS,
  PING_INTERVAL_MS,
  PONG_TIMEOUT_MS,
  RealtimeClient,
  type RealtimeStatus,
  type SocketLike,
  backoffDelay,
} from '../src/lib/realtime';

/** A socket the test drives by hand. */
class FakeSocket implements SocketLike {
  static instances: FakeSocket[] = [];
  readyState = 0;
  sent: string[] = [];
  closedWith: number | undefined;
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code: number }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;

  constructor() {
    FakeSocket.instances.push(this);
  }

  send(data: string) {
    this.sent.push(data);
  }

  close(code?: number) {
    this.closedWith = code;
    this.readyState = 3;
  }

  open() {
    this.readyState = 1;
    this.onopen?.({});
  }

  serverSends(frame: unknown) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }

  drop(code = 1006) {
    this.readyState = 3;
    this.onclose?.({ code });
  }

  sentFrames(): Record<string, unknown>[] {
    return this.sent.map((text) => JSON.parse(text) as Record<string, unknown>);
  }
}

interface Timer {
  fn: () => void;
  ms: number;
  id: number;
}

/** Timers the test advances by hand. */
function fakeTimers() {
  let nextId = 1;
  let pending: Timer[] = [];
  return {
    setTimer(fn: () => void, ms: number) {
      const id = nextId++;
      pending.push({ fn, ms, id });
      return id;
    },
    clearTimer(handle: unknown) {
      pending = pending.filter((timer) => timer.id !== handle);
    },
    pending: () => pending.slice(),
    /** Runs the timers that are due after `ms` milliseconds, in order. */
    run(ms: number) {
      const due = pending.filter((timer) => timer.ms <= ms);
      pending = pending.filter((timer) => timer.ms > ms);
      for (const timer of due) timer.fn();
    },
  };
}

function makeClient(overrides: { token?: () => string | null; onStatus?: (s: RealtimeStatus) => void } = {}) {
  FakeSocket.instances = [];
  const timers = fakeTimers();
  const frames: Record<string, unknown>[] = [];
  const client = new RealtimeClient({
    url: 'ws://test/v1/realtime',
    token: overrides.token ?? (() => null),
    onFrame: (frame) => frames.push(frame),
    onStatus: overrides.onStatus,
    createSocket: () => new FakeSocket(),
    random: () => 1,
    setTimer: timers.setTimer,
    clearTimer: timers.clearTimer,
  });
  const socket = (): FakeSocket => FakeSocket.instances[FakeSocket.instances.length - 1] as FakeSocket;
  return { client, timers, frames, socket };
}

describe('backoffDelay', () => {
  it('grows exponentially and is capped', () => {
    expect(backoffDelay(0, 1)).toBe(1000);
    expect(backoffDelay(3, 1)).toBe(8000);
    expect(backoffDelay(20, 1)).toBe(MAX_BACKOFF_MS);
  });

  it('uses full jitter', () => {
    expect(backoffDelay(3, 0)).toBe(0);
    expect(backoffDelay(3, 0.5)).toBe(4000);
  });
});

describe('RealtimeClient', () => {
  it('replays subscriptions from their last seq after every reconnect', () => {
    const { client, timers, socket } = makeClient();
    client.connect();
    socket().open();
    client.subscribe('d1', 12);
    expect(socket().sentFrames()).toContainEqual({ type: 'subscribe', discussion_id: 'd1', after_seq: 12 });

    client.advance('d1', 15);
    socket().drop();
    timers.run(1000);
    socket().open();

    expect(socket().sentFrames()).toContainEqual({ type: 'subscribe', discussion_id: 'd1', after_seq: 15 });
  });

  it('sends hello with the token when signed in', () => {
    const { client, socket } = makeClient({ token: () => 'ys_abc' });
    client.connect();
    socket().open();
    expect(socket().sentFrames()[0]).toMatchObject({ type: 'hello', protocol: 1, token: 'ys_abc' });
  });

  it('reports status changes', () => {
    const statuses: RealtimeStatus[] = [];
    const { client, timers, socket } = makeClient({ onStatus: (s) => statuses.push(s) });
    client.connect();
    socket().open();
    socket().drop();
    timers.run(1000);
    expect(statuses).toEqual(['connecting', 'open', 'reconnecting', 'reconnecting']);
  });

  it('treats a missing pong as a dead socket', () => {
    const { client, timers, socket } = makeClient();
    client.connect();
    socket().open();
    timers.run(PING_INTERVAL_MS);
    expect(socket().sentFrames().at(-1)).toMatchObject({ type: 'ping' });
    timers.run(PONG_TIMEOUT_MS);
    expect(socket().closedWith).toBe(4004);
  });

  it('does not treat a pong as a message', () => {
    const { client, frames, socket, timers } = makeClient();
    client.connect();
    socket().open();
    timers.run(PING_INTERVAL_MS);
    socket().serverSends({ type: 'pong', nonce: 1 });
    expect(frames).toEqual([]);
  });

  it('ignores frames that are not JSON objects with a type', () => {
    const { client, frames, socket } = makeClient();
    client.connect();
    socket().open();
    socket().onmessage?.({ data: 'not json' });
    socket().onmessage?.({ data: '42' });
    socket().serverSends({ type: 'subscribed', discussion_id: 'd1', last_seq: 3 });
    expect(frames).toEqual([{ type: 'subscribed', discussion_id: 'd1', last_seq: 3 }]);
  });

  it('does not reconnect after close() is called', () => {
    const { client, timers, socket } = makeClient();
    client.connect();
    socket().open();
    client.close();
    socket().drop();
    expect(timers.pending()).toEqual([]);
  });

  it('waits a minute after an origin refusal before trying again', () => {
    const { client, timers, socket } = makeClient();
    client.connect();
    socket().drop(4029);
    expect(timers.pending()[0]?.ms).toBe(60_000);
    client.close();
  });
});
