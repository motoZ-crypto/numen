// "<m1,m2,...>,<bestH>,<finH>"

const { connect, watchHeads, POW_ENGINE_ID, getUri, waitBlockAt } = require("../../js-scripts/lib");
const { Keyring } = require("@polkadot/keyring");
const { decodeAddress, cryptoWaitReady } = require("@polkadot/util-crypto");
const { u8aToHex } = require("@polkadot/util");

async function connectAll(networkInfo, nodeNames) {
    const apis = {};
    for (const name of nodeNames) apis[name] = await connect(networkInfo, name);
    return apis;
}

async function assertAllFinalized(apis, finH) {
    for (const [name, api] of Object.entries(apis)) {
        const finHash = await api.rpc.chain.getFinalizedHead();
        const finHeader = await api.rpc.chain.getHeader(finHash);
        const finNum = finHeader.number.toNumber();
        console.log("📜", `  ${name.padEnd(8)} finalized = #${finNum}`);
        if (finNum < finH) {
            throw new Error(`${name}: finalized #${finNum} < required #${finH}`);
        }
    }
}

async function assertFinalizedConsistency(apis, finH) {
    const nodeNames = Object.keys(apis);
    for (let h = 1; h <= finH; h += 1) {
        let reference = null;
        let referenceFrom = null;
        for (const name of nodeNames) {
            const hash = (await apis[name].rpc.chain.getBlockHash(h)).toHex();
            if (reference === null) {
                reference = hash;
                referenceFrom = name;
            } else if (hash !== reference) {
                throw new Error(`height #${h}: ${name}=${hash} != ${referenceFrom}=${reference}`);
            }
        }
        console.log("📜", `  #${h} hash = ${reference}`);
    }
}

function buildWatch(names, keyring) {
    const watch = {};
    for (const name of names) {
        const kp = keyring.addFromUri(getUri(name));
        watch[name] = {
            address: kp.address,
            addressHex: u8aToHex(decodeAddress(kp.address)).toLowerCase(),
            blocks: 0,
        };
    }
    const byHex = Object.fromEntries(
        Object.entries(watch).map(([n, v]) => [v.addressHex, n])
    );
    return { watch, byHex };
}

async function snapshotBalances(alice, watch, bestH) {
    const hash0 = await alice.rpc.chain.getBlockHash(0);
    const hashN = await alice.rpc.chain.getBlockHash(bestH);
    const apiAt0 = await alice.at(hash0);
    const apiAtN = await alice.at(hashN);
    for (const [name, v] of Object.entries(watch)) {
        v.init = (await apiAt0.query.system.account(v.address)).data.free.toBigInt();
        v.final = (await apiAtN.query.system.account(v.address)).data.free.toBigInt();
        console.log("📜", `  ${name.padEnd(8)} init@#0=${v.init} final@#${bestH}=${v.final}`);
    }
}

async function countAuthoredBlocks(alice, watch, byHex, bestH) {
    let watched = 0;
    let unwatched = 0;
    for (let h = 1; h <= bestH; h += 1) {
        const hash = await alice.rpc.chain.getBlockHash(h);
        const header = await alice.rpc.chain.getHeader(hash);
        let authorHex = null;
        for (const log of header.digest.logs) {
            if (log.isPreRuntime) {
                const [engine, data] = log.asPreRuntime;
                if (engine.toHex() === POW_ENGINE_ID) {
                    authorHex = u8aToHex(data.slice(0, 32)).toLowerCase();
                    break;
                }
            }
        }
        const who = authorHex ? byHex[authorHex] : null;
        if (who) {
            watch[who].blocks += 1;
            watched += 1;
            console.log("📜", `  #${h} -> ${who}`);
        } else {
            unwatched += 1;
            console.log("📜", `  #${h} -> (unwatched ${authorHex || "no-digest"})`);
        }
    }

    console.log("📜", `  authored #1..#${bestH}: watched=${watched}, unwatched=${unwatched}`);
}

function assertRewards(watch, reward) {
    for (const [name, v] of Object.entries(watch)) {
        const delta = v.final - v.init;
        const expected = BigInt(v.blocks) * reward;
        const tag = delta === expected ? "ok" : "MISMATCH";
        console.log("📜", `  ${name.padEnd(8)} blocks=${v.blocks} delta=${delta} expected=${expected}  [${tag}]`);
        if (delta !== expected) throw new Error(`miner reward reconciliation failed`);
    }
}

async function run(_zombie, networkInfo, args) {
    if (!args || args.length < 3) {
        console.error("📜", `  usage: with "<m1>,<m2>,...,<bestH>,<finH>"; got args=${JSON.stringify(args)}`);
        return 0;
    }
    const bestH = Number(args[args.length - 2]);
    const finH = Number(args[args.length - 1]);
    const names = args.slice(0, args.length - 2).map((s) => String(s).trim()).filter(Boolean);
    if (!Number.isFinite(bestH) || bestH < 1 || !Number.isFinite(finH) || finH < 1 || finH > bestH || names.length === 0) {
        console.error("📜", `  bad args: miners=${JSON.stringify(names)} bestH=${bestH} finH=${finH}`);
        return 0;
    }

    await cryptoWaitReady();
    const keyring = new Keyring({ type: "sr25519" });

    const nodeNames = Object.keys(networkInfo.nodesByName);
    console.log("📜", `  nodes = ${nodeNames.join(",")}`);
    console.log("📜", `  watched miners = ${names.join(",")}, bestH = #${bestH}, finH = #${finH}`);

    const apis = await connectAll(networkInfo, nodeNames);
    try {
        const alice = apis.alice || apis[nodeNames[0]];

        await waitBlockAt(alice, bestH);
        await assertAllFinalized(apis, finH);
        await assertFinalizedConsistency(apis, finH);

        // Reward halves every HalvingInterval blocks; integration runs stay far
        // below the first boundary, so every block pays the initial reward.
        const reward = alice.consts.blockReward.initialReward.toBigInt();
        console.log("📜", `  reward/block = ${reward}`);

        const { watch, byHex } = buildWatch(names, keyring);
        await snapshotBalances(alice, watch, bestH);
        await countAuthoredBlocks(alice, watch, byHex, bestH);
        assertRewards(watch, reward);

        return 1;
    } catch (e) {
        console.error("📜", `  ${e.message}`);
        return 0;
    } finally {
        for (const a of Object.values(apis)) await a.disconnect();
    }
}

module.exports = { run };
