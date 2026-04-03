//! SSE event broadcast channel using `tokio::sync::broadcast`.
//!
//! The `EventBus` holds a broadcast sender. Any number of subscribers can
//! listen for events (e.g. SSE connections in the web dashboard).

use mando_types::BusEvent;
use serde_json::Value;
use tokio::sync::broadcast;

/// Broadcast capacity — number of events buffered before slow receivers lag.
const BUS_CAPACITY: usize = 256;

/// SSE event bus backed by a tokio broadcast channel.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<(BusEvent, Option<Value>)>,
}

impl EventBus {
    /// Create a new event bus with default capacity (256).
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { tx }
    }

    /// Send an event to all subscribers. Silently drops if no subscribers.
    pub fn send(&self, event: BusEvent, data: Option<Value>) {
        // Ignore send errors (no active receivers).
        let _ = self.tx.send((event, data));
    }

    /// Subscribe to the event bus. Returns a receiver that gets all future events.
    pub fn subscribe(&self) -> broadcast::Receiver<(BusEvent, Option<Value>)> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_and_receive() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.send(BusEvent::Tasks, Some(serde_json::json!({"count": 5})));

        let (event, data) = rx.recv().await.unwrap();
        assert_eq!(event, BusEvent::Tasks);
        assert_eq!(data.unwrap()["count"], 5);
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.send(BusEvent::Status, None);

        let (ev1, _) = rx1.recv().await.unwrap();
        let (ev2, _) = rx2.recv().await.unwrap();
        assert_eq!(ev1, BusEvent::Status);
        assert_eq!(ev2, BusEvent::Status);
    }

    #[tokio::test]
    async fn send_without_subscribers_does_not_panic() {
        let bus = EventBus::new();
        bus.send(BusEvent::Status, None); // no receivers — should not panic
    }
}
