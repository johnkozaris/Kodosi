use super::{Arc, CString, CancellationToken, HashMap, HashSet, Mutex, Uuid, callbacks::Gate};

pub(super) struct SubscriptionState {
    pub(super) session: CString,
    pub(super) id: CString,
    pub(super) generation: u64,
    pub(super) cancel: CancellationToken,
    pub(super) refresh: tokio::sync::Notify,
    pub(super) gate: Gate,
    pub(super) connected: Mutex<Option<(Uuid, Uuid)>>,
}
impl SubscriptionState {
    pub(super) fn matches(&self, id: &str, generation: u64) -> bool {
        self.id.as_bytes() == id.as_bytes() && self.generation == generation
    }
    pub(super) fn retire(&self) {
        self.gate.close();
        self.cancel.cancel();
    }
}
#[derive(Default)]
pub(super) struct Subscriptions {
    pub(super) entries: HashMap<Uuid, Arc<SubscriptionState>>,
    pub(super) generations: HashMap<Uuid, u64>,
    pub(super) sessions: HashSet<Uuid>,
}
impl Subscriptions {
    pub(super) fn catalog(&mut self, sessions: HashSet<Uuid>) {
        self.sessions = sessions;
        self.generations
            .retain(|id, _| self.sessions.contains(id) || self.entries.contains_key(id));
    }
}
