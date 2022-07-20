# zkwork_ironminer

## 1. Overview

 **zkwork_ironminer** is a rust implementation for ironfish pool miner

## 2. Command Line interface

```powershell
The zk.work team <zk.work@6block.com>

USAGE:
    zkwork_ironminer [OPTIONS] --pool <POOL> --address <ADDRESS>

OPTIONS:
        --address <ADDRESS>            Specify your mining reward address
        --batch_size <BATCH_SIZE>      Specify batch size [default: 10000]
    -h, --help                         Print help information
        --pool <POOL>                  Specify the IP address and port of pool to connect to
        --threads <THREADS_COUNT>      Specify your worker thread count [default: 16]
    -V, --version                      Print version information
        --worker_name <WORKER_NAME>    Specify your worker name [default: "zkwork miner"]
 ```

## Compile

```powershell
./install_deps.sh
RUSTFLAGS="-C target-cpu=native"
cargo build --release
```

## Test

In one termimal, start the test server by running:

```powershell
export RUST_LOG=info
cargo run --bin test_server
```

In the second terminal, run:

```powershell
export RUST_LOG=info
cargo run --release  -- --pool 127.0.0.1:8181 --address "91f65bdad677058fe9e674931a7be0fa34d615317e992fb1af2ae30547c2c276bb9a"
```

Or, link a real ifonfish pool

## License

This code base and any contributions will be under the [MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/) Software License.
