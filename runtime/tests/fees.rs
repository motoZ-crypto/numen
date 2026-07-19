//! Fee routing. Substrate fees and EVM base fee plus tip all land on the PoW
//! author from the block digest, and burn when a block carries no author.

mod common;

use codec::Encode;
use common::new_test_ext;
use frame_support::traits::{
	tokens::{
		fungible::{Balanced, Mutate},
		Fortitude, Precision, Preservation,
	},
	OnUnbalanced,
};
use numen_runtime::{
	configs::DealWithFees, AccountId, Balance, Balances, Runtime, System, UNIT,
};
use pallet_evm::{AddressMapping, FeeCalculator, Runner};
use sp_consensus_pow::POW_ENGINE_ID;
use sp_core::{H160, U256};
use sp_keyring::Sr25519Keyring;
use sp_runtime::DigestItem;

/// Two gwei, comfortably above the 1 gwei base fee plus the tip below.
const MAX_FEE_PER_GAS: u64 = 2_000_000_000;
const TIP_PER_GAS: u64 = 500_000_000;
const GAS_LIMIT: u64 = 100_000;
const CALLER_FUNDS: Balance = 1_000 * UNIT;

fn miner() -> AccountId {
	Sr25519Keyring::Eve.to_account_id()
}

/// Stamp the block digest with a PoW author, the way both miners do.
fn set_pow_author(author: &AccountId) {
	System::deposit_log(DigestItem::PreRuntime(POW_ENGINE_ID, author.encode()));
}

fn evm_account(addr: H160) -> AccountId {
	<Runtime as pallet_evm::Config>::AddressMapping::into_account_id(addr)
}

/// Withdraw `fee` from the payer exactly like `FungibleAdapter` does before it
/// hands the credit to the fee sink.
fn fee_credit(
	payer: &AccountId,
	fee: Balance,
) -> frame_support::traits::fungible::Credit<AccountId, Balances> {
	<Balances as Balanced<AccountId>>::withdraw(
		payer,
		fee,
		Precision::Exact,
		Preservation::Expendable,
		Fortitude::Polite,
	)
	.expect("payer covers the fee")
}

/// A transactional EVM call to a plain address, paying a tip on top of the
/// base fee.
fn evm_plain_call_with_tip(caller: H160) -> fp_evm::CallInfo {
	<Runtime as pallet_evm::Config>::Runner::call(
		caller,
		H160::from_low_u64_be(0xD00D),
		Vec::new(),
		U256::zero(),
		GAS_LIMIT,
		Some(U256::from(MAX_FEE_PER_GAS)),
		Some(U256::from(TIP_PER_GAS)),
		None,
		Vec::new(),
		Vec::new(),
		true,
		true,
		None,
		None,
		None,
		<Runtime as pallet_evm::Config>::config(),
	)
	.expect("plain call must dispatch without runtime error")
}

#[test]
fn substrate_fees_credit_the_block_author() {
	new_test_ext().execute_with(|| {
		let author = miner();
		set_pow_author(&author);
		let payer = Sr25519Keyring::Alice.to_account_id();
		Balances::set_balance(&payer, CALLER_FUNDS);
		let issuance_before = Balances::total_issuance();
		let fee = UNIT;

		DealWithFees::on_nonzero_unbalanced(fee_credit(&payer, fee));

		assert_eq!(
			Balances::free_balance(&author),
			fee,
			"the whole fee lands on the digest author",
		);
		assert_eq!(
			Balances::total_issuance(),
			issuance_before,
			"nothing burns when the block has an author",
		);
	});
}

#[test]
fn substrate_fees_burn_without_author_digest() {
	new_test_ext().execute_with(|| {
		let payer = Sr25519Keyring::Alice.to_account_id();
		Balances::set_balance(&payer, CALLER_FUNDS);
		let issuance_before = Balances::total_issuance();
		let fee = UNIT;

		DealWithFees::on_nonzero_unbalanced(fee_credit(&payer, fee));

		assert_eq!(
			Balances::total_issuance(),
			issuance_before - fee,
			"an authorless fee credit burns in full",
		);
	});
}

#[test]
fn evm_base_fee_and_tip_credit_the_block_author() {
	new_test_ext().execute_with(|| {
		let author = miner();
		set_pow_author(&author);
		let caller = H160::from_low_u64_be(0xFEE1);
		let caller_acc = evm_account(caller);
		Balances::set_balance(&caller_acc, CALLER_FUNDS);
		let issuance_before = Balances::total_issuance();
		let (base_fee, _) = <Runtime as pallet_evm::Config>::FeeCalculator::min_gas_price();

		let info = evm_plain_call_with_tip(caller);
		assert!(info.exit_reason.is_succeed(), "unexpected exit: {:?}", info.exit_reason);

		let caller_spent = CALLER_FUNDS - Balances::free_balance(&caller_acc);
		let author_gain = Balances::free_balance(&author);
		assert_eq!(
			U256::from(author_gain),
			info.used_gas.effective * (base_fee + U256::from(TIP_PER_GAS)),
			"the author collects gas times base fee plus tip",
		);
		assert_eq!(author_gain, caller_spent, "every wei the caller pays reaches the author");
		assert_eq!(
			Balances::free_balance(evm_account(H160::zero())),
			0,
			"the tip must bypass the zero coinbase the default handler pays",
		);
		assert_eq!(Balances::total_issuance(), issuance_before, "nothing burns");
	});
}

#[test]
fn evm_fees_burn_without_author_digest() {
	new_test_ext().execute_with(|| {
		let caller = H160::from_low_u64_be(0xFEE1);
		let caller_acc = evm_account(caller);
		Balances::set_balance(&caller_acc, CALLER_FUNDS);
		let issuance_before = Balances::total_issuance();

		let info = evm_plain_call_with_tip(caller);
		assert!(info.exit_reason.is_succeed(), "unexpected exit: {:?}", info.exit_reason);

		let caller_spent = CALLER_FUNDS - Balances::free_balance(&caller_acc);
		assert!(caller_spent > 0);
		assert_eq!(
			Balances::total_issuance(),
			issuance_before - caller_spent,
			"base fee and tip both burn in an authorless block",
		);
	});
}
