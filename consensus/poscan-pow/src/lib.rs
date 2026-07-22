//! Runtime-delegated PoW consensus for the PoScan chain.
//!
//! Bundles the node-side `PowAlgorithm` with the runtime `PowVerifyApi` it calls.
//! Difficulty and seal verification both route through the runtime, so a runtime
//! upgrade can swap the PoW scheme without restarting the node.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use sp_core::{H256, U256};

// ── Runtime API declaration (no_std) ────────────────────────────────

sp_api::decl_runtime_apis! {
	/// Runtime-side seal verification API. Implementing it in the runtime keeps
	/// the PoW algorithm hot-swappable through a runtime upgrade.
	pub trait PowVerifyApi {
		/// Verify a raw seal against the pre-hash and difficulty target.
		fn verify_seal(pre_hash: H256, seal: alloc::vec::Vec<u8>, difficulty: U256) -> bool;

		/// Rebuild the miner mesh for a pre-hash and nonce, returned as an encoded
		/// `poscan::WireMesh`. Run at a block's parent, this replays the exact
		/// generator that verified the block, so an exported model stays faithful
		/// even after a generator upgrade. Spectral coordinates never enter the
		/// runtime metadata type system, so the mesh rides back as encoded bytes.
		fn generate_mesh(pre_hash: H256, nonce: U256) -> alloc::vec::Vec<u8>;
	}
}

// ── PowAlgorithm implementation (std only) ──────────────────────────

#[cfg(feature = "std")]
use pallet_difficulty::DifficultyApi;
#[cfg(feature = "std")]
use sc_consensus_pow::{Error as PowError, PowAlgorithm};
#[cfg(feature = "std")]
use sp_api::ProvideRuntimeApi;
#[cfg(feature = "std")]
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
#[cfg(feature = "std")]
use std::{marker::PhantomData, sync::Arc};

/// PoScan PoW algorithm backed by the runtime `DifficultyApi` and `PowVerifyApi`.
///
/// Verification is delegated to the runtime so the algorithm can be replaced by
/// a runtime upgrade without restarting the node.
#[cfg(feature = "std")]
pub struct PoScanAlgorithm<B: BlockT, C> {
	client: Arc<C>,
	_phantom: PhantomData<B>,
}

#[cfg(feature = "std")]
impl<B: BlockT, C> PoScanAlgorithm<B, C> {
	/// Create a new algorithm instance backed by the given client.
	pub fn new(client: Arc<C>) -> Self {
		Self { client, _phantom: PhantomData }
	}
}

#[cfg(feature = "std")]
impl<B: BlockT, C> Clone for PoScanAlgorithm<B, C> {
	fn clone(&self) -> Self {
		Self { client: self.client.clone(), _phantom: PhantomData }
	}
}

#[cfg(feature = "std")]
impl<B, C> PowAlgorithm<B> for PoScanAlgorithm<B, C>
where
	B: BlockT<Hash = H256>,
	C: ProvideRuntimeApi<B> + sp_blockchain::HeaderBackend<B> + Send + Sync,
	C::Api: DifficultyApi<B> + PowVerifyApi<B>,
{
	type Difficulty = U256;

	fn difficulty(&self, parent: B::Hash, timestamp_inherent: &[u8]) -> Result<U256, PowError<B>> {
		self.client
			.runtime_api()
			.difficulty_for_block(parent, timestamp_inherent.to_vec())
			.map_err(|err| PowError::Environment(format!(
				"Fetching difficulty from runtime failed: {err:?}"
			)))?
			.ok_or_else(|| {
				PowError::Runtime("Block carries no valid timestamp inherent".into())
			})
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
