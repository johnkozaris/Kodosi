use super::*;
mod audience;
use audience::audience;

type PendingControls = futures_util::stream::FuturesUnordered<
    std::pin::Pin<Box<dyn Future<Output = (ControlIdentity, bool)> + Send>>,
>;

struct HostKeys {
    key: Zeroizing<crypto::SessionKey>,
    dto: SessionDto,
    devices: BTreeMap<(String, String), Vec<u8>>,
    checkpoint_counter: u64,
    raw_counter: u64,
    revision: u64,
    connections: BTreeMap<Uuid, Connection>,
    retired: RetiredConnections,
    authorization: CancellationToken,
}

struct RetiredConnections(Box<[u64]>);

impl Default for RetiredConnections {
    fn default() -> Self {
        Self(vec![0; 16_384].into_boxed_slice())
    }
}
impl RetiredConnections {
    fn indices(id: Uuid) -> [usize; 3] {
        use sha2::Digest as _;
        let hash = sha2::Sha256::digest(id.as_bytes());
        [0, 4, 8].map(|at| {
            usize::try_from(u32::from_be_bytes([
                hash[at],
                hash[at + 1],
                hash[at + 2],
                hash[at + 3],
            ]))
            .unwrap_or(0)
                % (16_384 * 64)
        })
    }
    fn insert(&mut self, id: Uuid) {
        for index in Self::indices(id) {
            self.0[index / 64] |= 1 << (index % 64);
        }
    }
    fn contains(&self, id: Uuid) -> bool {
        Self::indices(id)
            .iter()
            .all(|index| self.0[index / 64] & (1 << (index % 64)) != 0)
    }
}

struct Connection {
    user: String,
    device: String,
    sequence: u64,
    authorization: CancellationToken,
}

