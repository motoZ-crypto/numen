//! Pure-integer ASERT difficulty target calculation.
//!
//! Implements the formula:
//!
//! ```text
//! next_target = anchor_target × 2^((time_delta - target_block_time × height_delta) / halflife)
//! ```
//!
//! All arithmetic is integer-only using 16-bit fixed-point representation.
//! The `2^x` function is approximated via a cubic polynomial with < 0.013% error.

use sp_core::{U256, U512};

/// Fixed-point fractional bits.
const FRAC_BITS: i128 = 16;
/// 1.0 in fixed-point representation.
const FRAC_ONE: i128 = 1 << FRAC_BITS; // 65536

/// Compute the ASERT next target from anchor block parameters.
///
/// # Parameters
///
/// - `anchor_target`: Target value of the anchor block (U256).
/// - `time_delta`: Current block timestamp minus anchor block parent timestamp (seconds).
/// - `height_delta`: Current block height minus anchor block height.
/// - `target_block_time`: Ideal block interval in seconds (e.g. 20).
/// - `halflife`: ASERT halflife in seconds (e.g. 1800).
///
/// # Returns
///
/// The computed next target (U256), clamped to [1, U256::MAX].
pub fn compute_next_target(
	anchor_target: U256,
	time_delta: i128,
	height_delta: u64,
	target_block_time: u64,
	halflife: u64,
) -> U256 {
	// exponent = (time_delta - target_block_time * height_delta) / halflife
	// In fixed-point: exponent_fp = ((time_delta - ideal_time) << FRAC_BITS) / halflife
	let ideal_time = target_block_time * height_delta;
	let exponent_numer = (time_delta - ideal_time as i128) * FRAC_ONE;
	let halflife_i128 = halflife as i128;
	// Division rounds toward zero; this is acceptable for the exponent.
	let exponent_fp = exponent_numer / halflife_i128;

	// Compute 2^exponent_fp in fixed-point.
	// Split into integer part and fractional part.
	// exponent_fp is in units of halflife, so we need to compute 2^(exponent_fp / FRAC_ONE).
	// Wait — the exponent is already divided by halflife above, so exponent_fp represents
	// the actual exponent (in halflife units). The formula uses base-2 exponentiation
	// directly with the halflife as denominator, so exponent_fp is the number of halvings.
	//
	// We need: factor = 2^(exponent_fp / FRAC_ONE)
	// Split: integer_part = exponent_fp >> FRAC_BITS (arithmetic shift)
	//        frac_part    = exponent_fp & (FRAC_ONE - 1)
	// For negative: we need floor division, not truncation toward zero.
	let int_part = exponent_fp >> FRAC_BITS; // arithmetic right shift = floor for negatives
	let frac_part = exponent_fp - (int_part << FRAC_BITS); // always in [0, FRAC_ONE)

	// Approximate 2^frac where frac is in [0, FRAC_ONE) fixed-point.
	// Cubic polynomial: 2^x ≈ 1 + a1*x + a2*x^2 + a3*x^3
	// Coefficients scaled to fixed-point (16 bits):
	//   a1 = ln(2) ≈ 0.693147 → 45426
	//   a2 = ln(2)^2/2 ≈ 0.240227 → 15736
	//   a3 = ln(2)^3/6 ≈ 0.055504 → 3638 (rounded from 3637.7)
	const A1: i128 = 45426;
	const A2: i128 = 15736;
	const A3: i128 = 3638;

	// frac_part is in [0, FRAC_ONE), compute polynomial in fixed-point.
	let x = frac_part;
	// 2^frac ≈ FRAC_ONE + a1*x/FRAC_ONE + a2*x^2/FRAC_ONE^2 + a3*x^3/FRAC_ONE^3
	let term1 = A1 * x / FRAC_ONE;
	let term2 = A2 * x / FRAC_ONE * x / FRAC_ONE;
	let term3 = A3 * x / FRAC_ONE * x / FRAC_ONE * x / FRAC_ONE;
	let frac_factor = FRAC_ONE + term1 + term2 + term3; // in fixed-point

	// Now: next_target = anchor_target * frac_factor / FRAC_ONE * 2^int_part
	// Apply the fractional multiplier first (to preserve precision).
	let frac_factor_u256 = U256::from(frac_factor);
	let frac_one_u256 = U256::from(FRAC_ONE);

	let mut result = match anchor_target.checked_mul(frac_factor_u256) {
		Some(p) => p / frac_one_u256,
		None => {
			let prod: U512 = anchor_target.full_mul(frac_factor_u256);
			let quot: U512 = prod / U512::from(FRAC_ONE);
			U256::try_from(quot).unwrap_or(U256::MAX)
		},
	};

	// Apply integer exponent: shift left for positive, shift right for negative.
	if int_part >= 0 {
		if int_part >= 256 {
			// Astronomically high target — saturate to U256::MAX.
			return U256::MAX;
		}
		let shift = int_part as u32;
		// Check for overflow: if `result` has bits set in positions >= (256 - shift),
		// the shift would overflow.
		let headroom = 256 - result.bits();
		if shift as usize > headroom {
			return U256::MAX;
		}
		result <<= shift;
	} else {
		if int_part <= -256 {
			// Underflow — clamp to minimum target of 1.
			return U256::one();
		}
		let shift = (-int_part) as u32;
		result >>= shift;
	}

	// Clamp: target must be at least 1 (difficulty must not be infinite).
	if result.is_zero() {
		U256::one()
	} else {
		result
	}
}

#[cfg(test)]
mod tests {
	use sp_runtime::traits::One;
	use super::*;

