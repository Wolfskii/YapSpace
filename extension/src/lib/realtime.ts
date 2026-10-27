// WebSocket client for the discussion stream (docs/architecture/realtime.md §G.4).
//
// One socket per open panel. Subscriptions are replayed after every reconnect, each from its last known seq, so the
// server sends exactly what was missed. Reconnects back off with full jitter. A missing pong counts as a dead socket,
// which covers laptop sleep and dropped NAT mappings.

export type RealtimeStatus = 'connecting' | 'open' | 'reconnecting' | 'unavailable';

/** The subset of the WebSocket API the client uses. Tests supply a fake. */
export interface SocketLike {
  readyState: number;
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: ((event: unknown) => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code: number }) => void) | null;
  onerror: ((event: unknown) => void) | null;
}

export interface RealtimeOptions {
  url: string;
  /** Returns the session token to send in `hello`, or null for an anonymous socket. */
  token: () => string | null;
  /** Called for every parsed server frame. */
  onFrame: (frame: Record<string, unknown>) => void;
  onStatus?: (status: RealtimeStatus) => void;
  createSocket?: (url: string) => SocketLike;
  random?: () => number;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (handle: unknown) => void;
}

export const PING_INTERVAL_MS = 25_000;
export const PONG_TIMEOUT_MS = 10_000;
export const MAX_BACKOFF_MS = 30_000;
/** Close codes that mean "do not retry quickly" (origin rejected, too many connections). */
const SLOW_RETRY_CODES = new Set([1008, 4029]);
const SLOW_RETRY_MS = 60_000;
const OPEN = 1;

export function backoffDelay(attempt: number, random: number): number {
  const ceiling = Math.min(MAX_BACKOFF_MS, 1000 * 2 ** attempt);
  return Math.floor(random * ceiling);
}

export class RealtimeClient {
  private readonly options: RealtimeOptions;
  private socket: SocketLike | null = null;
  private attempt = 0;
  private closedByUser = false;
  private readonly subscriptions = new Map<string, number>();
  private pingTimer: unknown = null;
  private pongTimer: unknown = null;
  private reconnectTimer: unknown = null;
  private readonly random: () => number;
  private readonly setTimer: (fn: () => void, ms: number) => unknown;
  private readonly clearTimer: (handle: unknown) => void;
  private readonly createSocket: (url: string) => SocketLike;

  constructor(options: RealtimeOptions) {
    this.options = options;
    this.random = options.random ?? Math.random;
    this.setTimer = options.setTimer ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimer = options.clearTimer ?? ((handle) => clearTimeout(handle as ReturnType<typeof setTimeout>));
    this.createSocket = options.createSocket ?? ((url) => new WebSocket(url) as unknown as SocketLike);
  }

  connect(): void {
    this.closedByUser = false;
    this.open();
  }

  close(): void {
    this.closedByUser = true;
    this.stopTimers();
    this.socket?.close(1000, 'panel closed');
    this.socket = null;
  }

  /** Starts (or moves) a subscription. `afterSeq` is the last seq the caller has. */
  subscribe(discussionId: string, afterSeq: number): void {
    this.subscriptions.set(discussionId, afterSeq);
    if (this.isOpen()) this.sendSubscribe(discussionId, afterSeq);
  }

  unsubscribe(discussionId: string): void {
    this.subscriptions.delete(discussionId);
    if (this.isOpen()) this.send({ type: 'unsubscribe', discussion_id: discussionId });
  }

  /** Records the highest seq the caller has, so a reconnect resumes from there. */
  advance(discussionId: string, seq: number): void {
    const current = this.subscriptions.get(discussionId);
    if (current !== undefined && seq > current) this.subscriptions.set(discussionId, seq);
  }

  private open(): void {
    this.options.onStatus?.(this.attempt === 0 ? 'connecting' : 'reconnecting');
    const socket = this.createSocket(this.options.url);
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.options.onStatus?.('open');
      const token = this.options.token();
      if (token) this.send({ type: 'hello', protocol: 1, client: 'yapspace-ext', token });
      for (const [id, seq] of this.subscriptions) this.sendSubscribe(id, seq);
      this.startHeartbeat();
    };
    socket.onmessage = (event) => this.receive(event.data);
    socket.onclose = (event) => {
      this.stopTimers();
      if (this.socket === socket) this.socket = null;
      if (!this.closedByUser) this.scheduleReconnect(event.code);
    };
    socket.onerror = () => {
      // The close handler does the work. Errors alone carry no details the client can use.
    };
  }

  private receive(raw: unknown): void {
    if (typeof raw !== 'string') return;
    let frame: unknown;
    try {
      frame = JSON.parse(raw);
    } catch {
      return;
    }
    if (typeof frame !== 'object' || frame === null || typeof (frame as { type?: unknown }).type !== 'string') return;
    const typed = frame as Record<string, unknown>;
    if (typed.type === 'pong') {
      this.clearPong();
      return;
    }
    this.options.onFrame(typed);
  }

  private startHeartbeat(): void {
    // Only the ping timer is reset here. The pong watchdog must keep running, or a dead socket would never be detected.
    if (this.pingTimer !== null) this.clearTimer(this.pingTimer);
    this.pingTimer = this.setTimer(() => {
      this.pingTimer = null;
      this.send({ type: 'ping', nonce: Date.now() });
      this.clearPong();
      this.pongTimer = this.setTimer(() => {
        this.pongTimer = null;
        // No pong: the socket is dead even though the browser has not noticed.
        this.socket?.close(4004, 'no pong');
      }, PONG_TIMEOUT_MS);
      this.startHeartbeat();
    }, PING_INTERVAL_MS);
  }

  private clearPong(): void {
    if (this.pongTimer !== null) {
      this.clearTimer(this.pongTimer);
      this.pongTimer = null;
    }
  }

  private stopTimers(): void {
    for (const handle of [this.pingTimer, this.pongTimer, this.reconnectTimer]) {
      if (handle !== null) this.clearTimer(handle);
    }
    this.pingTimer = null;
    this.pongTimer = null;
    this.reconnectTimer = null;
  }

  private scheduleReconnect(closeCode: number): void {
    this.options.onStatus?.('reconnecting');
    const delay = SLOW_RETRY_CODES.has(closeCode)
      ? SLOW_RETRY_MS
      : backoffDelay(this.attempt, this.random());
    this.attempt += 1;
    this.reconnectTimer = this.setTimer(() => {
      this.reconnectTimer = null;
      this.open();
    }, delay);
  }

  private sendSubscribe(discussionId: string, afterSeq: number): void {
    this.send({ type: 'subscribe', discussion_id: discussionId, after_seq: afterSeq });
  }

  private send(frame: Record<string, unknown>): void {
    if (this.isOpen()) this.socket?.send(JSON.stringify(frame));
  }

  private isOpen(): boolean {
    return this.socket !== null && this.socket.readyState === OPEN;
  }
}
