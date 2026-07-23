//! PoScan scanning proof of work.
//!
//! The work function grows a deterministic asteroid from the block seed, scans
//! it with spectral3d into rotation invariant features, fine quantizes them, and
//! hashes the bucket vector into a full entropy work value. One nonce drives one
//! full generate and scan, so scanning is the proof of work.
//!
//! The seal layout, difficulty rule, and runtime verification framework match
//! the placeholder algorithm, so PoScan only replaces the work computation. Core
//! types stay `no_std` so the runtime can verify seals, keeping the algorithm
//! hot-swappable via runtime upgrade.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::vec::Vec;

use codec::{Decode, DecodeAll, Encode};
use sha2::{Digest, Sha256};
use primitive_types::{H256, U256};
use spectral3d::{Mesh, N_FEATURES, QUANT_STEP};

// ── Protocol parameters ─────────────────────────────────────────────

/// Icosphere subdivision level for the generated mesh, pinned so miners cannot
/// trade resolution for speed.
pub const SUBDIVISIONS: u32 = 4;

/// Surface sample count fed to the scan.
pub const TARGET_SAMPLES: usize = 4096;

/// Tightens the spectral identity bucket widths to a per-dim step near 1e-6. The
/// fine step fills work with full entropy and kills the low-res and surrogate
/// shortcuts.
pub const EPS_SCALE: f64 = 1e-5;

/// Domain separation prefix. 
/// 
/// Any parameter change must bump it so old and new work land in disjoint spaces.
pub const POSCAN_PROTOCOL: &[u8] = b"poscan-v2";

// ── Core types (no_std) ─────────────────────────────────────────────

/// Seal produced by the miner, encoded into `Vec<u8>` for the block digest.
#[derive(Clone, PartialEq, Eq, Encode, Decode, Debug)]
pub struct Seal {
	/// Nonce that satisfies the difficulty target.
	pub nonce: U256,
	/// Resulting scan hash.
	pub work: H256,
}

/// An un-evaluated mining attempt. Call [`Compute::work`] to run the scan.
#[derive(Clone, PartialEq, Eq, Encode, Decode, Debug)]
pub struct Compute {
	/// Block pre-hash (header hash without the seal digest). Commits to the miner
	/// address via the pre-runtime digest, so the seed binds the author without a
	/// separate field.
	pub pre_hash: H256,
	/// Candidate nonce.
	pub nonce: U256,
}

impl Compute {
	/// Grow the mesh this attempt scans. Work hashing and model export both
	/// derive geometry through here, so an exported model is the exact mesh the
	/// work value committed to.
	pub fn mesh(&self) -> Mesh {
		let seed = derive_seed(&self.pre_hash, &self.nonce);
		obj_asteroid::asteroid(seed, SUBDIVISIONS)
	}

	/// Run the PoScan pipeline for this attempt. Returns `None` when the
	/// generated mesh is structurally unscannable, which a valid seal never is.
	pub fn work(&self) -> Option<H256> {
		let features = spectral3d::scan(self.mesh(), TARGET_SAMPLES).ok()?;
		Some(hash_buckets(&quantize(&features), &self.pre_hash))
	}

	/// Run the scan and bundle the resulting work into a [`Seal`].
	pub fn seal(self) -> Option<Seal> {
		let work = self.work()?;
		Some(Seal { nonce: self.nonce, work })
	}
}

/// A generated mesh crossing from the runtime to the node for model export.
/// Holds the exact vertices and faces the work scanned. Spectral coordinates
/// never enter the runtime metadata type system, so this rides the runtime API
/// as its encoded bytes rather than a typed return. The node formats it as OBJ.
#[derive(Clone, PartialEq, Encode, Decode, Debug)]
pub struct WireMesh {
	/// Vertex positions.
	pub vertices: Vec<[f64; 3]>,
	/// Triangle faces holding indices into `vertices`, counted from zero.
	pub faces: Vec<[u32; 3]>,
}

impl From<Mesh> for WireMesh {
	fn from(mesh: Mesh) -> Self {
		let Mesh { vertices, faces } = mesh;
		Self { vertices, faces }
	}
}

/// Derive the generator seed from the full digest, never a truncation.
///
/// Truncating caps the work domain at the truncated width, and a capped domain is
/// enumerable. A miner scans it offline once, keeps the seeds whose work clears
/// difficulty, then reaches them by hashing nonces instead of scanning. The edge
/// follows table size and holds at every difficulty. Retargeting cannot correct
/// for it.
///
/// The protocol tag goes in for the same reason it goes into the work value.
/// Two protocol revisions sharing one seed space would share their asteroids,
/// letting the generate and scan labour spent under one revision replay under
/// the other for the price of one bucket hash.
fn derive_seed(pre_hash: &H256, nonce: &U256) -> [u8; 32] {
	let mut hasher = Sha256::new();
	hasher.update(POSCAN_PROTOCOL);
	hasher.update([0u8]);
	hasher.update(pre_hash.as_bytes());
	hasher.update(nonce.encode());
	let mut seed = [0u8; 32];
	seed.copy_from_slice(&hasher.finalize());
	seed
}

/// Fine quantize the raw spectral features into the integer bucket vector.
fn quantize(features: &[f64; N_FEATURES]) -> [i64; N_FEATURES] {
	let mut buckets = [0i64; N_FEATURES];
	for i in 0..N_FEATURES {
		buckets[i] = libm::round(features[i] / (QUANT_STEP[i] * EPS_SCALE)) as i64;
	}
	buckets
}