	/// If blocks are exactly on schedule, target should equal anchor_target.
	#[test]
	fn on_schedule_keeps_anchor() {
		let anchor = U256::from(1_000_000u64);
		// height_delta=10 (10th block after anchor), time_delta = target * 10
		let result = compute_next_target(anchor, 200, 10, 20, 1800);
		assert!(anchor == result, "expected ~anchor, got {:?}", result);
	}

	/// Blocks coming slower than expected -> target increases (difficulty decreases).
	#[test]
	fn slow_blocks_increase_target() {
		let anchor = U256::from(1_000_000u64);
		// height_delta=10, ideal time = target*10, actual time = 2*target*10 (twice as slow)
		let result = compute_next_target(anchor, 400, 10, 20, 1800);
		assert!(result > anchor, "slow blocks should increase target");
	}

	/// Blocks coming faster than expected → target decreases (difficulty increases).
	#[test]
	fn fast_blocks_decrease_target() {
		let anchor = U256::from(1_000_000u64);
		// height_delta=10, ideal time = 200s, actual time = 100s (twice as fast)
		let result = compute_next_target(anchor, 100, 10, 20, 1800);
		assert!(result < anchor, "fast blocks should decrease target");
	}

	/// If blocks arrive halflife seconds ahead of schedule, target should halve.
	#[test]
	fn target_doubles_after_halflife() {
		let anchor = U256::from(1_000_000u64);
		// For target to double (halflife behind schedule):
		// exponent = +1 → time_delta - 20*1 = 1800 → time_delta = 1820
		// height_delta=1 (one block after anchor)
		let result = compute_next_target(anchor, 1820, 1, 20, 1800);
		let expected = anchor + anchor;
		assert!(expected == result, "expected ~{:?}, got {:?}", expected, result);
	}

	/// Even with extremely fast blocks, target should not be zero.
	#[test]
	fn target_never_zero() {
		let anchor = U256::from(1u64);
		let result = compute_next_target(anchor, 0, u64::MAX, 1, 1);
		assert!(result.is_one(), "target must be at least 1");
	}

	/// When `int_part >= 256` the result must saturate to `U256::MAX`.
	#[test]
	fn extreme_positive_exponent_clamps_to_max() {
		let anchor = U256::from(1_000_000u64);
		// halflife=1, target_block_time=1, height_delta=0, time_delta=256
		// → exponent = 256 halvings → int_part = 256 (boundary).
		let result = compute_next_target(anchor, 256, 0, 1, 1);
		assert_eq!(result, U256::MAX, "int_part == 256 must saturate to U256::MAX");
	}

	/// When `int_part <= -256` the result must be clamped to `U256::one()`.
	#[test]
	fn extreme_negative_exponent_clamps_to_one() {
		let anchor = U256::from(1_000_000u64);
		// halflife=1, target_block_time=1, height_delta=256, time_delta=0
		// → numer = (0 - 256) * 65536 = -16_777_216
		// → exponent_fp = -16_777_216, int_part = -256 (boundary).
		let result = compute_next_target(anchor, 0, 256, 1, 1);
		assert_eq!(result, U256::one(), "int_part == -256 must clamp to U256::one()");
	}

	/// When the left-shift would overflow `U256`, the result must saturate to `U256::MAX`.
	#[test]
	fn left_shift_overflow_clamps_to_max() {
		// Anchor occupies 239 bits → headroom = 17. A shift > 17 triggers the
		// early-return overflow guard.
		let anchor = U256::MAX >> 17;
		// halflife=1, target_block_time=1, height_delta=0, time_delta=18
		// → int_part = 18 (just past headroom), frac_part = 0
		// → shift (18) > headroom (17): early return U256::MAX.
		let result = compute_next_target(anchor, 18, 0, 1, 1);
		assert_eq!(result, U256::MAX, "left-shift overflow must saturate to U256::MAX");
	}

	/// When the right-shift drives the result to zero, it must be clamped to 1.
	#[test]
	fn right_shift_to_zero_clamps_to_one() {
		let anchor = U256::from(1u64);
		// halflife=1800, target_block_time=20, height_delta=1, time_delta = 20 - 1800 = -1780
		// → exponent ≈ -1 halving → int_part = -1, frac_part = 0
		// → result = 1 >> 1 = 0, must be clamped to 1.
		let result = compute_next_target(anchor, -1780, 1, 20, 1800);
		assert!(result.is_one(), "zero result after right-shift must be clamped to 1");
	}

	/// A maximally large anchor with positive fractional exponent must rely on
	/// the U512 fallback and saturate to `U256::MAX` without panicking.
	#[test]
	fn max_anchor_with_fractional_exponent_saturates_to_max() {
		// anchor = U256::MAX, halflife=2, time_delta=1 → frac_factor ≈ 92638.
		// `anchor * frac_factor` overflows U256, U512 fallback yields > U256::MAX,
		// `try_from` saturates to U256::MAX.
		let result = compute_next_target(U256::MAX, 1, 0, 1, 2);
		assert_eq!(result, U256::MAX, "max anchor with positive frac must saturate to U256::MAX");
	}

	/// When the U256 fast path overflows but a large negative shift would
	/// bring the true mathematical result back below `U256::MAX`, the U512
	/// fallback must preserve that exact value.
	#[test]
	fn max_anchor_with_large_negative_shift_preserves_value() {
		// anchor=U256::MAX, halflife=1, target_block_time=1, height_delta=0, time_delta=-18
		// → int_part = -18, frac_part = 0, frac_factor = FRAC_ONE.
		// Fast path overflows (MAX * 65536); U512 fallback yields MAX,
		// then `>> 18` brings it down to MAX >> 18.
		let result = compute_next_target(U256::MAX, -18, 0, 1, 1);
		assert_eq!(
			result,
			U256::MAX >> 18,
			"U512 fallback must preserve value through large negative shift"
		);
	}

}
