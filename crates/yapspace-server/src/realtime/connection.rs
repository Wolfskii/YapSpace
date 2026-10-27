//! One WebSocket connection (docs/architecture/realtime.md §G.3). A reader loop handles client frames. A writer task
//! drains a bounded queue. Each subscription has a forwarder task. Any failure closes the socket with a code from §G.2.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket};
use futures_util::{SinkExt, StreamExt};
use sqlx::PgPool;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{
    api::dto::{MessageDto, message_dto},
    app::AppState,
    db::{discussions, messages, sessions},
    domain::tokens,
    realtime::{
        hub::{Event, EventKind},
        protocol::{ClientFrame, MAX_REPLAY, MAX_SUBSCRIPTIONS, PROTOCOL_VERSION, ServerFrame, close},
        slots::{IpSlot, UserSlot},
    },
};

/// Frames a client may send per window before the connection is closed as abusive.
const FRAMES_PER_WINDOW: usize = 20;
const FRAME_WINDOW: Duration = Duration::from_secs(10);
/// How long a closing socket keeps reading so the client receives the close frame.
const CLOSE_DRAIN: Duration = Duration::from_secs(1);
/// Frames queued for one socket. A full queue means the client is not reading, so the socket is closed (§G.3).
const OUT_QUEUE: usize = 256;

/// Why a send could not be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendError {
    /// The queue is full: a slow consumer.
    Full,
    /// The writer has gone: the socket is already closing.
    Gone,
}

fn queue(out: &mpsc::Sender<Message>, message: Message) -> Result<(), SendError> {
    out.try_send(message).map_err(|err| match err {
        mpsc::error::TrySendError::Full(_) => SendError::Full,
        mpsc::error::TrySendError::Closed(_) => SendError::Gone,
    })
}

fn send(out: &mpsc::Sender<Message>, frame: &ServerFrame) -> Result<(), SendError> {
    let text = serde_json::to_string(frame).expect("server frames always serialize");
    queue(out, Message::Text(Utf8Bytes::from(text)))
}

fn close_message(code: u16) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: Utf8Bytes::from_static(""),
    }))
}

/// Fixed-window counter for client frames.
struct FrameBudget {
    window: VecDeque<Instant>,
}

impl FrameBudget {
    fn new() -> Self {
        Self {
            window: VecDeque::new(),
        }
    }

    /// Records one frame. Returns false when the connection has exceeded its budget.
    fn allow(&mut self, now: Instant) -> bool {
        while self
            .window
            .front()
            .is_some_and(|t| now.duration_since(*t) > FRAME_WINDOW)
        {
            self.window.pop_front();
        }
        if self.window.len() >= FRAMES_PER_WINDOW {
            return false;
        }
        self.window.push_back(now);
        true
    }
}

struct Connection {
    state: AppState,
    out: mpsc::Sender<Message>,
    /// Forwarders ask the reader loop to close the socket with a code (for example 4008 on a slow consumer).
    kill: mpsc::Sender<u16>,
    subscriptions: HashMap<Uuid, JoinHandle<()>>,
    user: Option<Uuid>,
    /// Holds the account's connection slot after a successful authenticated hello.
    user_slot: Option<UserSlot>,
    /// Set by a command that wants the socket closed after its reply is sent.
    pending_close: Option<u16>,
}