#[expect(
    clippy::too_many_lines,
    reason = "single owner select loop keeps ciphertext counters, authorization and ordered output together"
)]
pub(super) async fn run(
    network: &Network,
    publication: &Publication,
    output: &mut PublicationOutput,
    generation: u64,
) -> Result<()> {
    check_generation(network, generation)?;
    let credentials = network.credentials()?;
    let authorization = credentials.cancel.child_token();
    let _authorization_guard = authorization.clone().drop_guard();
    {
        let mut previous = publication.authorization.lock().await;
        previous.cancel();
        *previous = authorization.clone();
    }
    if publication.changing.load(Ordering::Acquire) {
        return Err(Error::Stale);
    }
    let current = current_publication(network, publication, &credentials).await?;
    let (mut socket, _ready) = socket(
        network,
        &credentials,
        &format!("ws/host/{}", current.id),
        "host",
        Some(&current),
        None,
    )
    .await?;
    check_generation(network, generation)?;
    let mut keys = tokio::select! {
        ()=authorization.cancelled()=>return Err(Error::Stale),
        result=distribute(network,publication,&credentials,authorization.clone())=>result?,
    };
    let result = tokio::select! {
        ()=authorization.cancelled()=>Err(Error::Stale),
        result=async {
        let marker=Uuid::now_v7();
        let initial=bootstrap(publication,marker).await?;
        drain_to_barrier(&mut socket,&mut keys,publication,output,marker,None).await?;
        let mut next_sequence=initial.next_sequence;
        send_checkpoint(&mut socket,&mut keys,initial,None).await?;
        let mut heartbeat=tokio::time::interval(Duration::from_secs(20));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut trust_refresh=tokio::time::interval(Duration::from_secs(30));
        trust_refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        trust_refresh.tick().await;
        let mut pending=PendingControls::new();
        let mut metadata_dirty=false;
        let mut metadata_tick=tokio::time::interval(Duration::from_secs(1));
        metadata_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            check_generation(network,generation)?;
            if authorization.is_cancelled(){return Err(Error::Stale);}
            tokio::select! {
                biased;
                ()=publication.cancel.cancelled()=>return Ok(()),
                ()=network.inner.shutdown.cancelled()=>return Ok(()),
                ()=authorization.cancelled()=>return Err(Error::Stale),
                _=metadata_tick.tick(),if metadata_dirty=>{metadata_dirty=false;send_json(&mut socket,json!({"type":"metadataChanged"})).await?;}
                _=heartbeat.tick()=>send_json(&mut socket,json!({"type":"ping"})).await?,
                _=trust_refresh.tick()=>{
                    let current_credentials=network.credentials()?;
                    check_generation(network,generation)?;
                    let current:SessionDto=network.inner.http.device(Method::GET,&format!("api/sessions/{}",keys.dto.id),&current_credentials,None).await?;
                    if !current.ready || current.authorization_revision!=keys.dto.authorization_revision || current.key_generation!=keys.dto.key_generation {return Err(Error::Stale);}
                    let current_audience=audience(network,publication,&current_credentials,&keys.dto).await?;
                    let current_devices=current_audience.into_iter().map(|(id,cert)|(id,cert.sig_public_key)).collect::<BTreeMap<_,_>>();
                    if current_devices!=keys.devices{return Err(Error::Stale);}
                }
                completed=pending.next(),if !pending.is_empty()=>{
                    if let Some((identity,accepted))=completed{send_control_result(&mut socket,&credentials,identity,accepted).await?;}
                }
                frame=output.recv()=>match frame {
                    Ok(PublishedFrame::MetadataChanged)=>{metadata_dirty=true;},
                    Ok(PublishedFrame::BootstrapBarrier {..})=>{},
                    Ok(PublishedFrame::Raw {sequence,bytes})=>{
                        if sequence<next_sequence {continue;}
                        if sequence!=next_sequence {return Err(invalid("Terminal output skipped a sequence."));}
                        let encoded=wire::raw_frame(&keys.key,keys.dto.key_generation,keys.raw_counter,sequence,&bytes)?;
                        keys.raw_counter=keys.raw_counter.checked_add(1).ok_or_else(||invalid("Terminal nonce space exhausted."))?;
                        next_sequence=sequence.checked_add(1).ok_or_else(||invalid("Terminal sequence exhausted."))?;
                        send_binary(&mut socket,encoded).await?;
                    }
                    Ok(PublishedFrame::Checkpoint {checkpoint,next_sequence:cut})=>{
                        if cut<next_sequence {continue;}
                        send_checkpoint(&mut socket,&mut keys,CheckpointCut {checkpoint,next_sequence:cut},None).await?;
                        next_sequence=cut;
                    }
                    Ok(PublishedFrame::Closed {final_sequence,..})=>{
                        while let Some((identity,accepted))=pending.next().await { send_control_result(&mut socket,&credentials,identity,accepted).await?; }
                        send_json(&mut socket,json!({"type":"end","reason":"Terminal stopped.","finalSequence":final_sequence})).await?;
                        let _ack = tokio::time::timeout(Duration::from_secs(11), async {
                            loop {
                                let message=socket.next().await.ok_or(Error::Closed)?.map_err(|error|invalid(error.to_string()))?;
                                if let Message::Text(text)=message {
                                    let value:Value=serde_json::from_str(&text)?;
                                    if wire::text(&value,"type")?=="endAcknowledged" { return Ok::<(), Error>(()); }
                                }
                            }
                        }).await;
                        publication.cancel.cancel();return Ok(());
                    }
                    Err(broadcast::error::RecvError::Lagged(_))=>return Err(invalid("Terminal relay fell behind; requesting a fresh checkpoint.")),
                    Err(broadcast::error::RecvError::Closed)=>return Ok(()),
                },
                incoming=socket.next()=>{
                    let message=incoming.ok_or(Error::Closed)?.map_err(|error|invalid(error.to_string()))?;
                    match message {
                        Message::Text(text)=>{
                            let value:Value=serde_json::from_str(&text)?;
                            match wire::text(&value,"type")? {
                                "ping"=>send_json(&mut socket,json!({"type":"pong"})).await?,
                                "pong"=>{},
                                "accessChanged"=>{
                                    if wire::number(&value,"authorizationRevision")?>keys.dto.authorization_revision || wire::number(&value,"keyGeneration")?>u64::from(keys.dto.key_generation){return Err(Error::Stale);}
                                }
                                "keysRequested"=>{
                                    if wire::number(&value,"authorizationRevision")?==keys.dto.authorization_revision && wire::number(&value,"keyGeneration")?==u64::from(keys.dto.key_generation){return Err(Error::Stale);}
                                }
                                "checkpointRequested"=>{
                                    let request=wire::id(&value,"requestId")?;
                                    let challenge:[u8;32]=wire::decode_b64(&value,"challenge",32)?.try_into().map_err(|_|invalid("Invalid viewer checkpoint challenge."))?;
                                    let connection=wire::id(&value,"connectionId")?;
                                    let user=wire::text(&value,"recipientUserId")?.to_owned();let device=wire::text(&value,"recipientDeviceId")?.to_owned();
                                    if !keys.devices.contains_key(&(user.clone(),device.clone())){return Err(Error::Trust("Checkpoint recipient is not currently authorized.".into()));}
                                    let identity=wire::CaptureIdentity::new(&keys.dto,connection,user,device);
                                    let marker=Uuid::now_v7();
                                    let cut=match bootstrap(publication,marker).await {
                                        Ok(cut)=>cut,
                                        Err(Error::Closed | Error::Busy)=>continue,
                                        Err(error)=>return Err(error),
                                    };
                                    drain_to_barrier(&mut socket,&mut keys,publication,output,marker,Some(&mut next_sequence)).await?;
                                    if cut.next_sequence!=next_sequence{return Err(invalid("Checkpoint barrier does not match terminal sequence."));}
                                    send_checkpoint(&mut socket,&mut keys,cut,Some((request,identity,challenge,credentials.keys.signing_pkcs8()))).await?;
                                }
                                "control"=>handle_control(network,publication,&credentials,&mut keys,&mut pending,&value).await?,
                                "participantConnected"=>{
                                    let connection_id=wire::id(&value,"connectionId")?;
                                    let user_id=wire::text(&value,"senderUserId")?.to_owned();
                                    let device=wire::text(&value,"senderDeviceId")?.to_owned();
                                    if !keys.devices.contains_key(&(user_id.clone(),device)){return Err(Error::Trust("Connected participant is not authorized.".into()));}
                                    publication.requests.send(HostRequest::Connected { connection_id, user_id }).await.map_err(|_|Error::Closed)?;
                                }
                                "participantDisconnected"=>{
                                    let connection=wire::id(&value,"connectionId")?;
                                    keys.retired.insert(connection);
                                    if let Some(peer)=keys.connections.remove(&connection){peer.authorization.cancel();}
                                    release(publication,connection).await;
                                }
                                _=>return Err(invalid("Unsupported host relay message.")),
                            }
                        }
                        Message::Ping(payload)=>send_pong(&mut socket,payload).await?,
                        Message::Pong(_)=>{},
                        Message::Close(_)=>return Err(Error::Closed),
                        _=>return Err(invalid("Unexpected host relay payload.")),
                    }
                }
            }
        }
        }=>result,
    };
    authorization.cancel();
    drop(publication.requests.send(HostRequest::ResetPresence).await);
    for connection in keys.connections.keys() {
        release(publication, *connection).await;
    }
    let _outcome = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
    result
}

