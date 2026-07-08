use crate::mock::*;
use frame_support::traits::Get;
use sp_keyring::Sr25519Keyring;
use sp_runtime::AccountId32;

fn initial_reward() -> Balance {
    <Test as crate::Config>::InitialReward::get()
}

fn halving_interval() -> u64 {
    <Test as crate::Config>::HalvingInterval::get()
}

/// Mine empty blocks up to `height - 1`, then a single block authored by
/// `miner` at `height`, and return the miner's resulting balance (which equals
/// the reward for that height, since the miner starts with nothing).
fn reward_at(height: u64, miner: &AccountId32) -> Balance {
    for _ in 1..height {
        advance_block();
    }
    advance_block_with(pow_author_digest(miner));
    pallet_balances::Pallet::<Test>::free_balance(miner.clone())
}

#[test]
fn mints_reward_only_to_block_author() {
    new_test_ext().execute_with(|| {
        let author = Sr25519Keyring::Alice.to_account_id();
        let other = Sr25519Keyring::Bob.to_account_id();

        advance_block_with(pow_author_digest(&author));

        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(author),
            initial_reward(),
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
        let author = Sr25519Keyring::Alice.to_account_id();
        let other = Sr25519Keyring::Bob.to_account_id();

        advance_block_with_array(Some(&[
            other_pre_runtime_digest(&other),
            other_digest(),
            pow_author_digest(&author),
        ]));

        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(author),
            initial_reward(),
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
        let reward = initial_reward();

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

#[test]
fn reward_is_full_within_first_interval() {
    new_test_ext().execute_with(|| {
        let miner = Sr25519Keyring::Alice.to_account_id();
        // Last block before the first halving still pays the initial reward.
        let balance = reward_at(halving_interval() - 1, &miner);
        assert_eq!(balance, initial_reward());
    });
}

#[test]
fn reward_halves_at_each_interval() {
    new_test_ext().execute_with(|| {
        let miner = Sr25519Keyring::Alice.to_account_id();
        // Height == HalvingInterval crosses the first boundary: reward halves.
        let balance = reward_at(halving_interval(), &miner);
        assert_eq!(balance, initial_reward() / 2);
    });
}

#[test]
fn reward_shifts_down_over_many_intervals() {
    new_test_ext().execute_with(|| {
        // InitialReward is 1024 == 2^10, so after 10 halvings it is exactly 1.
        let miner = Sr25519Keyring::Alice.to_account_id();
        let balance = reward_at(halving_interval() * 10, &miner);
        assert_eq!(balance, initial_reward() >> 10);
        assert_eq!(balance, 1);
    });
}

#[test]
fn emission_ends_once_reward_shifts_out() {
    new_test_ext().execute_with(|| {
        // After 11 halvings the 2^10 reward has been fully shifted out: zero.
        let miner = Sr25519Keyring::Alice.to_account_id();
        let balance = reward_at(halving_interval() * 11, &miner);
        assert_eq!(balance, 0, "emission must stop once the reward reaches zero");
    });
}
