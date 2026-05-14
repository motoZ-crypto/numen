//! ERC20 facade over the runtime's native balance pallet.
//!
//! Deployed at `0x0000000000000000000000000000000000000802` (matching the
//! Moonbeam / 3Dpass convention). Lets EVM-side users (MetaMask, Solidity
//! contracts) read and move the chain's native token without leaving the
//! EVM environment.
//!
//! ## Address mapping
//!
//! Funds shown by `balanceOf` are the free balance of the substrate account
//! that the configured [`pallet_evm::Config::AddressMapping`] derives from
//! the H160 (with the default `HashedAddressMapping`, this is
//! `blake2_256("evm:" ++ h160)`).
//!
//! ## Functions
//!
//! Standard ERC20: `name`, `symbol`, `decimals`, `totalSupply`, `balanceOf`,
//! `transfer`, `allowance`, `approve`, `transferFrom`.
//!
//! Bridge helper: `withdraw(bytes32 dest, uint256 amount)` lets an EVM
//! caller move funds from its mirror substrate account to an arbitrary
//! `AccountId32` destination.
//!
//! WETH9 deposit: a value-bearing call with empty calldata, the explicit
//! `deposit()` selector, or any unknown selector with attached value, is
//! treated as a deposit (refund pattern; see `Erc20BalancesPrecompile::deposit`).
//!
//! EIP-2612 permit: `nonces`, `DOMAIN_SEPARATOR`, `permit` for gasless
//! approvals.
//!
//! ## Allowances
//!
//! Allowances are kept in a precompile-owned storage map keyed under the
//! pseudo-pallet prefix `EvmBalancesErc20`. They are not part of any pallet
//! metadata but are deterministic and can be queried by raw state reads.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

mod eip2612;

use core::marker::PhantomData;

use fp_evm::PrecompileHandle;
use frame_support::{
	storage::types::{StorageDoubleMap, StorageMap, ValueQuery},
	traits::{
		tokens::{currency::Currency, ExistenceRequirement},
		StorageInstance,
	},
	Blake2_128Concat,
};
use pallet_evm::AddressMapping;
use precompile_utils::prelude::*;
use sp_core::{H160, H256, U256};
use sp_runtime::traits::Zero;

pub use eip2612::{compute_domain_separator, compute_eip712_digest, compute_permit_struct_hash, Eip2612};

/// Compile-time ERC20 metadata injected by the runtime.
///
/// Mirrors Moonbeam's `Erc20Metadata` so a single precompile can serve
/// multiple ERC20 facades (different name/symbol/decimals) by varying the
/// `Metadata` type parameter.
pub trait Erc20Metadata {
	/// Token name returned by `name()`.
	const NAME: &'static str;
	/// Token symbol returned by `symbol()`.
	const SYMBOL: &'static str;
	/// Token decimals returned by `decimals()`. Must match the runtime's
	/// `Balance` decimal layout.
	const DECIMALS: u8;
}

/// Selector of the `Transfer(address,address,uint256)` log topic.
pub const SELECTOR_LOG_TRANSFER: [u8; 32] =
	keccak256!("Transfer(address,address,uint256)");
/// Selector of the `Approval(address,address,uint256)` log topic.
pub const SELECTOR_LOG_APPROVAL: [u8; 32] =
	keccak256!("Approval(address,address,uint256)");
/// Selector of the `Deposit(address,uint256)` log topic.
pub const SELECTOR_LOG_DEPOSIT: [u8; 32] = keccak256!("Deposit(address,uint256)");
/// Selector of the custom `Withdrawal(address,bytes32,uint256)` log topic.
///
/// Differs from the WETH9 `Withdrawal(address,uint256)` topic because our
/// `withdraw` helper takes a substrate destination rather than implicitly
/// using `msg.sender`.
pub const SELECTOR_LOG_WITHDRAWAL: [u8; 32] =
	keccak256!("Withdrawal(address,bytes32,uint256)");

// Storage prefix for the allowance map. Uses an ad-hoc `StorageInstance`
// rather than a pallet so we don't need a `construct_runtime!` entry; the
// map still lives at a deterministic location in state.
pub struct AllowancesPrefix;
impl StorageInstance for AllowancesPrefix {
	const STORAGE_PREFIX: &'static str = "Allowances";
	fn pallet_prefix() -> &'static str {
		"EvmBalancesErc20"
	}
}

/// `(owner, spender) -> allowance`. ValueQuery returns 0 when missing.
pub type Allowances = StorageDoubleMap<
	AllowancesPrefix,
	Blake2_128Concat,
	H160,
	Blake2_128Concat,
	H160,
	U256,
	ValueQuery,
>;

// Storage prefix for the EIP-2612 nonces map. Same convention as
// `AllowancesPrefix`: lives under a stable pseudo-pallet prefix so that
// indexers can read it deterministically.
pub struct NoncesPrefix;
impl StorageInstance for NoncesPrefix {
	const STORAGE_PREFIX: &'static str = "Nonces";
	fn pallet_prefix() -> &'static str {
		"EvmBalancesErc20"
	}
}