/// Runs one connection until the client leaves, an error occurs, or shutdown starts. The IP slot is released on exit.
pub async fn run(socket: WebSocket, state: AppState, ip_slot: IpSlot) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(OUT_QUEUE);
    let (kill_tx, mut kill_rx) = mpsc::channel::<u16>(1);

    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            let last = matches!(message, Message::Close(_));
            if sink.send(message).await.is_err() || last {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let mut conn = Connection {
        state: state.clone(),
        out: out_tx.clone(),
        kill: kill_tx,
        subscriptions: HashMap::new(),
        user: None,
        user_slot: None,
        pending_close: None,
    };
    let _ = send(
        &out_tx,
        &ServerFrame::Welcome {
            protocol: PROTOCOL_VERSION,
            conn_id: Uuid::now_v7(),
            heartbeat_s: state.config.ws_heartbeat.as_secs(),
            max_subscriptions: MAX_SUBSCRIPTIONS,
        },
    );

    let mut heartbeat = tokio::time::interval(state.config.ws_heartbeat);
    heartbeat.tick().await;
    let mut last_inbound = Instant::now();
    let mut budget = FrameBudget::new();

    let closing: Option<u16> = loop {
        tokio::select! {
            () = state.shutdown.cancelled() => break Some(close::RESTARTING),
            code = kill_rx.recv() => break code,
            _ = heartbeat.tick() => {
                if last_inbound.elapsed() > state.config.ws_idle_timeout {
                    break Some(close::IDLE_TIMEOUT);
                }
                match queue(&out_tx, Message::Ping(Default::default())) {
                    Ok(()) => {}
                    Err(SendError::Full) => break Some(close::SLOW_CONSUMER),
                    Err(SendError::Gone) => break None,
                }
            }
            frame = stream.next() => {
                let Some(frame) = frame else { break None };
                let frame = match frame {
                    Ok(frame) => frame,
                    // Read errors are oversize frames or protocol violations. Both close the socket.
                    Err(_) => break Some(close::FRAME_TOO_BIG),
                };
                last_inbound = Instant::now();
                match frame {
                    Message::Text(text) => {
                        if !budget.allow(Instant::now()) {
                            break Some(close::PROTOCOL_ERROR);
                        }
                        let result = match serde_json::from_str::<ClientFrame>(&text) {
                            Ok(frame) => conn.handle(frame).await,
                            Err(_) => send(&out_tx, &ServerFrame::Error {
                                code: "bad_frame",
                                detail: "frame is not valid JSON or has an unknown type".into(),
                            }),
                        };
                        if let Err(err) = result {
                            break match err {
                                SendError::Full => Some(close::SLOW_CONSUMER),
                                SendError::Gone => None,
                            };
                        }
                        if let Some(code) = conn.pending_close.take() {
                            break Some(code);
                        }
                    }
                    Message::Binary(_) => break Some(close::PROTOCOL_ERROR),
                    Message::Close(_) => break None,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    };

    if let Some(code) = closing {
        let _ = queue(&out_tx, close_message(code));
        // Keep reading briefly. Closing a socket with unread inbound data makes TCP reset the connection, and then
        // the client can lose the close code.
        let _ = tokio::time::timeout(CLOSE_DRAIN, async {
            while let Some(item) = stream.next().await {
                if matches!(item, Ok(Message::Close(_))) {
                    break;
                }
            }
        })
        .await;
    }
    let subscribed: Vec<Uuid> = conn.subscriptions.keys().copied().collect();
    for (_, handle) in conn.subscriptions.drain() {
        handle.abort();
    }
    for discussion in subscribed {
        state.hub.release(discussion);
    }
    drop(conn);
    drop(out_tx);
    let _ = writer.await;
    drop(ip_slot);
}

impl Connection {
    async fn handle(&mut self, frame: ClientFrame) -> Result<(), SendError> {
        match frame {
            ClientFrame::Hello { protocol, token, .. } => self.hello(protocol, token).await,
            ClientFrame::Subscribe {
                discussion_id,
                after_seq,
            } => self.subscribe(discussion_id, after_seq).await,
            ClientFrame::Unsubscribe { discussion_id } => {
                self.unsubscribe(discussion_id);
                Ok(())
            }
            ClientFrame::Ping { nonce } => send(&self.out, &ServerFrame::Pong { nonce }),
        }
    }

    /// Optional authentication. A bad token leaves the connection anonymous, and the client is told.
    async fn hello(&mut self, protocol: u8, token: Option<String>) -> Result<(), SendError> {
        if protocol != PROTOCOL_VERSION {
            return send(
                &self.out,
                &ServerFrame::Error {
                    code: "bad_frame",
                    detail: "unsupported protocol version".into(),
                },
            );
        }
        let Some(token) = token else {
            return send(&self.out, &ServerFrame::HelloOk { user_id: self.user });
        };
        let hash = tokens::hash_token(&token);
        let row = match sessions::find_active_by_token_hash(&self.state.db, &hash).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                return send(
                    &self.out,
                    &ServerFrame::Error {
                        code: "unauthorized",
                        detail: "the session token is not valid".into(),
                    },
                );
            }
            Err(_) => {
                return send(
                    &self.out,
                    &ServerFrame::Error {
                        code: "unavailable",
                        detail: "could not check the session".into(),
                    },
                );
            }
        };
        let Some(slot) = self.state.ws.acquire_user(row.user_id) else {
            send(
                &self.out,
                &ServerFrame::Error {
                    code: "rate_limited",
                    detail: "too many open connections for this account".into(),
                },
            )?;
            self.pending_close = Some(close::TOO_MANY_CONNECTIONS);
            return Ok(());
        };
        self.user = Some(row.user_id);
        self.user_slot = Some(slot);
        send(&self.out, &ServerFrame::HelloOk { user_id: self.user })
    }

    /// Subscribes to a discussion. Steps follow realtime.md §G.3: subscribe first, then replay the backlog, so no
    /// event can fall between the two.
    async fn subscribe(&mut self, discussion: Uuid, after: Option<i64>) -> Result<(), SendError> {
        if !self.subscriptions.contains_key(&discussion) && self.subscriptions.len() >= MAX_SUBSCRIPTIONS {
            return send(
                &self.out,
                &ServerFrame::Error {
                    code: "too_many_subscriptions",
                    detail: format!("at most {MAX_SUBSCRIPTIONS} subscriptions per connection"),
                },
            );
        }
        let row = match discussions::find_by_id(&self.state.db, discussion).await {
            Ok(Some(row)) if row.status != "hidden" => row,
            Ok(_) => {
                return send(
                    &self.out,
                    &ServerFrame::Error {
                        code: "unknown_discussion",
                        detail: "no such discussion".into(),
                    },
                );
            }
            Err(_) => return self.unavailable(),
        };

        let rx = self.state.hub.subscribe(discussion);
        let from = after.unwrap_or(row.last_seq);
        let backlog = match messages::page_after(&self.state.db, discussion, from, MAX_REPLAY + 1).await {
            Ok(rows) => rows,
            Err(_) => {
                drop(rx);
                self.state.hub.release(discussion);
                return self.unavailable();
            }
        };
        if backlog.len() as i64 > MAX_REPLAY {
            drop(rx);
            self.state.hub.release(discussion);
            return send(
                &self.out,
                &ServerFrame::ResyncRequired {
                    discussion_id: discussion,
                    reason: "gap_too_large",
                },
            );
        }

        let mut last_sent = from;
        for message in &backlog {
            send(
                &self.out,
                &ServerFrame::MessageCreated {
                    discussion_id: discussion,
                    seq: message.seq,
                    message: message_dto(message),
                },
            )?;
            last_sent = message.seq;
        }
        send(
            &self.out,
            &ServerFrame::Subscribed {
                discussion_id: discussion,
                last_seq: last_sent,
            },
        )?;

        let handle = tokio::spawn(forward(
            rx,
            self.out.clone(),
            self.kill.clone(),
            self.state.db.clone(),
            discussion,
            last_sent,
        ));
        if let Some(old) = self.subscriptions.insert(discussion, handle) {
            old.abort();
        }
        Ok(())
    }

    fn unsubscribe(&mut self, discussion: Uuid) {
        if let Some(handle) = self.subscriptions.remove(&discussion) {
            handle.abort();
        }
        self.state.hub.release(discussion);
    }

    fn unavailable(&self) -> Result<(), SendError> {
        send(
            &self.out,
            &ServerFrame::Error {
                code: "unavailable",
                detail: "try again in a moment".into(),
            },
        )
    }
}

/// Sends one event to a socket. A full queue asks the reader loop to close the socket as a slow consumer.
fn deliver(out: &mpsc::Sender<Message>, kill: &mpsc::Sender<u16>, payload: Utf8Bytes) -> bool {
    match queue(out, Message::Text(payload)) {
        Ok(()) => true,
        Err(SendError::Full) => {
            let _ = kill.try_send(close::SLOW_CONSUMER);
            false
        }
        Err(SendError::Gone) => false,
    }
}

/// Forwards one discussion's events to a socket, in seq order.
///
/// Events can arrive out of order, because two commits may publish in either order. When a gap appears, the missing
/// messages are read back from the database (they are committed by then, since seqs are allocated under a lock).
/// That makes delivery exact without a broker.
async fn forward(
    mut rx: broadcast::Receiver<Arc<Event>>,
    out: mpsc::Sender<Message>,
    kill: mpsc::Sender<u16>,
    db: PgPool,
    discussion: Uuid,
    mut last_sent: i64,
) {
    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let _ = send(
                    &out,
                    &ServerFrame::ResyncRequired {
                        discussion_id: discussion,
                        reason: "lagged",
                    },
                );
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        };

        match event.kind {
            EventKind::Created => {
                if event.seq <= last_sent {
                    continue;
                }
                let missing = event.seq - last_sent - 1;
                if missing > 0 {
                    if missing > MAX_REPLAY {
                        let _ = send(
                            &out,
                            &ServerFrame::ResyncRequired {
                                discussion_id: discussion,
                                reason: "gap_too_large",
                            },
                        );
                        break;
                    }
                    let rows = match messages::page_after(&db, discussion, last_sent, missing).await {
                        Ok(rows) => rows,
                        Err(_) => {
                            let _ = send(
                                &out,
                                &ServerFrame::ResyncRequired {
                                    discussion_id: discussion,
                                    reason: "lagged",
                                },
                            );
                            break;
                        }
                    };
                    let mut failed = false;
                    for row in rows.iter().filter(|row| row.seq < event.seq) {
                        let frame = ServerFrame::MessageCreated {
                            discussion_id: discussion,
                            seq: row.seq,
                            message: message_dto(row),
                        };
                        let text = serde_json::to_string(&frame).expect("server frames always serialize");
                        if !deliver(&out, &kill, Utf8Bytes::from(text)) {
                            failed = true;
                            break;
                        }
                        last_sent = row.seq;
                    }
                    if failed {
                        break;
                    }
                }
                if event.seq <= last_sent {
                    continue;
                }
                if !deliver(&out, &kill, event.payload.clone()) {
                    break;
                }
                last_sent = event.seq;
            }
            EventKind::Updated | EventKind::DiscussionUpdated => {
                if !deliver(&out, &kill, event.payload.clone()) {
                    break;
                }
            }
        }
    }
}

/// Convenience for building a `message.created` payload outside the connection code.
pub fn message_frame(discussion: Uuid, seq: i64, message: MessageDto) -> ServerFrame {
    ServerFrame::MessageCreated {
        discussion_id: discussion,
        seq,
        message,
    }
}
