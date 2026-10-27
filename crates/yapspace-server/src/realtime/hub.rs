//! In-process fan-out: one broadcast channel per discussion that has at least one subscriber (realtime.md §G.3).
//!
//! The payload is serialized once, and each subscriber gets a cheap clone of the same bytes. The database stays the
//! source of truth. A lost event is recovered by the client's `after_seq` resume (§G.5).

use std::sync::Arc;

use axum::extract::ws::Utf8Bytes;
use dashmap::DashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Events buffered per discussion. A subscriber that falls further behind gets `Lagged` and must resync.
const CHANNEL_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Created,
    Updated,
    DiscussionUpdated,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub discussion_id: Uuid,
    pub kind: EventKind,
    pub seq: i64,
    /// The ready-to-send frame.
    pub payload: Utf8Bytes,
}

#[derive(Clone, Default)]
pub struct Hub {
    channels: Arc<DashMap<Uuid, broadcast::Sender<Arc<Event>>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to a discussion. The channel is created on first use.
    pub fn subscribe(&self, discussion: Uuid) -> broadcast::Receiver<Arc<Event>> {
        self.channels
            .entry(discussion)
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Publishes to every current subscriber. Returns how many received it. Nobody listening means nothing happens.
    pub fn publish(&self, event: Event) -> usize {
        let Some(sender) = self.channels.get(&event.discussion_id).map(|entry| entry.clone()) else {
            return 0;
        };
        sender.send(Arc::new(event)).unwrap_or(0)
    }

    /// Drops a discussion's channel once nobody listens to it any more.
    pub fn release(&self, discussion: Uuid) {
        self.channels
            .remove_if(&discussion, |_, sender| sender.receiver_count() == 0);
    }

    /// Number of discussions with a channel. Used by tests and metrics.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(discussion: Uuid, seq: i64) -> Event {
        Event {
            discussion_id: discussion,
            kind: EventKind::Created,
            seq,
            payload: Utf8Bytes::from(format!("{{\"seq\":{seq}}}")),
        }
    }

    #[tokio::test]
    async fn publish_without_subscribers_reaches_nobody() {
        let hub = Hub::new();
        assert_eq!(hub.publish(event(Uuid::now_v7(), 1)), 0);
        assert_eq!(hub.channel_count(), 0);
    }

    #[tokio::test]
    async fn every_subscriber_receives_each_event_in_order() {
        let hub = Hub::new();
        let discussion = Uuid::now_v7();
        let mut a = hub.subscribe(discussion);
        let mut b = hub.subscribe(discussion);
        assert_eq!(hub.publish(event(discussion, 1)), 2);
        assert_eq!(hub.publish(event(discussion, 2)), 2);
        for rx in [&mut a, &mut b] {
            assert_eq!(rx.recv().await.unwrap().seq, 1);
            assert_eq!(rx.recv().await.unwrap().seq, 2);
        }
    }

    #[tokio::test]
    async fn release_removes_the_channel_only_when_nobody_listens() {
        let hub = Hub::new();
        let discussion = Uuid::now_v7();
        let rx = hub.subscribe(discussion);
        hub.release(discussion);
        assert_eq!(hub.channel_count(), 1, "a listener is still attached");
        drop(rx);
        hub.release(discussion);
        assert_eq!(hub.channel_count(), 0);
    }

    #[tokio::test]
    async fn a_slow_subscriber_sees_lag_instead_of_blocking_publishers() {
        let hub = Hub::new();
        let discussion = Uuid::now_v7();
        let mut slow = hub.subscribe(discussion);
        for seq in 0..(CHANNEL_CAPACITY as i64 + 10) {
            hub.publish(event(discussion, seq));
        }
        assert!(matches!(
            slow.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
    }
}
