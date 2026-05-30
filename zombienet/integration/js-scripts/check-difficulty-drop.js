// Sample `pallet_difficulty.currentDifficulty` over a configurable window.
// Assert the minimum observed difficulty drops below the initial value AND
// that the alice best block keeps advancing. Run AFTER one or more miners
// have been paused so the network's effective hashrate falls.
//   args[0] = window in seconds (default 300)
const { connectAll, disconnectAll, bestNumber, sleep } = require("./lib");

async function run(_zombie, networkInfo, args) {
    const windowMs = (args && args[0] ? Number(args[0]) : 300) * 1000;
    const apis = await connectAll(networkInfo, ["alice"]);
    const aliceApi = apis[0];
    try {
        const startBest = await bestNumber(aliceApi);
        const startDiff = (await aliceApi.query.difficulty.currentDifficulty()).toBigInt();
        console.log(`  start: best=${startBest} difficulty=${startDiff}`);

        const deadline = Date.now() + windowMs;
        let minDiff = startDiff;
        let lastBest = startBest;
        let progressed = false;
        while (Date.now() < deadline) {
            await sleep(10_000);
            const b = await bestNumber(aliceApi);
            const d = (await aliceApi.query.difficulty.currentDifficulty()).toBigInt();
            if (d < minDiff) minDiff = d;
            if (b > lastBest) progressed = true;
            lastBest = b;
            console.log(`  obs: best=${b} difficulty=${d}`);
            if (minDiff < startDiff && b > startBest + 2) break;
        }
        if (!progressed) {
            console.error(`  block production stalled at #${lastBest}`);
            return 0;
        }
        if (minDiff >= startDiff) {
            console.error(`  difficulty did not drop (start=${startDiff} min=${minDiff})`);
            return 0;
        }
        console.log(`  difficulty dropped from ${startDiff} to ${minDiff}; best advanced ${startBest} -> ${lastBest}`);
        return 1;
    } catch (e) {
        console.error(`  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll(apis);
    }
}

module.exports = { run };
