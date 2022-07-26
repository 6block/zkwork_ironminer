/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use anyhow::Result;
use clap::Parser;
use log::*;
use std::{sync::Arc, time::Duration};
use tokio::{runtime, sync::oneshot, task};
use zkwork_ironminer::{cli::Cli, Miner};

fn main() -> Result<()> {
    pretty_env_logger::init_timed();
    let cli = Cli::parse();
    debug!("cli: {:?}", cli);
    let (num_tokio_worker_threads, max_tokio_blocking_threads) = (num_cpus::get(), 1024); // 512 is tokio's current default

    // Initialize the runtime configuration.
    let runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .worker_threads(num_tokio_worker_threads)
        .max_blocking_threads(max_tokio_blocking_threads)
        .build()?;

    runtime.block_on(async move {
        let miner = Miner::initialize(cli).await;
        let _ = handle_signals(miner.clone()).await;
        Miner::start(miner.clone()).await.unwrap();
    });
    Ok(())
}

// Handles OS signals for the node to intercept and perform a clean shutdown.
// Note: Only Ctrl-C is supported; it should work on both Unix-family systems and Windows.
async fn handle_signals(miner: Arc<Miner>) -> Result<()> {
    let (router, handler) = oneshot::channel();
    task::spawn(async move {
        let _ = router.send(());
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("shutdowning...");
                miner.stop().await;
                tokio::time::sleep(Duration::from_millis(5000)).await;
                info!("goodbye");
                std::process::exit(0);
            }
            Err(error) => error!("tokio::signal::ctrl_c encountered an error: {}", error),
        }
    });
    let _ = handler.await;
    debug!("install signals handle");
    Ok(())
}
