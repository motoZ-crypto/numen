// EVM end-to-end smoke test exercised via zombienet js-script.
//
// Connects to alice's WebSocket RPC, submits an EIP-1559 contract-creation
// transaction signed by Alith (a Frontier dev account pre-funded by every
// preset), waits for the receipt, and asserts that:
//   * the chain id reported by `eth_chainId` matches the configured 32026,
//   * the contract was deployed at a non-empty address,
//   * `eth_getCode(address)` returns the expected runtime bytecode (a single
//     STOP byte: 0x00).
//
// Returns 1 on success, 0 on failure (zombienet `js-script ... return is 1`).

const { WebSocketProvider, Wallet, hexlify } = require("ethers");

const ALITH_PK =
    "0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133";

// Init bytecode that returns a 1-byte runtime: STOP (0x00).
//
//   60 01           PUSH1 0x01      (size of runtime code)
//   60 0c           PUSH1 0x0c      (offset in code = 12)
//   60 00           PUSH1 0x00      (memory destination)
//   39              CODECOPY
//   60 01           PUSH1 0x01      (return size)
//   60 00           PUSH1 0x00      (return offset)
//   f3              RETURN
//   00              STOP            (runtime code byte 12)
const INIT_CODE = "0x6001600c60003960016000f300";
const EXPECTED_RUNTIME = "0x00";

async function run(_zombie, networkInfo, _args) {
    const info = networkInfo.nodesByName["alice"];
    if (!info) {
        console.error("  alice not in networkInfo");
        return 0;
    }
    const provider = new WebSocketProvider(info.wsUri);
    try {
        const network = await provider.getNetwork();
        if (Number(network.chainId) !== 32026) {
            console.error(`  unexpected chainId ${network.chainId}, want 32026`);
            return 0;
        }
        console.log(`  chainId=${network.chainId}`);

        const wallet = new Wallet(ALITH_PK, provider);
        const balance = await provider.getBalance(wallet.address);
        console.log(`  alith=${wallet.address} balance=${balance}`);
        if (balance === 0n) {
            console.error("  Alith has zero balance — preset did not pre-fund");
            return 0;
        }

        const tx = await wallet.sendTransaction({
            data: INIT_CODE,
            gasLimit: 200_000n,
        });
        console.log(`  deploy tx=${tx.hash}`);
        const receipt = await tx.wait();
        if (!receipt || receipt.status !== 1) {
            console.error(`  deploy failed: status=${receipt && receipt.status}`);
            return 0;
        }
        const addr = receipt.contractAddress;
        if (!addr) {
            console.error("  receipt missing contractAddress");
            return 0;
        }
        console.log(`  deployed at ${addr} (block ${receipt.blockNumber})`);

        const code = await provider.getCode(addr);
        if (hexlify(code).toLowerCase() !== EXPECTED_RUNTIME) {
            console.error(`  unexpected runtime code: got ${code}, want ${EXPECTED_RUNTIME}`);
            return 0;
        }
        console.log(`  runtime code matches ${EXPECTED_RUNTIME}`);
        return 1;
    } catch (e) {
        console.error(`  ${e.stack || e.message}`);
        return 0;
    } finally {
        try {
            await provider.destroy();
        } catch (_) {}
    }
}

module.exports = { run };
