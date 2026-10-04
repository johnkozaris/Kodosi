use super::*;
mod audience;
use crate::terminal::TerminalMetadata;
use audience::audience;

type PendingControls = futures_util::stream::FuturesUnordered<
    std::pin::Pin<Box<dyn Future<Output = (ControlIdentity, bool)> + Send>>,
>;
type Published = std::result::Result<PublishedFrame, broadcast::error::RecvError>;

const BATCH_BYTES: usize = 256 * 1024;
const OUTPUT_PACE: Duration = Duration::from_millis(2);
const JOIN_GRACE: Duration = Duration::from_secs(15);

struct HostKeys {
    key: Zeroizing<crypto::SessionKey>,
    dto: SessionDto,
    devices: BTreeMap<(String, String), Vec<u8>>,
    channels: BTreeMap<(String, String), Zeroizing<crypto::SessionKey>>,
    packer: wire::OutputPacker,
    checkpoint_counter: u64,
    raw_counter: u64,
    notice_counter: u64,
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
    network: &BackendClient,
    publication: &Publication,
    output: &mut PublicationOutput,
    generation: u64,
) -> Result<()> {
    check_generation(network, generation)?;
    let credentials = network.credentials()?;
    let mut authorization = credentials.cancel.child_token();
    let mut _authorization_guard = authorization.clone().drop_guard();
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
        result=distribute(network,publication,&credentials,authorization.clone(),None)=>result?,
    };
    let result = async {
        let marker=Uuid::now_v7();
        let initial=bootstrap(publication,marker).await?;
        skip_to_barrier(output,marker)?;
        let mut next_sequence=initial.next_sequence;
        let mut heartbeat=tokio::time::interval(Duration::from_secs(20));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut trust_refresh=tokio::time::interval(TRUST_CHECK);
        trust_refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        trust_refresh.tick().await;
        let mut pending=PendingControls::new();
        let mut metadata:Option<TerminalMetadata>=None;
        let mut held:Option<Published>=None;
        let mut paced=tokio::time::Instant::now();
        let mut viewers=BTreeSet::<Uuid>::new();
        let mut joining:Option<tokio::time::Instant>=None;
        let mut metadata_tick=tokio::time::interval(Duration::from_secs(1));
        metadata_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            check_generation(network,generation)?;
            if authorization.is_cancelled(){
                if credentials.cancel.is_cancelled(){return Err(Error::Stale);}
                while publication.changing.load(Ordering::Acquire) {
                    tokio::select! {
                        ()=publication.cancel.cancelled()=>return Ok(()),
                        ()=network.inner.shutdown.cancelled()=>return Ok(()),
                        ()=publication.refresh.notified()=>{},
                        ()=tokio::time::sleep(Duration::from_millis(200))=>{},
                    }
                }
                authorization=credentials.cancel.child_token();
                _authorization_guard=authorization.clone().drop_guard();
                {
                    let mut previous=publication.authorization.lock().await;
                    previous.cancel();
                    *previous=authorization.clone();
                }
                if publication.changing.load(Ordering::Acquire){authorization.cancel();continue;}
                keys=match distribute(network,publication,&credentials,authorization.clone(),Some(keys.dto.key_generation)).await {
                    Ok(keys)=>keys,
                    Err(Error::Stale) if authorization.is_cancelled() && !credentials.cancel.is_cancelled()=>continue,
                    Err(error)=>return Err(error),
                };
                pending=PendingControls::new();
                continue;
            }
            let watched=!viewers.is_empty() || joining.is_some_and(|sent|sent.elapsed()<JOIN_GRACE);
            tokio::select! {
                biased;
                ()=publication.cancel.cancelled()=>return Ok(()),
                ()=network.inner.shutdown.cancelled()=>return Ok(()),
                ()=authorization.cancelled()=>{},
                _=metadata_tick.tick(),if metadata.is_some()=>{if let Some(current)=metadata.take() && watched {send_notice(&mut socket,&mut keys,&wire::Notice::Metadata(&current)).await?;}}
                _=heartbeat.tick()=>send_json(&mut socket,json!({"type":"ping"})).await?,
                _=trust_refresh.tick()=>match access_unchanged(network,publication,generation,&keys).await {
                    Ok(true)=>{}
                    Ok(false)=>authorization.cancel(),
                    Err(error) if error.unanswered()=>tracing::warn!(%error,"terminal access check got no answer; it will be tried again"),
                    Err(error)=>return Err(error),
                },
                completed=pending.next(),if !pending.is_empty()=>{
                    if let Some((identity,accepted))=completed{send_control_result(&mut socket,&keys,&identity,accepted).await?;}
                }
                incoming=socket.next(),if held.is_none()=>{
                    let message=incoming.ok_or(Error::Closed)?.map_err(|error|invalid(error.to_string()))?;
                    match message {
                        Message::Text(text)=>{
                            let value:Value=serde_json::from_str(&text)?;
                            match wire::text(&value,"type")? {
                                "ping"=>send_json(&mut socket,json!({"type":"pong"})).await?,
                                "pong"=>{},
                                "accessChanged"=>{
                                    if wire::number(&value,"authorizationRevision")?>keys.dto.authorization_revision || wire::number(&value,"keyGeneration")?>u64::from(keys.dto.key_generation){authorization.cancel();}
                                }
                                "checkpointRequested"=>{
                                    let request=wire::id(&value,"requestId")?;
                                    let challenge:[u8;32]=wire::decode_b64(&value,"challenge",32)?.try_into().map_err(|_|invalid("Invalid viewer checkpoint challenge."))?;
                                    let connection=wire::id(&value,"connectionId")?;
                                    let user=wire::text(&value,"recipientUserId")?.to_owned();let device=wire::text(&value,"recipientDeviceId")?.to_owned();
                                    if !keys.devices.contains_key(&(user.clone(),device.clone())){continue;}
                                    let identity=wire::CaptureIdentity::new(&keys.dto,connection,user,device);
                                    let marker=Uuid::now_v7();
                                    let cut=match bootstrap(publication,marker).await {
                                        Ok(cut)=>cut,
                                        Err(Error::Closed | Error::Busy)=>continue,
                                        Err(error)=>return Err(error),
                                    };
                                    if !watched {
                                        skip_to_barrier(output,marker)?;
                                        next_sequence=cut.next_sequence;
                                    } else if !drain_to_barrier(&mut socket,&mut keys,publication,output,marker,&mut next_sequence,&mut metadata).await? {
                                        send_json(&mut socket,json!({"type":"resync"})).await?;
                                        next_sequence=cut.next_sequence;
                                        continue;
                                    }
                                    if cut.next_sequence!=next_sequence{return Err(invalid("Checkpoint barrier does not match terminal sequence."));}
                                    send_checkpoint(&mut socket,&mut keys,cut,request,&identity,&challenge,credentials.keys.signing_pkcs8()).await?;
                                    joining=Some(tokio::time::Instant::now());
                                }
                                "control"=>handle_control(network,publication,&credentials,&mut keys,&mut pending,&value).await?,
                                "participantConnected"=>{
                                    let connection_id=wire::id(&value,"connectionId")?;
                                    let user_id=wire::text(&value,"senderUserId")?.to_owned();
                                    let device=wire::text(&value,"senderDeviceId")?.to_owned();
                                    viewers.insert(connection_id);
                                    if !keys.devices.contains_key(&(user_id.clone(),device)){continue;}
                                    publication.requests.send(HostRequest::Connected { connection_id, user_id }).await.map_err(|_|Error::Closed)?;
                                }
                                "participantDisconnected"=>{
                                    let connection=wire::id(&value,"connectionId")?;
                                    viewers.remove(&connection);
                                    keys.retired.insert(connection);
                                    if let Some(peer)=keys.connections.remove(&connection){peer.authorization.cancel();}
                                    release(publication,connection).await;
                                }
                                _=>return Err(invalid("Unsupported host connection message.")),
                            }
                        }
                        Message::Ping(payload)=>send_pong(&mut socket,payload).await?,
                        Message::Pong(_)=>{},
                        Message::Close(_)=>return Err(Error::Closed),
                        _=>return Err(invalid("Unexpected host connection payload.")),
                    }
                }
                frame=async{match held.take(){Some(frame)=>frame,None=>output.recv().await}}=>match frame {
                    Ok(PublishedFrame::Metadata(current))=>{metadata=Some(current);},
                    Ok(PublishedFrame::BootstrapBarrier {..})=>{},
                    Ok(PublishedFrame::Raw {sequence,bytes})=>{
                        if sequence<next_sequence {continue;}
                        if sequence!=next_sequence {return Err(invalid("Terminal output skipped a sequence."));}
                        next_sequence=sequence.checked_add(1).ok_or_else(||invalid("Terminal sequence exhausted."))?;
                        if !watched {continue;}
                        let mut chunks=vec![bytes];
                        held=batch(output,&mut next_sequence,&mut chunks);
                        if held.is_none() && chunks.len()<wire::RAW_BATCH_LIMIT && tokio::time::Instant::now()<paced {
                            tokio::time::sleep_until(paced).await;
                            held=batch(output,&mut next_sequence,&mut chunks);
                        }
                        if authorization.is_cancelled(){continue;}
                        let encoded=wire::raw_frame(&keys.key,keys.dto.key_generation,keys.raw_counter,sequence,&chunks,&mut keys.packer)?;
                        keys.raw_counter=keys.raw_counter.checked_add(1).ok_or_else(||invalid("Terminal nonce space exhausted."))?;
                        send_binary(&mut socket,encoded).await?;
                        paced=tokio::time::Instant::now()+OUTPUT_PACE;
                    }
                    Ok(PublishedFrame::Resize {rows,cols,at_sequence})=>{
                        if at_sequence<next_sequence {continue;}
                        if at_sequence!=next_sequence {return Err(invalid("Terminal resize skipped a sequence."));}
                        if !watched {continue;}
                        send_notice(&mut socket,&mut keys,&wire::Notice::Resize {rows,cols,at_sequence}).await?;
                    }
                    Ok(PublishedFrame::Closed {final_sequence,..})=>{
                        while let Some((identity,accepted))=pending.next().await { send_control_result(&mut socket,&keys,&identity,accepted).await?; }
                        send_json(&mut socket,json!({"type":"end","reason":"Terminal closed.","finalSequence":final_sequence})).await?;
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
                    Err(broadcast::error::RecvError::Lagged(_))=>{next_sequence=resynchronize(&mut socket,publication,output).await?;},
                    Err(broadcast::error::RecvError::Closed)=>return Ok(()),
                },
            }
        }
    }.await;
    authorization.cancel();
    drop(publication.requests.send(HostRequest::ResetPresence).await);
    for connection in keys.connections.keys() {
        release(publication, *connection).await;
    }
    let _outcome = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
    result
}

