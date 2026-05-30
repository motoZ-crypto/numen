// Usage: "<exiter>,<joiner>,<maxBlocks>"

const {
    connect, disconnectAll, getUri, pair, submitExtrinsic,
    sessionContainsValidator, waitForSessionRotations, watchHeads,
    finalizedNumber, addressHex, powAuthorHexFromHeader,
} = require("../../js-scripts/lib");

// Watch new heads up to `maxBlocks`, return 1 once `authorHex` has authored
// a PoW block AND that block has reached finalized. Throws on timeout.
async function waitAuthoredAndFinalized(api, authorHex, maxBlocks, label) {
    let authoredAt = null;
    return watchHeads(api, async (header) => {
        const num = header.number.toNumber();
        if (authoredAt === null) {
            const a = powAuthorHexFromHeader(header);
            if (a === authorHex.toLowerCase()) {
                authoredAt = num;
                console.log("📜", `  ${label} authored #${num}`);
            }
        }
        const fin = await finalizedNumber(api);
        if (num % 5 === 0) console.log("📜", `  best=#${num} fin=#${fin}`);
        if (authoredAt !== null && fin >= authoredAt) {
            console.log("📜", `  ${label}: finalized #${fin} >= authored #${authoredAt}, ok`);
            return 1;
        }
    }, maxBlocks);
}

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

        // 1. Wait one session for steady state.
        console.log("📜", `  waiting one session`);
        await waitForSessionRotations(exiterApi, 1);

        // === Phase 1: exiter leaves =========================================
        await submitExtrinsic(exiterApi, exiter, exiterApi.tx.validator.requestExit());
        console.log("📜", `  ${EXITER}: validator.request_exit() included`);

        console.log("📜", `  waiting two sessions`);
        await waitForSessionRotations(exiterApi, 2);

        if (await sessionContainsValidator(exiterApi, exiter.address)) {
            console.error("📜", `  ${EXITER} still in session.validators()`);
            return 0;
        }
        console.log("📜", `  session.validators(): -${EXITER} ok`);

        const phase1 = await waitAuthoredAndFinalized(exiterApi, exiterHex, MAX_BLOCKS, EXITER);
        if (phase1 !== 1) return 0;

        // === Phase 2: joiner enters =========================================
        const generated = await joinerApi.rpc.author.rotateKeysWithOwner(joinerHex);
        await submitExtrinsic(joinerApi, joiner, joinerApi.tx.session.setKeys(generated.get("keys"), generated.get("proof")));
        console.log("📜", `  ${JOINER}: session.set_keys() included`);
        await submitExtrinsic(joinerApi, joiner, joinerApi.tx.validator.lock());
        console.log("📜", `  ${JOINER}: validator.lock() included`);

        console.log("📜", `  waiting two sessions`);
        await waitForSessionRotations(joinerApi, 2);

        if (!(await sessionContainsValidator(joinerApi, joiner.address))) {
            console.error("📜", `  ${JOINER} not in session.validators()`);
            return 0;
        }
        console.log("📜", `  session.validators(): +${JOINER} ok`);

        const phase2 = await waitAuthoredAndFinalized(joinerApi, joinerHex, MAX_BLOCKS, JOINER);
        return phase2 === 1 ? 1 : 0;

    } catch (e) {
        console.error("📜", `  ${e.stack || e.message}`);
        return 0;
    } finally {
        await disconnectAll([exiterApi, joinerApi]);
    }
}

module.exports = { run };
