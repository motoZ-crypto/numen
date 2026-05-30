// Wait until a given validator is kicked due to ImOnline-driven offline
// detection. Returns 1 on success, 0 on timeout/mismatch.
//
// Usage: js-script wait-for-offline-kick.js with "<account-uri>"
//   <account-uri> defaults to "//Bob".
//
// Pipeline being verified (test-runtime config):
//   * SessionPeriod ~ 1 min, OfflineThreshold = 1
//   * pause(target) -> target stops sending im_online heartbeats
//   * end of session N: ImOnline emits SomeOffline, our adapter calls
//     note_offline -> OfflineThisSession[target] = ()
//   * end of session N+1: process_offline_counters sees was_offline,
//     count = 1 >= threshold -> kick + RejoinCooldown
//   * end of session N+2: removed from session.validators

const {
    connect, disconnectAll, pair,
    sessionValidators, sessionIndex, sleep,
} = require("./lib");

const POLL_MS = 4000;
const DEFAULT_TARGET = "//Bob";

async function run(_zombie, networkInfo, args) {
    const targetUri = (args && args[0]) || DEFAULT_TARGET;
    const observer = await connect(networkInfo, "alice");
    try {
        const target = await pair(targetUri);
        const startSet = await sessionValidators(observer);
        if (!startSet.includes(target.address)) {
            console.error(`  target ${targetUri} (${target.address}) is not currently a validator`);
            return 0;
        }
        const startSession = await sessionIndex(observer);
        console.log(`  baseline: session=${startSession} validators=${startSet.length} target=${target.address}`);

        // The full pipeline takes ~3 sessions of slack, so we wait up to
        // 8 to absorb PoW jitter. Outer zndsl timeout caps this anyway.
        const deadline = Date.now() + 8 * 60 * 1000;
        let kicked = false;
        let kickedSession = null;
        while (Date.now() < deadline) {
            const cur = await sessionValidators(observer);
            if (!cur.includes(target.address)) {
                kicked = true;
                kickedSession = await sessionIndex(observer);
                break;
            }
            await sleep(POLL_MS);
        }
        if (!kicked) {
            console.error(`  ${target.address} still in session.validators after timeout`);
            return 0;
        }
        console.log(`  ${target.address} removed from session.validators at session=${kickedSession} (Δ=${kickedSession - startSession})`);

        // Confirm the on-chain kick reason matches: status should be Kicked
        // and there should be a RejoinCooldown deadline set.
        const lock = await observer.query.validator.validatorLocks(target.address);
        if (lock.isNone) {
            console.error(`  validatorLocks entry missing for ${target.address}`);
            return 0;
        }
        const status = lock.unwrap().status.toString();
        if (status !== "Kicked") {
            console.error(`  expected status=Kicked, got status=${status}`);
            return 0;
        }
        const cooldown = await observer.query.validator.rejoinCooldown(target.address);
        if (cooldown.isNone) {
            console.error(`  RejoinCooldown not set for ${target.address}`);
            return 0;
        }
        console.log(`  status=Kicked, RejoinCooldown until block ${cooldown.unwrap().toNumber()}`);
        return 1;
    } catch (e) {
        console.error(`  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll([observer]);
    }
}

module.exports = { run };
