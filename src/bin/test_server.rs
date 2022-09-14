/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;
use std::net::SocketAddr;

use clap::Parser;
use futures::SinkExt;
use log::*;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::{io::split, net::TcpStream};
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite};
use zkwork_ironminer::{
    MiningNotifyBody, MiningNotifyMessage, MiningSetTargetBody, MiningSetTargetMessage,
    MiningSubmitBody, MiningSubmitMessage, MiningSubscribeBody, MiningSubscribeMessage,
    MiningSubscribedBody, MiningSubscribedMessage, StratumMessage, StratumMessageCodec,
};

#[derive(Debug, Parser)]
#[clap(name = "zkwork_ironminer test server", author = "zk.work")]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Specify the IP address and port of pool to connect to.
    #[clap(long = "pool")]
    pub pool: SocketAddr,
}
enum Msg {
    SubmitWork(StratumMessage, u64),
}

async fn accept_connect(stream: TcpStream, work_id: u64, sender: mpsc::Sender<Msg>) {
    let (r, w) = split(stream);
    let mut w = FramedWrite::new(w, StratumMessageCodec::default());
    let mut r = FramedRead::new(r, StratumMessageCodec::default());
    tokio::task::spawn(async move {
        match r.next().await {
            Some(Ok(message)) => {
                if let StratumMessage::MiningSubscribeMessage(MiningSubscribeMessage {
                    id,
                    method,
                    body:
                        MiningSubscribeBody {
                            version,
                            name,
                            publicAddress: public_address,
                        },
                }) = message
                {
                    info!(
                        "id({}) method({}) version({}) worker_name({}) public address({})",
                        id, method, version, name, public_address
                    );
                    // "mining.subscribed"
                    let subscribed_message =
                        StratumMessage::MiningSubscribedMessage(MiningSubscribedMessage {
                            id: 0,
                            method: String::from("mining.subscribed"),
                            body: MiningSubscribedBody {
                                clientId: work_id,
                                graffiti: format!("Zk.Work.{}", work_id),
                            },
                        });
                    let _ = w.send(subscribed_message).await;

                    // "mining.set_target"
                    let set_target_message =
                    StratumMessage::MiningSetTargetMessage(MiningSetTargetMessage {
                        id: 1,
                        method: String::from("mining.set_target"),
                        body: MiningSetTargetBody {
                            target: String::from(
                                "00000049494cff9a3f4f473f91d116af7382c45e653facfeef85b8f43d9d6b64",
                            ),
                        },
                    });
                    let _ = w.send(set_target_message).await;

                    // "mining.notify"
                    let notify_message = StratumMessage::MiningNotifyMessage(
                        MiningNotifyMessage {
                            id: 2,
                            method: String::from("mining.notify"),
                            body: MiningNotifyBody {
                                miningRequestId: 0,
                                header: String::from("0000000000000000677101000000000000000000000232f50bb970eeab81d7e2053ebaa585d9b7297f7d14c2063a60e8509d3e86a44918c8f318377cbb327f4fc5b602e78784994cf2926f0addd55d1b0d36880100000000f1baa930706f8b9058bc55be1f464b472639a288763a16f7a5713aa761052e43f7bec3000000000000000000000c6072a3898d86f685d4b9bba50e87f750f9773da7ac2cf96663e357c8b30082010000000000007735ccc1666978796f750000000000000000000000000000000000000000000000000000"),
                            },
                        }
                    );
                    let _ = w.send(notify_message).await;
                } else {
                    error!("unexpected message, expected(MiningSubscribeMessage)");
                    return;
                }
            }
            _ => {
                error!("unexpected message, expected(MiningSubscribeMessage)");
                return;
            }
        }
        loop {
            match r.next().await {
                Some(Ok(message)) => {
                    debug!("worker {} - {:?}", work_id, message);
                    //verify(message, work_id);
                    let _ = sender.send(Msg::SubmitWork(message, work_id)).await;
                }
                Some(Err(error)) => {
                    error!("{}", error);
                    break;
                }
                None => {
                    info!("client({}) out.", work_id);
                    break;
                }
            }
        }
    });
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init_timed();
    let cli = Cli::parse();
    info!("server listen at {}", cli.pool);
    let listener = TcpListener::bind(cli.pool).await?;
    let mut work_id = 0;
    let (sender, mut receiver) = mpsc::channel(1024);
    // process share
    tokio::task::spawn(async move {
        let mut nonces = HashSet::new();
        loop {
            match receiver.recv().await {
                Some(Msg::SubmitWork(message, work_id)) => {
                    if let StratumMessage::MiningSubmitMessage(MiningSubmitMessage {
                        id: _,
                        method: _,
                        body:
                            MiningSubmitBody {
                                miningRequestId: _,
                                randomness,
                            },
                    }) = message.clone()
                    {
                        if !nonces.insert(format!("{}-{}", work_id, randomness)) {
                            println!(
                                "Duplicate SHARE commits client({}) randomness({})",
                                work_id, randomness
                            );
                        }
                    }
                    verify(message, work_id);
                }
                _ => {}
            }
        }
    });
    loop {
        let (stream, _) = listener.accept().await?;
        accept_connect(stream, work_id, sender.clone()).await;
        work_id += 1;
    }
}

