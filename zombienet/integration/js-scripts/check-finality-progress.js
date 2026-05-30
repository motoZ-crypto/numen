// Assert that finality has advanced by at least N blocks within an internal
// time window, AND that all nodes agree on the finalized block hash at the
// minimum common finalized height.
//   args[0] = N (default 4)
//   args[1] = window in seconds (default 300)
const { tryConnectAll, disconnectAll, finalizedNumber, waitForFinalizedAdvance } = require("./lib");

const NODES = ["alice", "bob", "charlie", "dave", "eve"];

async function run(_zombie, networkInfo, args) {
    const advance = args && args[0] ? Number(args[0]) : 4;
    const windowMs = (args && args[1] ? Number(args[1]) : 300) * 1000;
    // Use tolerant connect so paused nodes (mass-offline scenario) don't hang.
    const pairs = await tryConnectAll(networkInfo, NODES, 5_000);
    const live = pairs.filter(([_, api]) => api !== null);
    const apis = live.map(([_, api]) => api);
    if (apis.length === 0) {
        console.error("  no reachable nodes");
        return 0;
    }
    try {
        await waitForFinalizedAdvance(apis[0], advance, windowMs);
        const fins = await Promise.all(apis.map(finalizedNumber));
        for (let i = 0; i < live.length; i++) console.log(`  ${live[i][0]} finalized=${fins[i]}`);
        const common = Math.min(...fins);
        const hashes = await Promise.all(
            apis.map(async (api) => (await api.rpc.chain.getBlockHash(common)).toHex())
        );
        const ref = hashes[0];
        for (let i = 0; i < live.length; i++) {
            if (hashes[i] !== ref) {
                console.error(`  finalized hash mismatch at #${common}: ${live[0][0]}=${ref} ${live[i][0]}=${hashes[i]}`);
                return 0;
            }
        }
        console.log(`  all ${live.length} reachable nodes finalized same hash at #${common}`);
        return 1;
    } catch (e) {
        console.error(`  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll(apis);
    }
}

module.exports = { run };