/// `owner -> nonce`. ValueQuery returns 0 for first-time owners.
pub type Nonces = StorageMap<NoncesPrefix, Blake2_128Concat, H160, U256, ValueQuery>;

type CurrencyOf<R> = <R as pallet_evm::Config>::Currency;
type AccountIdOf<R> = pallet_evm::AccountIdOf<R>;
type BalanceOf<R> = <CurrencyOf<R> as Currency<AccountIdOf<R>>>::Balance;

/// EVM precompile exposing native balances as ERC20.
pub struct Erc20BalancesPrecompile<R, Metadata>(PhantomData<(R, Metadata)>);

#[precompile_utils::precompile]
impl<R, Metadata> Erc20BalancesPrecompile<R, Metadata>
where
	R: pallet_evm::Config + pallet_timestamp::Config<Moment = u64>,
	Metadata: Erc20Metadata + 'static,
	CurrencyOf<R>: Currency<AccountIdOf<R>>,
	AccountIdOf<R>: From<[u8; 32]>,
	BalanceOf<R>: TryFrom<U256> + Into<U256>,
{
	#[precompile::public("name()")]
	#[precompile::view]
	fn name(_handle: &mut impl PrecompileHandle) -> EvmResult<UnboundedBytes> {
		Ok(UnboundedBytes::from(Metadata::NAME.as_bytes()))
	}

	#[precompile::public("symbol()")]
	#[precompile::view]
	fn symbol(_handle: &mut impl PrecompileHandle) -> EvmResult<UnboundedBytes> {
		Ok(UnboundedBytes::from(Metadata::SYMBOL.as_bytes()))
	}

	#[precompile::public("decimals()")]
	#[precompile::view]
	fn decimals(_handle: &mut impl PrecompileHandle) -> EvmResult<u8> {
		Ok(Metadata::DECIMALS)
	}

	#[precompile::public("totalSupply()")]
	#[precompile::view]
	fn total_supply(_handle: &mut impl PrecompileHandle) -> EvmResult<U256> {
		Ok(CurrencyOf::<R>::total_issuance().into())
	}

	#[precompile::public("balanceOf(address)")]
	#[precompile::view]
	fn balance_of(_handle: &mut impl PrecompileHandle, owner: Address) -> EvmResult<U256> {
		let owner: H160 = owner.into();
		let account = <R as pallet_evm::Config>::AddressMapping::into_account_id(owner);
		Ok(CurrencyOf::<R>::free_balance(&account).into())
	}

	#[precompile::public("allowance(address,address)")]
	#[precompile::view]
	fn allowance(
		_handle: &mut impl PrecompileHandle,
		owner: Address,
		spender: Address,
	) -> EvmResult<U256> {
		Ok(Allowances::get(H160::from(owner), H160::from(spender)))
	}

	#[precompile::public("approve(address,uint256)")]
	fn approve(
		handle: &mut impl PrecompileHandle,
		spender: Address,
		value: U256,
	) -> EvmResult<bool> {
		handle.record_log_costs_manual(3, 32)?;

		let spender: H160 = spender.into();
		let owner = handle.context().caller;

		Allowances::insert(owner, spender, value);

		log3(
			handle.context().address,
			SELECTOR_LOG_APPROVAL,
			owner,
			spender,
			solidity::encode_event_data(value),
		)
		.record(handle)?;

		Ok(true)
	}

	#[precompile::public("transfer(address,uint256)")]
	fn transfer(
		handle: &mut impl PrecompileHandle,
		to: Address,
		value: U256,
	) -> EvmResult<bool> {
		handle.record_log_costs_manual(3, 32)?;

		let to: H160 = to.into();
		let from = handle.context().caller;

		do_transfer::<R>(from, to, value)?;

		log3(
			handle.context().address,
			SELECTOR_LOG_TRANSFER,
			from,
			to,
			solidity::encode_event_data(value),
		)
		.record(handle)?;

		Ok(true)
	}

	#[precompile::public("transferFrom(address,address,uint256)")]
	fn transfer_from(
		handle: &mut impl PrecompileHandle,
		from: Address,
		to: Address,
		value: U256,
	) -> EvmResult<bool> {
		handle.record_log_costs_manual(3, 32)?;

		let from: H160 = from.into();
		let to: H160 = to.into();
		let spender = handle.context().caller;

		// If owner == spender, skip the allowance check (matches the
		// canonical OpenZeppelin ERC20 behaviour).
		if from != spender {
			let current = Allowances::get(from, spender);
			let new_allowance = current
				.checked_sub(value)
				.ok_or_else(|| revert("ERC20: insufficient allowance"))?;
			Allowances::insert(from, spender, new_allowance);
		}

		do_transfer::<R>(from, to, value)?;

		log3(
			handle.context().address,
			SELECTOR_LOG_TRANSFER,
			from,
			to,
			solidity::encode_event_data(value),
		)
		.record(handle)?;

		Ok(true)
	}

	/// `withdraw(bytes32 dest, uint256 amount)` — moves `amount` from the
	/// caller's mirror account to the substrate account whose 32-byte
	/// representation is `dest`.
	#[precompile::public("withdraw(bytes32,uint256)")]
	fn withdraw(
		handle: &mut impl PrecompileHandle,
		dest: H256,
		value: U256,
	) -> EvmResult<bool> {
		handle.record_log_costs_manual(3, 32)?;

		let caller = handle.context().caller;
		let from = <R as pallet_evm::Config>::AddressMapping::into_account_id(caller);
		let to: AccountIdOf<R> = dest.0.into();

		let amount_t = u256_to_balance::<R>(value)?;
		if !amount_t.is_zero() {
			CurrencyOf::<R>::transfer(&from, &to, amount_t, ExistenceRequirement::AllowDeath)
				.map_err(|_| revert("ERC20: transfer failed"))?;
		}

		log3(
			handle.context().address,
			SELECTOR_LOG_WITHDRAWAL,
			caller,
			dest,
			solidity::encode_event_data(value),
		)
		.record(handle)?;

		Ok(true)
	}

	/// `deposit()` / fallback / receive — implements the WETH9-style
	/// pattern where an EVM call carrying value is treated as a deposit.
	///
	/// The EVM has already moved `apparent_value` from the caller to this
	/// precompile's substrate-mapped account before invoking us; we
	/// immediately refund it so the net balance change is zero and
	/// `balanceOf(caller)` is left untouched.
	#[precompile::public("deposit()")]
	#[precompile::fallback]
	#[precompile::payable]
	fn deposit(handle: &mut impl PrecompileHandle) -> EvmResult {
		let caller = handle.context().caller;
		let address = handle.context().address;
		let value = handle.context().apparent_value;

		// A zero-value deposit is a no-op selector mistake; refuse it so
		// the caller learns rather than silently accepting an empty call.
		if value.is_zero() {
			return Err(revert("ERC20: cannot deposit zero"));
		}

		handle.record_log_costs_manual(2, 32)?;

		let amount_t = u256_to_balance::<R>(value)?;
		let from = <R as pallet_evm::Config>::AddressMapping::into_account_id(address);
		let to = <R as pallet_evm::Config>::AddressMapping::into_account_id(caller);
		CurrencyOf::<R>::transfer(&from, &to, amount_t, ExistenceRequirement::AllowDeath)
			.map_err(|_| revert("ERC20: deposit refund failed"))?;

		log2(
			address,
			SELECTOR_LOG_DEPOSIT,
			caller,
			solidity::encode_event_data(value),
		)
		.record(handle)?;

		Ok(())
	}

	// ---------------------------------------------------------------------
	// EIP-2612 permit
	// ---------------------------------------------------------------------

	#[precompile::public("nonces(address)")]
	#[precompile::view]
	fn eip2612_nonces(handle: &mut impl PrecompileHandle, owner: Address) -> EvmResult<U256> {
		Eip2612::<R, Metadata>::nonces(handle, owner)
	}

	#[precompile::public("DOMAIN_SEPARATOR()")]
	#[precompile::view]
	fn eip2612_domain_separator(handle: &mut impl PrecompileHandle) -> EvmResult<H256> {
		Eip2612::<R, Metadata>::domain_separator(handle)
	}

	#[precompile::public("permit(address,address,uint256,uint256,uint8,bytes32,bytes32)")]
	#[allow(clippy::too_many_arguments)]
	fn eip2612_permit(
		handle: &mut impl PrecompileHandle,
		owner: Address,
		spender: Address,
		value: U256,
		deadline: U256,
		v: u8,
		r: H256,
		s: H256,
	) -> EvmResult {
		Eip2612::<R, Metadata>::permit(handle, owner, spender, value, deadline, v, r, s)
	}
}

