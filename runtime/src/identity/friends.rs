use std::{collections::BTreeMap, path::PathBuf};

use aws_lc_rs::signature::{ML_DSA_65, PqdsaKeyPair, VerificationAlgorithm};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use super::{
    device_cert::DeviceCertificate,
    pins::{Root, decode},
    storage::private_write,
    wire_codec::ML_DSA_65_SIGNATURE_LEN,
};
use crate::network::{Error, Result, crypto, invalid};

const FRIEND_LIST_V1: &[u8] = b"kodosi-friend-list-v1";
const MAX_BODY_LEN: usize = 256 * 1024;
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
    signer_device_id: String,
    pub(crate) friends: BTreeMap<String, Friend>,
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
            signer_device_id: String::new(),
            friends: BTreeMap::new(),
        }
    }

    pub(crate) fn by_handle(&self, handle: &str) -> Option<(&String, &Friend)> {
        self.friends
            .iter()
            .find(|(_, friend)| friend.handle.eq_ignore_ascii_case(handle))
    }

    pub(crate) fn sign(
        &mut self,
        revision: u64,
        device_id: &str,
        key: &PqdsaKeyPair,
    ) -> Result<SignedFriendList> {
        self.revision = revision;
        device_id.clone_into(&mut self.signer_device_id);
        let body = serde_json::to_vec(self)?;
        if body.len() > MAX_BODY_LEN {
            return Err(invalid("The friend list is too large."));
        }
        let signature = crypto::sign(key, &signed(&body))?;
        Ok(SignedFriendList {
            body: BASE64.encode(body),
            signature: BASE64.encode(signature),
        })
    }

    pub(crate) fn open(
        signed_list: &SignedFriendList,
        user_id: &str,
        devices: &BTreeMap<String, DeviceCertificate>,
    ) -> Result<Self> {
        let body = decode(&signed_list.body, MAX_BODY_LEN)?;
        let list = Self::parse(&body, user_id)?;
        let signer = devices.get(&list.signer_device_id).ok_or_else(|| {
            Error::Trust("The friend list is not signed by an approved device.".into())
        })?;
        ML_DSA_65
            .verify_sig(
                &signer.sig_public_key,
                &signed(&body),
                &decode(&signed_list.signature, ML_DSA_65_SIGNATURE_LEN)?,
            )
            .map_err(|_| Error::Trust("The friend list signature is not valid.".into()))?;
        Ok(list)
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
        private_write(&self.path, &serde_json::to_vec(&next)?)?;
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
    for (byte, pair) in root.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
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

    const USER: &str = "11111111-1111-1111-1111-111111111111";

    fn device(keys: &DeviceKeys) -> BTreeMap<String, DeviceCertificate> {
        let signed_certificate = build_self_cert(
            USER,
            &keys.device_id,
            "device",
            keys.kem_public(),
            &keys.signing_key().unwrap(),
            1000,
            None,
        )
        .unwrap();
        let certificate = DeviceCertificate::parse_body(&signed_certificate.body_bytes).unwrap();
        BTreeMap::from([(keys.device_id.clone(), certificate)])
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

    #[test]
    fn a_friend_list_opens_only_with_the_signature_of_an_approved_device_of_the_account() {
        let (keys, other) = (
            DeviceKeys::generate().unwrap(),
            DeviceKeys::generate().unwrap(),
        );
        let mut list = list();
        let signed_list = list
            .sign(4, &keys.device_id, &keys.signing_key().unwrap())
            .unwrap();
        let opened = FriendList::open(&signed_list, USER, &device(&keys)).unwrap();
        assert_eq!(opened, list);
        assert_eq!(opened.revision, 4);
        assert_eq!(opened.by_handle("BOB").unwrap().1.root(), Some([9; 32]));
        assert!(matches!(
            FriendList::open(&signed_list, USER, &device(&other)),
            Err(Error::Trust(_))
        ));
        assert!(
            FriendList::open(
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
            FriendList::open(&forged, USER, &device(&keys)),
            Err(Error::Trust(_))
        ));
    }

    #[test]
    fn friend_lists_are_kept_for_each_account_on_disk() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("friend-lists.json");
        let keys = DeviceKeys::generate().unwrap();
        let mut list = list();
        let signed_list = list
            .sign(1, &keys.device_id, &keys.signing_key().unwrap())
            .unwrap();
        let mut lists = FriendLists::load(path.clone()).unwrap();
        assert_eq!(lists.get(USER).unwrap(), None);
        lists.put(USER, signed_list).unwrap();
        assert_eq!(
            FriendLists::load(path).unwrap().get(USER).unwrap(),
            Some(list)
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
