// Usage: "<exiter>,<joiner>,<maxBlocks>"

const {
    connect, disconnectAll, getUri, pair, submitExtrinsic,
    sessionContainsValidator, waitForSessionRotations, watchHeads,
    finalizedNumber,
    addressHex, powAuthorHexFromHeader,
} = require("../../js-scripts/lib");

async function run(_zombie, networkInfo, args) {
    if (!args || args.length < 3) {
        console.error("📜", `  usage: with "<exiter>,<joiner>,<maxBlocks>"; got args=${JSON.stringify(args)}`);
        return 0;
    }
    const EXITER = String(args[0]).trim();
    const JOINER = String(args[1]).trim();
    const MAX_BLOCKS = Number(args[2]);
    if (!Number.isFinite(MAX_BLOCKS) || MAX_BLOCKS < 1) {
        console.error("📜", `  bad args: exiter=${EXITER} joiner=${JOINER} maxBlocks=${MAX_BLOCKS}`);
        return 0;
    }

    const exiterApi = await connect(networkInfo, EXITER);
    const joinerApi = await connect(networkInfo, JOINER);
    try {
        const exiter = await pair(getUri(EXITER));
        const joiner = await pair(getUri(JOINER));
        const exiterHex = addressHex(exiter.address);
        const joinerHex = addressHex(joiner.address);

        console.log("📜", `  exiter ${EXITER} = ${exiter.address}`);
        console.log("📜", `  joiner ${JOINER} = ${joiner.address}`);

        // 1. Wait one session for the chain to be in steady state.
        console.log("📜", `  waiting one session`);
        await waitForSessionRotations(exiterApi, 1);

        // 2. exiter: request_exit(); joiner: rotateKeys + setKeys + lock().
        await submitExtrinsic(exiterApi, exiter, exiterApi.tx.validator.requestExit());
        console.log("📜", `  ${EXITER}: validator.request_exit() included`);

        const generated = await joinerApi.rpc.author.rotateKeysWithOwner(joinerHex);
        await submitExtrinsic(joinerApi, joiner, joinerApi.tx.session.setKeys(generated.get("keys"), generated.get("proof")));
        console.log("📜", `  ${JOINER}: session.set_keys() included`);
        await submitExtrinsic(joinerApi, joiner, joinerApi.tx.validator.lock());
        console.log("📜", `  ${JOINER}: validator.lock() included`);

        // 3. Wait 2 sessions for the rotation to take effect.
        console.log("📜", `  waiting two sessions`);
        await waitForSessionRotations(exiterApi, 2);

        // 4. exiter must be gone, joiner must be present in session.validators().
        if (await sessionContainsValidator(exiterApi, exiter.address)) {
            console.error("📜", `  ${EXITER} still in session.validators()`);
            return 0;
        }
        if (!(await sessionContainsValidator(exiterApi, joiner.address))) {
            console.error("📜", `  ${JOINER} not in session.validators()`);
            return 0;
        }
        console.log("📜", `  session.validators(): -${EXITER} +${JOINER} ok`);

        // 5. Subscribe new heads; both nodes must author at least one block
        //    within MAX_BLOCKS, and each such block must reach finalized.
        const authoredAt = { [EXITER]: null, [JOINER]: null };
        const finalizedOk = { [EXITER]: false, [JOINER]: false };
        return await watchHeads(exiterApi, async (header) => {
            const num = header.number.toNumber();
            const authorHex = powAuthorHexFromHeader(header);
            if (authoredAt[EXITER] === null && authorHex === exiterHex) {
                authoredAt[EXITER] = num;
                console.log("📜", `  ${EXITER} authored #${num}`);
            }
            if (authoredAt[JOINER] === null && authorHex === joinerHex) {
                authoredAt[JOINER] = num;
                console.log("📜", `  ${JOINER} authored #${num}`);
            }
            const fin = await finalizedNumber(exiterApi);
            if (num % 5 === 0) console.log("📜", `  best=#${num} fin=#${fin}`);
            for (const name of [EXITER, JOINER]) {
                if (!finalizedOk[name] && authoredAt[name] !== null && fin >= authoredAt[name]) {
                    finalizedOk[name] = true;
                    console.log("📜", `  ${name}: finalized #${fin} >= authored #${authoredAt[name]}, ok`);
                }
            }
            if (finalizedOk[EXITER] && finalizedOk[JOINER]) return 1;
        }, MAX_BLOCKS);

    } catch (e) {
        console.error("📜", `  ${e.stack || e.message}`);
        return 0;
    } finally {
        await disconnectAll([exiterApi, joinerApi]);
    }
}

module.exports = { run };
