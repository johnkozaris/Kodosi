use super::*;

struct Pending {
    identity: ControlIdentity,
    reply: oneshot::Sender<Result<Value>>,
    deadline: tokio::time::Instant,
}

#[expect(
    clippy::too_many_lines,
    reason = "ordered trust-key-handshake admission must complete before exposing a live remote handle"
)]
pub(super) async fn connect(network: Network, id: Uuid) -> Result<RemoteConnection> {
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
    let blob: Value = network
        .inner
        .http
        .device(
            Method::GET,
            &format!("api/sessions/{id}/keys/mine"),
            &credentials,
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
            != u64::from(crate::protocol::RELAY_VERSION)
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
        &public,
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
    let key = crypto::unwrap_session_key(
        &credentials.keys.kem_key()?,
        &wrapped,
        &dto.id.to_string(),
        &credentials.keys.device_id,
        dto.key_generation,
    )?;
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
            key,
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
    network: &Network,
    mut socket: Socket,
    dto: SessionDto,
    credentials: Credentials,
    connection_id: Uuid,
    key: Zeroizing<crypto::SessionKey>,
    public: Vec<u8>,
    mut frames: wire::FreshFrames,
    mut commands: mpsc::Receiver<RemoteRequest>,
    updates: &mpsc::Sender<RemoteUpdate>,
    cancel: &CancellationToken,
) -> Result<()> {
    let generation = credentials.generation;
    let mut outgoing_sequence = 0u64;
    let mut pending = BTreeMap::<Uuid, Pending>::new();
    let mut metadata_dirty = false;
    let mut metadata_tick = interval(2_000);
    let mut heartbeat = interval(20_000);
    let mut deadlines = interval(250);
    let mut authorization_tick = interval(30_000);
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
            _=metadata_tick.tick(),if metadata_dirty && frames.ready()=>{
                if let Some(challenge)=frames.request()? {
                    metadata_dirty=false;
                    send_json(&mut socket,json!({"type":"checkpointRequest","challenge":BASE64.encode(challenge)})).await?;
                }
            }
            _=heartbeat.tick()=>send_json(&mut socket,json!({"type":"ping"})).await?,
            _=authorization_tick.tick()=>{
                let current_credentials=network.credentials()?;
                network.check_credentials(&credentials)?;
                let current:SessionDto=network.inner.http.device(Method::GET,&format!("api/sessions/{}",dto.id),&current_credentials,None).await?;
                if !current.ready || current.incarnation_id!=dto.incarnation_id || current.key_generation!=dto.key_generation || current.authorization_revision!=dto.authorization_revision{return Err(Error::Stale);}
                let owner=network.fetch_identity_with(&current_credentials,&dto.owner_user_id,false).await?;
                if owner.devices.get(&dto.host_device_id).is_none_or(|cert|cert.sig_public_key!=public){return Err(Error::Trust("The hosting device is no longer trusted.".into()));}
            }
            request=commands.recv()=>match request {
                Some(RemoteRequest::Checkpoint)=>{
                    if let Some(challenge)=frames.request()?{send_json(&mut socket,json!({"type":"checkpointRequest","challenge":BASE64.encode(challenge)})).await?;}
                }
                Some(RemoteRequest::Control {control,reply})=>{
                    if !frames.ready(){let _outcome=reply.send(Err(invalid("Wait for the terminal snapshot before sending input.")));continue;}
                    if pending.len()>=128{let _outcome=reply.send(Err(Error::Busy));continue;}
                    outgoing_sequence=outgoing_sequence.checked_add(1).ok_or_else(||invalid("Terminal control sequence exhausted."))?;
                    let request_id=Uuid::now_v7();
                    let identity=ControlIdentity {session_id:dto.id,incarnation_id:dto.incarnation_id,authorization_revision:dto.authorization_revision,key_generation:dto.key_generation,connection_id,user_id:credentials.user_id.clone(),device_id:credentials.keys.device_id.clone(),sequence:outgoing_sequence,request_id};
                    let encoded=match wire::encode_control(&control){Ok(value)=>value,Err(error)=>{let _outcome=reply.send(Err(error));outgoing_sequence-=1;continue;}};
                    let (nonce,ciphertext)=crypto::encrypt_control_payload(&key,&identity.aad()?,&encoded)?;
                    let signature=crypto::sign_control_message(credentials.keys.signing_pkcs8(),&identity.signature(&nonce,&ciphertext)?)?;
                    send_json(&mut socket,json!({"type":"control","sequence":outgoing_sequence,"requestId":request_id,"keyGeneration":dto.key_generation,"ciphertext":BASE64.encode(ciphertext),"nonce":BASE64.encode(nonce),"signature":BASE64.encode(signature)})).await?;
                    pending.insert(request_id,Pending {identity,reply,deadline:tokio::time::Instant::now()+Duration::from_secs(5)});
                }
                None=>return Ok(()),
            },
            incoming=socket.next()=>{
                let message=incoming.ok_or(Error::Closed)?.map_err(|error|invalid(error.to_string()))?;
                match message {
                    Message::Binary(frame)=>{
                        for update in frames.decode(&key,&frame)? {
                            updates.try_send(update).map_err(|_|Error::Busy)?;
                        }
                    }
                    Message::Text(text)=>{
                        let value:Value=serde_json::from_str(&text)?;
                        match wire::text(&value,"type")? {
                            "ping"=>send_json(&mut socket,json!({"type":"pong"})).await?,
                            "pong"=>{},
                            "accessChanged"=>return Err(Error::Stale),
                            "metadataChanged"=>{metadata_dirty=true;},
                            "checkpointProof"=>frames.proof(&value)?,
                            "controlResult"=>{
                                let request=wire::id(&value,"requestId")?;
                                let Some(pending_request)=pending.get(&request)else{continue;};
                                if wire::id(&value,"connectionId")?!=pending_request.identity.connection_id
                                    || wire::number(&value,"sequence")?!=pending_request.identity.sequence
                                    || wire::id(&value,"sessionId")?!=dto.id || wire::id(&value,"incarnationId")?!=dto.incarnation_id
                                    || wire::number(&value,"authorizationRevision")?!=dto.authorization_revision
                                    || wire::number(&value,"keyGeneration")?!=u64::from(dto.key_generation)
                                    || wire::text(&value,"senderUserId")?!=credentials.user_id
                                    || wire::text(&value,"senderDeviceId")?!=credentials.keys.device_id
                                {return Err(invalid("Host result targets another terminal command."));}
                                let accepted=value.get("accepted").and_then(Value::as_bool).ok_or_else(||invalid("Invalid terminal result."))?;
                                let message=value.get("message").and_then(Value::as_str).unwrap_or_default();
                                if message.len()>4096{return Err(invalid("Terminal result message exceeds its bound."));}
                                crypto::verify_control_message(&public,&pending_request.identity.result_signature(accepted,message)?,&wire::decode_b64(&value,"signature",3309)?)?;
                                if let Some(pending)=pending.remove(&request){let _outcome=pending.reply.send(if accepted {Ok(Value::Null)}else{Err(invalid(message))});}
                            }
                            "ended"=>{terminal_ended(&value, &frames)?; updates.try_send(RemoteUpdate::Ended { final_sequence: wire::number(&value,"finalSequence")? }).map_err(|_|Error::Busy)?; return Ok(());},
                            _=>return Err(invalid("Unsupported participant relay message.")),
                        }
                    }
                    Message::Ping(payload)=>send_pong(&mut socket,payload).await?,
                    Message::Pong(_)=>{},
                    Message::Close(_)=>return Err(Error::Closed),
                    Message::Frame(_)=>return Err(invalid("Unexpected participant relay payload.")),
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
