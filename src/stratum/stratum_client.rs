/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{
    Miner, MiningNotifyBody, MiningNotifyMessage, MiningSetTargetBody, MiningSetTargetMessage,
    MiningSubmitBody, MiningSubmitMessage, MiningSubscribeBody, MiningSubscribeMessage,
    MiningSubscribedBody, MiningSubscribedMessage, MiningWaitForWorkMessage, StratumMessage,
    StratumMessageCodec,
};
use anyhow::{anyhow, Result};
use futures::SinkExt;
use log::*;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc, Weak,
    },
    time::Duration,
};
use tokio::{
    io::{split, AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::{mpsc, oneshot, RwLock},
    task,
};
use tokio_native_tls::{native_tls, TlsConnector};
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite};

type Router = mpsc::Sender<StratumClientRequest>;
#[allow(dead_code)]
type Handler = mpsc::Receiver<StratumClientRequest>;

enum StratumClientRequest {
    Message(StratumMessage),
    Stop,
}

#[derive(Clone, Debug)]
pub struct StratumClientConfig {
    pub tls: bool,
    pub pool_address: SocketAddr,
    pub public_address: String,
    pub worker_name: String,
}

#[derive(Debug)]
pub struct StratumClient {
    config: StratumClientConfig,
    miner: RwLock<Option<Weak<Miner>>>,
    next_message_id: AtomicI64,
    router: RwLock<Option<Router>>,
    started: AtomicBool,
    stopped: AtomicBool,
    subscribed: AtomicBool,
}