async fn current_publication(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
) -> Result<SessionDto> {
    let _operation = network.inner.operations.lock().await;
    network.check_credentials(credentials)?;
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
            publication.clear_sharing().await;
            let dto:SessionDto=network.inner.http.device(Method::POST,"api/sessions",credentials,Some(json!({"id":info.session_id,"incarnationId":info.incarnation_id,"name":info.name,"hostDeviceId":credentials.keys.device_id,"hostName":crate::network::backend_client::host_label(),"missionId":null}))).await?;
            *publication.dto.write().await = dto.clone();
            network.emit_for(credentials.generation,Some(credentials.user_id.clone()),json!({"type":"system.error","message":"The server no longer had this terminal; choose friends again."}));
            Ok(dto)
        }
        Err(error) => Err(error),
    }
}

async fn resynchronize(
    socket: &mut Socket,
    publication: &Publication,
    output: &mut PublicationOutput,
) -> Result<u64> {
    send_json(socket, json!({"type":"resync"})).await?;
    let marker = Uuid::now_v7();
    let cut = bootstrap(publication, marker).await?;
    skip_to_barrier(output, marker)?;
    Ok(cut.next_sequence)
}

async fn access_unchanged(
    network: &BackendClient,
    publication: &Publication,
    generation: u64,
    keys: &HostKeys,
) -> Result<bool> {
    let credentials = network.credentials()?;
    check_generation(network, generation)?;
    let current: SessionDto = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{}", keys.dto.id),
            &credentials,
            None,
        )
        .await?;
    if !current.ready
        || current.authorization_revision != keys.dto.authorization_revision
        || current.key_generation != keys.dto.key_generation
    {
        return Ok(false);
    }
    let devices = audience(network, publication, &credentials, &keys.dto)
        .await?
        .into_iter()
        .map(|(id, certificate)| (id, certificate.sig_public_key))
        .collect::<BTreeMap<_, _>>();
    Ok(devices == keys.devices)
}

