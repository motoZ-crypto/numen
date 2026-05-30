// Assert that all 5 nodes agree on the canonical block hash at the given
// height. The height is supplied via args[0]; if omitted, picks the lowest
// best number across the network. Returns 1 on agreement, 0 on mismatch.
const { connectAll, disconnectAll, bestNumber } = require("./lib");

const NODES = ["alice", "bob", "charlie", "dave", "eve"];

async function run(_zombie, networkInfo, args) {
    const apis = await connectAll(networkInfo, NODES);
    try {
        const heights = await Promise.all(apis.map(bestNumber));
        const target = args && args[0] ? Number(args[0]) : Math.min(...heights);
        if (Number.isNaN(target) || target < 0) {
            console.error(`bad target height arg: ${args && args[0]}`);
            return 0;
        }
        const hashes = await Promise.all(
            apis.map(async (api) => (await api.rpc.chain.getBlockHash(target)).toHex())
        );
        const ref = hashes[0];
        let ok = true;
        for (let i = 0; i < NODES.length; i++) {
            const tag = hashes[i] === ref ? "ok" : "MISMATCH";
            console.log(`  ${NODES[i].padEnd(8)} #${target} = ${hashes[i]}  [${tag}]`);
            if (hashes[i] !== ref) ok = false;
        }
        return ok ? 1 : 0;
    } finally {
        await disconnectAll(apis);
    }
}

module.exports = { run };