impl StratumClient {
    pub fn new(config: StratumClientConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            miner: Default::default(),
            next_message_id: Default::default(),
            router: Default::default(),
            subscribed: Default::default(),
            started: Default::default(),
            stopped: Default::default(),
        })
    }

    pub fn is_subscribed(&self) -> bool {
        self.subscribed.load(Ordering::Relaxed)
    }

    pub async fn set_miner(&self, miner: Weak<Miner>) {
        *self.miner.write().await = Some(miner);
    }

    pub async fn submit(&self, mining_request_id: u32, randomness: String) {
        trace!("submit {} {}", mining_request_id, randomness);
        if !self.subscribed.load(Ordering::Relaxed) {
            return;
        }
        let message = StratumMessage::MiningSubmitMessage(MiningSubmitMessage {
            id: self.next_message_id.fetch_add(1, Ordering::SeqCst),
            method: String::from("mining.submit"),
            body: MiningSubmitBody {
                miningRequestId: mining_request_id,
                randomness,
            },
        });
        let _ = self
            .router
            .read()
            .await
            .as_ref()
            .unwrap()
            .send(StratumClientRequest::Message(message))
            .await;
    }

    pub async fn stop(&self) {
        if !self.started.load(Ordering::Relaxed) {
            return;
        }
        self.stopped.store(true, Ordering::SeqCst);
        if !self.is_subscribed() {
            return;
        }
        let _ = self
            .router
            .read()
            .await
            .as_ref()
            .unwrap()
            .send(StratumClientRequest::Stop)
            .await;
    }

    pub async fn start(client: Arc<Self>) {
        if client.started.load(Ordering::Relaxed) {
            return;
        }
        client.stopped.store(false, Ordering::SeqCst);
        client.started.store(true, Ordering::SeqCst);
        let (router, handler) = oneshot::channel();
        task::spawn(async move {
            let _ = router.send(());
            let client = client.clone();
            'outer: loop {
                info!("Connecting to pool({})...", client.config.pool_address);
                let mut connect_warned = false;
                loop {
                    if let Ok(tcp_stream) = TcpStream::connect(client.config.pool_address).await {
                        if client.config.tls {
                            let mut native_tls_builder = native_tls::TlsConnector::builder();
                            native_tls_builder.danger_accept_invalid_certs(true);
                            native_tls_builder.danger_accept_invalid_hostnames(true);
                            native_tls_builder.use_sni(false);
                            let native_tls_connector = native_tls_builder.build().unwrap();
                            let tokio_tls_connector = TlsConnector::from(native_tls_connector);
                            if let Ok(tls_stream) = tokio_tls_connector
                                .connect(&client.config.pool_address.to_string(), tcp_stream)
                                .await
                            {
                                if Self::handle_stratum_connect(client.clone(), tls_stream)
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        } else {
                            if Self::handle_stratum_connect(client.clone(), tcp_stream)
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    if client.stopped.load(Ordering::Relaxed) {
                        break 'outer;
                    }
                    if !connect_warned {
                        warn!(
                            "Failed to connect to pool ({}), retrying...",
                            client.config.pool_address
                        );
                        connect_warned = true;
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                // current link is closed, so reset stratum status
                client.subscribed.store(false, Ordering::SeqCst);
                if let Some(miner) = client.miner.read().await.clone() {
                    miner.upgrade().unwrap().wait_for_work().await;
                }
            }
            // has been stopped, reset stoped flag
            client.started.store(false, Ordering::SeqCst);
            client.stopped.store(false, Ordering::SeqCst);
        });
        let _ = handler.await;
    }

    async fn handle_stratum_connect<T: AsyncRead + AsyncWrite>(
        client: Arc<Self>,
        stream: T,
    ) -> Result<()> {
        info!("Connect pool success({})", client.config.pool_address);
        // process net message
        Self::handle_io_message(client, stream).await?;
        Ok(())
    }
    async fn handle_io_message<T: AsyncRead + AsyncWrite>(
        client: Arc<Self>,
        stream: T,
    ) -> Result<()> {
        let (r, w) = split(stream);
        let mut socket_w_handle = FramedWrite::new(w, StratumMessageCodec::default());
        let mut socket_r_handle = FramedRead::new(r, StratumMessageCodec::default());
        let (router, mut handler) = mpsc::channel(1024);
        *client.router.write().await = Some(router);
        // subscrible
        if let Err(error) = socket_w_handle
            .send(StratumMessage::MiningSubscribeMessage(
                MiningSubscribeMessage {
                    id: client.next_message_id.fetch_add(1, Ordering::SeqCst),
                    method: String::from("mining.subscribe"),
                    body: MiningSubscribeBody {
                        version: 1,
                        name: client.config.worker_name.clone(),
                        publicAddress: client.config.public_address.clone(),
                    },
                },
            ))
            .await
        {
            error!("[Connect pool] {}", error);
            return Ok(());
        }
        match socket_r_handle.next().await {
            Some(Ok(message)) => match message {
                StratumMessage::MiningSubscribedMessage(MiningSubscribedMessage {
                    id,
                    method,
                    body:
                        MiningSubscribedBody {
                            clientId: client_id,
                            graffiti,
                        },
                }) => {
                    debug!(
                        "message id({}) method({}) stratum client id({}) graffiti({})",
                        id, method, client_id, graffiti
                    );
                    client.subscribed.store(true, Ordering::SeqCst);
                    if let Some(miner) = client.miner.read().await.clone() {
                        miner.upgrade().unwrap().set_graffiti(&graffiti[..]).await;
                    }
                }
                _ => {
                    error!("connect pool error, unexpected response message");
                    return Ok(());
                }
            },
            Some(Err(error)) => {
                error!("[Connect pool] {}", error);
                return Ok(());
            }
            None => return Ok(()),
        }

        // main loop
        loop {
            tokio::select! {
                Some(request) = handler.recv() =>  match request {
                    StratumClientRequest::Message(
                        StratumMessage::MiningSubmitMessage(message)
                    ) => {
                        if let Err(error) = socket_w_handle.send(StratumMessage::MiningSubmitMessage(message)).await {
                            error!("[Stratum submit] {}", error);
                        }
                    }
                    StratumClientRequest::Stop => {
                        debug!("[Stratum client stoped]");
                        return Err(anyhow!("Exit"));
                    }
                    _ => error!("invalid message"),
                },

                message = socket_r_handle.next() => match message {
                    Some(Ok(message)) => match message {
                        // 'mining.settarget'
                        StratumMessage::MiningSetTargetMessage(
                            MiningSetTargetMessage {
                                id,
                                method,
                                body: MiningSetTargetBody { target },
                            }
                        ) => {
                            debug!("message id({}) method({}) target({})", id, method, target);
                            if let Some(miner) = client.miner.read().await.clone() {
                                miner.upgrade().unwrap().set_target(&target[..]).await;
                            }
                        }
                        // 'mining.notify'
                        StratumMessage::MiningNotifyMessage(
                            MiningNotifyMessage {
                                id,
                                method,
                                body: MiningNotifyBody {
                                    miningRequestId: mining_request_id,
                                    header,
                                }
                            }
                        ) => {
                            debug!("message id({}) method({}) mining request id({}) header({})", id, method, mining_request_id, header);
                            if let Some(miner) = client.miner.read().await.clone() {
                                miner.upgrade().unwrap().new_work(mining_request_id, header).await;
                            }
                        }
                        // 'mining.wait_for_work'
                        StratumMessage::MiningWaitForWorkMessage(
                            MiningWaitForWorkMessage {
                                id,
                                method,
                            }
                        ) => {
                            debug!("message id({}) method({})", id, method);
                            if let Some(miner) = client.miner.read().await.clone() {
                                miner.upgrade().unwrap().wait_for_work().await;
                            }
                        }
                        _ => {}
                    }
                    Some(Err(error)) => error!("failed to read message from server: {}", error),
                    None => {
                        error!("failed to read message from server");
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
