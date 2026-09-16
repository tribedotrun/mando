use tokio::sync::broadcast;

const BUS_CAPACITY: usize = 256;

/// Typed payload for every bus event variant.
///
/// Replaces the old untyped tuple so the broadcast channel carries fully
/// typed data end-to-end. Subscribers pattern-match the variant directly.
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
