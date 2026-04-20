use tokio::sync::broadcast;

const BUS_CAPACITY: usize = 256;

/// Typed payload for every bus event variant.
///
/// Replaces the old untyped tuple so the broadcast channel carries fully
/// typed data end-to-end. The `event()` helper recovers the corresponding
/// `BusEvent` discriminant for callers that still need it (e.g. logging,
/// routing in sse.rs).
// Tasks carries TaskItem which is a large struct (~760 bytes); the broadcast
// channel clones on every send so keeping variants inline is the right
// tradeoff — subscribers pattern-match and discard the unused variants
// immediately without ever storing the enum.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum BusPayload {
    Tasks(Option<api_types::TaskEventData>),
    Scout(Option<api_types::ScoutEventData>),
    Status(Option<api_types::StatusEventData>),
    Sessions(Option<api_types::SessionsEventData>),
    Notification(api_types::NotificationPayload),
    Workbenches(Option<api_types::WorkbenchEventData>),
    Config(Option<Box<api_types::MandoConfig>>),
    Research(Option<api_types::ResearchEventData>),
    Credentials(Option<api_types::CredentialsEventData>),
    Artifacts(Option<api_types::ArtifactEventData>),
}

impl BusPayload {
    pub fn event(&self) -> global_types::BusEvent {
        match self {
            BusPayload::Tasks(_) => global_types::BusEvent::Tasks,
            BusPayload::Scout(_) => global_types::BusEvent::Scout,
            BusPayload::Status(_) => global_types::BusEvent::Status,
            BusPayload::Sessions(_) => global_types::BusEvent::Sessions,
            BusPayload::Notification(_) => global_types::BusEvent::Notification,
            BusPayload::Workbenches(_) => global_types::BusEvent::Workbenches,
            BusPayload::Config(_) => global_types::BusEvent::Config,
            BusPayload::Research(_) => global_types::BusEvent::Research,
            BusPayload::Credentials(_) => global_types::BusEvent::Credentials,
            BusPayload::Artifacts(_) => global_types::BusEvent::Artifacts,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<BusPayload>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { tx }
    }

    pub fn send(&self, payload: BusPayload) {
        // Broadcast send returns `Err(SendError)` only when there are zero
        // subscribers, which is expected during startup and after graceful
        // shutdown. Drop the error rather than logging per-call noise.
        match self.tx.send(payload) {
            Ok(_) | Err(_) => {}
        }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<BusPayload> {
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
        bus.send(BusPayload::Tasks(Some(api_types::TaskEventData {
            action: Some("created".into()),
            item: None,
            id: Some(5),
            cleared_by: None,
        })));
        let payload = rx.recv().await.unwrap();
        assert!(matches!(payload.event(), global_types::BusEvent::Tasks));
        match payload {
            BusPayload::Tasks(Some(data)) => assert_eq!(data.id, Some(5)),
            other => panic!("unexpected payload: {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        bus.send(BusPayload::Status(None));
        let p1 = rx1.recv().await.unwrap();
        let p2 = rx2.recv().await.unwrap();
        assert!(matches!(p1.event(), global_types::BusEvent::Status));
        assert!(matches!(p2.event(), global_types::BusEvent::Status));
    }

    #[tokio::test]
    async fn send_without_subscribers_does_not_panic() {
        let bus = EventBus::new();
        bus.send(BusPayload::Status(None));
    }
}
