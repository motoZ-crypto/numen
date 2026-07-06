//! Criterion baseline for the block verification path.
//!
//! Measures the full `verify_seal` replay plus its two dominant segments,
//! asteroid generation and the spectral3d scan. Run `cargo bench -p poscan`
//! before and after any parameter change to compare baselines. Numbers are
//! for local comparison only and never gate CI.

use codec::Encode;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use poscan::{Compute, Seal, TARGET_SAMPLES, hash_meets_difficulty, verify_seal};
use primitive_types::{H256, U256};
use std::hint::black_box;

/// Mine a valid seal by scanning successive nonces. Difficulty one accepts
/// every work value, so the first structurally scannable mesh wins.
fn mine(pre_hash: H256, difficulty: U256) -> Seal {
	let mut nonce = U256::zero();
	loop {
		let compute = Compute { pre_hash, nonce };
		if let Some(work) = compute.work()
			&& hash_meets_difficulty(&work, difficulty)
		{
			return Seal { nonce, work };
		}
		nonce = nonce.saturating_add(U256::one());
	}
}

fn verify_path(c: &mut Criterion) {
	let pre_hash = H256::from_low_u64_be(42);
	let difficulty = U256::one();
	let seal = mine(pre_hash, difficulty);
	let raw = seal.encode();
	let compute = Compute { pre_hash, nonce: seal.nonce };
	let mesh = compute.mesh();

	c.bench_function("mesh_generation", |b| b.iter(|| black_box(compute.mesh())));
	c.bench_function("scan", |b| {
		b.iter_batched(
			|| mesh.clone(),
			|m| spectral3d::scan(m, TARGET_SAMPLES).unwrap(),
			BatchSize::SmallInput,
		)
	});
	c.bench_function("verify_seal", |b| {
		b.iter(|| verify_seal(black_box(pre_hash), black_box(&raw), black_box(difficulty)).unwrap())
	});
}

criterion_group!(benches, verify_path);
criterion_main!(benches);
