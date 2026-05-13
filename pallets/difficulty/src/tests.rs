//! Tests for the fixed-anchor ASERT with break-only re-anchoring.

use crate::{
	mock::*,
	AnchorHeight,
	AnchorTarget,
	AnchorTimestamp,
	CurrentDifficulty,
	LastBlockTimestamp,
};
use frame_support::traits::Get;
use sp_core::U256;

#[test]
fn normal_block_keeps_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = run_to_block_at(1, 1_000_000);
		let initial = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + target);
		let after = CurrentDifficulty::<Test>::get();

		assert!(after == initial, "drift too large: {initial:?} -> {after:?}");
	});
}

#[test]
fn slow_block_decrease_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = run_to_block_at(1, 1_000_000);
		let before = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + 2 * target);
		let after = CurrentDifficulty::<Test>::get();

		assert!(after < before, "slow block must lower difficulty: {before:?} -> {after:?}");
	});
}

#[test]
fn fast_block_increase_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = run_to_block_at(1, 1_000_000);
		let before = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + target / 2);
		let after = CurrentDifficulty::<Test>::get();

		assert!(before < after, "fast block must higher difficulty: {before:?} -> {after:?}");
	});
}

#[test]
fn anchor_unchanged_during_normal_operation() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		
		let mut t = run_to_block_at(1, 1_000_000);
		let anchor_h = AnchorHeight::<Test>::get();
		let anchor_ts = AnchorTimestamp::<Test>::get();

		for i in 2u64..=6 {
			t = run_to_block_at(i, t + target);
		}

		assert_eq!(AnchorHeight::<Test>::get(), anchor_h, "anchor height must not move");
		assert_eq!(AnchorTimestamp::<Test>::get(), anchor_ts, "anchor timestamp must not move");
	});
}

#[test]
fn anchor_unchanged_when_gap_below_threshold() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let break_threshold: u64 = <Test as crate::Config>::BreakThresholdSecs::get();

		let mut t = run_to_block_at(1, 1_000_000);
		let anchor_h = AnchorHeight::<Test>::get();
		let anchor_ts = AnchorTimestamp::<Test>::get();

		t = run_to_block_at(2, t + target);
		t = run_to_block_at(3, t + target);
		t = run_to_block_at(4, t + break_threshold);

		assert_eq!(AnchorHeight::<Test>::get(), anchor_h, "anchor height must not move");
		assert_eq!(AnchorTimestamp::<Test>::get(), anchor_ts, "anchor timestamp must not move");
		assert_eq!(LastBlockTimestamp::<Test>::get(), t);
	});
}

#[test]
fn anchor_changed_when_gap_exceeds_threshold() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let break_threshold: u64 = <Test as crate::Config>::BreakThresholdSecs::get();

		let mut t = run_to_block_at(1, 1_000_000);

		t = run_to_block_at(2, t + target);
		t = run_to_block_at(3, t + target);
		t = run_to_block_at(4, t + break_threshold + 1);

		assert_eq!(AnchorHeight::<Test>::get(), 4, "anchor must move to recovery block");
		assert_eq!(AnchorTimestamp::<Test>::get(), t);
		assert_eq!(LastBlockTimestamp::<Test>::get(), t);
	});
}

#[test]
fn realtime_difficulty_halves_after_halflife() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let halflife: u64 = <Test as crate::Config>::Halflife::get();

		let t = run_to_block_at(1, 1_000_000);
		let initial = CurrentDifficulty::<Test>::get();

		let realtime_difficulty = crate::Pallet::<Test>::realtime_difficulty(t + halflife + target);
		let realtime_difficulty_2 = realtime_difficulty + realtime_difficulty;

		assert!(realtime_difficulty_2 == initial, "drift too large: {initial:?} -> {realtime_difficulty:?}");
	});
}

#[test]
fn realtime_difficulty_returns_current_when_next_height_before_anchor_height() {
	let initial = U256::from(INITIAL_DIFFICULTY);
	new_test_ext_with(initial).execute_with(|| {
		CurrentDifficulty::<Test>::put(initial);
		AnchorTimestamp::<Test>::put(0);
		AnchorHeight::<Test>::put(2);
		// block_number = 0 (default) → next_height = 1 < anchor_height = 2.
		let d = crate::Pallet::<Test>::realtime_difficulty(0);
		assert_eq!(d, initial, "before-anchor query must return current difficulty");
	});
}

