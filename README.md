# Introduction

A blockchain project built using the Polkadot SDK.

## Requirements

|             | CPU     | RAM  | Disk  |
|-------------|---------|------|-------|
| Minimum     | 1 core  | 1 GB |  5 GB |
| Recommended | 4 cores | 4 GB | 20 GB |

Disk usage grows as the chain does.

## Getting Started

See [docs/how-to-build.md](docs/how-to-build.md) for instructions on building this blockchain node program in Rust.

## Mining

Mine locally and credit rewards to the given account.

```bash
./numen --miner <YOUR_ADDRESS> --node-miner
```

`--miner` sets the reward address and exposes the mining RPC so external miners can scan off the node and submit seals.
Pull the current task with `mining_getTask` or subscribe to `mining_subscribeTask` for a fresh task pushed every second, then return a found seal with `mining_submitSeal`. 
Add `--node-miner` to also run the in-process scan loop across every core, or `--node-miner <THREADS>` to cap the scan threads.
Drop it to leave block authoring entirely to external miners.

Mining never needs a private key. The node only puts the payout `AccountId` into the block header, 
so generate a keypair offline (e.g. with `subkey generate`) and pass only the SS58 address to the mining node. 
Keep the private key on a separate, offline machine.

If the address is invalid SS58 the node refuses to start.