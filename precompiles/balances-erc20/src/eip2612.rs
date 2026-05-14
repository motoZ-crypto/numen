// EIP-2612 permit (gasless approvals) implementation.
//
// Spec: https://eips.ethereum.org/EIPS/eip-2612
//
// Anyone may submit `permit(...)` on behalf of the owner; we only check
// that the EIP-712 signature recovers to the owner's address.

use core::marker::PhantomData;

use fp_evm::PrecompileHandle;
use precompile_utils::prelude::*;
use sp_core::{Get, H160, H256, U256};

use crate::{Allowances, Erc20Metadata, Nonces, SELECTOR_LOG_APPROVAL};

/// EIP-2612 contract version string used in the EIP-712 domain separator.
const PERMIT_VERSION: &[u8] = b"1";

pub struct Eip2612<R, Metadata>(PhantomData<(R, Metadata)>);

impl<R, Metadata> Eip2612<R, Metadata>
where
	R: pallet_evm::Config + pallet_timestamp::Config<Moment = u64>,
	Metadata: Erc20Metadata + 'static,
{
	pub fn nonces(_handle: &mut impl PrecompileHandle, owner: Address) -> EvmResult<U256> {
		Ok(Nonces::get(H160::from(owner)))
	}

	pub fn domain_separator(handle: &mut impl PrecompileHandle) -> EvmResult<H256> {
		let address = handle.context().address;
		let chain_id = <R as pallet_evm::Config>::ChainId::get();
		Ok(H256(compute_domain_separator(address, chain_id, Metadata::NAME.as_bytes())))
	}

	#[allow(clippy::too_many_arguments)]
	pub fn permit(
		handle: &mut impl PrecompileHandle,
		owner: Address,
		spender: Address,
		value: U256,
		deadline: U256,
		v: u8,
		r: H256,
		s: H256,
	) -> EvmResult {
		handle.record_log_costs_manual(3, 32)?;

		let owner: H160 = owner.into();
		let spender: H160 = spender.into();

		// Deadline is interpreted in seconds (per EIP-2612). Compare with
		// the current block timestamp (millis) divided by 1000.
		let now_ms: u64 = pallet_timestamp::Pallet::<R>::get();
		let now_secs = now_ms.saturating_div(1000);
		let deadline_secs: u64 = deadline.try_into().unwrap_or(u64::MAX);
		if now_secs > deadline_secs {
			return Err(revert("Permit expired"));
		}

		let nonce = Nonces::get(owner);
		let address = handle.context().address;
		let chain_id = <R as pallet_evm::Config>::ChainId::get();
		let ds = compute_domain_separator(address, chain_id, Metadata::NAME.as_bytes());
		let struct_hash = compute_permit_struct_hash(owner, spender, value, nonce, deadline);
		let digest = compute_eip712_digest(&ds, &struct_hash);

		let recovered = ecrecover(&digest, v, &r.0, &s.0).ok_or_else(|| revert("Invalid permit"))?;
		if recovered != owner {
			return Err(revert("Invalid permit"));
		}

		// Bump nonce *before* writing the allowance to make replay impossible.
		Nonces::insert(owner, nonce.saturating_add(U256::one()));
		Allowances::insert(owner, spender, value);

		log3(
			address,
			SELECTOR_LOG_APPROVAL,
			owner,
			spender,
			solidity::encode_event_data(value),
		)
		.record(handle)?;

		Ok(())
	}
}

// ---------------------------------------------------------------------------
// EIP-712 helpers
// ---------------------------------------------------------------------------

/// `keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")`
fn eip712_domain_typehash() -> [u8; 32] {
	sp_io::hashing::keccak_256(
		b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
	)
}

/// `keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)")`
fn permit_typehash() -> [u8; 32] {
	sp_io::hashing::keccak_256(
		b"Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)",
	)
}

/// Build the EIP-712 domain separator for this precompile instance.
///
/// `name` is the ERC20 token name as bytes (e.g. `b"UNIT"`).
pub fn compute_domain_separator(verifying_contract: H160, chain_id: u64, name: &[u8]) -> [u8; 32] {
	let mut buf = [0u8; 32 * 5];
	buf[0..32].copy_from_slice(&eip712_domain_typehash());
	buf[32..64].copy_from_slice(&sp_io::hashing::keccak_256(name));
	buf[64..96].copy_from_slice(&sp_io::hashing::keccak_256(PERMIT_VERSION));
	let cid = U256::from(chain_id).to_big_endian();
	buf[96..128].copy_from_slice(&cid);
	// Address is left-padded with 12 zero bytes inside its 32-byte slot.
	buf[128 + 12..160].copy_from_slice(verifying_contract.as_bytes());
	sp_io::hashing::keccak_256(&buf)
}

/// `keccak256(abi.encode(PERMIT_TYPEHASH, owner, spender, value, nonce, deadline))`
pub fn compute_permit_struct_hash(
	owner: H160,
	spender: H160,
	value: U256,
	nonce: U256,
	deadline: U256,
) -> [u8; 32] {
	let mut buf = [0u8; 32 * 6];
	buf[0..32].copy_from_slice(&permit_typehash());
	buf[32 + 12..64].copy_from_slice(owner.as_bytes());
	buf[64 + 12..96].copy_from_slice(spender.as_bytes());
	buf[96..128].copy_from_slice(&value.to_big_endian());
	buf[128..160].copy_from_slice(&nonce.to_big_endian());
	buf[160..192].copy_from_slice(&deadline.to_big_endian());
	sp_io::hashing::keccak_256(&buf)
}

/// `keccak256("\x19\x01" || domain_separator || struct_hash)`
pub fn compute_eip712_digest(domain_separator: &[u8; 32], struct_hash: &[u8; 32]) -> [u8; 32] {
	let mut buf = [0u8; 2 + 32 + 32];
	buf[0] = 0x19;
	buf[1] = 0x01;
	buf[2..34].copy_from_slice(domain_separator);
	buf[34..66].copy_from_slice(struct_hash);
	sp_io::hashing::keccak_256(&buf)
}

/// Recover the signer address from an EIP-712 digest.
fn ecrecover(digest: &[u8; 32], v: u8, r: &[u8; 32], s: &[u8; 32]) -> Option<H160> {
	let recovery_id = match v {
		27 => 0u8,
		28 => 1u8,
		// Some wallets sign with raw 0/1 — accept those too.
		0 | 1 => v,
		_ => return None,
	};
	let mut sig = [0u8; 65];
	sig[..32].copy_from_slice(r);
	sig[32..64].copy_from_slice(s);
	sig[64] = recovery_id;
	let pubkey = sp_io::crypto::secp256k1_ecdsa_recover(&sig, digest).ok()?;
	let hash = sp_io::hashing::keccak_256(&pubkey);
	Some(H160::from_slice(&hash[12..]))
}