#[test]
fn genesis_initializes_storage_from_difficulty() {
	let difficulty = U256::from(INITIAL_DIFFICULTY);
    new_test_ext_with(difficulty).execute_with(|| {
        assert_eq!(CurrentDifficulty::<Test>::get(), difficulty);
        assert_eq!(AnchorTarget::<Test>::get(), U256::MAX / difficulty);
        assert_eq!(AnchorTimestamp::<Test>::get(), 0);
        assert_eq!(AnchorHeight::<Test>::get(), 0);
    });
}

#[test]
fn anchor_initializes() {
    new_test_ext().execute_with(|| {
		run_to_block_at(1, 1);
        assert_eq!(AnchorTimestamp::<Test>::get(), 1);
        assert_eq!(AnchorHeight::<Test>::get(), 1);
        assert_eq!(LastBlockTimestamp::<Test>::get(), 1);
    });
}

#[test]
fn on_finalize_updates_last_block_timestamp_on_auto_init() {
	new_test_ext().execute_with(|| {
		let now = 1_234_567u64;
		run_to_block_at(1, now);
		assert_eq!(LastBlockTimestamp::<Test>::get(), now);
	});
}

#[test]
fn on_finalize_updates_last_block_timestamp_on_anchor_block_branch() {
	new_test_ext().execute_with(|| {
		// Force the anchor to sit ahead of the current block so the
		// `current_height <= anchor_height` short-circuit fires.
		AnchorTimestamp::<Test>::put(1_000_000);
		AnchorHeight::<Test>::put(100);
		LastBlockTimestamp::<Test>::put(0);

		let now = 1_000_500u64;
		run_to_block_at(5, now);

		// Anchor must remain untouched (we hit the early-return path)…
		assert_eq!(AnchorHeight::<Test>::get(), 100);
		assert_eq!(AnchorTimestamp::<Test>::get(), 1_000_000);
		// …but LastBlockTimestamp must still be refreshed.
		assert_eq!(LastBlockTimestamp::<Test>::get(), now);
	});
}

/// Zero-anchor-target branch: when `AnchorTarget` is zero the hook
/// short-circuits but must still update `LastBlockTimestamp`.
#[test]
fn on_finalize_updates_last_block_timestamp_when_anchor_target_zero() {
	new_test_ext().execute_with(|| {
		// Move past auto-init so we reach the ASERT path.
		let t = run_to_block_at(1, 1_000_000);
		// Wipe the anchor target to trigger the zero-target short-circuit.
		AnchorTarget::<Test>::put(U256::zero());
		let difficulty_before = CurrentDifficulty::<Test>::get();

		let now = t + 20;
		run_to_block_at(2, now);

		// Difficulty must not have been recomputed (we took the early return).
		assert_eq!(CurrentDifficulty::<Test>::get(), difficulty_before);
		assert_eq!(LastBlockTimestamp::<Test>::get(), now);
	});
}

/// Normal ASERT path: after a regular block the timestamp is recorded.
#[test]
fn on_finalize_updates_last_block_timestamp_on_normal_path() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let t = run_to_block_at(1, 1_000_000);
		let now = t + target;
		run_to_block_at(2, now);
		assert_eq!(LastBlockTimestamp::<Test>::get(), now);
	});
}

/// Break-recovery path: even when the hook re-anchors, the final write
/// to `LastBlockTimestamp` still wins.
#[test]
fn on_finalize_updates_last_block_timestamp_on_break_recovery() {
	new_test_ext().execute_with(|| {
		let break_threshold: u64 = <Test as crate::Config>::BreakThresholdSecs::get();
		let t = run_to_block_at(1, 1_000_000);
		let now = t + break_threshold + 1;
		run_to_block_at(2, now);
		assert_eq!(LastBlockTimestamp::<Test>::get(), now);
	});
}

