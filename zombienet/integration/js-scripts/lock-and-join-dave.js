// Submit `validator.lock()` from Dave on the dave node. Then wait until
// Dave appears in `session.validators()` on every node and in
// `grandpa.authorities()` on alice. Returns 1 on success.
//
// Dave's session keys are pre-registered at genesis by the `integration`
// preset (see `runtime/src/genesis_config_presets.rs`), so no
// `session.set_keys()` call is required here.
const {
    connectAll, disconnectAll, pair, sendAndWait, sessionValidators, sleep,
} = require("./lib");

const NODES = ["alice", "bob", "charlie", "dave", "eve"];

async function run(_zombie, networkInfo) {
    const apis = await connectAll(networkInfo, NODES);
    const daveApi = apis[3];
    try {
        const dave = await pair("//Dave");
        const startSet = await sessionValidators(daveApi);
        if (startSet.includes(dave.address)) {
            console.log(`  Dave (${dave.address}) already a validator; nothing to do`);
            return 1;
        }
        const baselineGrandpa = (await daveApi.query.grandpa.authorities()).length;
        console.log(`  pre: validators=${startSet.length} grandpa=${baselineGrandpa}`);

        await sendAndWait(daveApi, dave, daveApi.tx.validator.lock());
        console.log(`  validator.lock() included`);

        const deadline = Date.now() + 600_000;
        let joined = false;
        while (Date.now() < deadline) {
            const ok = await Promise.all(apis.map(async (a) => (await sessionValidators(a)).includes(dave.address)));
            if (ok.every(Boolean)) { joined = true; break; }
            await sleep(3000);
        }
        if (!joined) {
            console.error(`  Dave never appeared in session.validators on every node`);
            return 0;
        }
        const finalGrandpa = (await daveApi.query.grandpa.authorities()).length;
        console.log(`  post: validators=${(await sessionValidators(daveApi)).length} grandpa=${finalGrandpa}`);
        if (finalGrandpa <= baselineGrandpa) {
            console.error(`  grandpa authorities did not grow (${baselineGrandpa} -> ${finalGrandpa})`);
            return 0;
        }
        return 1;
    } catch (e) {
        console.error(`  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll(apis);
    }
}

module.exports = { run };
