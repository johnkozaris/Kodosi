use serde::Deserialize;

#[cfg(feature = "host")]
use super::BTreeSet;
use super::{
    BackendClient, Credentials, Error, IdentityBundle, Method, Result, Value, VerifiedIdentity,
    invalid, json, path_segment, untrusted, wire,
};
use crate::identity::{
    friends::{
        Chain, Friend, FriendList, Head, Newest, SignedFriendList, broken, invite, parse_invite,
        root_hex,
    },
    pins::Root,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredLists {
    revision: u64,
    lists: Vec<SignedFriendList>,
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

    async fn stored_friends(
        &self,
        credentials: &Credentials,
        after: Option<u64>,
    ) -> Result<Option<StoredLists>> {
        let path = after.map_or_else(
            || "api/me/friend-list".to_owned(),
            |after| format!("api/me/friend-list?after={after}"),
        );
        match self
            .inner
            .http
            .device(Method::GET, &path, credentials, None)
            .await
        {
            Ok(stored) => Ok(Some(stored)),
            Err(Error::Backend { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn sync_friends(
        &self,
        credentials: &Credentials,
    ) -> Result<(FriendList, Head)> {
        for _ in 0..3 {
            match self.follow_friends(credentials).await {
                Err(Error::Backend { status: 409, .. }) => {}
                result => return result,
            }
        }
        Err(Error::Busy)
    }

    async fn follow_friends(&self, credentials: &Credentials) -> Result<(FriendList, Head)> {
        let user = &credentials.user_id;
        let kept = self.inner.state.lock().await.friend_lists.signed(user);
        let kept_head = kept
            .as_ref()
            .map(|kept| FriendList::read(kept, user))
            .transpose()?;
        let stored = self
            .stored_friends(
                credentials,
                kept_head
                    .as_ref()
                    .map(|(_, head)| head.revision.saturating_sub(1)),
            )
            .await?;
        let Some(mut stored) = stored else {
            return match kept_head {
                Some((list, head)) => self.put_friends(credentials, list, &head, 0).await,
                None => Ok((FriendList::new(user), Head::default())),
            };
        };
        if kept_head
            .as_ref()
            .is_some_and(|(_, head)| stored.revision < head.revision)
        {
            return Err(broken());
        }
        let bundle: IdentityBundle = self
            .inner
            .http
            .bearer(Method::GET, "api/me/identity", &credentials.token, None)
            .await?;
        let own = self.verify_bundle(&bundle, None).await?;
        let pins = std::sync::Arc::clone(&self.inner.state.lock().await.pins);
        let removed = pins.lock().map_err(|_| Error::Closed)?.revoked(user);
        let mut chain = Chain::new(user, &own.devices, &removed);
        if let Some(kept) = kept {
            chain.anchor(kept)?;
        }
        loop {
            let more = !stored.lists.is_empty();
            for signed in stored.lists {
                chain.follow(signed)?;
            }
            if !more || chain.revision() >= stored.revision {
                break;
            }
            stored = self
                .stored_friends(credentials, Some(chain.revision()))
                .await?
                .ok_or(Error::Busy)?;
        }
        match chain.end()? {
            Newest::Signed(list, head, signed) => {
                if kept_head.is_none_or(|(_, kept)| kept != head) {
                    self.inner
                        .state
                        .lock()
                        .await
                        .friend_lists
                        .put(user, signed)?;
                }
                Ok((list, head))
            }
            Newest::Resign(list, head) => {
                let expected = head.revision;
                self.put_friends(credentials, list, &head, expected).await
            }
        }
    }

    async fn put_friends(
        &self,
        credentials: &Credentials,
        mut list: FriendList,
        on: &Head,
        expected: u64,
    ) -> Result<(FriendList, Head)> {
        let (signed, head) = list.sign(
            on,
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
                Some(json!({"expectedRevision":expected,"revision":head.revision,"body":signed.body,"signature":signed.signature})),
            )
            .await?;
        self.inner
            .state
            .lock()
            .await
            .friend_lists
            .put(&credentials.user_id, signed)?;
        Ok((list, head))
    }

    async fn change_friends(
        &self,
        credentials: &Credentials,
        change: impl Fn(&mut FriendList) -> bool,
    ) -> Result<FriendList> {
        for _ in 0..3 {
            let (mut list, head) = self.sync_friends(credentials).await?;
            if !change(&mut list) {
                return Ok(list);
            }
            let expected = head.revision;
            match self.put_friends(credentials, list, &head, expected).await {
                Err(Error::Backend { status: 409, .. }) => {}
                result => return result.map(|(list, _)| list),
            }
        }
        Err(Error::Busy)
    }

    pub(super) async fn resign_friends(&self, credentials: &Credentials) -> Result<()> {
        let (list, head) = self.sync_friends(credentials).await?;
        if head.revision > 0 {
            let expected = head.revision;
            self.put_friends(credentials, list, &head, expected).await?;
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
            return Err(untrusted(
                "Returned device identity belongs to another account.",
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
            return Err(Error::Trust(text!(
                "This friend has a new identity. Trust it in Friends before you share or connect."
            )));
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
            Error::Trust(text!(
                "Trust the identity of this friend in Friends before you share or connect."
            ))
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

    #[cfg(feature = "host")]
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
            Err(Error::Trust(text!(
                "Trust the new identity of this friend in Friends before you share."
            )))
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
                    "inviteNeeded": changed && record.is_none_or(|record| record.verified),
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

    /// The person whom a removal parts from: the friend that the signed list names with this
    /// user name, or else the person that the server names, unless the signed list knows that
    /// person by another name.
    async fn parting_user(&self, credentials: &Credentials, handle: &str) -> Result<String> {
        let list = self.sync_friends(credentials).await?.0;
        if let Some((user, _)) = list.by_handle(handle) {
            return Ok(user.clone());
        }
        let user = self.friend_user(credentials, handle).await?;
        if list.friends.contains_key(&user) {
            return Err(untrusted(
                "The server named another friend for this user name.",
            ));
        }
        Ok(user)
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
            .ok_or_else(|| Error::Text(text!("This person is not in your friends or requests.")))
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
            if !credentials.enrolled {
                return Err(Error::EnrollmentRequired);
            }
            let profile: Value = self
                .inner
                .http
                .bearer(Method::GET, "api/me", &credentials.token, None)
                .await?;
            let own = self
                .fetch_identity_with(&credentials, &credentials.user_id)
                .await?;
            return Ok(vec![
                json!({"type":"friends.result","requestId":args["requestId"],"operation":operation,"text":invite(wire::text(&profile, "handle")?, &own.root)}),
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
                    return Err(Error::Trust(text!(
                        "This invite is for another identity than the one that Kodosi has for this friend."
                    )));
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
                    let other = self.parting_user(&credentials, typed).await?;
                    #[cfg(feature = "host")]
                    self.exclude_friend(&other).await;
                    self.end_rooms_with(&credentials, &other).await?;
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
                let (user, record) = self
                    .sync_friends(&credentials)
                    .await?
                    .0
                    .trustable(typed)
                    .map(|(user, record)| (user.clone(), record.clone()))
                    .ok_or_else(|| {
                        Error::Trust(text!(
                            "Ask this friend for a new invite, and verify their identity with it."
                        ))
                    })?;
                let bundle = self.friend_bundle(&credentials, &user).await?;
                let root = bundle.root()?;
                self.change_friends(&credentials, |list| {
                    let next = Friend {
                        handle: record.handle.clone(),
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
                    .ok_or_else(|| Error::Text(text!("This text is not a Kodosi invite.")))?;
                if !handle.eq_ignore_ascii_case(typed) {
                    return Err(Error::Trust(text!("This invite is for another person.")));
                }
                let user = self.friend_user(&credentials, typed).await?;
                let bundle = self.friend_bundle(&credentials, &user).await?;
                if bundle.root()? != root {
                    return Err(Error::Trust(text!(
                        "This invite is not for the identity that this friend has now. Ask them for a new invite."
                    )));
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
        #[cfg(feature = "host")]
        if matches!(
            operation,
            "friends.remove" | "friends.identity.trust" | "friends.verify"
        ) {
            self.review_access().await?;
        }
        Ok(vec![event])
    }
}
