//! Benchmarks for `pallet-difficulty`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::U256;
use sp_runtime::traits::SaturatedConversion;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Interruption recovery is the costliest path. It runs the full ASERT
	/// computation and then re-anchors, which writes every anchor item on top
	/// of the difficulty update.
	#[benchmark]
	fn on_finalize() -> Result<(), BenchmarkError> {
		let anchor_secs = 1_000u64;
		let anchor_height = 1u64;

		AnchorTimestamp::<T>::put(anchor_secs);
		AnchorHeight::<T>::put(anchor_height);
		AnchorTarget::<T>::put(U256::MAX / U256::from(1_000u32));
		LastBlockTimestamp::<T>::put(anchor_secs);

		// Past the anchor height so the ASERT branch is taken rather than the
		// anchor-block shortcut.
		let now = BlockNumberFor::<T>::from(100u32);
		frame_system::Pallet::<T>::set_block_number(now);

		// Far enough beyond the parent to trip `BreakThresholdSecs`.
		let gap = T::BreakThresholdSecs::get().saturating_add(1);
		let now_secs = anchor_secs.saturating_add(gap);
		pallet_timestamp::Now::<T>::put(now_secs.saturating_mul(1_000).saturated_into::<T::Moment>());

		#[block]
		{
			Pallet::<T>::on_finalize(now);
		}

		assert_eq!(AnchorHeight::<T>::get(), 100u64);
		assert_eq!(LastBlockTimestamp::<T>::get(), now_secs);
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
