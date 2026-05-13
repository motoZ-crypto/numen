//! SHA-256 double hash PoW placeholder algorithm.
//!
//! Algorithm: `SHA-256(SHA-256(pre_hash || nonce))` — the resulting hash is
//! compared against a difficulty target using the multiplication-overflow method.
//!
//! This is an interim algorithm and will be replaced by an ASIC-resistant scheme.
//!
//! The core types ([`Seal`], [`Compute`], [`hash_meets_difficulty`], [`verify_seal`])
//! are `no_std`-compatible so the runtime can perform seal verification, 
//! making the algorithm hot-swappable via runtime upgrade.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::vec::Vec;

use codec::{Decode, Encode};
#[cfg(feature = "std")]
use sc_consensus_pow::{Error as PowError, PowAlgorithm};
use sha2::{Digest, Sha256};
use sp_core::{H256, U256};

// ── Core types (no_std) ─────────────────────────────────────────────

/// Seal produced by the miner, encoded into `Vec<u8>` for the block digest.
#[derive(Clone, PartialEq, Eq, Encode, Decode, Debug)]
pub struct Seal {
	/// Nonce that satisfies the difficulty target.
	pub nonce: U256,
	/// Difficulty at which this seal was mined.
	pub difficulty: U256,
	/// Resulting double SHA-256 hash.
	pub work: H256,
}

/// An un-evaluated mining attempt.  Call [`Compute::work`] to obtain the hash.
#[derive(Clone, PartialEq, Eq, Encode, Decode, Debug)]
pub struct Compute {
	/// Block pre-hash (header hash without the seal digest).
	pub pre_hash: H256,
	/// Candidate nonce.
	pub nonce: U256,
}

impl Compute {
	/// `SHA-256(SHA-256(pre_hash || nonce))`
	pub fn work(&self) -> H256 {
		let mut buf = Vec::with_capacity(64);
		buf.extend_from_slice(self.pre_hash.as_bytes());
		buf.extend_from_slice(&self.nonce.encode());

		let first = Sha256::digest(&buf);
		let second = Sha256::digest(first);

		H256::from_slice(&second)
	}

	/// Mine: compute the work hash and bundle it into a [`Seal`].
	pub fn seal(self, difficulty: U256) -> Seal {
		let work = self.work();
		Seal { nonce: self.nonce, difficulty, work }
	}
}

/// Returns `true` when `hash` satisfies `difficulty`.
///
/// The check multiplies the numeric value of the hash by the difficulty.
/// If the product overflows `U256`, the hash is too large (i.e. the work was
/// not sufficient).
pub fn hash_meets_difficulty(hash: &H256, difficulty: U256) -> bool {
	let num_hash = U256::from_big_endian(hash.as_bytes());
	let (_, overflowed) = num_hash.overflowing_mul(difficulty);
	!overflowed
}

/// Standalone seal verification usable from both runtime and node.
///
/// Returns `Ok(true)` only when:
///   1. The seal can be SCALE-decoded.
///   2. The contained `work` hash meets the difficulty target.
///   3. Re-computing the hash from `pre_hash` and `nonce` reproduces `work`.
pub fn verify_seal(pre_hash: H256, raw_seal: &[u8], difficulty: U256) -> Result<bool, codec::Error> {
	let seal = Seal::decode(&mut &raw_seal[..])?;

	if !hash_meets_difficulty(&seal.work, difficulty) {
		return Ok(false);
	}

	let compute = Compute { pre_hash, nonce: seal.nonce };
	if compute.work() != seal.work {
		return Ok(false);
	}

	Ok(true)
}

// ── Runtime API declaration (no_std) ────────────────────────────────

sp_api::decl_runtime_apis! {
	/// Runtime-side seal verification API.
	///
	/// Implementing this in the runtime makes the PoW algorithm
	/// hot-swappable via a runtime upgrade.
	pub trait PowVerifyApi {
		fn verify_seal(pre_hash: H256, seal: Vec<u8>, difficulty: U256) -> bool;
	}
}

// ── PowAlgorithm implementation (std only) ──────────────────────────

