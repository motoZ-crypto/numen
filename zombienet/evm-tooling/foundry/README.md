# Foundry compatibility sample

Smoke-tests `forge` / `cast` against a local crypto-node dev chain over the
Frontier Ethereum-compatible JSON-RPC.

## Prerequisites

- [Foundry](https://book.getfoundry.sh/) installed (`foundryup`)
- A locally built node:

  ```bash
  cargo build --release
  ```

## Run

```bash
# 1. start a single-node dev chain in another terminal
./target/release/solochain-template-node --dev --rpc-cors all --rpc-port 9944

# 2. install forge-std (only on first checkout)
cd tests/evm-tooling/foundry
forge install foundry-rs/forge-std --no-commit

# 3. compile
forge build

# 4. deploy via the Forge script
export ALITH_PRIVATE_KEY=0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133
forge script script/Deploy.s.sol \
  --rpc-url crypto_node \
  --private-key $ALITH_PRIVATE_KEY \
  --broadcast

# 5. ad-hoc deploy with `forge create`
forge create src/Token.sol:Token \
  --rpc-url crypto_node \
  --private-key $ALITH_PRIVATE_KEY \
  --broadcast \
  --constructor-args 1000000000000000000000000   # 1_000_000 * 10^18

# 6. read state with `cast call`
TOKEN=<address printed in step 4 or 5>
cast call $TOKEN "totalSupply()(uint256)" --rpc-url crypto_node
cast call $TOKEN "balanceOf(address)(uint256)" \
  0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac --rpc-url crypto_node

# 7. send a transfer with `cast send`
BALTATHAR=0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0
cast send $TOKEN "transfer(address,uint256)" $BALTATHAR 100000000000000000000 \
  --rpc-url crypto_node --private-key $ALITH_PRIVATE_KEY

# 8. verify
cast call $TOKEN "balanceOf(address)(uint256)" $BALTATHAR --rpc-url crypto_node
```

## Pre-funded accounts

See [`../README.md`](../README.md) for the full address / private-key table.
The dev / local_testnet / integration genesis presets pre-fund six standard
Frontier dev ECDSA accounts (Alith, Baltathar, Charleth, Dorothy, Ethan,
Faith). They MUST NOT be used in any non-development network.
