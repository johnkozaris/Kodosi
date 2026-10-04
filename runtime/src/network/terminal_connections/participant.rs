use super::*;

const ACKNOWLEDGE_BYTES: usize = 4 * 1024;

struct Pending {
    identity: ControlIdentity,
    reply: oneshot::Sender<Result<Value>>,
    deadline: tokio::time::Instant,
}

#[expect(
    clippy::too_many_lines,
    reason = "ordered trust-key-handshake admission must complete before exposing a live remote handle"
)]
pub(super) async fn connect(network: BackendClient, id: Uuid) -> Result<RemoteConnection> {
    let credentials = network.credentials()?;
    let dto: SessionDto = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{id}"),
            &credentials,
            None,
        )
        .await?;
    if !dto.ready || dto.key_generation == 0 || dto.authorization_revision == 0 {
        return Err(Error::Stale);
    }
    let owner = network
        .fetch_identity_with(&credentials, &dto.owner_user_id, true)
        .await?;
    let host = owner
        .devices
        .get(&dto.host_device_id)
        .ok_or_else(|| Error::Trust("The hosting device is not approved.".into()))?;
    let public = host.sig_public_key.clone();
    let keys = session_keys(&network, &credentials, &dto, &public).await?;
    let checkpoint_challenge = crypto::random_bytes()?;
    let (socket, ready) = socket(
        &network,
        &credentials,
        &format!("ws/participant/{id}"),
        "participant",
        Some(&dto),
        Some(&checkpoint_challenge),
    )
    .await?;
    if wire::number(&ready, "keyGeneration")? != u64::from(dto.key_generation)
        || wire::number(&ready, "authorizationRevision")? != dto.authorization_revision
    {
        return Err(Error::Stale);
    }
    let connection_id = wire::id(&ready, "connectionId")?;
    let frames = wire::FreshFrames::new(
        wire::CaptureIdentity::new(
            &dto,
            connection_id,
            credentials.user_id.clone(),
            credentials.keys.device_id.clone(),
        ),
        public.clone(),
        checkpoint_challenge,
    );
    let (updates_tx, updates) = mpsc::channel(128);
    let (commands, commands_rx) = mpsc::channel(128);
    network.check_credentials(&credentials)?;
    let cancellation = credentials.cancel.child_token();
    network
        .inner
        .connections
        .lock()
        .await
        .retain(|token| !token.is_cancelled());
    network
        .inner
        .connections
        .lock()
        .await
        .push(cancellation.clone());
    let session = dto.clone().into();
    let cancel = cancellation.clone();
    tokio::spawn(async move {
        let result = tokio::select! {
            ()=cancel.cancelled()=>Ok(()),
            ()=network.inner.shutdown.cancelled()=>Ok(()),
            result=run(
            &network,
            socket,
            dto,
            credentials,
            connection_id,
            keys,
            public,
            frames,
            commands_rx,
            &updates_tx,
            &cancel,
        )=>result,
        };
        if !cancel.is_cancelled() {
            let _outcome = updates_tx.try_send(RemoteUpdate::Closed {
                reason: result.err().map_or_else(
                    || "Remote terminal disconnected.".into(),
                    |error| error.to_string(),
                ),
            });
        }
        cancel.cancel();
    });
    Ok(RemoteConnection {
        session,
        updates,
        commands,
        cancellation,
    })
}

struct Keys {
    session: Zeroizing<crypto::SessionKey>,
    channel: Zeroizing<crypto::SessionKey>,
}

async fn session_keys(
    network: &BackendClient,
    credentials: &Credentials,
    dto: &SessionDto,
    public: &[u8],
) -> Result<Keys> {
    let blob: Value = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{}/keys/mine", dto.id),
            credentials,
            None,
        )
        .await?;
    if wire::text(&blob, "state")? != "ready" {
        return Err(Error::Stale);
    }
    let record = blob
        .get("keyBlob")
        .ok_or_else(|| invalid("Session key is absent."))?;
    if wire::id(record, "incarnationId")? != dto.incarnation_id
        || wire::text(record, "senderDeviceId")? != dto.host_device_id
        || wire::number(record, "keyGeneration")? != u64::from(dto.key_generation)
        || wire::number(record, "signatureVersion")? != 2
        || wire::number(record, "incarnationProtocolVersion")?
            != u64::from(crate::protocol::TERMINAL_CONNECTION_VERSION)
        || wire::number(&blob, "authorizationRevision")? != dto.authorization_revision
    {
        return Err(Error::Stale);
    }
    let wrapped = wire::decode_b64(record, "encryptedSessionKey", 1 + 1088 + 12 + 32 + 16)?;
    let signature = wire::decode_b64(record, "signature", 3309)?;
    let issued = wire::number(record, "issuedAtMs")?;
    if issued > identity::now_ms() + 5 * 60_000 {
        return Err(invalid("Session key publication is from the future."));
    }
    crypto::verify_control_message(
        public,
        &crypto::key_blob_digest(
            &dto.id.to_string(),
            &dto.incarnation_id,
            &credentials.keys.device_id,
            &wrapped,
            dto.key_generation,
            issued,
        )?,
        &signature,
    )?;
    let (session, channel) = crypto::unwrap_session_key(
        &credentials.keys.kem_key()?,
        &wrapped,
        &dto.id.to_string(),
        &credentials.keys.device_id,
        dto.key_generation,
    )?;
    Ok(Keys { session, channel })
}

