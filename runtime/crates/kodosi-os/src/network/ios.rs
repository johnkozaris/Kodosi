use super::Link;

/// iOS lets only an app with a restricted entitlement send a broadcast, so an iPhone or iPad
/// sends no wake signal and needs none of its cards.
pub(super) const fn links() -> Vec<Link> {
    Vec::new()
}

pub(super) const fn fast_startup() -> bool {
    false
}
