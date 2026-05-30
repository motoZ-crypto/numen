// Verify capacity-rejection on `validator.lock()`.
//
// Pre-conditions (set up by the .zndsl that drives this script):
//   * alice/bob/charlie active at genesis (3 / MaxValidators=4)
//   * dave already joined as 4th -> the active+pending set is now full
//
// This script attempts `validator.lock()` from Eve and expects it to fail
// with `validator.TooManyValidators`. Note that we do NOT need to call
// `session.set_keys` for Eve first because the capacity check fires
// BEFORE the SessionKeysNotRegistered check (see pallet `lock` body).
//
// Returns 1 on the expected failure, 0 on unexpected success or wrong
// error variant.

const { connect, disconnectAll, pair } = require("./lib");

async function run(_zombie, networkInfo) {
    const eveApi = await connect(networkInfo, "eve");
    try {
        const eve = await pair("//Eve");
        const validators = await eveApi.query.session.validators();
        const lockedBefore = await eveApi.query.validator.validatorLocks(eve.address);
        console.log(`  active=${validators.length} eve_locked=${!lockedBefore.isNone}`);

        // signAndSend manually so we can introspect the dispatch error.
        const result = await new Promise((resolve, reject) => {
            eveApi.tx.validator
                .lock()
                .signAndSend(eve, ({ status, dispatchError }) => {
                    if (dispatchError) {
                        if (dispatchError.isModule) {
                            const decoded = eveApi.registry.findMetaError(dispatchError.asModule);
                            return resolve({
                                ok: false,
                                section: decoded.section,
                                name: decoded.name,
                            });
                        }
                        return resolve({ ok: false, raw: dispatchError.toString() });
                    }
                    if (status.isInBlock || status.isFinalized) {
                        return resolve({ ok: true });
                    }
                })
                .catch(reject);
        });

        if (result.ok) {
            const validatorsNow = await eveApi.query.session.validators();
            console.error(`  expected TooManyValidators, but extrinsic succeeded`);
            console.error(`  current validators (${validatorsNow.length}): ${validatorsNow.map(v => v.toString()).join(", ")}`);
            return 0;
        }
        if (result.section !== "validator" || result.name !== "TooManyValidators") {
            console.error(`  expected validator.TooManyValidators, got ${result.section}.${result.name || result.raw}`);
            return 0;
        }
        console.log(`  got expected error: validator.TooManyValidators`);
        return 1;
    } catch (e) {
        console.error(`  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll([eveApi]);
    }
}

module.exports = { run };
