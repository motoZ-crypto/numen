//! Benchmarks for `pallet-validator`.
//!
//! Every case is set up at the worst point of its input range rather than
//! carrying a `Linear` component, so the dispatchables keep a flat weight and
//! the call sites stay free of storage lookups. The exception is the block
//! sweep, whose cost tracks the number of locks.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use alloc::{vec, vec::Vec};
use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Hooks, WithdrawReasons};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use sp_runtime::traits::{Bounded, One};

use crate::Pallet as Validator;

const SEED: u32 = 0;

/// Halved rather than maxed so later balance arithmetic cannot overflow.
fn seed_balance<T: Config>(who: &T::AccountId) {
    T::Currency::make_free_balance_be(who, BalanceOf::<T>::max_value() / 2u32.into());
}

fn fill_pending<T: Config>(n: u32) -> Result<(), BenchmarkError> {
    let queue: Vec<T::AccountId> = (0..n).map(|i| account("filler", i, SEED)).collect();
    let bounded = BoundedVec::try_from(queue)
        .map_err(|_| BenchmarkError::Stop("filler count exceeds MaxValidators"))?;
    PendingValidators::<T>::put(bounded);
    Ok(())
}

fn insert_active_lock<T: Config>(who: &T::AccountId, expiry_block: BlockNumberFor<T>) {
    ValidatorLocks::<T>::insert(
        who,
        LockInfo {
            amount: T::LockAmount::get(),
            lock_block: BlockNumberFor::<T>::zero(),
            expiry_block,
            status: ValidatorStatus::Active,
        },
    );
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn lock() -> Result<(), BenchmarkError> {
        // Genesis may have appointed validators. Clearing the active set hands
        // the whole cap to the pending queue, the branch that decodes and
        // rewrites the most data.
        DesiredValidators::<T>::kill();
        fill_pending::<T>(T::MaxValidators::get().saturating_sub(1))?;

        // Block one so the cooldown seeded below reads as already expired.
        frame_system::Pallet::<T>::set_block_number(One::one());

        let caller: T::AccountId = whitelisted_caller();
        whitelist_account!(caller);
        seed_balance::<T>(&caller);
        T::BenchmarkHelper::make_eligible(&caller);

        // An expired cooldown costs more than a missing one since it is cleared.
        RejoinCooldown::<T>::insert(&caller, BlockNumberFor::<T>::zero());

        #[extrinsic_call]
        _(RawOrigin::Signed(caller.clone()));

        assert!(ValidatorLocks::<T>::contains_key(&caller));
        assert!(!RejoinCooldown::<T>::contains_key(&caller));
        Ok(())
    }

    #[benchmark]
    fn request_exit() -> Result<(), BenchmarkError> {
        let caller: T::AccountId = whitelisted_caller();
        whitelist_account!(caller);

        // Caller sits at the front of a full queue so its removal shifts every
        // remaining entry.
        let mut queue = vec![caller.clone()];
        queue.extend((1..T::MaxValidators::get()).map(|i| account("filler", i, SEED)));
        let bounded = BoundedVec::try_from(queue)
            .map_err(|_| BenchmarkError::Stop("filler count exceeds MaxValidators"))?;
        PendingValidators::<T>::put(bounded);

        insert_active_lock::<T>(&caller, T::LockDuration::get());

        #[extrinsic_call]
        _(RawOrigin::Signed(caller.clone()));

        assert_eq!(
            ValidatorLocks::<T>::get(&caller).map(|info| info.status),
            Some(ValidatorStatus::ExitRequested),
        );
        Ok(())
    }

    #[benchmark]
    fn set_stake_exempt() -> Result<(), BenchmarkError> {
        // `Stop` rather than `Weightless`. This origin comes out of genesis
        // state, and a preset that drops it would otherwise silently downgrade
        // the call to zero weight.
        let origin = T::ExemptOrigin::try_successful_origin()
            .map_err(|_| BenchmarkError::Stop("ExemptOrigin has no successful origin"))?;
        let who: T::AccountId = account("exempt", 0, SEED);

        // Granting inserts; revoking only clears. Benchmark the costlier side.
        #[extrinsic_call]
        _(origin as T::RuntimeOrigin, who.clone(), true);

        assert!(StakeExemptAccounts::<T>::contains_key(&who));
        Ok(())
    }

    /// Block sweep over `n` lock records, all of them expired so each one takes
    /// the release path.
    #[benchmark]
    fn on_initialize(n: Linear<0, { T::MaxValidators::get() }>) -> Result<(), BenchmarkError> {
        // Genesis appoints validators carrying their own lock records. Clearing
        // them keeps the scan length equal to `n`.
        let _ = ValidatorLocks::<T>::clear(u32::MAX, None);

        let now = BlockNumberFor::<T>::one();
        frame_system::Pallet::<T>::set_block_number(now);

        let amount = T::LockAmount::get();
        for i in 0..n {
            let who: T::AccountId = account("validator", i, SEED);
            seed_balance::<T>(&who);
            T::Currency::set_lock(T::LockId::get(), &who, amount, WithdrawReasons::all());
            insert_active_lock::<T>(&who, BlockNumberFor::<T>::zero());
        }

        #[block]
        {
            Validator::<T>::on_initialize(now);
        }

        assert_eq!(ValidatorLocks::<T>::iter().count(), 0);
        Ok(())
    }

    impl_benchmark_test_suite!(
        Validator,
        crate::mock::new_test_ext(vec![]),
        crate::mock::Test
    );
}
