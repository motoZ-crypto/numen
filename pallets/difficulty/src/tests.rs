//! Tests for the fixed-anchor ASERT with break-only re-anchoring.

use crate::{
	asert::compute_next_target,
	mock::*,
	AnchorHeight,
	AnchorTimestamp,
	CurrentDifficulty,
	LastBlockTimestamp,
};
use frame_support::traits::Get;
use sp_core::U256;

// Pallet behavior tests.

#[test]
fn normal_block_keeps_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = bootstrap(1_000_000);
		let initial = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + target);
		let after = CurrentDifficulty::<Test>::get();

		assert!(after == initial, "drift too large: {initial:?} -> {after:?}");
	});
}

#[test]
fn slow_block_decreases_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = bootstrap(1_000_000);
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

		let t = bootstrap(1_000_000);
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
		
		let mut t = bootstrap(1_000_000);
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

		let mut t = bootstrap(1_000_000);
		let anchor_h = AnchorHeight::<Test>::get();
		let anchor_ts = AnchorTimestamp::<Test>::get();

		t = run_to_block_at(2, t + target);
		t = run_to_block_at(3, t + target);
		t = run_to_block_at(4, t + break_threshold - 1);

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

		let mut t = bootstrap(1_000_000);

		t = run_to_block_at(2, t + target);
		t = run_to_block_at(3, t + target);
		t = run_to_block_at(4, t + break_threshold);

		assert_eq!(AnchorHeight::<Test>::get(), 4, "anchor must move to recovery block");
		assert_eq!(AnchorTimestamp::<Test>::get(), t);
		assert_eq!(LastBlockTimestamp::<Test>::get(), t);
	});
}

#[test]
fn realtime_difficulty_halves_after_one_halflife_gap() {
	new_test_ext().execute_with(|| {
		let halflife: u64 = <Test as crate::Config>::Halflife::get();

		let t = bootstrap(1_000_000);
		let initial = CurrentDifficulty::<Test>::get();

		let realtime_difficulty = crate::Pallet::<Test>::realtime_difficulty(t + halflife + 20);
		let realtime_difficulty_2 = realtime_difficulty + realtime_difficulty;
		let diff = if realtime_difficulty_2 > initial { realtime_difficulty_2 - initial } else { initial - realtime_difficulty_2 };

		assert!(diff  == U256::from(0u64), "drift too large: {initial:?} -> {realtime_difficulty:?}");
	});
}

// Pure ASERT target calculation tests.

#[test]
fn on_schedule_returns_anchor() {
	let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
	let halflife: u64 = <Test as crate::Config>::Halflife::get();
	// If blocks are exactly on schedule, target should equal anchor_target.
	let anchor = U256::from(1_000_000u64);
	// time_delta = target_block_time * height_delta
	// height_delta=10 (10th block after anchor), time_delta = target * 10
	let result = compute_next_target(anchor, (target * 10) as i64, 10, target, halflife);
	// Should be very close to anchor (within rounding).
	let diff = if result > anchor { result - anchor } else { anchor - result };
	assert!(diff <= U256::from(1u64), "expected ~anchor, got {:?}", result);
}

#[test]
fn slow_blocks_increase_target() {
	let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
	let halflife: u64 = <Test as crate::Config>::Halflife::get();
	// Blocks coming slower than expected -> target increases (difficulty decreases).
	let anchor = U256::from(1_000_000u64);
	// height_delta=10, ideal time = target*10, actual time = 2*target*10 (twice as slow)
	let result = compute_next_target(anchor, (target * 20) as i64, 10, target, halflife);
	assert!(result > anchor, "slow blocks should increase target");
}

#[test]
fn fast_blocks_decrease_target() {
	let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
	let halflife: u64 = <Test as crate::Config>::Halflife::get();
	// Blocks coming faster than expected -> target decreases (difficulty increases).
	let anchor = U256::from(1_000_000u64);
	// height_delta=10, ideal time = target*10, actual time = target*5 (twice as fast)
	let result = compute_next_target(anchor, (target * 5) as i64, 10, target, halflife);
	assert!(result < anchor, "fast blocks should decrease target");
}

#[test]
fn halflife_halves_target_when_fast() {
	let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
	let halflife: u64 = <Test as crate::Config>::Halflife::get();
	// If blocks arrive halflife seconds behind schedule, target should double (difficulty halves).
	let anchor = U256::from(1u64) << 128;
	// exponent = +1 -> time_delta - target*1 = halflife -> time_delta = halflife + target
	// height_delta=1 (one block after anchor)
	let result = compute_next_target(anchor, (halflife + target) as i64, 1, target, halflife);
	let expected = anchor * U256::from(2u64);
	// Allow ~1% tolerance due to polynomial approximation.
	let tolerance = expected / U256::from(100u64);
	let diff = if result > expected { result - expected } else { expected - result };
	assert!(diff < tolerance, "expected ~{:?}, got {:?}", expected, result);
}

#[test]
fn no_blocks_for_30min_halves_difficulty() {
	let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
	let halflife: u64 = <Test as crate::Config>::Halflife::get();
	// One halflife (1800s) without blocks from anchor.
	// height_delta=1, time_delta = halflife + target
	// exponent = (time_delta - target*1) / halflife = halflife/halflife = 1
	// target doubles -> difficulty halves.
	let anchor = U256::from(1u64) << 128;
	let result = compute_next_target(anchor, (halflife + target) as i64, 1, target, halflife);
	let expected = anchor * U256::from(2u64);
	let tolerance = expected / U256::from(100u64);
	let diff = if result > expected { result - expected } else { expected - result };
	assert!(diff < tolerance, "difficulty should halve after 30min gap");
}

#[test]
fn result_never_zero() {
	let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
	let halflife: u64 = <Test as crate::Config>::Halflife::get();
	// Even with extremely fast blocks, target should not be zero.
	let anchor = U256::from(1u64);
	let result = compute_next_target(anchor, 0, 1000, target, halflife);
	assert!(!result.is_zero(), "target must never be zero");
}