/// Hash the bucket vector into the work value, prefixed for domain separation.
///
/// The pre-hash goes in directly, not only through the seed. Work stays
/// block-specific even if a later generator narrows its state, so a scanned seed
/// table never outlives one block.
fn hash_buckets(buckets: &[i64; N_FEATURES], pre_hash: &H256) -> H256 {
	let mut hasher = Sha256::new();
	hasher.update(POSCAN_PROTOCOL);
	hasher.update([0u8]);
	hasher.update(pre_hash.as_bytes());
	for &bucket in buckets {
		hasher.update(bucket.to_le_bytes());
	}
	H256::from_slice(&hasher.finalize())
}

/// Returns `true` when `hash` satisfies `difficulty`.
///
/// The check multiplies the numeric value of the hash by the difficulty. An
/// overflow past `U256::MAX` means the hash is too large, so the work was not
/// sufficient.
pub fn hash_meets_difficulty(hash: &H256, difficulty: U256) -> bool {
	let num_hash = U256::from_big_endian(hash.as_bytes());
	let (_, overflowed) = num_hash.overflowing_mul(difficulty);
	!overflowed
}

/// Standalone seal verification usable from both runtime and node.
///
/// Returns `Ok(true)` only when the seal decodes, its work meets the difficulty
/// target, and replaying the scan from `pre_hash` and `nonce` reproduces it.
pub fn verify_seal(pre_hash: H256, raw_seal: &[u8], difficulty: U256) -> Result<bool, codec::Error> {
	let seal = Seal::decode_all(&mut &raw_seal[..])?;

	if !hash_meets_difficulty(&seal.work, difficulty) {
		return Ok(false);
	}

	let compute = Compute { pre_hash, nonce: seal.nonce };
	Ok(compute.work() == Some(seal.work))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Mine a valid seal at a low difficulty, scanning successive nonces.
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
			assert!(nonce < U256::from(1_000u64), "should find a seal quickly at low difficulty");
		}
	}

	#[test]
	fn work_is_reproducible() {
		let c = Compute { pre_hash: H256::from_low_u64_be(42), nonce: U256::from(1) };
		assert!(c.work().is_some());
		assert_eq!(c.work(), c.work());
	}

	#[test]
	fn mesh_is_reproducible() {
		let c = Compute { pre_hash: H256::from_low_u64_be(42), nonce: U256::from(1) };
		assert_eq!(WireMesh::from(c.mesh()), WireMesh::from(c.mesh()));
	}

	#[test]
	fn wire_mesh_roundtrips() {
		let c = Compute { pre_hash: H256::from_low_u64_be(7), nonce: U256::from(3) };
		let wire = WireMesh::from(c.mesh());
		assert!(!wire.vertices.is_empty() && !wire.faces.is_empty());
		let decoded = WireMesh::decode_all(&mut &wire.encode()[..]).expect("wire mesh decodes");
		assert_eq!(wire, decoded);
	}

	#[test]
	fn distinct_nonces_diverge() {
		let pre_hash = H256::from_low_u64_be(7);
		let a = Compute { pre_hash, nonce: U256::from(1) }.work();
		let b = Compute { pre_hash, nonce: U256::from(2) }.work();
		assert_ne!(a, b);
	}

	#[test]
	fn verify_seal_accepts_valid_seal() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(1);
		let seal = mine(pre_hash, difficulty);
		assert_eq!(verify_seal(pre_hash, &seal.encode(), difficulty), Ok(true));
	}

	#[test]
	fn verify_seal_rejects_insufficient_difficulty() {
		let pre_hash = H256::from_low_u64_be(12345);
		let easy = U256::from(1);
		let seal = mine(pre_hash, easy);
		assert_eq!(verify_seal(pre_hash, &seal.encode(), easy), Ok(true));
		assert_eq!(verify_seal(pre_hash, &seal.encode(), U256::MAX), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_tampered_nonce() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(1);
		let mut seal = mine(pre_hash, difficulty);
		seal.nonce = seal.nonce.saturating_add(U256::one());
		assert_eq!(verify_seal(pre_hash, &seal.encode(), difficulty), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_tampered_work() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(1);
		let mut seal = mine(pre_hash, difficulty);
		// Zero always meets the difficulty yet cannot match the replayed scan.
		seal.work = H256::zero();
		assert_eq!(verify_seal(pre_hash, &seal.encode(), difficulty), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_wrong_pre_hash() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(1);
		let seal = mine(pre_hash, difficulty);
		let wrong = H256::from_low_u64_be(99999);
		assert_eq!(verify_seal(wrong, &seal.encode(), difficulty), Ok(false));
	}

	#[test]
	fn verify_seal_rejects_malformed_bytes() {
		let garbage = [0xDEu8, 0xAD];
		assert!(verify_seal(H256::zero(), &garbage, U256::from(1)).is_err());
	}

	#[test]
	fn verify_seal_rejects_trailing_bytes() {
		let pre_hash = H256::from_low_u64_be(12345);
		let difficulty = U256::from(1);
		let seal = mine(pre_hash, difficulty);
		let mut bytes = seal.encode();
		bytes.push(0xFF);
		assert!(verify_seal(pre_hash, &bytes, difficulty).is_err());
	}
}
