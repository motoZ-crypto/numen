//! Benchmarks for `pallet-reward`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Hooks};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_consensus_pow::POW_ENGINE_ID;
use sp_runtime::{traits::Zero, DigestItem};

const SEED: u32 = 0;

#[benchmarks]
mod benchmarks {
    use super::*;

    /// Author present and reward still non zero, so the hook takes the mint
    /// path. Height zero skips the halving loop, whose shifts are noise next
    /// to the balance write.
    #[benchmark]
    fn on_finalize() -> Result<(), BenchmarkError> {
        // Not `whitelisted_caller`. No extrinsic paid for this account up
        // front, so the mint has to show up in the counts.
        let author: T::AccountId = account("author", 0, SEED);
        frame_system::Pallet::<T>::deposit_log(DigestItem::PreRuntime(
            POW_ENGINE_ID,
            author.encode(),
        ));

        let now = BlockNumberFor::<T>::zero();

        #[block]
        {
            Pallet::<T>::on_finalize(now);
        }

        assert!(!T::Currency::free_balance(&author).is_zero());
        Ok(())
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