async fn host_trusted(
    network: &BackendClient,
    credentials: &Credentials,
    dto: &SessionDto,
    public: &[u8],
) -> Result<()> {
    let current_credentials = network.credentials()?;
    network.check_credentials(credentials)?;
    let current: SessionDto = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{}", dto.id),
            &current_credentials,
            None,
        )
        .await?;
    if current.incarnation_id != dto.incarnation_id {
        return Err(Error::Stale);
    }
    let owner = network
        .fetch_identity_with(&current_credentials, &dto.owner_user_id, false)
        .await?;
    if owner
        .devices
        .get(&dto.host_device_id)
        .is_none_or(|certificate| certificate.sig_public_key != public)
    {
        return Err(Error::Trust(
            "The hosting device is no longer trusted.".into(),
        ));
    }
    Ok(())
}

async fn acknowledge(socket: &mut Socket, frame: &[u8], unacknowledged: &mut usize) -> Result<()> {
    let Some(sequence) = wire::output_end(frame) else {
        return Ok(());
    };
    *unacknowledged += frame.len();
    if *unacknowledged >= ACKNOWLEDGE_BYTES {
        *unacknowledged = 0;
        send_json(socket, json!({"type":"received","sequence":sequence})).await?;
    }
    Ok(())
}

fn interval(milliseconds: u64) -> tokio::time::Interval {
    let mut timer = tokio::time::interval(Duration::from_millis(milliseconds));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    timer
}

