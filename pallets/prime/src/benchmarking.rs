//! Benchmarks for `pallet-prime`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

const SEED: u32 = 0;

#[benchmarks]
mod benchmarks {
    use super::*;

    /// Measures the origin check alone. Forwarding to `set_code` is charged
    /// separately at the call site, and a real upgrade needs a runtime blob
    /// that passes the version check, which a benchmark cannot produce.
    #[benchmark]
    fn upgrade() -> Result<(), BenchmarkError> {
        let caller: T::AccountId = whitelisted_caller();
        whitelist_account!(caller);
        Key::<T>::put(&caller);
        let origin = RawOrigin::Signed(caller);

        #[block]
        {
            Pallet::<T>::ensure_prime(origin.into())?;
        }

        Ok(())
    }

    #[benchmark]
    fn set_key() -> Result<(), BenchmarkError> {
        let caller: T::AccountId = whitelisted_caller();
        whitelist_account!(caller);
        Key::<T>::put(&caller);
        let new: T::AccountId = account("new", 0, SEED);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), new.clone());

        assert_eq!(Key::<T>::get(), Some(new));
        Ok(())
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