async fn current_publication(
    network: &Network,
    publication: &Publication,
    credentials: &Credentials,
) -> Result<SessionDto> {
    let info = publication.info.read().await.clone();
    match network
        .inner
        .http
        .device::<SessionDto>(
            Method::GET,
            &format!("api/sessions/{}", info.session_id),
            credentials,
            None,
        )
        .await
    {
        Ok(dto) => {
            if dto.incarnation_id != info.incarnation_id
                || dto.host_device_id != credentials.keys.device_id
                || dto.owner_user_id != credentials.user_id
            {
                return Err(Error::Stale);
            }
            *publication.dto.write().await = dto.clone();
            Ok(dto)
        }
        Err(Error::Backend { status: 404, .. }) => {
            network.check_credentials(credentials)?;
            if publication.cancel.is_cancelled() {
                return Err(Error::Stale);
            }
            let dto:SessionDto=network.inner.http.device(Method::POST,"api/sessions",credentials,Some(json!({"id":info.session_id,"incarnationId":info.incarnation_id,"name":info.name,"hostDeviceId":credentials.keys.device_id,"hostName":"This computer","roomId":null}))).await?;
            {
                let mut info = publication.info.write().await;
                info.shared_with.clear();
                info.room_id = None;
            }
            *publication.dto.write().await = dto.clone();
            network.emit_for(credentials.generation,Some(credentials.user_id.clone()),json!({"type":"system.error","message":"Remote sharing expired while this host was offline; choose friends again."}));
            Ok(dto)
        }
        Err(error) => Err(error),
    }
}