#[expect(
    clippy::too_many_arguments,
    reason = "one remote connection owns its exact authenticated tuple"
)]
async fn run(
    network: &BackendClient,
    mut socket: Socket,
    mut dto: SessionDto,
    credentials: Credentials,
    connection_id: Uuid,
    mut keys: Keys,
    public: Vec<u8>,
    mut frames: wire::FreshFrames,
    mut commands: mpsc::Receiver<RemoteRequest>,
    updates: &mpsc::Sender<RemoteUpdate>,
    cancel: &CancellationToken,
) -> Result<()> {
    let generation = credentials.generation;
    let mut outgoing_sequence = 0u64;
    let mut unacknowledged = 0usize;
    let mut pending = BTreeMap::<Uuid, Pending>::new();
    let mut heartbeat = interval(20_000);
    let mut deadlines = interval(250);
    let mut authorization_tick = tokio::time::interval(TRUST_CHECK);
    authorization_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    authorization_tick.tick().await;
    loop {
        check_generation(network, generation)?;
        tokio::select! {
            ()=cancel.cancelled()=>{let _outcome=socket.close(None).await;return Ok(());}
            ()=network.inner.shutdown.cancelled()=>return Ok(()),
            _=deadlines.tick()=>{
                let now=tokio::time::Instant::now();
                frames.check_deadline()?;
                let expired=pending.iter().filter(|(_,pending)|pending.deadline<=now).map(|(id,_)|*id).collect::<Vec<_>>();
                for id in expired {if let Some(pending)=pending.remove(&id){let _outcome=pending.reply.send(Err(invalid("The host did not confirm the operation; it was not retried.")));}}
            }
            _=heartbeat.tick()=>send_json(&mut socket,json!({"type":"ping"})).await?,
            _=authorization_tick.tick()=>match host_trusted(network,&credentials,&dto,&public).await {
                Err(error) if error.unanswered()=>tracing::warn!(%error,"host trust check got no answer; it will be tried again"),
                result=>result?,
            },
            request=commands.recv()=>match request {
                Some(RemoteRequest::Checkpoint)=>{
                    if let Some(challenge)=frames.request()?{send_json(&mut socket,json!({"type":"checkpointRequest","challenge":BASE64.encode(challenge)})).await?;}
                }
                Some(RemoteRequest::Control {control,reply})=>{
                    if !frames.admitted(){let _outcome=reply.send(Err(invalid("Wait for the terminal snapshot before sending input.")));continue;}
                    if pending.len()>=128{let _outcome=reply.send(Err(Error::Busy));continue;}
                    outgoing_sequence=outgoing_sequence.checked_add(1).ok_or_else(||invalid("Terminal control sequence exhausted."))?;
                    let request_id=Uuid::now_v7();
                    let identity=ControlIdentity {session_id:dto.id,incarnation_id:dto.incarnation_id,authorization_revision:dto.authorization_revision,key_generation:dto.key_generation,connection_id,user_id:credentials.user_id.clone(),device_id:credentials.keys.device_id.clone(),sequence:outgoing_sequence,request_id};
                    let encoded=match wire::encode_control(&control){Ok(value)=>value,Err(error)=>{let _outcome=reply.send(Err(error));outgoing_sequence-=1;continue;}};
                    let (nonce,ciphertext)=crypto::encrypt_control_payload(&keys.channel,crypto::TrafficStream::Control,&identity.aad()?,&encoded)?;
                    send_json(&mut socket,json!({"type":"control","sequence":outgoing_sequence,"requestId":request_id,"keyGeneration":dto.key_generation,"ciphertext":BASE64.encode(ciphertext),"nonce":BASE64.encode(nonce)})).await?;
                    pending.insert(request_id,Pending {identity,reply,deadline:tokio::time::Instant::now()+Duration::from_secs(5)});
                }
                None=>return Ok(()),
            },
            incoming=socket.next()=>{
                let message=incoming.ok_or(Error::Closed)?.map_err(|error|invalid(error.to_string()))?;
                match message {
                    Message::Binary(frame)=>{
                        for update in frames.decode(&keys.session,&frame)? {
                            updates.send(update).await.map_err(|_|Error::Closed)?;
                        }
                        acknowledge(&mut socket,&frame,&mut unacknowledged).await?;
                    }
                    Message::Text(text)=>{
                        let value:Value=serde_json::from_str(&text)?;
                        match wire::text(&value,"type")? {
                            "ping"=>send_json(&mut socket,json!({"type":"pong"})).await?,
                            "pong"=>{},
                            "accessChanged"=>return Err(Error::Stale),
                            "rekey"=>{
                                let current_credentials=network.credentials()?;
                                network.check_credentials(&credentials)?;
                                dto.authorization_revision=wire::number(&value,"authorizationRevision")?;
                                dto.key_generation=u32::try_from(wire::number(&value,"keyGeneration")?).map_err(|_|invalid("Invalid key generation."))?;
                                keys=session_keys(network,&current_credentials,&dto,&public).await?;
                                let challenge=frames.rekey(dto.authorization_revision,dto.key_generation)?;
                                outgoing_sequence=0;
                                for (_,waiting) in std::mem::take(&mut pending){let _outcome=waiting.reply.send(Err(Error::Refreshed));}
                                send_json(&mut socket,json!({"type":"resync","challenge":BASE64.encode(challenge)})).await?;
                                updates.send(RemoteUpdate::Resync).await.map_err(|_|Error::Closed)?;
                            }
                            "resync"=>{
                                let challenge=frames.restart()?;
                                send_json(&mut socket,json!({"type":"resync","challenge":BASE64.encode(challenge)})).await?;
                                updates.send(RemoteUpdate::Resync).await.map_err(|_|Error::Closed)?;
                            }
                            "checkpointProof"=>frames.proof(&value)?,
                            "controlResult"=>{
                                let Some(waiting)=pending.remove(&wire::id(&value,"requestId")?)else{continue;};
                                let plaintext=crypto::decrypt_control_payload(&keys.channel,crypto::TrafficStream::ControlResult,&waiting.identity.aad()?,&wire::decode_b64(&value,"nonce",12)?,&wire::decode_b64(&value,"ciphertext",8192)?)?;
                                let result:Value=serde_json::from_slice(&plaintext)?;
                                let accepted=result.get("accepted").and_then(Value::as_bool).ok_or_else(||invalid("Invalid terminal result."))?;
                                let _outcome=waiting.reply.send(if accepted {Ok(Value::Null)}else{Err(invalid(result.get("message").and_then(Value::as_str).unwrap_or_default()))});
                            }
                            "ended"=>{terminal_ended(&value, &frames)?; updates.send(RemoteUpdate::Ended { final_sequence: wire::number(&value,"finalSequence")? }).await.map_err(|_|Error::Closed)?; return Ok(());},
                            _=>return Err(invalid("Unsupported participant connection message.")),
                        }
                    }
                    Message::Ping(payload)=>send_pong(&mut socket,payload).await?,
                    Message::Pong(_)=>{},
                    Message::Close(_)=>return Err(Error::Closed),
                    Message::Frame(_)=>return Err(invalid("Unexpected participant connection payload.")),
                }
            }
        }
    }
}

fn terminal_ended(value: &Value, frames: &wire::FreshFrames) -> Result<()> {
    if wire::number(value, "finalSequence")? != frames.next_sequence().ok_or(Error::Stale)? {
        return Err(invalid("Terminal end crossed output boundary."));
    }
    Ok(())
}
