//! Real-time frames (docs/architecture/realtime.md §G.2). Each frame is one JSON text message.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::dto::{DiscussionDto, MessageDto};

pub const PROTOCOL_VERSION: u8 = 1;

/// Frames a client may send.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// Optional. Authenticates the connection with a session token.
    Hello {
        protocol: u8,
        #[serde(default)]
        client: Option<String>,
        #[serde(default)]
        token: Option<String>,
    },
    /// Starts receiving events for a discussion. `after_seq` is the last seq the client has.
    Subscribe {
        discussion_id: Uuid,
        #[serde(default)]
        after_seq: Option<i64>,
    },
    Unsubscribe {
        discussion_id: Uuid,
    },
    Ping {
        #[serde(default)]
        nonce: Option<u64>,
    },
}

/// Frames the server sends.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ServerFrame {
    #[serde(rename = "welcome")]
    Welcome {
        protocol: u8,
        conn_id: Uuid,
        heartbeat_s: u64,
        max_subscriptions: usize,
    },
    #[serde(rename = "hello_ok")]
    HelloOk { user_id: Option<Uuid> },
    /// Sent after the backlog has been replayed. `last_seq` is the last seq the client now has.
    #[serde(rename = "subscribed")]
    Subscribed { discussion_id: Uuid, last_seq: i64 },
    #[serde(rename = "message.created")]
    MessageCreated {
        discussion_id: Uuid,
        seq: i64,
        message: MessageDto,
    },
    #[serde(rename = "message.updated")]
    MessageUpdated {
        discussion_id: Uuid,
        seq: i64,
        message: MessageDto,
    },
    #[serde(rename = "discussion.updated")]
    DiscussionUpdated {
        discussion_id: Uuid,
        discussion: DiscussionDto,
    },
    /// The client has missed too much, or lagged. It should reload the newest page and resubscribe.
    #[serde(rename = "resync_required")]
    ResyncRequired {
        discussion_id: Uuid,
        reason: &'static str,
    },
    #[serde(rename = "pong")]
    Pong { nonce: Option<u64> },
    #[serde(rename = "error")]
    Error { code: &'static str, detail: String },
}

/// WebSocket close codes from §G.2.
pub mod close {
    pub const NORMAL: u16 = 1000;
    pub const GOING_AWAY: u16 = 1001;
    pub const ORIGIN_REJECTED: u16 = 1008;
    pub const FRAME_TOO_BIG: u16 = 1009;
    pub const INTERNAL: u16 = 1011;
    pub const RESTARTING: u16 = 1012;
    pub const PROTOCOL_ERROR: u16 = 4000;
    pub const IDLE_TIMEOUT: u16 = 4004;
    pub const SLOW_CONSUMER: u16 = 4008;
    pub const TOO_MANY_CONNECTIONS: u16 = 4029;
}

/// Most subscriptions a single connection may hold.
pub const MAX_SUBSCRIPTIONS: usize = 5;
/// Largest backlog a subscribe will replay. More than this and the client must reload (§G.3 step 4).
pub const MAX_REPLAY: i64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_frame_parses_with_and_without_cursor() {
        let id = Uuid::now_v7();
        let with: ClientFrame = serde_json::from_str(&format!(
            r#"{{"type":"subscribe","discussion_id":"{id}","after_seq":5}}"#
        ))
        .unwrap();
        assert_eq!(
            with,
            ClientFrame::Subscribe {
                discussion_id: id,
                after_seq: Some(5)
            }
        );
        let without: ClientFrame =
            serde_json::from_str(&format!(r#"{{"type":"subscribe","discussion_id":"{id}"}}"#)).unwrap();
        assert!(matches!(without, ClientFrame::Subscribe { after_seq: None, .. }));
    }

    #[test]
    fn unknown_frame_type_is_a_parse_error() {
        assert!(serde_json::from_str::<ClientFrame>(r#"{"type":"bogus"}"#).is_err());
    }

    #[test]
    fn server_frames_use_the_documented_type_names() {
        let frame = ServerFrame::Pong { nonce: Some(7) };
        let json = serde_json::to_value(&frame).unwrap();
        assert_eq!(json["type"], "pong");
        assert_eq!(json["nonce"], 7);
        let resync = serde_json::to_value(ServerFrame::ResyncRequired {
            discussion_id: Uuid::nil(),
            reason: "gap_too_large",
        })
        .unwrap();
        assert_eq!(resync["type"], "resync_required");
        assert_eq!(resync["reason"], "gap_too_large");
    }
}
