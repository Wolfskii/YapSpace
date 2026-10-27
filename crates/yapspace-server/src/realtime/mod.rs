//! Real-time delivery: protocol frames, the in-process hub, per-socket connections, and connection caps.
//! See docs/architecture/realtime.md.

pub mod connection;
pub mod hub;
pub mod protocol;
pub mod slots;

use axum::extract::ws::Utf8Bytes;
use uuid::Uuid;

use crate::{api::dto::MessageDto, app::AppState, realtime::protocol::ServerFrame};

pub use hub::{Event, EventKind, Hub};

/// Serializes a frame once and publishes it to everyone subscribed to the discussion. Call this after the database
/// transaction has committed (realtime.md §G.3).
pub fn publish(state: &AppState, kind: EventKind, discussion: Uuid, seq: i64, frame: &ServerFrame) {
    let text = serde_json::to_string(frame).expect("server frames always serialize");
    state.hub.publish(Event {
        discussion_id: discussion,
        kind,
        seq,
        payload: Utf8Bytes::from(text),
    });
}

/// Announces a new message.
pub fn publish_created(state: &AppState, discussion: Uuid, seq: i64, message: MessageDto) {
    let frame = ServerFrame::MessageCreated {
        discussion_id: discussion,
        seq,
        message,
    };
    publish(state, EventKind::Created, discussion, seq, &frame);
}

/// Announces a change to an existing message, for example a deletion.
pub fn publish_updated(state: &AppState, discussion: Uuid, seq: i64, message: MessageDto) {
    let frame = ServerFrame::MessageUpdated {
        discussion_id: discussion,
        seq,
        message,
    };
    publish(state, EventKind::Updated, discussion, seq, &frame);
}
