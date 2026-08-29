#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendClientConfig {
    pub api: Option<String>,
    pub host_relay: Option<String>,
    pub viewer_relay: Option<String>,
    pub user_events: Option<String>,
}

impl BackendClientConfig {
    pub fn new(
        api: Option<String>,
        host_relay: Option<String>,
        viewer_relay: Option<String>,
        user_events: Option<String>,
    ) -> Self {
        Self {
            api,
            host_relay,
            viewer_relay,
            user_events,
        }
    }
}
