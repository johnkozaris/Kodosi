use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use aws_lc_rs::{
    digest::{SHA256, digest},
    signature::{ML_DSA_65, PqdsaKeyPair, VerificationAlgorithm},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use super::{
    device_cert::DeviceCertificate,
    pins::{Root, decode},
    wire_codec::ML_DSA_65_SIGNATURE_LEN,
};
use crate::network::{Error, Result, crypto, invalid, untrusted};
use kodosi_os::fs::write_private;

const FRIEND_LIST_V1: &[u8] = b"kodosi-friend-list-v1";
const MAX_BODY_LEN: usize = 256 * 1024;
const MAX_FOLLOWED: usize = 1025;
const INVITE: &str = "kodosi";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Friend {
    pub(crate) handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) root: Option<String>,
    pub(crate) verified: bool,
}

impl Friend {
    pub(crate) fn root(&self) -> Option<Root> {
        root_from_hex(self.root.as_deref()?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FriendList {
    version: u8,
    user_id: String,
    pub(crate) revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous: Option<String>,
    signer_device_id: String,
    pub(crate) friends: BTreeMap<String, Friend>,
}

/// The newest signed list of an account, which the next list goes on top of.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Head {
    pub(crate) revision: u64,
    digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SignedFriendList {
    pub(crate) body: String,
    pub(crate) signature: String,
}

impl FriendList {
    pub(crate) fn new(user_id: &str) -> Self {
        Self {
            version: 1,
            user_id: user_id.to_owned(),
            revision: 0,
            previous: None,
            signer_device_id: String::new(),
            friends: BTreeMap::new(),
        }
    }

    pub(crate) fn by_handle(&self, handle: &str) -> Option<(&String, &Friend)> {
        self.friends
            .iter()
            .find(|(_, friend)| friend.handle.eq_ignore_ascii_case(handle))
    }

    /// The friend whose new identity a click can trust: one that has a record and was never
    /// verified. A verified friend proves a new identity with a new invite.
    pub(crate) fn trustable(&self, handle: &str) -> Option<(&String, &Friend)> {
        self.by_handle(handle)
            .filter(|(_, friend)| !friend.verified)
    }

    pub(crate) fn sign(
        &mut self,
        on: &Head,
        device_id: &str,
        key: &PqdsaKeyPair,
    ) -> Result<(SignedFriendList, Head)> {
        self.revision = on
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("The friend list revision is too large."))?;
        self.previous.clone_from(&on.digest);
        device_id.clone_into(&mut self.signer_device_id);
        let body = serde_json::to_vec(self)?;
        if body.len() > MAX_BODY_LEN {
            return Err(invalid("The friend list is too large."));
        }
        let signature = crypto::sign(key, &signed(&body))?;
        let head = Head {
            revision: self.revision,
            digest: Some(body_digest(&body)),
        };
        Ok((
            SignedFriendList {
                body: BASE64.encode(body),
                signature: BASE64.encode(signature),
            },
            head,
        ))
    }

    pub(crate) fn read(signed_list: &SignedFriendList, user_id: &str) -> Result<(Self, Head)> {
        let body = decode(&signed_list.body, MAX_BODY_LEN)?;
        let list = Self::parse(&body, user_id)?;
        let head = Head {
            revision: list.revision,
            digest: Some(body_digest(&body)),
        };
        Ok((list, head))
    }

    fn verify(
        &self,
        body: &[u8],
        signature: &str,
        devices: &BTreeMap<String, DeviceCertificate>,
    ) -> Result<()> {
        let signer = devices
            .get(&self.signer_device_id)
            .ok_or_else(|| untrusted("The friend list is not signed by an approved device."))?;
        ML_DSA_65
            .verify_sig(
                &signer.sig_public_key,
                &signed(body),
                &decode(signature, ML_DSA_65_SIGNATURE_LEN)?,
            )
            .map_err(|_| untrusted("The friend list signature is not valid."))
    }

    fn parse(body: &[u8], user_id: &str) -> Result<Self> {
        let list: Self = serde_json::from_slice(body)?;
        if list.version != 1 || list.user_id != user_id {
            return Err(invalid("The friend list belongs to another account."));
        }
        Ok(list)
    }
}

fn signed(body: &[u8]) -> Vec<u8> {
    [FRIEND_LIST_V1, body].concat()
}

fn body_digest(body: &[u8]) -> String {
    let digest = digest(&SHA256, body);
    let mut bytes = [0; 32];
    bytes.copy_from_slice(digest.as_ref());
    root_hex(&bytes)
}

pub(crate) fn broken() -> Error {
    Error::Trust(text!(
        "The friend list on the server does not continue the friend list on this device. Remove this device from another approved device, then approve it again."
    ))
}

/// What a device does with the lists it followed.
#[derive(Debug)]
pub(crate) enum Newest {
    /// The device takes the newest list, which an approved device signed.
    Signed(FriendList, Head, SignedFriendList),
    /// The device signs its newest trusted list again, on top of a newest list that a removed
    /// device signed.
    Resign(FriendList, Head),
}

/// Follows the signed lists of an account in revision order. Each list names the SHA-256 of
/// the list before it, so a device takes only lists that continue the list it keeps. A list
/// of an approved device needs its valid signature; the newest list may come from a device
/// that is not approved only when this device knows that it was removed.
pub(crate) struct Chain<'a> {
    user_id: &'a str,
    devices: &'a BTreeMap<String, DeviceCertificate>,
    removed: &'a BTreeSet<String>,
    head: Option<Head>,
    trusted: Option<FriendList>,
    signed: Option<SignedFriendList>,
    unknown: bool,
    followed: usize,
}

impl<'a> Chain<'a> {
    pub(crate) const fn new(
        user_id: &'a str,
        devices: &'a BTreeMap<String, DeviceCertificate>,
        removed: &'a BTreeSet<String>,
    ) -> Self {
        Self {
            user_id,
            devices,
            removed,
            head: None,
            trusted: None,
            signed: None,
            unknown: false,
            followed: 0,
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.head.as_ref().map_or(0, |head| head.revision)
    }

    /// Starts from the list that this device keeps, which it trusts even when the device
    /// that signed it was removed.
    pub(crate) fn anchor(&mut self, kept: SignedFriendList) -> Result<()> {
        self.step(kept, true)
    }

    pub(crate) fn follow(&mut self, signed_list: SignedFriendList) -> Result<()> {
        self.step(signed_list, false)
    }

    fn step(&mut self, signed_list: SignedFriendList, kept: bool) -> Result<()> {
        self.followed += 1;
        if self.followed > MAX_FOLLOWED {
            return Err(broken());
        }
        let body = decode(&signed_list.body, MAX_BODY_LEN)?;
        let list = FriendList::parse(&body, self.user_id)?;
        let next = Head {
            revision: list.revision,
            digest: Some(body_digest(&body)),
        };
        match &self.head {
            Some(head) if *head == next => return Ok(()),
            Some(head)
                if head.revision.checked_add(1) != Some(list.revision)
                    || list.previous != head.digest =>
            {
                return Err(broken());
            }
            _ => {}
        }
        self.head = Some(next);
        let approved = self.devices.contains_key(&list.signer_device_id);
        if approved {
            list.verify(&body, &signed_list.signature, self.devices)?;
        }
        if approved || kept {
            self.trusted = Some(list);
            self.signed = approved.then_some(signed_list);
            self.unknown = false;
        } else {
            self.unknown = !self.removed.contains(&list.signer_device_id);
        }
        Ok(())
    }

    pub(crate) fn end(self) -> Result<Newest> {
        let (Some(head), Some(trusted)) = (self.head, self.trusted) else {
            return Err(untrusted(
                "The friend list is not signed by an approved device.",
            ));
        };
        if self.unknown {
            return Err(untrusted(
                "The friend list is not signed by an approved device.",
            ));
        }
        Ok(match self.signed {
            Some(signed_list) if trusted.revision == head.revision => {
                Newest::Signed(trusted, head, signed_list)
            }
            _ => Newest::Resign(trusted, head),
        })
    }
}

pub(crate) struct FriendLists {
    path: PathBuf,
    lists: BTreeMap<String, SignedFriendList>,
}

impl FriendLists {
    pub(crate) fn load(path: PathBuf) -> Result<Self> {
        let lists = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, lists })
    }