async fn drain_to_barrier(
    socket: &mut Socket,
    keys: &mut HostKeys,
    publication: &Publication,
    output: &mut PublicationOutput,
    marker: Uuid,
    mut next_sequence: Option<&mut u64>,
) -> Result<()> {
    loop {
        let frame = output
            .try_recv()
            .map_err(|_| invalid("Terminal checkpoint ordering barrier was lost."))?;
        if keys.authorization.is_cancelled() {
            return Err(Error::Stale);
        }
        match frame {
            PublishedFrame::MetadataChanged => {
                send_json(socket, json!({"type":"metadataChanged"})).await?;
            }
            PublishedFrame::BootstrapBarrier { request_id } => {
                if request_id == marker {
                    return Ok(());
                }
            }
            PublishedFrame::Closed { .. } => {
                publication.cancel.cancel();
                return Err(Error::Closed);
            }
            PublishedFrame::Raw { sequence, bytes } => {
                if let Some(next) = next_sequence.as_deref_mut() {
                    if sequence < *next {
                        continue;
                    }
                    if sequence != *next {
                        return Err(invalid(
                            "Terminal output skipped a sequence before checkpoint.",
                        ));
                    }
                    let frame = wire::raw_frame(
                        &keys.key,
                        keys.dto.key_generation,
                        keys.raw_counter,
                        sequence,
                        &bytes,
                    )?;
                    keys.raw_counter = keys
                        .raw_counter
                        .checked_add(1)
                        .ok_or_else(|| invalid("Terminal nonce space exhausted."))?;
                    *next = sequence
                        .checked_add(1)
                        .ok_or_else(|| invalid("Terminal sequence exhausted."))?;
                    send_binary(socket, frame).await?;
                }
            }
            PublishedFrame::Checkpoint {
                checkpoint,
                next_sequence: cut,
            } => {
                if let Some(next) = next_sequence.as_deref_mut() {
                    if cut < *next {
                        continue;
                    }
                    send_checkpoint(
                        socket,
                        keys,
                        CheckpointCut {
                            checkpoint,
                            next_sequence: cut,
                        },
                        None,
                    )
                    .await?;
                    *next = cut;
                }
            }
        }
    }
}

async fn release(publication: &Publication, connection_id: Uuid) {
    let _outcome = tokio::time::timeout(
        Duration::from_millis(100),
        publication
            .requests
            .send(HostRequest::Disconnected { connection_id }),
    )
    .await;
}

async fn send_checkpoint(
    socket: &mut Socket,
    keys: &mut HostKeys,
    cut: CheckpointCut,
    request: Option<(Uuid, wire::CaptureIdentity, [u8; 32], &[u8])>,
) -> Result<()> {
    if keys.authorization.is_cancelled() {
        return Err(Error::Stale);
    }
    keys.revision = keys
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("Checkpoint revision exhausted."))?;
    let frame = wire::checkpoint_frame(
        &keys.key,
        keys.dto.key_generation,
        keys.checkpoint_counter,
        keys.revision,
        cut.next_sequence,
        &cut.checkpoint,
    )?;
    keys.checkpoint_counter = keys
        .checkpoint_counter
        .checked_add(1)
        .ok_or_else(|| invalid("Checkpoint nonce space exhausted."))?;
    if let Some((request, identity, challenge, signing)) = request {
        let signature = crypto::sign_control_message(
            signing,
            &identity.preimage(&challenge, cut.next_sequence, &wire::frame_hash(&frame))?,
        )?;
        if keys.authorization.is_cancelled() {
            return Err(Error::Stale);
        }
        send_json(
            socket,
            json!({"type":"checkpoint","requestId":request,"frame":BASE64.encode(frame),"signature":BASE64.encode(signature)}),
        )
        .await
    } else {
        send_binary(socket, frame).await
    }
}

