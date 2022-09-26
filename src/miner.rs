/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{Cli, Meter, StratumClient, StratumClientConfig};
use anyhow::Result;
use ironfish_rust::mining;
use log::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot, RwLock},
    task, time,
};

type MinerRouter = mpsc::Sender<MinerRequest>;
type MinerHandler = mpsc::Receiver<MinerRequest>;

const GRAFFITI_SIZE: usize = 32;
#[derive(Debug)]
enum MinerRequest {
    NewWork(Vec<u8>, [u8; 32], u32),
    WaitForWork,
    Stop,
}
#[derive(Debug)]
pub struct Miner {
    cli: Cli,
    graffiti: RwLock<Option<[u8; GRAFFITI_SIZE]>>,
    hashrare: Arc<Meter>,
    router: RwLock<Option<MinerRouter>>,
    stratum_client: Arc<StratumClient>,
    target: RwLock<[u8; 32]>,
    waiting: AtomicBool,
}

impl Miner {
    pub async fn initialize(cli: Cli) -> Arc<Self> {
        let stratum_client_config = StratumClientConfig {
            tls: cli.tls,
            pool_address: cli.pool,
            public_address: cli.address.clone(),
            worker_name: cli.worker_name.clone(),
        };
        let miner = Arc::new(Miner {
            cli,
            graffiti: RwLock::default(),
            hashrare: Meter::new(),
            router: RwLock::default(),
            stratum_client: StratumClient::new(stratum_client_config),
            target: RwLock::default(),
            waiting: Default::default(),
        });
        miner.stratum_client.set_miner(Arc::downgrade(&miner)).await;
        miner
    }

    pub async fn set_target(&self, target: &str) {
        self.target
            .write()
            .await
            .copy_from_slice(hex::decode(target).unwrap().as_slice());
    }

    pub async fn set_graffiti(&self, graffiti: &str) {
        let mut graffiti_bytes: [u8; 32] = [0; 32];
        let len = graffiti.as_bytes().len();
        graffiti_bytes[0..len].copy_from_slice(graffiti.as_bytes());
        *self.graffiti.write().await = Some(graffiti_bytes);
    }

    pub async fn new_work(&self, mining_request_id: u32, header: String) {
        debug!(
            "new work: target({}) mining request id({})",
            hex::encode(*self.target.read().await),
            mining_request_id
        );
        let mut header_bytes = hex::decode(header).unwrap();
        header_bytes[176..176 + 32].copy_from_slice(self.graffiti.read().await.unwrap().as_slice());
        self.waiting.store(false, Ordering::SeqCst);

        let request =
            MinerRequest::NewWork(header_bytes, *self.target.read().await, mining_request_id);
        self.send_request(request).await;
    }

    pub async fn wait_for_work(&self) {
        self.waiting.store(true, Ordering::SeqCst);
        self.send_request(MinerRequest::WaitForWork).await;
    }

    pub async fn start(miner: Arc<Miner>) -> Result<()> {
        StratumClient::start(miner.stratum_client.clone()).await;
        Meter::start(miner.hashrare.clone()).await;
        let (router, handler) = mpsc::channel(1024);
        *miner.router.write().await = Some(router);
        Miner::mine(miner, handler).await;
        // Do not delete the following line of code
        std::future::pending::<()>().await;
        Ok(())
    }

    pub async fn stop(&self) {
        self.stratum_client.stop().await;
        self.hashrare.stop().await;
        self.send_request(MinerRequest::Stop).await;
    }