// --- shared helpers -------------------------------------------------------

pub(crate) fn do_transfer<R>(from: H160, to: H160, amount: U256) -> EvmResult<()>
where
	R: pallet_evm::Config,
	CurrencyOf<R>: Currency<AccountIdOf<R>>,
	BalanceOf<R>: TryFrom<U256>,
{
	let from_acc = <R as pallet_evm::Config>::AddressMapping::into_account_id(from);
	let to_acc = <R as pallet_evm::Config>::AddressMapping::into_account_id(to);
	let amount_t = u256_to_balance::<R>(amount)?;

	if amount_t.is_zero() {
		// Zero-value transfers must succeed without touching balances.
		return Ok(());
	}

	CurrencyOf::<R>::transfer(&from_acc, &to_acc, amount_t, ExistenceRequirement::AllowDeath)
		.map_err(|_| revert("ERC20: transfer failed"))?;
	Ok(())
}

pub(crate) fn u256_to_balance<R>(amount: U256) -> EvmResult<BalanceOf<R>>
where
	R: pallet_evm::Config,
	CurrencyOf<R>: Currency<AccountIdOf<R>>,
	BalanceOf<R>: TryFrom<U256>,
{
	BalanceOf::<R>::try_from(amount).map_err(|_| revert("ERC20: amount overflow"))
}
