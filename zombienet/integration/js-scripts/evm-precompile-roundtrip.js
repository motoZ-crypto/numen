// EVM <-> Substrate round-trip via the balances-erc20 precompile (0x0802).
//
// 1. Reads Alice's substrate balance via @polkadot/api.
// 2. Submits an EVM tx from Alith calling
//    `withdraw(<Alice-pubkey>, AMOUNT)` on the precompile.
// 3. Asserts that:
//      - the receipt contains a `Withdrawal(address,bytes32,uint256)` log
//        whose fields match the call,
//      - Alice's substrate free balance increased by exactly `AMOUNT`,
//      - Alith's EVM-side balance dropped by at least `AMOUNT` (gas on top).
//
// Returns 1 on success, 0 on failure (zombienet `js-script ... return is 1`).

const {
  WebSocketProvider,
  Wallet,
  Contract,
  keccak256,
  toUtf8Bytes,
  zeroPadValue,
  hexlify,
  getAddress,
  parseEther,
} = require("ethers");
const { ApiPromise, WsProvider } = require("@polkadot/api");
const { Keyring } = require("@polkadot/keyring");
const { cryptoWaitReady } = require("@polkadot/util-crypto");

const ALITH_PK =
  "0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133";
const PRECOMPILE = "0x0000000000000000000000000000000000000802";
const AMOUNT = parseEther("4.2");
const WITHDRAWAL_TOPIC = keccak256(toUtf8Bytes("Withdrawal(address,bytes32,uint256)"));

const ABI = [
  "function balanceOf(address) view returns (uint256)",
  "function withdraw(bytes32,uint256) returns (bool)",
];

async function run(_zombie, networkInfo, _args) {
  const info = networkInfo.nodesByName["alice"];
  if (!info) {
    console.error("  alice not in networkInfo");
    return 0;
  }

  const evmProvider = new WebSocketProvider(info.wsUri);
  const subProvider = new WsProvider(info.wsUri);
  let api;
  try {
    api = await ApiPromise.create({ provider: subProvider });

    await cryptoWaitReady();
    const keyring = new Keyring({ type: "sr25519" });
    const alice = keyring.addFromUri("//Alice");
    const alicePub = hexlify(alice.publicKey);
    console.log(`  alice pubkey=${alicePub}`);

    const wallet = new Wallet(ALITH_PK, evmProvider);
    const erc20 = new Contract(PRECOMPILE, ABI, wallet);

    const beforeAliceSub = (await api.query.system.account(alice.address)).data.free.toBigInt();
    const beforeAlithEvm = await erc20.balanceOf(wallet.address);
    console.log(`  before  alith(EVM)=${beforeAlithEvm} alice(sub)=${beforeAliceSub}`);

    const tx = await erc20.withdraw(alicePub, AMOUNT);
    console.log(`  tx hash=${tx.hash}`);
    const receipt = await tx.wait();
    if (!receipt || receipt.status !== 1) {
      console.error(`  withdraw failed: status=${receipt && receipt.status}`);
      return 0;
    }
    console.log(`  withdraw mined in block ${receipt.blockNumber}`);

    const log = receipt.logs.find(
      (l) =>
        l.address.toLowerCase() === PRECOMPILE.toLowerCase() &&
        l.topics[0] === WITHDRAWAL_TOPIC,
    );
    if (!log) {
      console.error("  Withdrawal log not found");
      return 0;
    }
    const srcTopic = getAddress("0x" + log.topics[1].slice(26));
    const destTopic = log.topics[2].toLowerCase();
    const wad = BigInt(log.data);
    if (srcTopic !== getAddress(wallet.address)) {
      console.error(`  log src ${srcTopic} != ${wallet.address}`);
      return 0;
    }
    if (destTopic !== zeroPadValue(alicePub, 32).toLowerCase()) {
      console.error(`  log dest ${destTopic} != ${alicePub}`);
      return 0;
    }
    if (wad !== AMOUNT) {
      console.error(`  log wad ${wad} != ${AMOUNT}`);
      return 0;
    }

    // Wait briefly for substrate-side state to reflect the EVM tx (same block).
    const afterAliceSub = (await api.query.system.account(alice.address)).data.free.toBigInt();
    const afterAlithEvm = await erc20.balanceOf(wallet.address);
    console.log(`  after   alith(EVM)=${afterAlithEvm} alice(sub)=${afterAliceSub}`);

    const aliceDelta = afterAliceSub - beforeAliceSub;
    if (aliceDelta !== AMOUNT) {
      console.error(`  alice substrate delta ${aliceDelta} != ${AMOUNT}`);
      return 0;
    }
    if (beforeAlithEvm - afterAlithEvm < AMOUNT) {
      console.error(
        `  alith EVM delta ${beforeAlithEvm - afterAlithEvm} < requested ${AMOUNT}`,
      );
      return 0;
    }
    console.log("  round-trip ok");
    return 1;
  } catch (e) {
    console.error(`  ${e.stack || e.message}`);
    return 0;
  } finally {
    try {
      if (api) await api.disconnect();
    } catch (_) {}
    try {
      await evmProvider.destroy();
    } catch (_) {}
  }
}

module.exports = { run };
