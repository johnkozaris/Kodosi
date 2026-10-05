use serde::Deserialize;

use super::{
    BTreeSet, BackendClient, Credentials, Error, IdentityBundle, Method, Result, Value,
    VerifiedIdentity, invalid, json, path_segment, wire,
};
use crate::identity::{
    friends::{Friend, FriendList, SignedFriendList, invite, parse_invite, root_hex},
    pins::Root,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredList {
    revision: u64,
    body: String,
    signature: String,
}

impl BackendClient {
    async fn kept_friends(&self, credentials: &Credentials) -> Result<Option<FriendList>> {
        self.inner
            .state
            .lock()
            .await
            .friend_lists
            .get(&credentials.user_id)
    }

    async fn sync_friends(&self, credentials: &Credentials) -> Result<(FriendList, u64)> {
        let kept = self.kept_friends(credentials).await?;
        let stored = match self
            .inner
            .http
            .device::<StoredList>(Method::GET, "api/me/friend-list", credentials, None)
            .await
        {
            Ok(stored) => Some(stored),
            Err(Error::Backend { status: 404, .. }) => None,
            Err(error) => return Err(error),
        };
        let kept_revision = kept.as_ref().map_or(0, |list| list.revision);
        let Some(stored) = stored else {
            let list = kept.unwrap_or_else(|| FriendList::new(&credentials.user_id));
            if kept_revision == 0 {
                return Ok((list, 0));
            }
            let list = self
                .put_friends(credentials, list, kept_revision, 0)
                .await?;
            return Ok((list, kept_revision));
        };
        let bundle: IdentityBundle = self
            .inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await?;
        let own = self.verify_bundle(&bundle, None).await?;
        let signed = SignedFriendList {
            body: stored.body,
            signature: stored.signature,
        };
        match FriendList::open(&signed, &credentials.user_id, &own.devices) {
            Ok(list) if list.revision >= kept_revision => {
                self.inner
                    .state
                    .lock()
                    .await
                    .friend_lists
                    .put(&credentials.user_id, signed)?;
                Ok((list, stored.revision))
            }
            Ok(_) | Err(_) => {
                let list = kept.unwrap_or_else(|| FriendList::new(&credentials.user_id));
                let revision = kept_revision.max(stored.revision) + 1;
                let list = self
                    .put_friends(credentials, list, revision, stored.revision)
                    .await?;
                Ok((list, revision))
            }
        }
    }

    async fn put_friends(
        &self,
        credentials: &Credentials,
        mut list: FriendList,
        revision: u64,
        expected: u64,
    ) -> Result<FriendList> {
        let signed = list.sign(
            revision,
            &credentials.keys.device_id,
            &credentials.keys.signing_key()?,
        )?;
        let _response: Value = self
            .inner
            .http
            .device(
                Method::PUT,
                "api/me/friend-list",
                credentials,
                Some(json!({"expectedRevision":expected,"revision":revision,"body":signed.body,"signature":signed.signature})),
            )
            .await?;
        self.inner
            .state
            .lock()
            .await
            .friend_lists
            .put(&credentials.user_id, signed)?;
        Ok(list)
    }

    async fn change_friends(
        &self,
        credentials: &Credentials,
        change: impl Fn(&mut FriendList) -> bool,
    ) -> Result<FriendList> {
        for _ in 0..3 {
            let (mut list, stored) = self.sync_friends(credentials).await?;
            if !change(&mut list) {
                return Ok(list);
            }
            let revision = list.revision.max(stored) + 1;
            match self.put_friends(credentials, list, revision, stored).await {
                Err(Error::Backend { status: 409, .. }) => {}
                result => return result,
            }
        }
        Err(Error::Busy)
    }

    pub(super) async fn resign_friends(&self, credentials: &Credentials) -> Result<()> {
        let (list, stored) = self.sync_friends(credentials).await?;
        if stored > 0 {
            self.put_friends(credentials, list, stored + 1, stored)
                .await?;
        }
        Ok(())
    }

    async fn friend_bundle(
        &self,
        credentials: &Credentials,
        user_id: &str,
    ) -> Result<IdentityBundle> {
        self.forget_identity(user_id);
        self.tagged_friend_bundle(credentials, user_id, None)
            .await?
            .map(|(bundle, _)| bundle)
            .ok_or_else(|| invalid("The server sent no device identity."))
    }

    async fn tagged_friend_bundle(
        &self,
        credentials: &Credentials,
        user_id: &str,
        known: Option<&str>,
    ) -> Result<Option<(IdentityBundle, String)>> {
        let fresh: Option<(IdentityBundle, String)> = self
            .inner
            .http
            .tagged(
                &format!("api/users/{}/identity", path_segment(user_id)?),
                credentials,
                true,
                known,
            )
            .await?;
        self.check_credentials(credentials)?;
        if fresh
            .as_ref()
            .is_some_and(|(bundle, _)| bundle.user_id != user_id)
        {
            return Err(Error::Trust(
                "Returned device identity belongs to another account.".into(),
            ));
        }
        Ok(fresh)
    }

    async fn recorded_root(
        &self,
        credentials: &Credentials,
        user_id: &str,
    ) -> Result<Option<Root>> {
        let changed = self
            .inner
            .state
            .lock()
            .await
            .changed_friends
            .contains(user_id);
        Ok(self
            .kept_friends(credentials)
            .await?
            .filter(|_| !changed)
            .and_then(|list| list.friends.get(user_id).and_then(Friend::root)))
    }

    pub(super) async fn friend_identity(
        &self,
        credentials: &Credentials,
        user_id: &str,
    ) -> Result<VerifiedIdentity> {
        let recorded = self.recorded_root(credentials, user_id).await?;
        let kept = self
            .kept_identity(credentials, user_id)
            .filter(|kept| recorded == Some(kept.identity.root));
        let fresh = self
            .tagged_friend_bundle(
                credentials,
                user_id,
                kept.as_ref().map(|kept| kept.tag.as_str()),
            )
            .await?;
        let (bundle, tag) = match (fresh, kept) {
            (None, Some(kept)) => {
                self.keep_identity(credentials, user_id, &kept.identity, kept.tag);
                return self.without_blocked_devices(user_id, kept.identity).await;
            }
            (None, None) => return Err(invalid("The server sent no device identity.")),
            (Some(fresh), _) => fresh,
        };
        let root = self.friend_root(credentials, user_id, &bundle).await?;
        if bundle.root()? != root {
            self.forget_identity(user_id);
            self.inner
                .state
                .lock()
                .await
                .changed_friends
                .insert(user_id.to_owned());
            self.inner.friends_changed.notify_one();
            return Err(Error::Trust(
                "This friend has a new identity. Trust it in Friends before you share or connect."
                    .into(),
            ));
        }
        let verified = self.verify_bundle(&bundle, Some(root)).await?;
        self.keep_identity(credentials, user_id, &verified, tag);
        self.without_blocked_devices(user_id, verified).await
    }

    async fn friend_root(
        &self,
        credentials: &Credentials,
        user_id: &str,
        bundle: &IdentityBundle,
    ) -> Result<Root> {
        let list = match self.kept_friends(credentials).await? {
            Some(list) if list.friends.contains_key(user_id) => list,
            _ => self.sync_friends(credentials).await?.0,
        };
        let friend = list.friends.get(user_id).ok_or_else(|| {
            Error::Trust(
                "Trust the identity of this friend in Friends before you share or connect.".into(),
            )
        })?;
        if let Some(root) = friend.root() {
            return Ok(root);
        }
        let pinned = self
            .inner
            .state
            .lock()
            .await
            .pins
            .lock()
            .map_err(|_| Error::Closed)?
            .root(user_id);
        let root = match pinned {
            Some(root) => root,
            None => bundle.root()?,
        };
        let recorded = self
            .change_friends(credentials, |list| {
                list.friends
                    .get_mut(user_id)
                    .filter(|friend| friend.root.is_none())
                    .is_some_and(|friend| {
                        friend.root = Some(root_hex(&root));
                        true
                    })
            })
            .await;
        if let Err(error) = recorded {
            tracing::warn!(%error, "the identity of a friend is fixed on this device only");
        }
        Ok(root)
    }

    pub(super) async fn require_trusted_friends(
        &self,
        credentials: &Credentials,
        users: &BTreeSet<String>,
    ) -> Result<()> {
        if users.is_empty() {
            return Ok(());
        }
        let mut list = self.kept_friends(credentials).await?;
        if !list
            .as_ref()
            .is_some_and(|list| users.iter().all(|user| list.friends.contains_key(user)))
        {
            list = Some(self.sync_friends(credentials).await?.0);
        }
        let changed = self.inner.state.lock().await.changed_friends.clone();
        if users.iter().all(|user| {
            !changed.contains(user)
                && list
                    .as_ref()
                    .is_some_and(|list| list.friends.contains_key(user))
        }) {
            Ok(())
        } else {
            Err(Error::Trust(
                "Trust the new identity of this friend in Friends before you share.".into(),
            ))
        }
    }

    pub(super) async fn friend_event(&self) -> Result<Value> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Ok(json!({"type":"friends.snapshot","friends":[],"incoming":[],"outgoing":[]}));
        }
        let (friends, requests) = self.friend_directory(&credentials).await?;
        let (list, _) = self.sync_friends(&credentials).await?;
        for friend in &friends {
            let user = wire::text(friend, "userId")?;
            if list
                .friends
                .get(user)
                .is_some_and(|friend| friend.root.is_none())
                && let Err(error) = self.friend_identity(&credentials, user).await
            {
                tracing::warn!(%error, "the identity of a new friend is not fixed yet");
            }
        }
        let list = self
            .kept_friends(&credentials)
            .await?
            .unwrap_or_else(|| FriendList::new(&credentials.user_id));
        let mut state = self.inner.state.lock().await;
        let replaced = {
            let pins = state.pins.lock().map_err(|_| Error::Closed)?;
            friends
                .iter()
                .filter_map(|friend| {
                    let user = friend.get("userId").and_then(Value::as_str)?;
                    let pinned = pins.incarnation(user)?.to_string();
                    (friend.get("identityIncarnationId").and_then(Value::as_str)
                        != Some(pinned.as_str()))
                    .then(|| user.to_owned())
                })
                .collect::<Vec<_>>()
        };
        state.changed_friends.extend(replaced);
        let friends = friends
            .iter()
            .map(|friend| {
                let user = wire::text(friend, "userId")?;
                let record = list.friends.get(user);
                let changed = record.is_none() || state.changed_friends.contains(user);
                Ok(json!({
                    "userId": user,
                    "handle": record.map_or(wire::text(friend, "handle")?, |record| record.handle.as_str()),
                    "displayName": friend["displayName"],
                    "verified": !changed && record.is_some_and(|record| record.verified),
                    "identityState": if changed { "changed" } else { "fixed" },
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        drop(state);
        Ok(
            json!({"type":"friends.snapshot","friends":friends,"incoming":requests["incoming"],"outgoing":requests["outgoing"]}),
        )
    }

    async fn friend_directory(&self, credentials: &Credentials) -> Result<(Vec<Value>, Value)> {
        let friends = self
            .inner
            .http
            .device(Method::GET, "api/friends", credentials, None)
            .await?;
        let requests = self
            .inner
            .http
            .device(Method::GET, "api/friends/requests", credentials, None)
            .await?;
        Ok((friends, requests))
    }

    async fn friend_user(&self, credentials: &Credentials, handle: &str) -> Result<String> {
        let (friends, requests) = self.friend_directory(credentials).await?;
        let listed = |people: Option<&Vec<Value>>| {
            people.into_iter().flatten().find_map(|person| {
                person
                    .get("handle")
                    .and_then(Value::as_str)
                    .filter(|listed| listed.eq_ignore_ascii_case(handle))
                    .and_then(|_| person.get("userId").and_then(Value::as_str))
                    .map(str::to_owned)
            })
        };
        listed(Some(&friends))
            .or_else(|| listed(requests["outgoing"].as_array()))
            .or_else(|| listed(requests["incoming"].as_array()))
            .ok_or_else(|| invalid("This person is not in your friends or requests."))
    }

    async fn friend_request(
        &self,
        credentials: &Credentials,
        action: &str,
        handle: &str,
    ) -> Result<()> {
        let (method, path, body) = match action {
            "send" => (
                Method::POST,
                "api/friends/requests".to_owned(),
                Some(json!({"username":handle})),
            ),
            "remove" => (
                Method::DELETE,
                format!("api/friends/{}", path_segment(handle)?),
                None,
            ),
            action => (
                Method::POST,
                format!("api/friends/requests/{}/{action}", path_segment(handle)?),
                None,
            ),
        };
        let _response: Value = self
            .inner
            .http
            .device(method, &path, credentials, body)
            .await?;
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one closed set of friend commands, each a few lines"
    )]
    pub(super) async fn friend_command(&self, operation: &str, args: &Value) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        if operation == "friends.invite" {
            let profile: Value = self
                .inner
                .http
                .bearer(Method::GET, "api/me", &credentials.token, None)
                .await?;
            let own = self
                .fetch_identity_with(&credentials, &credentials.user_id)
                .await?;
            return Ok(vec![
                json!({"type":"friends.invite","text":invite(wire::text(&profile, "handle")?, &own.root)}),
            ]);
        }
        let typed = wire::text(args, "username")?;
        match operation {
            "friends.request.send" => {
                let (handle, root) = match parse_invite(typed) {
                    Some((handle, root)) => (handle, Some(root)),
                    None => (typed.to_owned(), None),
                };
                let known = self
                    .sync_friends(&credentials)
                    .await?
                    .0
                    .by_handle(&handle)
                    .and_then(|(_, friend)| friend.root());
                if let (Some(root), Some(known)) = (root, known)
                    && root != known
                {
                    return Err(Error::Trust(
                        "This invite is for another identity than the one that Kodosi has for this friend.".into(),
                    ));
                }
                self.friend_request(&credentials, "send", &handle).await?;
                let user = self.friend_user(&credentials, &handle).await?;
                self.change_friends(&credentials, |list| {
                    let mut friend = list.friends.get(&user).cloned().unwrap_or_else(|| Friend {
                        handle: handle.clone(),
                        root: None,
                        verified: false,
                    });
                    if let Some(root) = root {
                        friend.root = Some(root_hex(&root));
                        friend.verified = true;
                    }
                    list.friends.insert(user.clone(), friend.clone()) != Some(friend)
                })
                .await?;
            }
            "friends.request.accept" => {
                let user = self.friend_user(&credentials, typed).await?;
                self.change_friends(&credentials, |list| {
                    if list.friends.contains_key(&user) {
                        return false;
                    }
                    list.friends.insert(
                        user.clone(),
                        Friend {
                            handle: typed.to_ascii_lowercase(),
                            root: None,
                            verified: false,
                        },
                    );
                    true
                })
                .await?;
                self.friend_request(&credentials, "accept", typed).await?;
            }
            "friends.request.reject" => self.friend_request(&credentials, "reject", typed).await?,
            "friends.request.cancel" | "friends.remove" => {
                if operation == "friends.remove" {
                    self.exclude_friend(&credentials, typed).await?;
                }
                let action = operation.rsplit('.').next().unwrap_or_default();
                self.friend_request(&credentials, action, typed).await?;
                self.change_friends(&credentials, |list| {
                    let user = list.by_handle(typed).map(|(user, _)| user.clone());
                    user.is_some_and(|user| list.friends.remove(&user).is_some())
                })
                .await?;
            }
            "friends.identity.trust" => {
                let user = self.friend_user(&credentials, typed).await?;
                let bundle = self.friend_bundle(&credentials, &user).await?;
                let root = bundle.root()?;
                self.change_friends(&credentials, |list| {
                    let next = Friend {
                        handle: typed.to_ascii_lowercase(),
                        root: Some(root_hex(&root)),
                        verified: false,
                    };
                    list.friends
                        .insert(user.clone(), next.clone())
                        .is_none_or(|before| before.root != next.root)
                })
                .await?;
                self.verify_bundle(&bundle, Some(root)).await?;
                self.inner.state.lock().await.changed_friends.remove(&user);
            }
            "friends.verify" => {
                let (handle, root) = parse_invite(wire::text(args, "invite")?)
                    .ok_or_else(|| invalid("This text is not a Kodosi invite."))?;
                if !handle.eq_ignore_ascii_case(typed) {
                    return Err(Error::Trust("This invite is for another person.".into()));
                }
                let user = self.friend_user(&credentials, typed).await?;
                let bundle = self.friend_bundle(&credentials, &user).await?;
                if bundle.root()? != root {
                    return Err(Error::Trust(
                        "This invite is not for the identity that this friend has now. Ask them for a new invite.".into(),
                    ));
                }
                self.change_friends(&credentials, |list| {
                    let friend = Friend {
                        handle: handle.clone(),
                        root: Some(root_hex(&root)),
                        verified: true,
                    };
                    list.friends.insert(user.clone(), friend.clone()) != Some(friend)
                })
                .await?;
                self.verify_bundle(&bundle, Some(root)).await?;
                self.inner.state.lock().await.changed_friends.remove(&user);
            }
            _ => return Err(invalid("Unsupported friend command.")),
        }
        let event = self.friend_event().await?;
        if matches!(
            operation,
            "friends.remove" | "friends.identity.trust" | "friends.verify"
        ) {
            self.review_access().await?;
        }
        Ok(vec![event])
    }
}
