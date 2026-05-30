// Read-only RPC smoke check using web3.js v4.
//
//   node query-web3.js

const { Web3 } = require("web3");

const RPC_URL = process.env.CRYPTO_NODE_RPC || "http://127.0.0.1:9944";

async function main() {
  const web3 = new Web3(RPC_URL);

  const [chainId, blockNumber, gasPrice] = await Promise.all([
    web3.eth.getChainId(),
    web3.eth.getBlockNumber(),
    web3.eth.getGasPrice(),
  ]);

  console.log(`chainId:     ${chainId}`);
  console.log(`blockNumber: ${blockNumber}`);
  console.log(`gasPrice:    ${gasPrice}`);

  const accounts = [
    ["Alith    ", "0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac"],
    ["Baltathar", "0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0"],
    ["Charleth ", "0x798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc"],
  ];
  for (const [name, addr] of accounts) {
    const bal = await web3.eth.getBalance(addr);
    console.log(`${name} ${addr}: ${web3.utils.fromWei(bal, "ether")} UNIT`);
  }

  if (Number(chainId) !== 32026) {
    console.error(`unexpected chainId ${chainId}, want 32026`);
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