    pub(crate) fn signed(&self, user_id: &str) -> Option<SignedFriendList> {
        self.lists.get(user_id).cloned()
    }

    pub(crate) fn get(&self, user_id: &str) -> Result<Option<FriendList>> {
        self.lists
            .get(user_id)
            .map(|signed_list| {
                FriendList::parse(&decode(&signed_list.body, MAX_BODY_LEN)?, user_id)
            })
            .transpose()
    }

    pub(crate) fn put(&mut self, user_id: &str, signed_list: SignedFriendList) -> Result<()> {
        let mut next = self.lists.clone();
        next.insert(user_id.to_owned(), signed_list);
        write_private(&self.path, &serde_json::to_vec(&next)?)?;
        self.lists = next;
        Ok(())
    }

    /// Forgets the list of an account, as a device with new keys starts without one.
    pub(crate) fn forget(&mut self, user_id: &str) -> Result<()> {
        if !self.lists.contains_key(user_id) {
            return Ok(());
        }
        let mut next = self.lists.clone();
        next.remove(user_id);
        write_private(&self.path, &serde_json::to_vec(&next)?)?;
        self.lists = next;
        Ok(())
    }
}

pub(crate) fn root_hex(root: &Root) -> String {
    use std::fmt::Write as _;
    root.iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn root_from_hex(text: &str) -> Option<Root> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut root = [0; 32];
    for (byte, pair) in root.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(root)
}

