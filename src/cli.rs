/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use clap::Parser;
use std::net::SocketAddr;

#[derive(Debug, Parser)]
#[clap(name = "zkwork_ironminer", author = "zk.work")]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Specify the IP address and port of pool to connect to.
    #[clap(long = "pool")]
    pub pool: SocketAddr,
    /// Specify your mining reward address.
    #[clap(long = "address")]
    pub address: String,
    /// Specify your worker name.
    #[clap(long = "worker_name", default_value = "zkwork miner")]
    pub worker_name: String,
    /// Specify your worker thread count.
    #[clap(long = "threads", default_value_t = num_cpus::get())]
    pub threads_count: usize,
    /// Specify batch size
    #[clap(long = "batch_size", default_value_t = 10000)]
    pub batch_size: u32,
    /// Connect to server over tls
    #[clap(long = "tls", default_value_t = false)]
    pub tls: bool,
}
