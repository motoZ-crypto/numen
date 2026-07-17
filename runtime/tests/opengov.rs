//! OpenGov wiring. Tiered spender origins approve treasury backed bounties up
//! to each tier cap, and a higher tier clears what a lower tier cannot.

mod common;

use common::new_test_ext;
use frame_support::{assert_noop, assert_ok, traits::tokens::fungible::Mutate};
use pallet_bounties::BountyStatus;
use solochain_template_runtime::{
	configs::governance::pallet_custom_origins, Balance, Balances, Bounties, Runtime, RuntimeOrigin,
	UNIT,
};
use sp_keyring::Sr25519Keyring;
use sp_runtime::DispatchError;

/// Small tier funding cap. A referendum on the small track releases at most this.
const SMALL_CAP: Balance = 100_000 * UNIT;

/// Fund a proposer and place a bounty of `value`, returning its id.
fn proposed_bounty(value: Balance) -> u32 {
	let proposer = Sr25519Keyring::Alice.to_account_id();
	Balances::set_balance(&proposer, 10_000 * UNIT);
	let id = pallet_bounties::BountyCount::<Runtime>::get();
	assert_ok!(Bounties::propose_bounty(
		RuntimeOrigin::signed(proposer),
		value,
		b"work".to_vec(),
	));
	id
}

fn assert_queued_for_funding(id: u32) {
	let bounty = pallet_bounties::Bounties::<Runtime>::get(id).expect("bounty exists");
	assert!(matches!(bounty.get_status(), BountyStatus::Approved));
	assert!(
		pallet_bounties::BountyApprovals::<Runtime>::get().contains(&id),
		"an approved bounty must be queued for the next spend period",
	);
}

fn assert_still_proposed(id: u32) {
	let bounty = pallet_bounties::Bounties::<Runtime>::get(id).expect("bounty exists");
	assert!(
		matches!(bounty.get_status(), BountyStatus::Proposed),
		"a rejected approval must leave the bounty proposed",
	);
	assert!(pallet_bounties::BountyApprovals::<Runtime>::get().is_empty());
}

#[test]
fn small_spender_approves_bounty_at_its_cap() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty(SMALL_CAP);
		let small = RuntimeOrigin::from(pallet_custom_origins::Origin::SmallSpender);

		assert_ok!(Bounties::approve_bounty(small, id));

		assert_queued_for_funding(id);
	});
}

#[test]
fn small_spender_cannot_approve_above_its_cap() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty(SMALL_CAP + 1);
		let small = RuntimeOrigin::from(pallet_custom_origins::Origin::SmallSpender);

		assert_noop!(
			Bounties::approve_bounty(small, id),
			pallet_treasury::Error::<Runtime>::InsufficientPermission,
		);

		assert_still_proposed(id);
	});
}

#[test]
fn higher_tier_approves_what_lower_tier_cannot() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty(SMALL_CAP + 1);
		let medium = RuntimeOrigin::from(pallet_custom_origins::Origin::MediumSpender);

		assert_ok!(Bounties::approve_bounty(medium, id));

		assert_queued_for_funding(id);
	});
}

#[test]
fn root_origin_still_approves_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty(SMALL_CAP + 1);

		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), id));

		assert_queued_for_funding(id);
	});
}

#[test]
fn signed_origin_cannot_approve_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty(1_000 * UNIT);
		let who = Sr25519Keyring::Bob.to_account_id();

		assert_noop!(
			Bounties::approve_bounty(RuntimeOrigin::signed(who), id),
			DispatchError::BadOrigin,
		);

		assert_still_proposed(id);
	});
}