fn bytes_lte(a: &[u8], b: &[u8]) -> bool {
    let len = a.len();
    for i in 0..len {
        if a[i] < b[i] {
            return true;
        } else if a[i] > b[i] {
            return false;
        }
    }
    true
}

fn verify(message: StratumMessage, work_id: u64) {
    let mut graffiti_bytes: [u8; 32] = [0; 32];
    let graffiti = format!("Zk.Work.{}", work_id);
    let len = graffiti.as_bytes().len();
    graffiti_bytes[0..len].copy_from_slice(graffiti.as_bytes());
    let header  = String::from("0000000000000000677101000000000000000000000232f50bb970eeab81d7e2053ebaa585d9b7297f7d14c2063a60e8509d3e86a44918c8f318377cbb327f4fc5b602e78784994cf2926f0addd55d1b0d36880100000000f1baa930706f8b9058bc55be1f464b472639a288763a16f7a5713aa761052e43f7bec3000000000000000000000c6072a3898d86f685d4b9bba50e87f750f9773da7ac2cf96663e357c8b30082010000000000007735ccc1666978796f750000000000000000000000000000000000000000000000000000");
    let mut header_bytes = hex::decode(header).unwrap();
    header_bytes[176..176 + 32].copy_from_slice(graffiti_bytes.as_slice());
    let target = hex::decode(String::from(
        "00000049494cff9a3f4f473f91d116af7382c45e653facfeef85b8f43d9d6b64",
    ))
    .unwrap();

    if let StratumMessage::MiningSubmitMessage(MiningSubmitMessage {
        id: _,
        method: _,
        body: MiningSubmitBody {
            miningRequestId,
            randomness,
        },
    }) = message
    {
        let nonce = hex::decode(randomness).unwrap();
        header_bytes[0..8].copy_from_slice(nonce.as_slice());
        let hash = blake3::hash(&header_bytes);
        trace!("header: {:x?}", header_bytes);
        trace!("hash: {:x?}", hash.as_bytes());
        if !bytes_lte(hash.as_bytes(), &target) {
            println!(
                "Invalid nonce returned nonce: {:#x?} mining_request_id {}",
                nonce, miningRequestId
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_verify() {
        let message = StratumMessage::MiningSubmitMessage(MiningSubmitMessage {
            id: 0,
            method: String::from("mining.submit"),
            body: MiningSubmitBody {
                miningRequestId: (0),
                randomness: (hex::encode(0x000000000e45e45d_u64.to_be_bytes())),
            },
        });
        verify(message, 1);
    }
}