async fn distribute(
    network: &Network,
    publication: &Publication,
    credentials: &Credentials,
    authorization: CancellationToken,
) -> Result<HostKeys> {
    let info = publication.info.read().await.clone();
    let mut dto: SessionDto = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{}", info.session_id),
            credentials,
            None,
        )
        .await?;
    if dto.incarnation_id != info.incarnation_id
        || dto.host_device_id != credentials.keys.device_id
        || dto.owner_user_id != credentials.user_id
    {
        return Err(Error::Stale);
    }
    let backend_members = dto.shared_with.iter().cloned().collect::<BTreeSet<_>>();
    if !backend_members.is_subset(&info.shared_with) {
        return Err(Error::Trust(
            "The server proposed recipients that the host did not share with.".into(),
        ));
    }
    dto=network.inner.http.device(Method::POST,&format!("api/sessions/{}/keys/rotate",info.session_id),credentials,Some(json!({"incarnationId":info.incarnation_id,"expectedRevision":dto.authorization_revision,"expectedGeneration":dto.key_generation}))).await?;
    let key = Zeroizing::new(crypto::generate_session_key()?);
    let audience = audience(network, publication, credentials, &dto).await?;
    let mut blobs = Vec::new();
    let mut devices = BTreeMap::new();
    let now = identity::now_ms();
    let signing_key = credentials.keys.signing_key()?;
    for ((user, id), device) in audience {
        let blob = crypto::wrap_session_key(
            &device.kem_public_key,
            &key,
            &info.session_id.to_string(),
            &id,
            dto.key_generation,
        )?;
        let signature = crypto::sign(
            &signing_key,
            &crypto::key_blob_digest(
                &info.session_id.to_string(),
                &info.incarnation_id,
                &id,
                &blob,
                dto.key_generation,
                now,
            )?,
        )?;
        blobs.push(json!({"recipientUserId":user,"recipientDeviceId":id,"encryptedSessionKey":BASE64.encode(blob),"senderDeviceId":credentials.keys.device_id,"signature":BASE64.encode(signature),"signatureVersion":2,"issuedAtMs":now}));
        devices.insert((user.clone(), id), device.sig_public_key);
    }
    network.check_credentials(credentials)?;
    if authorization.is_cancelled() || publication.changing.load(Ordering::Acquire) {
        return Err(Error::Stale);
    }
    let _response:Value=network.inner.http.device(Method::POST,&format!("api/sessions/{}/keys",info.session_id),credentials,Some(json!({"incarnationId":info.incarnation_id,"authorizationRevision":dto.authorization_revision,"keyGeneration":dto.key_generation,"blobs":blobs}))).await?;
    if authorization.is_cancelled() {
        return Err(Error::Stale);
    }
    dto.ready = true;
    *publication.dto.write().await = dto.clone();
    Ok(HostKeys {
        key,
        dto,
        devices,
        checkpoint_counter: 0,
        raw_counter: 0,
        revision: 0,
        connections: BTreeMap::new(),
        retired: RetiredConnections::default(),
        authorization,
    })
}