    async fn mine(miner: Arc<Miner>, mut miner_handler: MinerHandler) {
        let (router, handler) = oneshot::channel();
        task::spawn(async move {
            let _ = router.send(());
            let mut thread_pool =
                mining::threadpool::ThreadPool::new(miner.cli.threads_count, miner.cli.batch_size);
            let mut interval = time::interval(Duration::from_millis(10));
            let mut hash_rate_printer = 0;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !miner.stratum_client.is_subscribed() {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            continue;
                        }
                        let graffiti_is_none = miner.graffiti.read().await.is_none();
                        if graffiti_is_none {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            continue;
                        }
                        let block_result = thread_pool.get_found_block();
                        if let Some((randomness, mining_request_id)) = block_result {
                            info!(
                                "Found share: randomness({}) mining_request_id({}) {} .",
                                randomness,
                                mining_request_id,
                                Meter::format(miner.hashrare.get_rate_1s().await),
                             );
                            miner.stratum_client.submit(mining_request_id, hex::encode(randomness.to_be_bytes())).await;
                            hash_rate_printer = 0;
                        }
                        // hashrate
                        let amounts = thread_pool.get_hash_rate_submission();
                        miner.hashrare.add(amounts as u64).await;
                        hash_rate_printer = (hash_rate_printer + 1) % 10000;
                        if hash_rate_printer == 0 {
                            info!("Hash Rate: {}", Meter::format(miner.hashrare.get_rate_1s().await));
                        }

                    }
                    Some(request) = miner_handler.recv() => match request {
                        MinerRequest::NewWork(header_bytes, target, mining_request_id) => {
                            thread_pool.new_work(header_bytes.as_slice(), target.as_slice(), mining_request_id);
                        },
                        MinerRequest::WaitForWork => {
                            thread_pool.pause();
                        }
                        MinerRequest::Stop => {
                            debug!("miner stop.");
                            thread_pool.stop();
                            // A delay to allow thread pool resources to be released
                            tokio::time::sleep(Duration::from_millis(2000)).await;
                            break;
                        }
                    }
                }
            }
        });
        let _ = handler.await;
    }

    async fn send_request(&self, request: MinerRequest) {
        if self.router.read().await.is_none() {
            return;
        }
        let _ = self
            .router
            .read()
            .await
            .as_ref()
            .unwrap()
            .send(request)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn prepare_test_miner() -> Arc<Miner> {
        let cli = Cli {
            pool: "127.0.0.1:8080".parse().unwrap(),
            address: String::from("xxxxxx"),
            worker_name: String::from("xxxxxx"),
            threads_count: 16,
            batch_size: 10000,
        };
        Miner::initialize(cli).await
    }
    #[tokio::test]
    async fn test_target() {
        let target_hex = [
            0x00, 0x00, 0x00, 0x00, 0x49, 0x4c, 0xff, 0x9a, 0x3f, 0x4f, 0x47, 0x3f, 0x91, 0xd1,
            0x16, 0xaf, 0x73, 0x82, 0xc4, 0x5e, 0x65, 0x3f, 0xac, 0xfe, 0xef, 0x85, 0xb8, 0xf4,
            0x3d, 0x9d, 0x6b, 0x64,
        ];
        let target_string =
            String::from("00000000494cff9a3f4f473f91d116af7382c45e653facfeef85b8f43d9d6b64");
        let miner = prepare_test_miner().await;
        miner.set_target(&target_string[..]).await;
        assert_eq!(target_hex, *miner.target.read().await);
    }

    #[tokio::test]
    async fn test_graffiti() {
        let graffiti_hex = [
            0x49, 0x72, 0x6f, 0x6e, 0x20, 0x46, 0x69, 0x73, 0x68, 0x20, 0x50, 0x6f, 0x6f, 0x6c,
            0x2e, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let graffiti_string = String::from("Iron Fish Pool.1");
        let miner = prepare_test_miner().await;
        miner.set_graffiti(&graffiti_string[..]).await;
        println!("{:0x?}", miner.graffiti.read().await.unwrap());
        assert_eq!(graffiti_hex, miner.graffiti.read().await.unwrap());
    }

    #[test]
    fn test_randomness() {
        let randomness = 0x00001234u64;
        let s_1 = format!("{:016x}", randomness);
        let s_2 = hex::encode(randomness.to_be_bytes());
        println!("{}", s_1);
        println!("{}", s_2);
    }
}
