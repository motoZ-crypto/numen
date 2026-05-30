// Assert "fork choice respects finality" invariant on the live network:
// for every node, the canonical block at alice.finalized must equal
// alice.finalizedHash. A divergence implies a node has accepted a chain
// branch that conflicts with finality.
const { connectAll, disconnectAll, finalizedNumber, bestNumber } = require("./lib");

const NODES = ["alice", "bob", "charlie", "dave", "eve"];

async function run(_zombie, networkInfo) {
    const apis = await connectAll(networkInfo, NODES);
    try {
        const finNum = await finalizedNumber(apis[0]);
        const finHash = (await apis[0].rpc.chain.getBlockHash(finNum)).toHex();
        console.log(`  alice finalized #${finNum} = ${finHash}`);

        let ok = true;
        for (let i = 0; i < NODES.length; i++) {
            const best = await bestNumber(apis[i]);
            if (best < finNum) {
                console.error(`  ${NODES[i]} best=${best} below finalized ${finNum}`);
                ok = false;
                continue;
            }
            const h = (await apis[i].rpc.chain.getBlockHash(finNum)).toHex();
            if (h !== finHash) {
                console.error(`  ${NODES[i]} canonical hash at #${finNum} != alice's finalized`);
                ok = false;
            } else {
                console.log(`  ${NODES[i]} best=${best} contains finalized ${finNum}`);
            }
        }
        return ok ? 1 : 0;
    } finally {
        await disconnectAll(apis);
    }
}

module.exports = { run };