pub(crate) fn invite(handle: &str, root: &Root) -> String {
    format!("{INVITE}:{handle}:{}", root_hex(root))
}

pub(crate) fn parse_invite(text: &str) -> Option<(String, Root)> {
    let mut parts = text.trim().split(':');
    let (INVITE, handle, root, None) = (parts.next()?, parts.next()?, parts.next()?, parts.next())
    else {
        return None;
    };
    (!handle.is_empty()).then_some((handle.to_ascii_lowercase(), root_from_hex(root)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{device_cert::build_self_cert, keys::DeviceKeys};
    use crate::network::Error;

    const USER: &str = "11111111-1111-1111-1111-111111111111";

    fn device(keys: &DeviceKeys) -> BTreeMap<String, DeviceCertificate> {
        let signed_certificate = build_self_cert(
            USER,
            &keys.device_id,
            "device",
            &keys.signing_key().unwrap(),
            1000,
            None,
        )
        .unwrap();
        let certificate = DeviceCertificate::parse_body(&signed_certificate.body_bytes).unwrap();
        BTreeMap::from([(keys.device_id.clone(), certificate)])
    }

    fn open(
        signed_list: &SignedFriendList,
        user_id: &str,
        devices: &BTreeMap<String, DeviceCertificate>,
    ) -> Result<FriendList> {
        let mut chain = Chain::new(user_id, devices, NO_REMOVED);
        chain.follow(signed_list.clone())?;
        match chain.end()? {
            Newest::Signed(list, ..) => Ok(list),
            Newest::Resign(..) => Err(invalid("The test list needs no new signature.")),
        }
    }

    fn list() -> FriendList {
        let mut list = FriendList::new(USER);
        list.friends.insert(
            "22222222-2222-2222-2222-222222222222".into(),
            Friend {
                handle: "bob".into(),
                root: Some(root_hex(&[9; 32])),
                verified: true,
            },
        );
        list
    }

    const NO_REMOVED: &BTreeSet<String> = &BTreeSet::new();

    #[test]
    fn a_friend_list_opens_only_with_the_signature_of_an_approved_device_of_the_account() {
        let (keys, other) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let mut list = list();
        let (signed_list, _) = list
            .sign(
                &Head {
                    revision: 3,
                    digest: None,
                },
                &keys.device_id,
                &keys.signing_key().unwrap(),
            )
            .unwrap();
        let opened = open(&signed_list, USER, &device(&keys)).unwrap();
        assert_eq!(opened, list);
        assert_eq!(opened.revision, 4);
        assert_eq!(opened.by_handle("BOB").unwrap().1.root(), Some([9; 32]));
        assert!(matches!(
            open(&signed_list, USER, &device(&other)),
            Err(Error::Trust(_))
        ));
        assert!(
            open(
                &signed_list,
                "33333333-3333-3333-3333-333333333333",
                &device(&keys)
            )
            .is_err()
        );
        let mut changed = list.clone();
        changed.friends.clear();
        let forged = SignedFriendList {
            body: BASE64.encode(serde_json::to_vec(&changed).unwrap()),
            ..signed_list
        };
        assert!(matches!(
            open(&forged, USER, &device(&keys)),
            Err(Error::Trust(_))
        ));
    }

    #[test]
    fn friend_lists_are_kept_for_each_account_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("friend-lists.json");
        let keys = DeviceKeys::generate().unwrap();
        let mut list = list();
        let (signed_list, _) = list
            .sign(
                &Head::default(),
                &keys.device_id,
                &keys.signing_key().unwrap(),
            )
            .unwrap();
        let mut lists = FriendLists::load(path.clone()).unwrap();
        assert_eq!(lists.get(USER).unwrap(), None);
        lists.put(USER, signed_list).unwrap();
        assert_eq!(
            FriendLists::load(path).unwrap().get(USER).unwrap(),
            Some(list)
        );
    }

    fn carol(list: &mut FriendList) {
        list.friends.insert(
            "33333333-3333-3333-3333-333333333333".into(),
            Friend {
                handle: "carol".into(),
                root: None,
                verified: false,
            },
        );
    }

    #[test]
    fn a_device_takes_only_friend_lists_that_continue_the_list_it_keeps() {
        let keys = DeviceKeys::generate().unwrap();
        let (key, devices) = (keys.signing_key().unwrap(), device(&keys));
        let mut first = list();
        let (kept, head) = first.sign(&Head::default(), &keys.device_id, &key).unwrap();
        let mut second = first.clone();
        second.friends.clear();
        let (newer, newer_head) = second.sign(&head, &keys.device_id, &key).unwrap();
        let mut chain = Chain::new(USER, &devices, NO_REMOVED);
        chain.anchor(kept.clone()).unwrap();
        chain.follow(kept.clone()).unwrap();
        chain.follow(newer.clone()).unwrap();
        assert!(matches!(
            chain.end().unwrap(),
            Newest::Signed(list, head, _) if list == second && head == newer_head
        ));

        let mut other = first.clone();
        carol(&mut other);
        let (fork, _) = other.sign(&head, &keys.device_id, &key).unwrap();
        let mut chain = Chain::new(USER, &devices, NO_REMOVED);
        chain.anchor(newer.clone()).unwrap();
        assert!(matches!(chain.follow(fork.clone()), Err(Error::Trust(_))));
        let mut chain = Chain::new(USER, &devices, NO_REMOVED);
        chain.anchor(kept.clone()).unwrap();
        chain.follow(fork).unwrap();
        assert!(matches!(chain.follow(newer), Err(Error::Trust(_))));

        let (third, _) = second
            .clone()
            .sign(&newer_head, &keys.device_id, &key)
            .unwrap();
        let mut chain = Chain::new(USER, &devices, NO_REMOVED);
        chain.anchor(kept).unwrap();
        assert!(matches!(chain.follow(third), Err(Error::Trust(_))));
    }

    #[test]
    fn a_device_signs_its_trusted_list_again_only_on_a_list_of_a_removed_device() {
        let (keys, removed) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let (key, devices) = (keys.signing_key().unwrap(), device(&keys));
        let gone = BTreeSet::from([removed.device_id.clone()]);
        let mut first = list();
        let (kept, head) = first.sign(&Head::default(), &keys.device_id, &key).unwrap();
        let mut late = first.clone();
        carol(&mut late);
        let (unsigned, late_head) = late
            .clone()
            .sign(&head, &removed.device_id, &removed.signing_key().unwrap())
            .unwrap();
        let mut chain = Chain::new(USER, &devices, &gone);
        chain.anchor(kept.clone()).unwrap();
        chain.follow(unsigned.clone()).unwrap();
        assert!(matches!(
            chain.end().unwrap(),
            Newest::Resign(list, head) if list == first && head == late_head
        ));

        let mut chain = Chain::new(USER, &devices, NO_REMOVED);
        chain.anchor(kept.clone()).unwrap();
        chain.follow(unsigned.clone()).unwrap();
        assert!(
            matches!(chain.end(), Err(Error::Trust(_))),
            "a device that this device does not know may be a new one"
        );

        let (mut forged, _) = late.sign(&head, &keys.device_id, &key).unwrap();
        forged.signature.clone_from(&unsigned.signature);
        let mut chain = Chain::new(USER, &devices, &gone);
        chain.anchor(kept).unwrap();
        assert!(
            matches!(chain.follow(forged), Err(Error::Trust(_))),
            "the list of an approved device needs its signature"
        );

        let mut chain = Chain::new(USER, &devices, &gone);
        chain.follow(unsigned).unwrap();
        assert!(matches!(chain.end(), Err(Error::Trust(_))));
    }

    #[test]
    fn a_click_trusts_only_a_recorded_friend_that_was_never_verified() {
        let mut list = list();
        assert!(list.trustable("bob").is_none());
        assert!(list.trustable("carol").is_none());
        list.friends.insert(
            "33333333-3333-3333-3333-333333333333".into(),
            Friend {
                handle: "carol".into(),
                root: Some(root_hex(&[7; 32])),
                verified: false,
            },
        );
        assert_eq!(
            list.trustable("Carol").map(|(user, _)| user.as_str()),
            Some("33333333-3333-3333-3333-333333333333")
        );
    }

    #[test]
    fn an_invite_text_gives_the_handle_and_the_identity() {
        let text = invite("alice", &[0xab; 32]);
        assert_eq!(text, format!("kodosi:alice:{}", "ab".repeat(32)));
        assert_eq!(
            parse_invite(&format!("  {text}\n")),
            Some(("alice".into(), [0xab; 32]))
        );
        for bad in ["alice", "kodosi:alice", "kodosi::abab", "other:alice:abab"] {
            assert_eq!(parse_invite(bad), None);
        }
        assert_eq!(parse_invite(&format!("{text}:more")), None);
        assert_eq!(parse_invite(&text[..text.len() - 2]), None);
    }
}
