use super::{
    BTreeMap, BTreeSet, Credentials, Error, Method, Network, Publication, Result, SessionDto,
};
use crate::identity::device_cert::DeviceCertificate;
use crate::identity::pins::VerifiedIdentity;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidates {
    authorization_revision: u64,
    key_generation: u32,
    devices: Vec<Recipient>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Recipient {
    user_id: String,
    device_id: String,
}

pub(super) async fn audience(
    network: &Network,
    publication: &Publication,
    credentials: &Credentials,
    dto: &SessionDto,
) -> Result<BTreeMap<(String, String), DeviceCertificate>> {
    let candidates: Candidates = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{}/devices", dto.id),
            credentials,
            None,
        )
        .await?;
    if candidates.authorization_revision != dto.authorization_revision
        || candidates.key_generation != dto.key_generation
    {
        return Err(Error::Stale);
    }
    let selected = publication.info.read().await.shared_with.clone();
    let mut grouped = BTreeMap::<String, BTreeSet<String>>::new();
    for candidate in candidates.devices {
        if candidate.user_id != credentials.user_id && !selected.contains(&candidate.user_id) {
            return Err(Error::Trust(
                "The server proposed an unselected terminal recipient.".into(),
            ));
        }
        grouped
            .entry(candidate.user_id)
            .or_default()
            .insert(candidate.device_id);
    }
    if !grouped.contains_key(&credentials.user_id) {
        return Err(Error::EnrollmentRequired);
    }
    let mut audience = BTreeMap::new();
    for (user, devices) in grouped {
        let verified = network.fetch_identity_with(credentials, &user, true).await;
        let recipients = usable_identity(&credentials.user_id, &user, verified)?;
        for (id, certificate) in recipients {
            if devices.contains(&id) {
                audience.insert((user.clone(), id), certificate);
            }
        }
    }
    if !audience.contains_key(&(
        credentials.user_id.clone(),
        credentials.keys.device_id.clone(),
    )) {
        return Err(Error::EnrollmentRequired);
    }
    Ok(audience)
}

fn usable_identity(
    owner: &str,
    user: &str,
    verified: Result<VerifiedIdentity>,
) -> Result<BTreeMap<String, DeviceCertificate>> {
    match verified {
        Ok(identity) => Ok(identity.devices),
        Err(error) if user == owner => Err(error),
        Err(Error::Stale | Error::SignedOut | Error::Closed) => Err(Error::Stale),
        Err(_) => Ok(BTreeMap::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unusable_friend_is_excluded_without_invalidating_owner_identity() {
        assert!(
            usable_identity("owner", "friend", Err(Error::Trust("expired list".into())))
                .unwrap()
                .is_empty()
        );
        assert!(
            usable_identity(
                "owner",
                "friend",
                Err(Error::Trust("changed pinned keys".into()))
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            usable_identity("owner", "owner", Err(Error::Trust("expired list".into()))).is_err()
        );
        assert!(usable_identity("owner", "friend", Err(Error::Stale)).is_err());
    }
}