#[cfg(feature = "std")]
use sp_api::ProvideRuntimeApi;
#[cfg(feature = "std")]
use pallet_difficulty::DifficultyApi;
#[cfg(feature = "std")]
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
#[cfg(feature = "std")]
use std::{marker::PhantomData, sync::Arc};

/// SHA-256 double-hash PoW algorithm backed by a runtime `DifficultyApi`
/// and `PowVerifyApi`.
///
/// Verification is delegated to the runtime so that the algorithm can be
/// replaced by a runtime upgrade without restarting the node.
#[cfg(feature = "std")]
pub struct Sha256DoubleHashAlgorithm<B: BlockT, C> {
	client: Arc<C>,
	_phantom: PhantomData<B>,
}

#[cfg(feature = "std")]
impl<B: BlockT, C> Sha256DoubleHashAlgorithm<B, C> {
	/// Create a new algorithm instance backed by the given client.
	pub fn new(client: Arc<C>) -> Self {
		Self { client, _phantom: PhantomData }
	}
}

#[cfg(feature = "std")]
impl<B: BlockT, C> Clone for Sha256DoubleHashAlgorithm<B, C> {
	fn clone(&self) -> Self {
		Self { client: self.client.clone(), _phantom: PhantomData }
	}
}

#[cfg(feature = "std")]
impl<B, C> PowAlgorithm<B> for Sha256DoubleHashAlgorithm<B, C>
where
	B: BlockT<Hash = H256>,
	C: ProvideRuntimeApi<B> + sp_blockchain::HeaderBackend<B> + Send + Sync,
	C::Api: DifficultyApi<B> + PowVerifyApi<B>,
{
	type Difficulty = U256;

	fn difficulty(&self, parent: B::Hash) -> Result<U256, PowError<B>> {
		// Use wall-clock time so difficulty naturally decays even when no
		// blocks are being produced.
		let now_secs = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs();

		self.client
			.runtime_api()
			.realtime_difficulty(parent, now_secs)
			.map_err(|err| PowError::Environment(format!(
				"Fetching realtime difficulty from runtime failed: {err:?}"
			)))
	}

	fn verify(
		&self,
		parent: &BlockId<B>,
		pre_hash: &B::Hash,
		_pre_digest: Option<&[u8]>,
		seal: &sp_consensus_pow::Seal,
		difficulty: U256,
	) -> Result<bool, PowError<B>> {
		let parent_hash = match parent {
			BlockId::Hash(h) => *h,
			BlockId::Number(_) => {
				return Err(PowError::Environment(
					"BlockId::Number not supported for verify".into(),
				));
			},
		};

		self.client
			.runtime_api()
			.verify_seal(parent_hash, *pre_hash, seal.clone(), difficulty)
			.map_err(|err| PowError::Runtime(format!("Runtime verify_seal failed: {err:?}")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn compute_produces_deterministic_work() {
		let c1 = Compute { pre_hash: H256::from_low_u64_be(42), nonce: U256::from(1) };
		let c2 = c1.clone();
		assert_eq!(c1.work(), c2.work());
	}

	#[test]
	fn seal_roundtrip() {
		let compute = Compute { pre_hash: H256::from_low_u64_be(99), nonce: U256::from(7) };
		let seal = compute.seal(U256::from(1));
		let encoded = seal.encode();
		let decoded = Seal::decode(&mut &encoded[..]).expect("should decode");
		assert_eq!(decoded, seal);
	}

	#[test]
	fn trivial_difficulty_always_met() {
		let compute = Compute { pre_hash: H256::from_low_u64_be(0), nonce: U256::from(0) };
		let work = compute.work();
		assert!(hash_meets_difficulty(&work, U256::from(1)));
	}

	#[test]
	fn impossible_difficulty_never_met() {
		let compute = Compute { pre_hash: H256::from_low_u64_be(1), nonce: U256::from(1) };
		let work = compute.work();
		assert!(!hash_meets_difficulty(&work, U256::MAX));
	}

	#[test]
	fn mining_finds_valid_seal() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(10);

		let mut nonce = U256::zero();
		let seal = loop {
			let compute = Compute { pre_hash, nonce };
			let work = compute.work();
			if hash_meets_difficulty(&work, difficulty) {
				break compute.seal(difficulty);
			}
			nonce = nonce.saturating_add(U256::one());
			assert!(nonce < U256::from(1_000_000), "should find a seal within 1M attempts");
		};

		assert!(hash_meets_difficulty(&seal.work, difficulty));
	}

	#[test]
	fn malformed_seal_returns_decode_error() {
		let garbage = [0xDE, 0xAD];
		let result = Seal::decode(&mut &garbage[..]);
		assert!(result.is_err(), "malformed bytes must fail to decode");
	}

	#[test]
	fn wrong_nonce_rejected() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(10);

		let mut nonce = U256::zero();
		let mut seal = loop {
			let compute = Compute { pre_hash, nonce };
			let work = compute.work();
			if hash_meets_difficulty(&work, difficulty) {
				break compute.seal(difficulty);
			}
			nonce = nonce.saturating_add(U256::one());
			assert!(nonce < U256::from(1_000_000));
		};

		seal.nonce = seal.nonce.saturating_add(U256::one());
		let recomputed = Compute { pre_hash, nonce: seal.nonce }.work();
		assert_ne!(recomputed, seal.work, "tampered nonce should produce different work");
	}

	#[test]
	fn verify_seal_accepts_valid_seal() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(10);

		let mut nonce = U256::zero();
		let seal = loop {
			let compute = Compute { pre_hash, nonce };
			let work = compute.work();
			if hash_meets_difficulty(&work, difficulty) {
				break compute.seal(difficulty);
			}
			nonce = nonce.saturating_add(U256::one());
			assert!(nonce < U256::from(1_000_000));
		};

		let encoded = seal.encode();
		assert_eq!(verify_seal(pre_hash, &encoded, difficulty), Ok(true));
	}

	#[test]
	fn verify_seal_rejects_insufficient_difficulty() {
		let pre_hash = H256::from_low_u64_be(12345);
		let easy_difficulty = U256::from(2);

		let mut nonce = U256::zero();
		let seal = loop {
			let compute = Compute { pre_hash, nonce };
			let work = compute.work();
			if hash_meets_difficulty(&work, easy_difficulty) {
				break compute.seal(easy_difficulty);
			}
			nonce = nonce.saturating_add(U256::one());
			assert!(nonce < U256::from(1_000_000));
		};

		let encoded = seal.encode();
		// Valid at easy difficulty
		assert_eq!(verify_seal(pre_hash, &encoded, easy_difficulty), Ok(true));
		// Rejected at much harder difficulty
		assert_eq!(verify_seal(pre_hash, &encoded, U256::MAX), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_tampered_work() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(10);

		let mut nonce = U256::zero();
		let mut seal = loop {
			let compute = Compute { pre_hash, nonce };
			let work = compute.work();
			if hash_meets_difficulty(&work, difficulty) {
				break compute.seal(difficulty);
			}
			nonce = nonce.saturating_add(U256::one());
			assert!(nonce < U256::from(1_000_000));
		};

		// Tamper with the nonce (work hash no longer matches)
		seal.nonce = seal.nonce.saturating_add(U256::one());
		let encoded = seal.encode();
		assert_eq!(verify_seal(pre_hash, &encoded, difficulty), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_wrong_pre_hash() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(10);

		let mut nonce = U256::zero();
		let seal = loop {
			let compute = Compute { pre_hash, nonce };
			let work = compute.work();
			if hash_meets_difficulty(&work, difficulty) {
				break compute.seal(difficulty);
			}
			nonce = nonce.saturating_add(U256::one());
			assert!(nonce < U256::from(1_000_000));
		};

		let encoded = seal.encode();
		let wrong_pre_hash = H256::from_low_u64_be(99999);
		assert_eq!(verify_seal(wrong_pre_hash, &encoded, difficulty), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_malformed_bytes() {
		let garbage = vec![0xDE, 0xAD];
		assert!(verify_seal(H256::zero(), &garbage, U256::from(1)).is_err());
	}
}