#[expect(
    clippy::needless_pass_by_ref_mut,
    clippy::too_many_lines,
    reason = "exclusive borrow keeps non-Sync pending futures Send across authorization lock await"
)]
async fn handle_control(
    network: &Network,
    publication: &Publication,
    credentials: &Credentials,
    keys: &mut HostKeys,
    pending: &mut PendingControls,
    value: &Value,
) -> Result<()> {
    let connection = wire::id(value, "connectionId")?;
    let sequence = wire::number(value, "sequence")?;
    let user = wire::text(value, "senderUserId")?.to_owned();
    let device = wire::text(value, "senderDeviceId")?.to_owned();
    let request = wire::id(value, "requestId")?;
    let revision = wire::number(value, "authorizationRevision")?;
    let key_generation = u32::try_from(wire::number(value, "keyGeneration")?)
        .map_err(|_| invalid("Invalid key generation."))?;
    if revision != keys.dto.authorization_revision
        || key_generation != keys.dto.key_generation
        || keys.authorization.is_cancelled()
    {
        return Err(Error::Stale);
    }
    if keys.retired.contains(connection) {
        return Ok(());
    }
    if let Some(peer) = keys.connections.get(&connection) {
        if peer.authorization.is_cancelled()
            || peer.user != user
            || peer.device != device
            || peer.sequence.checked_add(1) != Some(sequence)
        {
            return Err(invalid("Repeated or out-of-order terminal control."));
        }
    } else if sequence != 1 || keys.connections.len() >= 256 {
        return Err(invalid(
            "Terminal connection budget or sequence invalid; reconnecting with fresh keys.",
        ));
    }
    let public = keys
        .devices
        .get(&(user.clone(), device.clone()))
        .ok_or_else(|| {
            Error::Trust("This device is not in the host's authorized audience.".into())
        })?;
    let identity = ControlIdentity {
        session_id: keys.dto.id,
        incarnation_id: keys.dto.incarnation_id,
        authorization_revision: revision,
        key_generation,
        connection_id: connection,
        user_id: user.clone(),
        device_id: device.clone(),
        sequence,
        request_id: request,
    };
    let nonce = wire::decode_b64(value, "nonce", 12)?;
    let ciphertext = wire::decode_b64(value, "ciphertext", wire::INPUT_LIMIT * 2)?;
    let signature = wire::decode_b64(value, "signature", 3309)?;
    crypto::verify_control_message(
        public,
        &identity.signature(&nonce, &ciphertext)?,
        &signature,
    )?;
    let plaintext = Zeroizing::new(crypto::decrypt_control_payload(
        &keys.key,
        &identity.aad()?,
        &nonce,
        &ciphertext,
    )?);
    let control = wire::decode_control(&plaintext, request)?;
    let peer = keys
        .connections
        .entry(connection)
        .or_insert_with(|| Connection {
            user: user.clone(),
            device: device.clone(),
            sequence: 0,
            authorization: keys.authorization.child_token(),
        });
    peer.sequence = sequence;
    let authorization = peer.authorization.clone();
    network.check_credentials(credentials)?;
    if authorization.is_cancelled()
        || publication.cancel.is_cancelled()
        || publication.changing.load(Ordering::Acquire)
    {
        return Err(Error::Stale);
    }
    if user != credentials.user_id && !publication.info.read().await.shared_with.contains(&user) {
        return Err(Error::Stale);
    }
    if pending.len() >= 128 {
        return Err(Error::Busy);
    }
    let (reply, response) = oneshot::channel();
    let admitted = publication
        .requests
        .try_send(HostRequest::Control {
            sender_user_id: user,
            sender_device_id: device,
            connection_id: connection,
            control,
            authorization: authorization.clone(),
            reply,
        })
        .is_ok();
    pending.push(Box::pin(async move {
        let accepted=if admitted {tokio::select! {
            ()=authorization.cancelled()=>false,
            result=tokio::time::timeout(Duration::from_millis(4750),response)=>matches!(result,Ok(Ok(Ok(_)))),
        }}else{false};
        (identity,accepted)
    }));
    Ok(())
}

async fn send_control_result(
    socket: &mut Socket,
    credentials: &Credentials,
    identity: ControlIdentity,
    accepted: bool,
) -> Result<()> {
    let sequence = identity.sequence;
    let request = identity.request_id;
    let message = if accepted {
        ""
    } else {
        "The terminal did not confirm the operation; it was not retried."
    };
    let signature = crypto::sign_control_message(
        credentials.keys.signing_pkcs8(),
        &identity.result_signature(accepted, message)?,
    )?;
    send_json(socket,json!({"type":"controlResult","sessionId":identity.session_id,"incarnationId":identity.incarnation_id,"authorizationRevision":identity.authorization_revision,"keyGeneration":identity.key_generation,"connectionId":identity.connection_id,"senderUserId":identity.user_id,"senderDeviceId":identity.device_id,"sequence":sequence,"requestId":request,"accepted":accepted,"message":message,"signature":BASE64.encode(signature)})).await
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    #[test]
    fn retired_connections_remain_rejected_without_a_cumulative_live_budget() {
        let mut retired = RetiredConnections::default();
        let ids = (0..4096).map(|_| Uuid::now_v7()).collect::<Vec<_>>();
        for id in &ids {
            retired.insert(*id);
        }
        assert!(ids.iter().all(|id| retired.contains(*id)));
        assert_eq!(retired.0.len(), 16_384);
    }
}
