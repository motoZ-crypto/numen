use crate::mock::*;
use frame_support::traits::Get;
use sp_keyring::Sr25519Keyring;

#[test]
fn mints_reward_only_to_block_author() {
    new_test_ext().execute_with(|| {
        let reward = <Test as crate::Config>::BlockReward::get();

        let author = Sr25519Keyring::Alice.to_account_id();
        let other = Sr25519Keyring::Bob.to_account_id();

        advance_block_with(pow_author_digest(&author));

        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(author),
            reward,
            "Reward should be minted to the block author"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(other),
            0,
            "Other accounts should not receive the block reward"
        );
    });
}

#[test]
fn mints_reward_only_to_block_author_when_multiple_digests() {
    new_test_ext().execute_with(|| {
        let reward = <Test as crate::Config>::BlockReward::get();

        let author = Sr25519Keyring::Alice.to_account_id();
        let other = Sr25519Keyring::Bob.to_account_id();

        advance_block_with_array(Some(&[
            other_pre_runtime_digest(&other),
            other_digest(),
            pow_author_digest(&author),
        ]));

        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(author),
            reward,
            "Reward should be minted to the block author"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(other),
            0,
            "Other accounts should not receive the block reward"
        );
    });
}

#[test]
fn reward_accumulates_over_blocks() {
    new_test_ext().execute_with(|| {
        let reward: Balance = <Test as crate::Config>::BlockReward::get();

        let miner1 = Sr25519Keyring::Alice.to_account_id();
        let miner2 = Sr25519Keyring::Bob.to_account_id();

        advance_block_with(pow_author_digest(&miner1));
        advance_block_with(pow_author_digest(&miner2));
        advance_block_with(pow_author_digest(&miner1));
        advance_block_with(pow_author_digest(&miner2));

        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner1), reward * 2);
        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner2), reward * 2);
    });
}

#[test]
fn no_reward_without_digest() {
    new_test_ext().execute_with(|| {
        let miner = Sr25519Keyring::Alice.to_account_id();
        advance_block();
        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner), 0);
    });
}

#[test]
fn no_reward_without_pow_digest() {
    new_test_ext().execute_with(|| {
        let miner = Sr25519Keyring::Alice.to_account_id();
        advance_block_with(other_pre_runtime_digest(&miner));
        assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner), 0);
    });
}
