use crate::mock::*;
use codec::Encode;
use frame_support::traits::Hooks;
use sp_keyring::Sr25519Keyring;
use sp_runtime::DigestItem;

// Reward issuance tests.

#[test]
fn mints_reward_to_block_author() {
	new_test_ext().execute_with(|| {
		let miner = Sr25519Keyring::Alice.to_account_id();
		set_author_digest(&miner);

		crate::pallet::Pallet::<Test>::on_finalize(1);

		assert_eq!(
			pallet_balances::Pallet::<Test>::free_balance(miner),
			50_000_000_000_000_000_000u128,
		);
	});
}

#[test]
fn reward_accumulates_over_blocks() {
	new_test_ext().execute_with(|| {
		let miner = Sr25519Keyring::Alice.to_account_id();

		set_author_digest(&miner);
		crate::pallet::Pallet::<Test>::on_finalize(1);

		set_author_digest(&miner);
		crate::pallet::Pallet::<Test>::on_finalize(2);

		assert_eq!(
			pallet_balances::Pallet::<Test>::free_balance(miner),
			100_000_000_000_000_000_000u128,
		);
	});
}

// Digest handling tests.

#[test]
fn no_reward_without_digest() {
	new_test_ext().execute_with(|| {
		let miner = Sr25519Keyring::Alice.to_account_id();

		crate::pallet::Pallet::<Test>::on_finalize(1);

		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner), 0);
	});
}

#[test]
fn ignores_non_pow_digest() {
	new_test_ext().execute_with(|| {
		let miner = Sr25519Keyring::Alice.to_account_id();
		let digest_item = DigestItem::PreRuntime(*b"aura", miner.encode());
		frame_system::Pallet::<Test>::deposit_log(digest_item);

		crate::pallet::Pallet::<Test>::on_finalize(1);

		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner), 0);
	});
}