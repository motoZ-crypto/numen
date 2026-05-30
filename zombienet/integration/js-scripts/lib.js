// Shared CommonJS helpers for zombienet js-script blocks.
//
// Every js-script in this directory receives `(zombie, networkInfo, args)`
// from zombienet, where `networkInfo.nodesByName[<name>].wsUri` is the
// dynamically allocated WebSocket endpoint. These helpers wrap that pattern
// and expose a small set of common operations against any node.

const { ApiPromise, WsProvider } = require("@polkadot/api");
const { Keyring } = require("@polkadot/keyring");
const { cryptoWaitReady } = require("@polkadot/util-crypto");

async function connect(networkInfo, nodeName) {
    const info = networkInfo.nodesByName[nodeName];
    if (!info) throw new Error(`unknown node: ${nodeName}`);
    const provider = new WsProvider(info.wsUri);
    const api = await ApiPromise.create({ provider, throwOnConnect: true, noInitWarn: true });
    return api;
}

// Connect with a hard timeout; returns null if the node is unreachable
// (e.g. paused via SIGSTOP in the validator-mass-offline scenario).
async function tryConnect(networkInfo, nodeName, timeoutMs = 5000) {
    return await Promise.race([
        connect(networkInfo, nodeName).catch(() => null),
        new Promise((r) => setTimeout(() => r(null), timeoutMs)),
    ]);
}

async function connectAll(networkInfo, names) {
    return Promise.all(names.map((n) => connect(networkInfo, n)));
}

// Like `connectAll`, but tolerant of nodes that are paused / unreachable.
// Returns a `[name, api|null]` map preserving the original order.
async function tryConnectAll(networkInfo, names, timeoutMs = 5000) {
    return Promise.all(
        names.map(async (n) => [n, await tryConnect(networkInfo, n, timeoutMs)])
    );
}

async function disconnectAll(apis) {
    for (const api of apis) {
        try { await api.disconnect(); } catch (_) {}
    }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function bestNumber(api) {
    const h = await api.rpc.chain.getHeader();
    return h.number.toNumber();
}

async function finalizedNumber(api) {
    const hash = await api.rpc.chain.getFinalizedHead();
    const h = await api.rpc.chain.getHeader(hash);
    return h.number.toNumber();
}

async function sessionIndex(api) {
    return (await api.query.session.currentIndex()).toNumber();
}

async function sessionValidators(api) {
    const set = await api.query.session.validators();
    return set.map((id) => id.toString());
}

async function waitForFinalizedAdvance(api, by, maxMs) {
    const start = await finalizedNumber(api);
    const deadline = Date.now() + maxMs;
    while (Date.now() < deadline) {
        const cur = await finalizedNumber(api);
        if (cur >= start + by) return cur;
        await sleep(2000);
    }
    throw new Error(`finalized only advanced ${(await finalizedNumber(api)) - start}/${by} in ${maxMs}ms`);
}

async function waitForSessionRotations(api, n, maxMs) {
    const start = await sessionIndex(api);
    const deadline = Date.now() + maxMs;
    while (Date.now() < deadline) {
        const cur = await sessionIndex(api);
        if (cur >= start + n) return cur;
        await sleep(3000);
    }
    throw new Error(`session only rotated ${(await sessionIndex(api)) - start}/${n} in ${maxMs}ms`);
}

let _keyring;
async function keyring() {
    if (_keyring) return _keyring;
    await cryptoWaitReady();
    _keyring = new Keyring({ type: "sr25519", ss58Format: 42 });
    return _keyring;
}

async function pair(uri) {
    const k = await keyring();
    return k.addFromUri(uri);
}

function sendAndWait(api, signer, extrinsic) {
    return new Promise((resolve, reject) => {
        extrinsic
            .signAndSend(signer, ({ status, dispatchError, events }) => {
                if (dispatchError) {
                    let msg = dispatchError.toString();
                    if (dispatchError.isModule) {
                        const decoded = api.registry.findMetaError(dispatchError.asModule);
                        msg = `${decoded.section}.${decoded.name}: ${decoded.docs.join(" ")}`;
                    }
                    return reject(new Error(`extrinsic failed: ${msg}`));
                }
                if (status.isInBlock || status.isFinalized) resolve({ status, events });
            })
            .catch(reject);
    });
}

module.exports = {
    connect, connectAll, tryConnect, tryConnectAll, disconnectAll,
    sleep,
    bestNumber, finalizedNumber, sessionIndex, sessionValidators,
    waitForFinalizedAdvance, waitForSessionRotations,
    keyring, pair, sendAndWait,
};
