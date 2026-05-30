// Submit `validator.request_exit()` from Dave. Wait for Dave to disappear
// from the active set on every node and assert finality keeps progressing.
const {
    connectAll, disconnectAll, pair, sendAndWait, sessionValidators,
    finalizedNumber, sleep,
} = require("./lib");

const NODES = ["alice", "bob", "charlie", "dave", "eve"];

async function run(_zombie, networkInfo) {
    const apis = await connectAll(networkInfo, NODES);
    const aliceApi = apis[0];
    const daveApi = apis[3];
    try {
        const dave = await pair("//Dave");
        const set = await sessionValidators(daveApi);
        if (!set.includes(dave.address)) {
            console.error(`  Dave not currently a validator; run lock-and-join first`);
            return 0;
        }
        const startFin = await finalizedNumber(aliceApi);

        await sendAndWait(daveApi, dave, daveApi.tx.validator.requestExit());
        console.log(`  validator.request_exit() included`);

        const deadline = Date.now() + 600_000;
        let removed = false;
        while (Date.now() < deadline) {
            const out = await Promise.all(apis.map(async (a) => !(await sessionValidators(a)).includes(dave.address)));
            if (out.every(Boolean)) { removed = true; break; }
            await sleep(3000);
        }
        if (!removed) {
            console.error(`  Dave still in some node's session.validators after timeout`);
            return 0;
        }
        const endFin = await finalizedNumber(aliceApi);
        if (endFin <= startFin) {
            console.error(`  finality stalled across exit: ${startFin} -> ${endFin}`);
            return 0;
        }
        console.log(`  removed; finality continued ${startFin} -> ${endFin}`);
        return 1;
    } catch (e) {
        console.error(`  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll(apis);
    }
}

module.exports = { run };
