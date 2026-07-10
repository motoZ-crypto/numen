// Native coin transfer using ethers v6.
//
//   node transfer-ethers.js
//
// Defaults: Alith → Baltathar, 1.5 NUMN, RPC at 127.0.0.1:9944.

const { JsonRpcProvider, Wallet, formatEther, parseEther } = require("ethers");

const RPC_URL = process.env.CRYPTO_NODE_RPC || "http://127.0.0.1:9944";
const ALITH_PK =
  process.env.ALITH_PK ||
  "0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133";
const BALTATHAR = "0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0";
const AMOUNT = parseEther(process.env.AMOUNT || "1.5");
const EXPECTED_CHAIN_ID = 320262n;

async function main() {
  const provider = new JsonRpcProvider(RPC_URL);
  const network = await provider.getNetwork();
  console.log(`chainId: ${network.chainId}`);
  if (network.chainId !== EXPECTED_CHAIN_ID) {
    console.error(`unexpected chainId ${network.chainId}, want ${EXPECTED_CHAIN_ID}`);
    process.exit(1);
  }

  const wallet = new Wallet(ALITH_PK, provider);
  console.log(`from:    ${wallet.address}`);
  console.log(`to:      ${BALTATHAR}`);
  console.log(`amount:  ${formatEther(AMOUNT)} NUMN`);

  const beforeFrom = await provider.getBalance(wallet.address);
  const beforeTo = await provider.getBalance(BALTATHAR);
  console.log(`before:  from=${formatEther(beforeFrom)} to=${formatEther(beforeTo)}`);

  const tx = await wallet.sendTransaction({ to: BALTATHAR, value: AMOUNT });
  console.log(`tx hash: ${tx.hash}`);
  const receipt = await tx.wait();
  console.log(`status:  ${receipt.status === 1 ? "ok" : "failed"} (block ${receipt.blockNumber})`);
  if (receipt.status !== 1) {
    console.error("transaction failed");
    process.exit(1);
  }

  const afterFrom = await provider.getBalance(wallet.address);
  const afterTo = await provider.getBalance(BALTATHAR);
  console.log(`after:   from=${formatEther(afterFrom)} to=${formatEther(afterTo)}`);

  if (afterTo - beforeTo !== AMOUNT) {
    console.error("recipient delta does not match transfer amount");
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
