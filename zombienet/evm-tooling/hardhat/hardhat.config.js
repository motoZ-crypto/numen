// Hardhat 3 configuration targeting a local crypto-node dev chain.
//
// `cryptoNode` connects to the HTTP JSON-RPC port exposed by:
//
//   ./target/release/solochain-template-node \
//       --dev --rpc-port 9944 --rpc-cors all
//
// Accounts are the publicly documented Frontier dev keys pre-funded by every
// preset in `runtime/src/genesis_config_presets.rs`. They MUST NOT be used in
// any non-development network.

import hardhatToolboxMochaEthersPlugin from "@nomicfoundation/hardhat-toolbox-mocha-ethers";
import { defineConfig } from "hardhat/config";

const ALITH_PK =
  "0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133";
const BALTATHAR_PK =
  "0x8075991ce870b93a8870eca0c0f91913d12f47948ca0fd25b49c6fa7cdbeee8b";
const CHARLETH_PK =
  "0x0b6e18cafb6ed99687ec547bd28139cafdd2bffe70e6b688025de6b445aa5c5b";

export default defineConfig({
  plugins: [hardhatToolboxMochaEthersPlugin],
  solidity: {
    profiles: {
      default: {
        version: "0.8.24",
        settings: {
          optimizer: { enabled: true, runs: 200 },
          evmVersion: "cancun",
        },
      },
    },
  },
  networks: {
    cryptoNode: {
      type: "http",
      chainType: "generic",
      url: process.env.CRYPTO_NODE_RPC ?? "http://127.0.0.1:9944",
      chainId: 320262,
      accounts: [ALITH_PK, BALTATHAR_PK, CHARLETH_PK],
    },
  },
});
