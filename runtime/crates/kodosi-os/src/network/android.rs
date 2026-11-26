use super::{Link, Medium, Wakes, unix};

/// Android shows an app neither the hardware address of a card nor the one of the router. A card
/// that can broadcast is the Wi-Fi card on a phone.
pub(super) fn links() -> Vec<Link> {
    unix::cards()
        .into_values()
        .filter_map(|card| {
            let (address, prefix) = card.address?;
            Some(Link {
                index: card.index,
                medium: Medium::Radio,
                hardware: None,
                address,
                prefix,
                router: None,
                wakes: Wakes::Unknown,
            })
        })
        .collect()
}

pub(super) const fn fast_startup() -> bool {
    false
}
