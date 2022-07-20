/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use futures::SinkExt;
use log::*;
use tokio::io::split;
use tokio::net::TcpListener;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite};
use zkwork_ironminer::{
    MiningNotifyBody, MiningNotifyMessage, MiningSetTargetBody, MiningSetTargetMessage,
    MiningSubscribeBody, MiningSubscribeMessage, MiningSubscribedBody, MiningSubscribedMessage,
    StratumMessage, StratumMessageCodec,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init_timed();
    info!("server listen at 127.0.0.1:8181");
    let listener = TcpListener::bind("127.0.0.1:8181").await?;
    let (stream, _) = listener.accept().await?;
    let (r, w) = split(stream);
    let mut w = FramedWrite::new(w, StratumMessageCodec::default());
    let mut r = FramedRead::new(r, StratumMessageCodec::default());

    match r.next().await {
        Some(Ok(message)) => {
            if let StratumMessage::MiningSubscribeMessage(MiningSubscribeMessage {
                id,
                method,
                body:
                    MiningSubscribeBody {
                        version,
                        publicAddress: public_address,
                    },
            }) = message
            {
                info!(
                    "id({}) method({}) version({}) public address({})",
                    id, method, version, public_address
                );
                // "mining.subscribed"
                let subscribed_message =
                    StratumMessage::MiningSubscribedMessage(MiningSubscribedMessage {
                        id: 0,
                        method: String::from("mining.subscribed"),
                        body: MiningSubscribedBody {
                            clientId: 1,
                            graffiti: String::from("Iron Fish Pool.1"),
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
                return Ok(());
            }
        }
        _ => {
            error!("unexpected message, expected(MiningSubscribeMessage)");
            return Ok(());
        }
    }
    loop {
        match r.next().await {
            Some(Ok(message)) => {
                info!("{:?}", message);
            }
            Some(Err(error)) => {
                error!("{}", error);
                break;
            }
            None => break,
        }
    }
    Ok(())
}
