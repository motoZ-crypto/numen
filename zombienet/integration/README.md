# Multi-Node Integration & Zombienet Tests

End-to-end test harness for the crypto-node multi-node network. Drives a 5-node topology with [zombienet](https://github.com/paritytech/zombienet) and exercises PoW production, GRANDPA finality, validator lifecycle (lock/exit), difficulty adjustment, and network-partition recovery.

## Topology (`zombienet.toml`)

| Node    | Role            | Initial validator | Mines | Notes                           |
| ------- | --------------- | ----------------: | :---: | ------------------------------- |
| alice   | validator+miner |               yes |  yes  | bootnode                        |
| bob     | validator+miner |               yes |  yes  |                                 |
| charlie | validator+miner |               yes |  yes  |                                 |
| dave    | miner only      |      no (standby) |  yes  | locks at runtime in scenario 01 |
| eve     | full node       |                no |  no   | observer                        |

Ports are dynamically allocated by zombienet; `js-script` blocks read the WebSocket endpoint from `networkInfo.nodesByName[<name>].wsUri`.

## Runtime parameters (`test-runtime` feature)

The node binary used by this harness MUST be compiled with `--features test-runtime`. Otherwise session/lock/cooldown/difficulty timing cannot be observed within scenario timeouts.

With `TargetBlockTime = 20s` (so `MINUTES = 3` blocks) the derived constants are:

| Constant                         | test-runtime |
| -------------------------------- | -----------: |
| `SessionPeriod`                  |       3 mins |
| `LockAmount`                     |       1 UNIT |
| `LockDuration`                   |      30 mins |
| `RenewInterval`                  |      25 mins |
| `RejoinCooldownPeriod`           |       5 mins |
| `OfflineThreshold`               |            1 |
| `MaxValidators`                  |            4 |
| `DifficultyHalflife`             |          60s |
| `DifficultyBreakThresholdSecs`   |        1800s |

## Prerequisites

```bash
# Node.js (>= 18; tested with v24) and npm
apt install curl unzip
curl -o- https://fnm.vercel.app/install | bash
fnm install 24
node -v # Should print "v24.15.0" or newer.
npm -v # Should print "11.12.1" or newer.

# zombienet binary (linux x86_64 example, tested with v1.3.138)
sudo curl -L -o /usr/local/bin/zombienet \
  https://github.com/paritytech/zombienet/releases/download/v1.3.138/zombienet-linux-x64
sudo chmod +x /usr/local/bin/zombienet
zombienet version
```

## Quick start

```bash
cd tests/integration

# 1. install JS deps (used by js-script blocks)
npm install

# 2. build node with test-runtime AND pre-generate the raw chainspec.
#    Re-run after every change to runtime/ or the integration preset.
bash scripts/build-node.sh                # release; PROFILE=debug for debug

# 3. run all scenarios (auto-creates /tmp/zn-creds.cfg if missing)
bash scripts/run-all.sh
# or individually (note `-p native` is implied by zombienet.toml):
zombienet -p native test scenarios/00-basic-and-finality.zndsl
zombienet -p native test scenarios/01-validator-lifecycle.zndsl
zombienet -p native test scenarios/02-validator-mass-offline.zndsl
zombienet -p native test scenarios/03-hashrate-fluctuation.zndsl
zombienet -p native test scenarios/04-network-partition.zndsl
zombienet -p native test scenarios/05-validator-offline-kick.zndsl
zombienet -p native test scenarios/06-max-validators-capacity.zndsl
zombienet -p native test scenarios/07-equivocation-placeholder.zndsl
zombienet -p native test scenarios/08-evm-tooling.zndsl

# 4. (optional) spawn the network without a test for manual exploration
zombienet -p native spawn zombienet.toml
```

> Each scenario file declares `Creds: /tmp/zn-creds.cfg`. The native
> provider does not consume the file but zombienet still requires the
> header. `scripts/run-all.sh` creates an empty placeholder automatically;
> if you invoke `zombienet test` directly, run `touch /tmp/zn-creds.cfg`
> first.

## Scenario coverage

| Scenario file             | Verifies                                                                                                                                                                                                                                                                                                      |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 00-basic-and-finality     | All 5 nodes start and peer up; PoW chain advances and converges to one canonical hash on every node; GRANDPA finalized number progresses; the canonical block at alice's finalized height is identical on every node (fork-choice respects finality).                                                         |
| 01-validator-lifecycle    | Dave (initially not a validator) submits `validator.lock()`; after one session rotation Dave appears in `session.validators()` on every node and the GRANDPA authority set grows. Dave then submits `validator.request_exit()`; he is removed from every node's validator set and finality keeps progressing. |
| 02-validator-mass-offline | Taking 1/3 of validators offline (via real process restart) does not stall finality. Pausing 2/3 stalls finality but PoW block production continues. Resuming all validators restores finality.                                                                                                               |
| 03-hashrate-fluctuation   | After a long warm-up that lets ASERT climb to a difficulty that throttles the combined 4-miner hashrate near the 20s target, pausing 3 of 4 miners forces alice solo, ASERT lowers `pallet_difficulty.currentDifficulty`, alice keeps producing blocks, and resuming the miners restores finality.            |
| 04-network-partition      | Isolating bob from the rest (via real process restart) and then healing the split causes the minority side to reorg to the canonical chain, no conflicting finalized blocks exist, all nodes agree on the canonical hash, and finality continues progressing.                                                 |
| 05-validator-offline-kick | Pausing bob causes ImOnline to detect the missing heartbeat and pallet-validator to kick him after the threshold. After he is removed from `session.validators` and his process is resumed, GRANDPA finality continues on the reduced set.                                                                    |
| 06-max-validators-capacity| Capacity check: with `MaxValidators = 4` and 4 already-active validators, an additional `validator.lock()` from eve is rejected at the candidate-promotion stage and the active set stays at 4.                                                                                                               |
| 07-equivocation-placeholder | Placeholder pending a proper GRANDPA equivocation-injection harness.                                                                                                                                                                                                                                       |
| 08-evm-tooling            | Connects ethers v6 to alice's Frontier-compatible RPC, asserts `eth_chainId == 32026`, signs and broadcasts a contract-creation tx with the pre-funded Alith dev key, then verifies the deployed runtime bytecode matches `0x00` (single STOP).                                                              |

## Known limitations

* zombienet `<node>: pause` (SIGSTOP) freezes the process while keeping its TCP sockets open, so GRANDPA peers wait for the paused node's votes until round timeouts elapse and finality stalls (~10 min/block). Whenever a sub-scenario asserts on finality during the outage, it must use `<node>: restart after N seconds` (a real SIGTERM + re-spawn) instead. `pause` is still acceptable when the offline window only verifies non-finality properties (e.g. PoW continues, ImOnline kicks).
* Scenario 03 needs a long warm-up (~5 minutes / 60 blocks) so the ASERT controller actually throttles the combined 4-miner hashrate near the target before miners are paused. Without this warm-up, alice solo can still meet the target on her own and ASERT keeps raising difficulty.
* zombienet's bundled `@polkadot/util` (13.x) emits a "multiple versions installed" warning on stderr because our `tests/integration/node_modules` ships 14.x. The warning is harmless and originates inside the zombienet binary itself.
* The equivocation scenario is still a placeholder pending an equivocation-injection harness.
* Long-soak runs are not automated; wrap `bash scripts/run-all.sh` in a loop or extend a scenario with longer timeouts as needed.
* Scenarios 01, 02, 05 can be timing-sensitive: they assume PoW blocks land within ~20s and that ImOnline does not kick a validator within the test window. On slow hardware or under heavy load the difficulty / heartbeat dynamics can stretch the validator-rotation steps past the assertion deadlines. If a run flakes, retry; if it flakes consistently, raise the per-step `within ... seconds` budget in the corresponding `.zndsl` file.

## Cleanup

zombienet writes node data under its own tmp directory; spawned processes terminate when the test exits. If a run is interrupted, force-clean with `pkill -f solochain-template-node`.
