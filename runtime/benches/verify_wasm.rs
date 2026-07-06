//! Criterion baseline for seal verification inside the wasm runtime.
//!
//! Loads the real runtime blob and drives `PowVerifyApi` through
//! `sc_executor::WasmExecutor`, the same execution path block import takes,
//! so numbers include the executor call overhead on top of the scan itself.
//! Run `cargo bench -p solochain-template-runtime` before and after any
//! parameter change to compare baselines. Numbers never gate CI.

use codec::{Decode, Encode};
use criterion::{Criterion, criterion_group, criterion_main};
use poscan::{Compute, Seal, hash_meets_difficulty};
use sc_executor::WasmExecutor;
use sp_core::traits::{CallContext, CodeExecutor, RuntimeCode, WrappedRuntimeCode};
use sp_core::{H256, U256};
use std::hint::black_box;

/// Host function set mirroring the node executor.
type HostFunctions = (
	sp_io::SubstrateHostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

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

/// Run one runtime api call on the wasm blob under fresh externalities,
/// matching how block import verifies each seal.
fn call(
	executor: &WasmExecutor<HostFunctions>,
	code: &RuntimeCode,
	method: &str,
	args: &[u8],
) -> Vec<u8> {
	let mut ext = sp_io::TestExternalities::default();
	let mut ext = ext.ext();
	executor
		.call(&mut ext, code, method, args, CallContext::Offchain)
		.0
		.expect("runtime call succeeds")
}

fn wasm_verify_path(c: &mut Criterion) {
	let blob = solochain_template_runtime::WASM_BINARY.expect("runtime wasm built");
	let executor = WasmExecutor::<HostFunctions>::builder().build();
	let wrapped = WrappedRuntimeCode(blob.into());
	let runtime_code = RuntimeCode {
		code_fetcher: &wrapped,
		hash: sp_crypto_hashing::blake2_256(blob).to_vec(),
		heap_pages: None,
	};

	let pre_hash = H256::from_low_u64_be(42);
	let difficulty = U256::one();
	let seal = mine(pre_hash, difficulty);
	let verify_args = (pre_hash, seal.encode(), difficulty).encode();
	let mesh_args = (pre_hash, seal.nonce).encode();

	// The first calls compile and cache the wasm module and prove the mined
	// seal actually verifies before any timing starts.
	let out = call(&executor, &runtime_code, "PowVerifyApi_verify_seal", &verify_args);
	assert_eq!(bool::decode(&mut &out[..]), Ok(true));
	call(&executor, &runtime_code, "PowVerifyApi_generate_mesh", &mesh_args);

	c.bench_function("wasm_verify_seal", |b| {
		b.iter(|| black_box(call(&executor, &runtime_code, "PowVerifyApi_verify_seal", &verify_args)))
	});
	c.bench_function("wasm_generate_mesh", |b| {
		b.iter(|| black_box(call(&executor, &runtime_code, "PowVerifyApi_generate_mesh", &mesh_args)))
	});
}

criterion_group!(benches, wasm_verify_path);
criterion_main!(benches);