async fn send_notice(
    socket: &mut Socket,
    keys: &mut HostKeys,
    notice: &wire::Notice<'_>,
) -> Result<()> {
    if keys.authorization.is_cancelled() {
        return Ok(());
    }
    let frame = wire::notice_frame(
        &keys.key,
        keys.dto.key_generation,
        keys.notice_counter,
        keys.revision,
        notice,
    )?;
    keys.notice_counter = keys
        .notice_counter
        .checked_add(1)
        .ok_or_else(|| invalid("Terminal nonce space exhausted."))?;
    send_binary(socket, frame).await
}

fn batch(
    output: &mut PublicationOutput,
    next_sequence: &mut u64,
    chunks: &mut Vec<Bytes>,
) -> Option<Published> {
    let mut size = chunks.iter().map(Bytes::len).sum::<usize>();
    while chunks.len() < wire::RAW_BATCH_LIMIT && size < BATCH_BYTES {
        match output.try_recv() {
            Ok(PublishedFrame::Raw { sequence, bytes }) if sequence == *next_sequence => {
                let Some(next) = sequence.checked_add(1) else {
                    return Some(Ok(PublishedFrame::Raw { sequence, bytes }));
                };
                *next_sequence = next;
                size += bytes.len();
                chunks.push(bytes);
            }
            Ok(frame) => return Some(Ok(frame)),
            Err(broadcast::error::TryRecvError::Empty) => return None,
            Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                return Some(Err(broadcast::error::RecvError::Lagged(missed)));
            }
            Err(broadcast::error::TryRecvError::Closed) => {
                return Some(Err(broadcast::error::RecvError::Closed));
            }
        }
    }
    None
}

fn skip_to_barrier(output: &mut PublicationOutput, marker: Uuid) -> Result<()> {
    loop {
        match output.try_recv() {
            Ok(PublishedFrame::BootstrapBarrier { request_id }) if request_id == marker => {
                return Ok(());
            }
            Ok(PublishedFrame::Closed { .. }) => return Err(Error::Closed),
            Ok(_) | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
            Err(_) => return Err(invalid("Terminal checkpoint ordering barrier was lost.")),
        }
    }
}

async fn drain_to_barrier(
    socket: &mut Socket,
    keys: &mut HostKeys,
    publication: &Publication,
    output: &mut PublicationOutput,
    marker: Uuid,
    next_sequence: &mut u64,
    metadata: &mut Option<TerminalMetadata>,
) -> Result<bool> {
    loop {
        let frame = match output.try_recv() {
            Ok(frame) => frame,
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                skip_to_barrier(output, marker)?;
                return Ok(false);
            }
            Err(_) => return Err(invalid("Terminal checkpoint ordering barrier was lost.")),
        };
        if keys.authorization.is_cancelled() {
            skip_to_barrier(output, marker)?;
            return Ok(false);
        }
        match frame {
            PublishedFrame::Metadata(current) => *metadata = Some(current),
            PublishedFrame::BootstrapBarrier { request_id } => {
                if request_id == marker {
                    return Ok(true);
                }
            }
            PublishedFrame::Closed { .. } => {
                publication.cancel.cancel();
                return Err(Error::Closed);
            }
            PublishedFrame::Raw { sequence, bytes } => {
                if sequence < *next_sequence {
                    continue;
                }
                if sequence != *next_sequence {
                    return Err(invalid(
                        "Terminal output skipped a sequence before checkpoint.",
                    ));
                }
                let frame = wire::raw_frame(
                    &keys.key,
                    keys.dto.key_generation,
                    keys.raw_counter,
                    sequence,
                    &[bytes],
                    &mut keys.packer,
                )?;
                keys.raw_counter = keys
                    .raw_counter
                    .checked_add(1)
                    .ok_or_else(|| invalid("Terminal nonce space exhausted."))?;
                *next_sequence = sequence
                    .checked_add(1)
                    .ok_or_else(|| invalid("Terminal sequence exhausted."))?;
                send_binary(socket, frame).await?;
            }
            PublishedFrame::Resize {
                rows,
                cols,
                at_sequence,
            } => {
                if at_sequence < *next_sequence {
                    continue;
                }
                if at_sequence != *next_sequence {
                    return Err(invalid(
                        "Terminal resize skipped a sequence before checkpoint.",
                    ));
                }
                let notice = wire::Notice::Resize {
                    rows,
                    cols,
                    at_sequence,
                };
                send_notice(socket, keys, &notice).await?;
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
    request: Uuid,
    identity: &wire::CaptureIdentity,
    challenge: &[u8; 32],
    signing: &[u8],
) -> Result<()> {
    if keys.authorization.is_cancelled() {
        return Ok(());
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
    let signature = crypto::sign_control_message(
        signing,
        &identity.preimage(challenge, cut.next_sequence, &wire::frame_hash(&frame))?,
    )?;
    if keys.authorization.is_cancelled() {
        return Ok(());
    }
    keys.packer.restart();
    send_json(
        socket,
        json!({"type":"checkpoint","requestId":request,"frame":BASE64.encode(frame),"signature":BASE64.encode(signature)}),
    )
    .await
}

async fn distribute(
    network: &BackendClient,
    publication: &Publication,
    credentials: &Credentials,
    authorization: CancellationToken,
    served: Option<u32>,
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
    if dto.ready || served.is_none_or(|generation| dto.key_generation <= generation) {
        dto=network.inner.http.device(Method::POST,&format!("api/sessions/{}/keys/rotate",info.session_id),credentials,Some(json!({"incarnationId":info.incarnation_id,"expectedRevision":dto.authorization_revision,"expectedGeneration":dto.key_generation}))).await?;
    }
    let key = Zeroizing::new(crypto::generate_session_key()?);
    let audience = audience(network, publication, credentials, &dto).await?;
    let mut blobs = Vec::new();
    let mut devices = BTreeMap::new();
    let mut channels = BTreeMap::new();
    let now = identity::now_ms();
    let signing_key = credentials.keys.signing_key()?;
    for ((user, id), device) in audience {
        let (blob, channel) = crypto::wrap_session_key(
            &device.kem_public_key,
            &key,
            &info.session_id.to_string(),
            &id,
            dto.key_generation,
        )?;
        channels.insert((user.clone(), id.clone()), channel);
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
        channels,
        packer: wire::OutputPacker::new(),
        checkpoint_counter: 0,
        raw_counter: 0,
        notice_counter: 0,
        revision: 0,
        connections: BTreeMap::new(),
        retired: RetiredConnections::default(),
        authorization,
    })
}

#[expect(
    clippy::needless_pass_by_ref_mut,
    reason = "exclusive borrow keeps non-Sync pending futures Send across authorization lock await"
)]
async fn handle_control(
    network: &BackendClient,
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
        return Ok(());
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
    let Some(channel) = keys.channels.get(&(user.clone(), device.clone())) else {
        return Ok(());
    };
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
    let plaintext = Zeroizing::new(crypto::decrypt_control_payload(
        channel,
        crypto::TrafficStream::Control,
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
        || user != credentials.user_id && !publication.info.read().await.shared_with.contains(&user)
    {
        return Ok(());
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
    keys: &HostKeys,
    identity: &ControlIdentity,
    accepted: bool,
) -> Result<()> {
    let Some(channel) = keys
        .channels
        .get(&(identity.user_id.clone(), identity.device_id.clone()))
    else {
        return Ok(());
    };
    let message = if accepted {
        ""
    } else {
        "The terminal did not confirm the operation; it was not retried."
    };
    let (nonce, ciphertext) = crypto::encrypt_control_payload(
        channel,
        crypto::TrafficStream::ControlResult,
        &identity.aad()?,
        &serde_json::to_vec(&json!({"accepted":accepted,"message":message}))?,
    )?;
    send_json(socket,json!({"type":"controlResult","connectionId":identity.connection_id,"sequence":identity.sequence,"requestId":identity.request_id,"nonce":BASE64.encode(nonce),"ciphertext":BASE64.encode(ciphertext)})).await
}

#[cfg(test)]
mod barrier_tests {
    use super::*;

    #[test]
    fn reconnect_reaches_its_barrier_after_missed_output() {
        let (frames, mut output) = broadcast::channel(256);
        for sequence in 0..300 {
            drop(frames.send(PublishedFrame::Raw {
                sequence,
                bytes: Bytes::new(),
            }));
        }
        for _ in 0..2 {
            let marker = Uuid::now_v7();
            drop(frames.send(PublishedFrame::BootstrapBarrier { request_id: marker }));
            assert!(skip_to_barrier(&mut output, marker).is_ok());
        }
        assert!(skip_to_barrier(&mut output, Uuid::now_v7()).is_err());
    }

    #[test]
    fn output_is_batched_in_order_and_the_next_other_frame_is_kept() {
        let (frames, mut output) = broadcast::channel(256);
        for sequence in 11..14 {
            drop(frames.send(PublishedFrame::Raw {
                sequence,
                bytes: Bytes::from_static(b"x"),
            }));
        }
        drop(frames.send(PublishedFrame::Metadata(TerminalMetadata::default())));
        drop(frames.send(PublishedFrame::Raw {
            sequence: 14,
            bytes: Bytes::from_static(b"x"),
        }));
        let (mut next_sequence, mut chunks) = (11, vec![Bytes::from_static(b"x")]);
        let held = batch(&mut output, &mut next_sequence, &mut chunks);
        assert_eq!((next_sequence, chunks.len()), (14, 4));
        assert!(matches!(held, Some(Ok(PublishedFrame::Metadata(_)))));
    }
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
